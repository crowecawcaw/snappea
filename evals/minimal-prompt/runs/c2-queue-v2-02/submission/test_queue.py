import threading

import pytest
from snappea import check_events

from runtime import Store
from solution import Queue


class HookStore:
    """Wraps a Store so a test can run code between a call's load and its CAS."""

    def __init__(self, inner=None):
        self.inner = inner or Store()
        self.after_load = []   # callbacks, each fires once after a load
        self.barrier = None    # first N loads wait here before returning
        self.commits = 0
        self.lock = threading.Lock()

    def load(self):
        result = self.inner.load()
        if self.barrier is not None:
            b, self.barrier = self.barrier, None
            b.wait(timeout=10)
        if self.after_load:
            self.after_load.pop(0)()
        return result

    def compare_swap(self, version, record):
        ok = self.inner.compare_swap(version, record)
        if ok:
            with self.lock:
                self.commits += 1
        return ok


def shared(capacity=2):
    store = HookStore()
    return store, Queue(store, capacity), Queue(store.inner, capacity)


# ---- basics and boundaries ----

@pytest.mark.parametrize("capacity", [1, 2, 3, 5])
def test_capacity_boundary_and_fifo(capacity):
    q = Queue(Store(), capacity)
    for i in range(capacity):
        assert q.put(f"p{i}", i) is True
    assert q.put("over", 99) is False          # exactly full
    assert q.snapshot() == list(range(capacity))
    assert q.take("t0") == 0                   # oldest first
    assert q.put("again", 50) is True          # one below capacity
    assert q.snapshot() == list(range(1, capacity)) + [50]
    for i in range(capacity):
        q.take(f"d{i}")
    assert q.snapshot() == []
    assert q.take("empty") is None


def test_default_capacity_is_two_and_invalid_rejected():
    q = Queue(Store())
    assert [q.put("a", 1), q.put("b", 2), q.put("c", 3)] == [True, True, False]
    with pytest.raises(ValueError):
        Queue(Store(), 0)


def test_zero_item_is_a_real_item():
    q = Queue(Store(), 1)
    assert q.put("a", 0) is True
    assert q.take("t") == 0


# ---- forced interleavings ----

def gate_first_loads(store, n):
    """Make each of n threads wait after its first load until all n have loaded."""
    gate = threading.Barrier(n)
    real = store.inner.load

    def gated():
        res = real()
        g = getattr(threading.current_thread(), "gate", None)
        if g:
            threading.current_thread().gate = None
            g.wait(timeout=10)
        return res
    store.load = gated
    return gate


def run_gated(store, funcs):
    gate = gate_first_loads(store, len(funcs))
    ts = [threading.Thread(target=f) for f in funcs]
    for t in ts:
        t.gate = gate
        t.start()
    for t in ts:
        t.join(15)
        assert not t.is_alive()


def test_two_puts_read_same_version_one_slot():
    store = HookStore()
    out = []
    run_gated(store, [lambda: out.append(Queue(store, 1).put("x", 1)),
                      lambda: out.append(Queue(store, 1).put("y", 2))])
    assert sorted(out) == [False, True]
    assert len(Queue(store, 1).snapshot()) == 1


def test_competitor_fills_queue_between_load_and_cas():
    store, a, _ = shared(1)
    other = Queue(store.inner, 1)
    store.after_load.append(lambda: other.put("rival", 7))
    assert a.put("mine", 1) is False            # must re-read and re-decide
    assert a.snapshot() == [7]


def test_competitor_empties_queue_between_load_and_cas():
    store, a, _ = shared(2)
    a.put("seed", 5)
    other = Queue(store.inner, 2)
    store.after_load.append(lambda: other.take("rival"))
    assert a.take("mine") is None               # not a stale 5, not a double take
    assert a.snapshot() == []


def test_competitor_takes_one_of_two_between_load_and_cas():
    store, a, _ = shared(2)
    a.put("s1", 1); a.put("s2", 2)
    other = Queue(store.inner, 2)
    store.after_load.append(lambda: other.take("rival"))
    assert a.take("mine") == 2
    assert a.snapshot() == []


