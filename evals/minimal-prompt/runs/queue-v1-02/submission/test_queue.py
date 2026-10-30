import random
import threading

import pytest
from snappea import check_spec, emit

from runtime import Store
from solution import Queue


def test_basic_fifo_and_full():
    q = Queue(Store())
    assert q.put("a", 1) and q.put("b", 2)
    assert not q.put("c", 3)
    assert q.snapshot() == [1, 2]
    assert q.take("t1") == 1
    assert not q.put("c", 3)  # retry of a rejected put stays rejected
    assert q.put("d", 3)
    assert q.snapshot() == [2, 3]
    assert [q.take("t2"), q.take("t3"), q.take("t4")] == [2, 3, None]


@pytest.mark.parametrize("cap", [1, 2, 5])
def test_capacity_any(cap):
    q = Queue(Store(), capacity=cap)
    assert [q.put(f"p{i}", i) for i in range(cap + 1)] == [True] * cap + [False]


def test_retries_and_same_id_distinct_kinds():
    s = Store()
    q = Queue(s, capacity=1)
    assert q.put("x", 5)
    assert q.put("x", 5)  # retry: no second effect
    assert q.snapshot() == [5]
    assert q.take("x") == 5  # same id, different request
    assert q.take("x") == 5  # retry
    assert q.snapshot() == []
    assert q.put("y", 6) and q.put("x", 5)  # old put x remains True, no re-add
    assert q.snapshot() == [6]
    assert not q.put("z", 7)
    assert q.take("t") == 6
    assert not q.put("z", 7)  # rejected put stays rejected though room now
    q2 = Queue(Store())
    assert q2.take("e") is None
    assert q2.put("a", 1)
    assert q2.take("e") is None  # old empty result stays
    assert q2.snapshot() == [1]


def test_shared_store_many_queue_objects():
    s = Store()
    a, b = Queue(s, 2), Queue(s, 2)
    assert a.put("1", 1) and b.put("2", 2) and not a.put("3", 3)
    assert b.take("t") == 1 and a.snapshot() == [2]


@pytest.mark.parametrize("seed", range(5))
def test_concurrent_conservation(seed):
    rnd = random.Random(seed)
    s = Store()
    cap = rnd.choice([1, 2, 3])
    accepted, taken, lock = [], [], threading.Lock()

    def worker(w):
        q = Queue(s, cap)
        for i in range(30):
            item = w * 100 + i
            r = q.put(f"p{w}-{i}", item)
            assert q.put(f"p{w}-{i}", item) == r
            if r:
                with lock:
                    accepted.append(item)
            v = q.take(f"t{w}-{i}")
            assert q.take(f"t{w}-{i}") == v
            if v is not None:
                with lock:
                    taken.append(v)
            assert len(q.snapshot()) <= cap

    ts = [threading.Thread(target=worker, args=(w,)) for w in range(4)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    rest = Queue(s, cap).snapshot()
    assert sorted(taken + rest) == sorted(accepted)  # nothing lost or duplicated
    for w in range(4):  # per-producer FIFO
        mine = [x for x in taken if x // 100 == w]
        assert mine == sorted(mine)


def test_observed_history_matches_monitor():
    class Rec:
        def __init__(self, q):
            self.q = q

        def put(self, i, item):
            r = self.q.put(i, item)
            emit("ePut", id=i, item=item, ok=r)
            return r

        def take(self, i):
            r = self.q.take(i)
            emit("eTake", id=i, ok=r is not None, val=-1 if r is None else r)
            return r

        def snap(self):
            emit("eSnap", items=self.q.snapshot())

    with check_spec("QueueObserved", spec_files=["observe/Observe.p"],
                    strict=True, unknown_events="error"):
        emit("eOpen", cap=2)
        r = Rec(Queue(Store(), 2))
        r.put(1, 10); r.put(1, 10); r.put(2, 11); r.put(3, 12); r.snap()
        r.take(1); r.take(1); r.put(3, 12); r.put(4, 13); r.snap()
        r.take(2); r.take(3); r.take(4); r.take(5); r.snap()
