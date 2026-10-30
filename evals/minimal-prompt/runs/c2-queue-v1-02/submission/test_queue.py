"""Run the real Queue and check observed histories with QueueContract (Queue.p).

Events come from real calls: eObserved at each successful compare_swap (recorded
under a lock so order = commit order), eReturned at each API return.
"""
import random
import threading
from pathlib import Path

import pytest
from snappea import SpecViolation, check_events

import runtime
from solution import Queue

SPEC = str(Path(__file__).with_name("Queue.p"))


class Recording:
    def __init__(self, capacity):
        self.events = [{"event": "eInit", "payload": {"cap": capacity, "items": []}}]
        self.lock = threading.Lock()
        self.commits = 0
        self.calls = 0
        rec = self

        class ObservedStore(runtime.Store):
            def compare_swap(self, version, record):
                with rec.lock:
                    _, old = self.load()
                    ok = super().compare_swap(version, record)
                    if ok:
                        old_memo = (old or {"memo": {}})["memo"]
                        new = [k for k in record["memo"] if k not in old_memo]
                        if not new:  # defective impl rewrote an existing memo; monitor sees replies
                            return ok
                        (key,) = new
                        kind, rid = key.split(":")
                        arg, res = record["memo"][key]
                        take = kind == "take"
                        r = {"ok": res is not None, "val": res or 0} if take else {"ok": res, "val": 0}
                        rec.events.append({"event": "eObserved", "payload": {
                            "cmd": {"id": int(rid), "isTake": take, "item": arg or 0},
                            "r": r, "items": list(record["items"])}})
                        rec.commits += 1
                    return ok

        self.store = ObservedStore()

    def returned(self, rid, take, item, result):
        r = {"ok": result is not None, "val": result or 0} if take else {"ok": result, "val": 0}
        with self.lock:
            self.calls += 1
            self.events.append({"event": "eReturned", "payload": {
                "cmd": {"id": rid, "isTake": take, "item": item or 0}, "r": r}})

    def put(self, q, rid, item):
        res = q.put(str(rid), item)
        self.returned(rid, False, item, res)
        return res

    def take(self, q, rid):
        res = q.take(str(rid))
        self.returned(rid, True, None, res)
        return res

    def check(self):
        self.events.append({"event": "eDone", "payload": {"obs": self.commits, "replies": self.calls}})
        check_events(self.events, "QueueContract", spec_files=[SPEC], strict=True, check_hot=True)


@pytest.mark.parametrize("cap", [1, 2, 3, 5])
def test_sequential_boundaries_and_retries(cap):
    r = Recording(cap)
    q = Queue(r.store, cap)
    assert q.snapshot() == []
    assert r.take(q, 0) is None                      # empty
    assert r.take(q, 0) is None                      # retry of empty take
    for i in range(cap):                             # fill, incl. 0 and negative items
        assert r.put(q, 10 + i, [0, -3, 7, 8, 9][i]) is True
    assert q.snapshot() == [0, -3, 7, 8, 9][:cap]
    assert r.put(q, 100, 42) is False                # full: rejected, no effect
    assert q.snapshot() == [0, -3, 7, 8, 9][:cap]
    assert r.take(q, 1) == 0                         # oldest, falsy item
    assert r.put(q, 100, 42) is False                # retry stays rejected though room exists
    assert r.put(q, 10, 0) is True                   # retry of accepted put: original result
    assert q.snapshot() == [0, -3, 7, 8, 9][1:cap]   # ...and no second effect
    assert r.take(q, 1) == 0                         # retry returns original, removes nothing
    assert r.take(q, 0) is None                      # old empty take stays None
    assert r.put(q, 1, 55) is True                   # put/take same id are different requests
    assert q.snapshot() == [0, -3, 7, 8, 9][1:cap] + [55]
    r.check()


def test_new_queue_objects_share_state():
    r = Recording(2)
    a, b = Queue(r.store, 2), Queue(r.store, 2)
    assert r.put(a, 1, 5) and r.put(b, 2, 6)
    assert not r.put(a, 3, 7)
    assert b.snapshot() == [5, 6]
    assert r.take(b, 4) == 5 and r.take(a, 5) == 6
    r.check()


@pytest.mark.parametrize("seed", range(6))
def test_threads_with_retries(seed):
    cap = 1 + seed % 3
    r = Recording(cap)
    nthreads, nops = 6, 25
    start = threading.Barrier(nthreads)
    errors = []

    def worker(t):
        rnd = random.Random(seed * 100 + t)
        q = Queue(r.store, cap)
        start.wait()
        try:
            for n in range(nops):
                rid = t * 1000 + n
                if rnd.random() < 0.5:
                    first = r.put(q, rid, rid)
                    again = [r.put(q, rid, rid) for _ in range(rnd.randint(0, 2))]
                else:
                    first = r.take(q, rid)
                    again = [r.take(q, rid) for _ in range(rnd.randint(0, 2))]
                assert all(x == first for x in again)
        except BaseException as e:  # noqa
            errors.append(e)

    ts = [threading.Thread(target=worker, args=(t,)) for t in range(nthreads)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    assert not errors, errors
    assert len(Queue(r.store, cap).snapshot()) <= cap
    r.check()


def test_same_request_concurrently_applies_once():
    r = Recording(3)
    start = threading.Barrier(8)

    def go():
        q = Queue(r.store, 3)
        start.wait()
        assert r.put(q, 7, 77) is True

    ts = [threading.Thread(target=go) for _ in range(8)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    assert Queue(r.store, 3).snapshot() == [77]
    r.check()


# --- the monitor must reject broken implementations (checks the checker) ---

class NoMemo(Queue):
    """Defect: retries re-execute instead of replaying the memo."""
    def _apply(self, key, arg, op):
        while True:
            version, rec = self._store.load()
            rec = rec or {"items": [], "memo": {}}
            rec["memo"].pop(key, None)
            result = op(rec["items"])
            rec["memo"][key] = [arg, result]
            if self._store.compare_swap(version, rec):
                return result


class IgnoresCapacity(Queue):
    def __init__(self, store, capacity=2):
        super().__init__(store, capacity + 1)


@pytest.mark.parametrize("bad", [NoMemo, IgnoresCapacity])
def test_monitor_rejects_defects(bad):
    r = Recording(2)
    q = bad(r.store, 2)
    r.put(q, 0, 1); r.put(q, 1, 2); r.put(q, 2, 3)
    r.take(q, 0); r.take(q, 0); r.put(q, 1, 2)
    with pytest.raises(SpecViolation):
        r.check()
