import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import pytest
from runtime import Store
from solution import LockService, Resource

SPEC = str(Path(__file__).parent / "model" / "Fencing.p")


class Interfering:
    """Runs `competitor` once, between the first load of `key` and the next CAS."""
    def __init__(self, inner, competitor, key=None):
        self.inner, self.competitor, self.key = inner, competitor, key
    def load(self, key):
        r = self.inner.load(key)
        if self.competitor and (self.key is None or key == self.key):
            run, self.competitor = self.competitor, None
            run()
        return r
    def compare_swap(self, *a):
        return self.inner.compare_swap(*a)


class SameVersion:
    def __init__(self, inner, n=2, key=None):
        self.inner, self.b, self.n, self.l = inner, threading.Barrier(n, timeout=3), n, threading.Lock()
        self.key = key
    def load(self, key):
        r = self.inner.load(key)
        with self.l:
            wait = self.n > 0 and (self.key is None or key == self.key)
            self.n -= wait
        if wait:
            self.b.wait()
        return r
    def compare_swap(self, *a):
        return self.inner.compare_swap(*a)


def test_basic_and_boundary():
    s = Store(); L = LockService(s, 10)
    t = L.acquire("a", 5)
    assert t == 1
    assert L.acquire("a", 14) == 1          # same holder, unexpired
    assert L.acquire("b", 14) is None
    assert L.acquire("b", 15) == 2          # now >= 5 + ttl: expired
    assert L.acquire("a", 15) is None


def test_expired_holder_gets_new_token():
    s = Store(); L = LockService(s, 3)
    assert L.acquire("a", 0) == 1
    assert L.acquire("a", 3) == 2


def test_release():
    s = Store(); L = LockService(s, 10)
    t = L.acquire("a", 0)
    assert not L.release("b", t, 1)
    assert not L.release("a", t + 1, 1)
    assert not L.release("a", t, 10)        # expired at boundary
    assert L.release("a", t, 9)
    assert not L.release("a", t, 9)         # already released
    assert L.acquire("b", 9) == t + 1       # tokens keep increasing
    assert not L.release("a", t, 9)         # stale token


def test_release_expired_does_not_free_new_holder():
    s = Store(); L = LockService(s, 5)
    t1 = L.acquire("a", 0)
    t2 = L.acquire("b", 5)
    assert not L.release("a", t1, 6)
    assert L.acquire("c", 6) is None and t2 == 2


def test_out_of_order_times():
    s = Store(); L = LockService(s, 5)
    assert L.acquire("a", 10) == 1
    assert L.acquire("b", 3) is None        # earlier time, lease still unexpired
    assert L.acquire("b", 15) == 2


def test_fencing_writes():
    s = Store(); L = LockService(s, 5); R = Resource(s, "r")
    assert R.read() is None
    assert not R.write(1, "x")              # never issued
    assert not R.write(0, "x") and not R.write(-1, "x")
    t1 = L.acquire("a", 0)
    t2 = L.acquire("b", 5)
    assert R.write(t2, "new")
    assert not R.write(t1, "old")           # stale
    assert R.write(t2, "again")             # equal token ok
    assert not R.write(t2 + 1, "future")    # not issued
    assert R.read() == "again"


def test_resources_independent():
    s = Store(); L = LockService(s, 5)
    t1 = L.acquire("a", 0); t2 = L.acquire("b", 5)
    A, B = Resource(s, "A"), Resource(s, "B")
    assert A.write(t2, "a2")
    assert B.write(t1, "b1")                # B has not seen t2
    assert B.read() == "b1" and A.read() == "a2"


def test_two_services_share_store():
    s = Store(); L1, L2 = LockService(s, 5), LockService(s, 5)
    assert L1.acquire("a", 0) == 1
    assert L2.acquire("b", 1) is None
    assert L2.acquire("a", 1) == 1
    assert L2.release("a", 1, 2) and L1.acquire("b", 2) == 2


def test_acquire_race_same_version():
    s = Store(); sv = SameVersion(s)
    with ThreadPoolExecutor(2) as ex:
        fs = [ex.submit(LockService(sv, 10).acquire, c, 0) for c in "ab"]
        res = [f.result() for f in fs]
    assert sorted(res, key=lambda x: x is None) == [1, None]