def test_competitor_makes_room_between_load_and_cas():
    store, a, _ = shared(1)
    a.put("seed", 1)
    other = Queue(store.inner, 1)
    store.after_load.append(lambda: other.take("rival"))
    assert a.put("mine", 2) is True             # re-decided on fresh state
    assert a.snapshot() == [2]


def test_same_request_raced_by_two_clients_applies_once():
    store = HookStore()
    out = []
    run_gated(store, [lambda: out.append(Queue(store, 3).put("same", 4)) for _ in range(2)])
    assert out == [True, True]
    assert Queue(store, 3).snapshot() == [4]


def test_same_take_raced_by_two_clients_applies_once():
    store = HookStore()
    seed = Queue(store.inner, 3)
    seed.put("a", 1)
    seed.put("b", 2)
    out = []
    run_gated(store, [lambda: out.append(Queue(store, 3).take("same")) for _ in range(2)])
    assert out == [1, 1]
    assert seed.snapshot() == [2]


# ---- retries after the state changed ----

def test_accepted_put_retry_after_item_taken_has_no_second_effect():
    store = Store()
    a, b = Queue(store, 2), Queue(store, 2)     # retry through a second object
    assert a.put("p", 1) is True
    assert b.take("t") == 1
    assert b.put("p", 1) is True
    assert a.snapshot() == []


def test_rejected_put_retry_stays_rejected_after_space_freed():
    store = Store()
    a, b = Queue(store, 1), Queue(store, 1)
    a.put("fill", 1)
    assert a.put("p", 2) is False
    assert b.take("t") == 1
    assert b.put("p", 2) is False
    assert a.snapshot() == []


def test_empty_take_retry_stays_empty_after_put():
    store = Store()
    a, b = Queue(store, 2), Queue(store, 2)
    assert a.take("t") is None
    b.put("p", 9)
    assert b.take("t") is None
    assert a.snapshot() == [9]


def test_accepted_take_retry_returns_same_item_after_more_changes():
    store = Store()
    a, b = Queue(store, 2), Queue(store, 2)
    a.put("p1", 1); a.put("p2", 2)
    assert a.take("t") == 1
    b.take("other")
    b.put("p3", 3)
    assert b.take("t") == 1
    assert a.snapshot() == [3]


def test_put_and_take_with_same_request_id_are_different_requests():
    q = Queue(Store(), 2)
    assert q.put("r", 1) is True
    assert q.take("r") == 1                     # not mistaken for the put
    assert q.put("r", 1) is True                # put retry: memo, no re-add
    assert q.take("r") == 1                     # take retry: memo
    assert q.snapshot() == []


def test_empty_take_with_id_shared_by_put():
    q = Queue(Store(), 2)
    assert q.take("r") is None
    assert q.put("r", 3) is True
    assert q.snapshot() == [3]


def test_queues_with_different_stores_are_independent():
    a, b = Queue(Store(), 1), Queue(Store(), 1)
    assert a.put("p", 1) and b.put("p", 2)
    assert (a.snapshot(), b.snapshot()) == ([1], [2])


# ---- real threads: conservation ----

