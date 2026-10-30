import threading
from pathlib import Path

import pytest
from snappea import check_events

import solution
from runtime import Store
from solution import Queue

SPEC = str(Path(__file__).with_name("Queue.p"))


class HookStore:
    """Wraps a real Store; hooks run right after a load, before the caller's CAS."""

    def __init__(self, inner=None):
        self.inner = inner or Store()
        self.after_load = None
        self.cas_results = []

    def load(self):
        out = self.inner.load()
        hook, self.after_load = self.after_load, None  # one-shot
        if hook:
            hook()
        return out

    def compare_swap(self, version, record):
        ok = self.inner.compare_swap(version, record)
        self.cas_results.append(ok)
        return ok


def test_fifo_and_capacity():
    q = Queue(Store())
    assert q.take("t0") is None
    assert q.put("a", 1) and q.put("b", 2)
    assert not q.put("c", 3)
    assert q.snapshot() == [1, 2]
    assert q.take("t1") == 1
    assert q.put("d", 4)
    assert [q.take("t2"), q.take("t3"), q.take("t4")] == [2, 4, None]


@pytest.mark.parametrize("cap", [1, 2, 3, 5])
def test_capacity_boundary(cap):
    q = Queue(Store(), capacity=cap)
    for i in range(cap):
        assert q.put("p%d" % i, i + 1)
    assert q.snapshot() == list(range(1, cap + 1))
    assert not q.put("over", 99)  # exactly one past the limit
    assert q.take("t") == 1
    assert q.put("again", 99)  # exactly at the limit after a take
    assert not q.put("over2", 100)


def test_put_retry_does_not_reapply():
    q = Queue(Store())
    assert q.put("a", 1)
    assert q.take("t") == 1
    assert q.put("a", 1)  # retry: original True, no new item
    assert q.snapshot() == []


def test_rejected_put_stays_rejected_after_space_frees():
    q = Queue(Store())
    q.put("a", 1); q.put("b", 2)
    assert not q.put("c", 3)
    assert q.take("t") == 1
    assert not q.put("c", 3)  # still rejected, no effect
    assert q.snapshot() == [2]


def test_take_retry_returns_original_and_does_not_remove():
    q = Queue(Store())
    q.put("a", 1); q.put("b", 2)
    assert q.take("t") == 1
    assert q.take("t") == 1
    assert q.snapshot() == [2]


def test_empty_take_stays_empty_after_put():
    q = Queue(Store())
    assert q.take("t") is None
    assert q.put("a", 1)
    assert q.take("t") is None  # retry of empty result
    assert q.snapshot() == [1]


def test_put_and_take_same_id_are_different():
    q = Queue(Store())
    assert q.put("x", 7)
    assert q.take("x") == 7
    assert q.put("x", 7)  # retry of the put, not a new put
    assert q.snapshot() == []
    assert q.take("x") == 7  # retry of the take


def test_retry_through_second_object():
    s = Store()
    q1, q2 = Queue(s), Queue(s)
    assert q1.put("a", 1)
    assert q1.take("t") == 1
    assert q2.put("a", 1)
    assert q2.take("t") == 1
    assert q1.snapshot() == q2.snapshot() == []
    s2 = Store()
    assert Queue(s2, 1).take("e") is None
    assert Queue(s2, 1).put("a", 1)
    assert Queue(s2, 1).take("e") is None


def test_two_clients_read_same_version():
    store = HookStore()
    q1, q2 = Queue(store), Queue(store)
    barrier = threading.Barrier(2, timeout=5)
    seen = []
    orig = store.load

    def load():
        out = store.inner.load()
        if len(seen) < 2:
            seen.append(out[0])
            barrier.wait()
        return out

    store.load = load
    results = []
    ts = [threading.Thread(target=lambda q=q, n=n: results.append(q.put(n, ord(n))))
          for q, n in ((q1, "a"), (q2, "b"))]
    [t.start() for t in ts]; [t.join(10) for t in ts]
    assert seen[0] == seen[1]
    assert results == [True, True]
    assert sorted(Queue(store.inner).snapshot()) == [ord("a"), ord("b")]
    assert store.cas_results.count(False) == 1


