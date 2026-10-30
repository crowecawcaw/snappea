import random
from pathlib import Path
from threading import Barrier, Thread

import pytest
from snappea import check_spec, emit

from runtime import Store
from solution import Queue

SPEC = str(Path(__file__).with_name("Queue.p"))


class Observed:
    """Wraps real calls and emits the actual returned values for the monitor."""

    def __init__(self, q, cap):
        self.q = q
        emit("eConfig", cap=cap)

    def put(self, rid, item):
        r = self.q.put(rid, item)
        emit("eReturn", kind=0, rid=str(rid), result=int(r))
        return r

    def take(self, rid):
        r = self.q.take(rid)
        emit("eReturn", kind=1, rid=str(rid), result=-1 if r is None else r)
        return r


@pytest.mark.parametrize("cap", [1, 2, 3])
def test_fifo_full_empty_and_retries_observed(cap):
    with check_spec("QueueContract", spec_files=[SPEC], strict=True, check_hot=True):
        if True:
            s = Store()
            q = Observed(Queue(s, cap), cap)
            assert q.take("0") is None            # empty
            for i in range(cap):
                assert q.put(str(i), 10 + i) is True
            assert q.put("9", 99) is False         # full; rejection has no effect
            assert Queue(s, cap).snapshot() == [10 + i for i in range(cap)]
            assert q.put("9", 99) is False         # retry of rejected put
            assert q.take("1") == 10
            assert q.put("9", 99) is False         # still the original result
            assert q.put("0", 10) is True          # retry of old accepted put: no re-add
            assert Queue(s, cap).snapshot() == [10 + i for i in range(1, cap)]
            assert q.take("1") == 10               # retry of take: same item, nothing removed
            assert q.take("2") == (11 if cap > 1 else None)
        emit("eFinished")


def test_rejected_take_on_empty_is_remembered():
    s = Store(); q = Queue(s, 2)
    assert q.take("t") is None
    assert q.put("p", 5) is True
    assert q.take("t") is None                     # retry must not see the new item
    assert q.snapshot() == [5]
    assert q.take("u") == 5


def test_put_and_take_same_request_id_are_distinct():
    s = Store(); q = Queue(s, 2)
    assert q.put("x", 1) is True
    assert q.take("x") == 1
    assert q.put("x", 1) is True                   # retry of put: no effect
    assert q.snapshot() == []
    assert q.take("x") == 1


def test_zero_and_negative_items():
    s = Store(); q = Queue(s, 3)
    assert q.put("a", 0) and q.put("b", -5)
    assert q.take("c") == 0 and q.take("d") == -5 and q.take("e") is None


def test_state_lives_in_store_and_is_shared():
    s = Store()
    a, b = Queue(s, 2), Queue(s, 2)
    a.put("1", 1); b.put("2", 2)
    assert a.snapshot() == b.snapshot() == [1, 2]
    assert b.put("3", 3) is False
    assert a.take("4") == 1


@pytest.mark.parametrize("cap", [1, 2, 5])
def test_concurrent_producers_consumers(cap):
    for seed in range(20):
        s = Store()
        n = 6
        start = Barrier(2 * n)
        puts, takes = {}, {}

        def producer(i):
            q = Queue(s, cap); start.wait()
            puts[i] = q.put(f"p{i}", i)
            assert q.put(f"p{i}", i) == puts[i]     # immediate retry

        def consumer(i):
            q = Queue(s, cap); start.wait()
            takes[i] = q.take(f"t{i}")
            assert q.take(f"t{i}") == takes[i]

        ts = [Thread(target=producer, args=(i,)) for i in range(n)] + \
             [Thread(target=consumer, args=(i,)) for i in range(n)]
        for t in ts: t.start()
        for t in ts: t.join()
        accepted = sorted(i for i, ok in puts.items() if ok)
        got = sorted(v for v in takes.values() if v is not None)
        left = Queue(s, cap).snapshot()
        assert len(left) <= cap
        assert sorted(got + left) == accepted       # conservation, no dup/loss


def test_duplicate_retry_races():
    # Same request id issued from many threads takes effect once.
    s = Store(); start = Barrier(8); out = []
    def go():
        q = Queue(s, 3); start.wait(); out.append(q.put("same", 7))
    ts = [Thread(target=go) for _ in range(8)]
    for t in ts: t.start()
    for t in ts: t.join()
    assert out == [True] * 8 and Queue(s, 3).snapshot() == [7]


def test_random_sequential_vs_reference():
    rnd = random.Random(1)
    for cap in (1, 2, 4):
        s = Store(); ref = []; log = {}
        for step in range(400):
            q = Queue(s, cap)
            rid = str(rnd.randrange(30)); 
            if rnd.random() < .5:
                item = int(rid)
                key = ("p", rid)
                if key not in log:
                    log[key] = len(ref) < cap
                    if log[key]: ref.append(item)
                assert q.put(rid, item) == log[key]
            else:
                key = ("t", rid)
                if key not in log:
                    log[key] = ref.pop(0) if ref else None
                assert q.take(rid) == log[key]
            assert q.snapshot() == ref


def test_monitor_catches_defective_queue():
    from snappea import expect_violation

    class NoCapacity(Queue):
        def put(self, request_id, item):  # defect: ignores capacity
            self.capacity = 10 ** 6
            r = super().put(request_id, item)
            emit("eReturn", kind=0, rid=request_id, result=int(r))
            return r

    with expect_violation("QueueContract", match="full queue", spec_files=[SPEC]):
        emit("eConfig", cap=1)
        q = NoCapacity(Store(), 1)
        q.put("a", 1); q.put("b", 2)