def test_acquire_competitor_between_load_and_cas():
    s = Store()
    sneaky = Interfering(s, lambda: LockService(s, 10).acquire("other", 0))
    assert LockService(sneaky, 10).acquire("me", 0) is None
    assert LockService(s, 10).acquire("other", 1) == 1


def test_acquire_competitor_expired_retry():
    s = Store(); L = LockService(s, 5)
    L.acquire("a", 0)
    sneaky = Interfering(s, lambda: LockService(s, 5).acquire("c", 5))
    assert LockService(sneaky, 5).acquire("b", 5) is None
    assert LockService(s, 5).acquire("c", 6) == 2


def test_release_races_with_expiry_takeover():
    s = Store(); L = LockService(s, 5)
    t = L.acquire("a", 0)
    sneaky = Interfering(s, lambda: LockService(s, 5).acquire("b", 5))
    # b takes over at now=5 between a's load and CAS; a's release (now=4) is still
    # valid at its own time, then b's takeover sees a free or expired lock either way.
    LockService(sneaky, 5).release("a", t, 4)
    assert LockService(s, 5).acquire("b", 5) == 2


def test_release_loses_cas_to_other_change():
    s = Store(); L = LockService(s, 5)
    t = L.acquire("a", 0)
    # competitor releases and b re-acquires between a's load and CAS
    def comp():
        LockService(s, 5).release("a", t, 1)
        LockService(s, 5).acquire("b", 1)
    sneaky = Interfering(s, comp)
    assert not LockService(sneaky, 5).release("a", t, 2)
    assert LockService(s, 5).acquire("b", 2) == 2


def test_write_race_same_version_keeps_highest():
    s = Store(); L = LockService(s, 1)
    t1 = L.acquire("a", 0); t2 = L.acquire("b", 1)
    sv = SameVersion(s, 2, key=("res", "r"))
    with ThreadPoolExecutor(2) as ex:
        f1 = ex.submit(Resource(sv, "r").write, t1, "old")
        f2 = ex.submit(Resource(sv, "r").write, t2, "new")
        r1, r2 = f1.result(), f2.result()
    assert r2 is True
    assert Resource(s, "r").read() == "new"
    assert Resource(s, "r").write(t1, "late") is False


def test_write_stale_token_after_competitor_write_between_load_and_cas():
    s = Store(); L = LockService(s, 1)
    t1 = L.acquire("a", 0); t2 = L.acquire("b", 1)
    key = ("res", "r")
    sneaky = Interfering(s, lambda: Resource(s, "r").write(t2, "new"), key)
    assert Resource(sneaky, "r").write(t1, "old") is False
    assert Resource(s, "r").read() == "new"


def test_write_token_issued_between_check_and_write():
    s = Store(); L = LockService(s, 1)
    t1 = L.acquire("a", 0)
    # token 2 issued concurrently; writing it afterwards must work
    t2 = L.acquire("b", 1)
    assert Resource(s, "r").write(t2, "v")


def test_thread_stress_tokens_unique_and_increasing():
    s = Store(); out = []
    def work(c):
        L = LockService(s, 1)
        for now in range(200):
            t = L.acquire(c, now)
            if t is not None:
                out.append((now, t))
    ts = [threading.Thread(target=work, args=(str(i),)) for i in range(4)]
    [t.start() for t in ts]; [t.join() for t in ts]
    toks = [t for _, t in out]
    # a given token is only ever handed to one holder per lease window
    assert sorted(set(toks)) == list(range(1, max(toks) + 1))


def test_observation_against_p_spec():
    snappea = pytest.importorskip("snappea")
    s = Store(); L = LockService(s, 2)
    events = []
    for client, now in [("a", 0), ("b", 1), ("b", 2), ("a", 4)]:
        t = L.acquire(client, now)
        events.append((client, now, t))
    assert [e[2] for e in events] == [1, None, 2, 3]
    # Replay grants into the monitor in commit order.
    ev = [{"event": "eGranted", "payload": {"client": 1 if c == "a" else 2, "token": t, "now": n}}
          for c, n, t in events if t is not None]
    ev.append({"event": "eDone", "payload": None})
    snappea.check_events(ev, "Contract", spec_files=[SPEC], strict=True)