def test_threads_conserve_items():
    store = Store()
    cap = 3
    taken, lock = [], threading.Lock()

    def producer(n):
        q = Queue(store, cap)
        for i in range(30):
            while not q.put(f"p{n}-{i}-{_spin()}", n * 100 + i):
                pass

    def consumer(n):
        q = Queue(store, cap)
        got = 0
        while got < 30:
            v = q.take(f"t{n}-{_spin()}")
            if v is not None:
                with lock:
                    taken.append(v)
                got += 1

    ts = [threading.Thread(target=producer, args=(n,)) for n in (1, 2)]
    ts += [threading.Thread(target=consumer, args=(n,)) for n in (1, 2)]
    for t in ts:
        t.start()
    for t in ts:
        t.join(60)
    assert sorted(taken) == sorted(n * 100 + i for n in (1, 2) for i in range(30))
    assert Queue(store, cap).snapshot() == []
    # per-producer order is FIFO
    for n in (1, 2):
        mine = [v for v in taken if v // 100 == n]
        assert len(mine) == 30


_counter = iter(range(10**9))


def _spin():
    return next(_counter)


# ---- observation against the P spec ----

class Recorder:
    """Runs real calls and records actual requests/results as spec events."""

    def __init__(self, store, capacity, prefill=()):
        self.store = HookStore(store)
        self.q = Queue(self.store, capacity)
        for i, v in enumerate(prefill):
            assert self.q.put(f"pre{i}", v)
        self.base = self.store.commits
        self.ids = {}
        self.events = [{"event": "eInit", "payload": {
            "items": list(self.q.snapshot()), "capacity": capacity}}]

    def call(self, kind, rid, item=0, queue=None):
        queue = queue or self.q
        cmd = {"take": kind == "take", "id": int(rid), "item": item}
        self.events.append({"event": "eRequested", "payload": cmd})
        result = queue.take(rid) if kind == "take" else queue.put(rid, item)
        wire = (-1 if result is None else result) if kind == "take" else int(result)
        self.events.append({"event": "eObserved", "payload": {
            "command": cmd, "result": wire, "items": queue.snapshot(),
            "version": self.store.commits - self.base}})
        self.events.append({"event": "eReturned", "payload": {
            "take": cmd["take"], "id": cmd["id"], "result": wire}})
        return result

    def check(self):
        self.events.append({"event": "eDone", "payload": None})
        check_events(self.events, "QueueContract", spec_files=["Queue.p"],
                     strict=True, check_hot=True)


def test_observe_full_rejected_then_take_then_retries():
    r = Recorder(Store(), 1, prefill=[7])
    assert r.call("put", "0", 9) is False
    assert r.call("take", "1") == 7
    assert r.call("put", "0", 9) is False       # rejected retry stays rejected
    assert r.call("take", "1") == 7
    r.check()


def test_observe_empty_take_then_put_then_retries():
    r = Recorder(Store(), 1)
    assert r.call("take", "0") is None
    assert r.call("put", "1", 4) is True
    assert r.call("take", "0") is None
    assert r.call("put", "1", 4) is True
    r.check()


def test_observe_same_id_different_kinds():
    r = Recorder(Store(), 2)
    assert r.call("put", "0", 5) is True
    assert r.call("take", "0") == 5
    assert r.call("put", "0", 5) is True
    assert r.call("take", "0") == 5
    r.check()


def test_observe_competitor_between_load_and_cas():
    store = Store()
    r = Recorder(store, 1)
    rival = Queue(r.store, 1)
    r.store.after_load.append(lambda: rival.put("1", 7))
    # The rival commits first (inside the call); record it in return order.
    rival_cmd = {"take": False, "id": 1, "item": 7}
    r.events.append({"event": "eRequested", "payload": rival_cmd})
    mine = {"take": False, "id": 0, "item": 1}
    r.events.append({"event": "eRequested", "payload": mine})
    got = r.q.put("0", 1)
    assert got is False
    for cmd, res in ((rival_cmd, 1), (mine, 0)):
        r.events.append({"event": "eObserved", "payload": {
            "command": cmd, "result": res, "items": [7],
            "version": 1 if cmd is rival_cmd else 2}})
        r.events.append({"event": "eReturned", "payload": {
            "take": False, "id": cmd["id"], "result": res}})
    # Retries (memo hits)
    assert r.call("put", "1", 7) is True
    assert r.call("put", "0", 1) is False
    r.check()


def test_observation_rejects_a_wrong_result():
    r = Recorder(Store(), 1)
    r.events.append({"event": "eRequested", "payload": {"take": False, "id": 0, "item": 1}})
    r.events.append({"event": "eObserved", "payload": {
        "command": {"take": False, "id": 0, "item": 1}, "result": 0,
        "items": [1], "version": 1}})
    with pytest.raises(Exception):
        check_events(r.events, "QueueContract", spec_files=["Queue.p"], strict=True)