def test_competitor_fills_queue_between_load_and_cas():
    store = HookStore()
    q = Queue(store, 1)
    other = Queue(store.inner, 1)
    store.after_load = lambda: other.put("rival", 5)
    assert q.put("mine", 6) is False  # must re-read and reject, not overwrite
    assert q.snapshot() == [5]
    assert q.put("mine", 6) is False


def test_competitor_empties_queue_between_load_and_cas():
    store = HookStore()
    q = Queue(store)
    q.put("a", 1)
    other = Queue(store.inner)
    store.after_load = lambda: other.take("rival")
    assert q.take("mine") is None  # item is gone: no duplicate delivery
    assert q.snapshot() == []


def test_competing_takes_do_not_duplicate_item():
    store = HookStore()
    q = Queue(store)
    q.put("a", 1); q.put("b", 2)
    other = Queue(store.inner)
    got = []
    store.after_load = lambda: got.append(other.take("rival"))
    got.append(q.take("mine"))
    assert sorted(got) == [1, 2]


def test_same_request_races_with_itself():
    store = HookStore()
    q = Queue(store)
    other = Queue(store.inner)
    got = []
    store.after_load = lambda: got.append(other.put("a", 1))
    got.append(q.put("a", 1))
    assert got == [True, True]
    assert q.snapshot() == [1]  # applied once


def test_thread_stress_no_loss_no_duplicates():
    s = Store()
    cap = 3
    produced, consumed = [], []

    def prod(n):
        q = Queue(s, cap)
        k = 0
        for i in range(30):
            v = n * 100 + i
            while True:  # a rejected id is memoized, so every attempt needs a fresh id
                k += 1
                if q.put("p%d-%d" % (n, k), v):
                    break
            produced.append(v)

    def cons2(n):
        q = Queue(s, cap)
        k = 0
        got = 0
        while got < 30:
            k += 1
            r = q.take("c%d-%d" % (n, k))
            if r is not None:
                consumed.append(r); got += 1
    ts = [threading.Thread(target=prod, args=(n,)) for n in (1, 2)] + \
         [threading.Thread(target=cons2, args=(n,)) for n in (1, 2)]
    [t.start() for t in ts]; [t.join(60) for t in ts]
    assert sorted(consumed) == sorted(produced) and len(consumed) == 60
    assert Queue(s, cap).snapshot() == []


class Recorder:
    """Records real calls and their results as P events for QueueContract."""

    def __init__(self, q):
        self.q, self.ev, self.n = q, [], 0

    def _op(self, kind, rid, item, result):
        self.n += 1
        self.ev.append({"event": "eObserved", "payload": {
            "op": {"id": rid, "kind": kind, "item": item}, "result": result,
            "items": self.q.snapshot()}})
        self.ev.append({"event": "eReturned", "payload": {
            "op": {"id": rid, "kind": kind, "item": item}, "result": result}})

    def put(self, rid, item):
        r = self.q.put(str(rid), item)
        self._op(0, rid, item, 1 if r else 0)
        return r

    def take(self, rid):
        r = self.q.take(str(rid))
        self._op(1, rid, 0, -1 if r is None else r)
        return r

    def check(self):
        self.ev.append({"event": "eDone", "payload": self.n})
        check_events(self.ev, "QueueContract", spec_files=[SPEC], strict=True, check_hot=True)


def test_observation_against_p_spec():
    r = Recorder(Queue(Store()))
    r.put(0, 1); r.put(1, 2)
    r.put(2, 3)          # full
    r.take(0)
    r.put(2, 3)          # retry of rejected, space now free
    r.take(5); r.take(6)
    r.take(7)            # empty
    r.put(0, 1)          # retry of accepted put after state changed
    r.take(7)            # retry of empty
    r.check()


def test_observation_detects_wrong_result():
    r = Recorder(Queue(Store()))
    r.put(0, 1)
    r.ev[-1]["payload"]["result"] = 0  # lie about the reply
    r.ev.append({"event": "eDone", "payload": r.n})
    with pytest.raises(Exception):
        check_events(r.ev, "QueueContract", spec_files=[SPEC], strict=True, check_hot=True)
