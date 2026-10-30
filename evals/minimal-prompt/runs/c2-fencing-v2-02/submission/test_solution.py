import threading

import pytest
from snappea import check_events

from runtime import Store
from solution import LockService, Resource

TTL = 2


class Hook:
    """Real store; runs `before_cas` / `after_load` hooks for one key, once each."""

    def __init__(self, inner, key, after_load=None, before_cas=None):
        self.inner, self.key = inner, key
        self.after_load, self.before_cas = after_load, before_cas

    def load(self, key):
        r = self.inner.load(key)
        if key == self.key and self.after_load:
            f, self.after_load = self.after_load, None
            f()
        return r

    def compare_swap(self, key, version, value):
        if key == self.key and self.before_cas:
            f, self.before_cas = self.before_cas, None
            f()
        return self.inner.compare_swap(key, version, value)


class SameVersion:
    """First two loads of `key` wait for each other: both read one version."""

    def __init__(self, inner, key):
        self.inner, self.key = inner, key
        self.barrier, self.n, self.lk = threading.Barrier(2, timeout=3), 0, threading.Lock()

    def load(self, key):
        r = self.inner.load(key)
        if key == self.key:
            with self.lk:
                self.n += 1
                wait = self.n <= 2
            if wait:
                self.barrier.wait()
        return r

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


def svc(store=None):
    return LockService(store or Store(), TTL)


# --- lease basics and boundaries ---
def test_acquire_grants_and_returns_same_token_to_holder():
    s = svc()
    assert s.acquire("a", 0) == 1
    assert s.acquire("a", 1) == 1
    assert s.acquire("b", 1) is None


def test_expiry_boundary():
    s = svc()
    assert s.acquire("a", 10) == 1
    assert s.acquire("b", 11) is None          # now = exp - 1: still held
    assert s.acquire("b", 12) == 2             # now == exp: expired
    assert s.acquire("a", 12) is None


def test_holder_at_expiry_gets_new_token_not_old():
    s = svc()
    assert s.acquire("a", 0) == 1
    assert s.acquire("a", 2) == 2


def test_tokens_increase_across_release_and_expiry():
    s = svc()
    assert s.acquire("a", 0) == 1
    assert s.release("a", 1, 0)
    assert s.acquire("b", 0) == 2
    assert s.release("b", 2, 1)
    assert s.acquire("a", 1) == 3


def test_release_preconditions_each_false_alone():
    s = svc()
    assert s.acquire("a", 0) == 1
    assert not s.release("b", 1, 1)       # wrong client only
    assert not s.release("a", 2, 1)       # wrong token only
    assert not s.release("a", 1, 2)       # expired only (now == exp)
    assert s.release("a", 1, 1)           # now == exp - 1
    assert not s.release("a", 1, 1)       # already released
    assert s.acquire("b", 1) == 2


def test_release_never_acquired_and_stale_token_after_reacquire():
    s = svc()
    assert not s.release("a", 1, 0)
    assert s.acquire("a", 0) == 1
    assert s.acquire("a", 5) == 2
    assert not s.release("a", 1, 5)       # old token of same client
    assert s.release("a", 2, 5)


def test_out_of_order_times():
    s = svc()
    assert s.acquire("a", 10) == 1
    assert s.acquire("b", 3) is None      # an earlier clock still sees the live lease
    assert s.release("a", 1, 9)
    assert s.acquire("b", 3) == 2


def test_second_service_object_shares_store():
    store = Store()
    s1, s2 = svc(store), svc(store)
    assert s1.acquire("a", 0) == 1
    assert s2.acquire("b", 0) is None
    assert s2.release("a", 1, 1)
    assert s1.acquire("b", 1) == 2


# --- forced interleavings ---
def test_two_acquires_read_same_version():
    store = Store()
    gate = SameVersion(store, "lock")
    out = {}
    ts = [threading.Thread(target=lambda c=c: out.__setitem__(c, LockService(gate, TTL).acquire(c, 0)))
          for c in "ab"]
    [t.start() for t in ts]
    [t.join() for t in ts]
    assert sorted(out.values(), key=lambda v: (v is None, v)) == [1, None]
    winner = [c for c in out if out[c] == 1][0]
    assert svc(store).acquire(winner, 1) == 1       # exactly one holder


def test_competing_acquire_between_load_and_cas_forces_redecision():
    store = Store()
    svc(store).acquire("a", 0)
    # b sees an expired lease at now=5; a re-acquires before b's CAS.
    hooked = Hook(store, "lock", after_load=lambda: svc(store).acquire("a", 5))
    assert LockService(hooked, TTL).acquire("b", 5) is None
    assert svc(store).acquire("a", 5) == 2


def test_release_races_with_reacquire():
    store = Store()
    assert svc(store).acquire("a", 0) == 1
    # a's release loads, then the lease expires and b takes it; a's release must fail.
    hooked = Hook(store, "lock", after_load=lambda: svc(store).acquire("b", 2))
    assert not LockService(hooked, TTL).release("a", 1, 1)
    assert svc(store).acquire("b", 3) == 2


def test_release_cas_retry_sees_competing_change():
    store = Store()
    assert svc(store).acquire("a", 0) == 1
    # Two releases of the same lease race: exactly one wins.
    hooked = Hook(store, "lock", after_load=lambda: svc(store).release("a", 1, 1))
    assert not LockService(hooked, TTL).release("a", 1, 1)


def test_concurrent_acquire_threads_unique_tokens():
    store = Store()
    toks = []

    def go(i):
        t = LockService(store, TTL).acquire("c%d" % i, 0)
        if t is not None:
            toks.append(t)

    ts = [threading.Thread(target=go, args=(i,)) for i in range(8)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    assert toks == [1]


# --- resource fencing ---
def issue(s, n):
    for i in range(n):
        assert s.acquire("c", 10 * i) == i + 1


def test_write_unissued_tokens_rejected():
    store = Store()
    r = Resource(store, "r")
    assert not r.write(1, "x")            # nothing issued yet
    s = svc(store)
    issue(s, 2)
    for bad in (0, -1, 3):
        assert not r.write(bad, "x")
    assert r.read() is None
    assert r.write(2, "y") and r.read() == "y"


def test_write_fencing_order_and_equal_token():
    store = Store()
    s, r = svc(store), Resource(store, "r")
    issue(s, 3)
    assert r.write(2, "a")
    assert not r.write(1, "b")            # lower than accepted
    assert r.read() == "a"
    assert r.write(2, "c")                # equal token accepted
    assert r.write(3, "d")
    assert not r.write(2, "e")
    assert r.read() == "d"


def test_rejected_write_does_not_raise_high_mark():
    store = Store()
    s, r = svc(store), Resource(store, "r")
    issue(s, 3)
    assert not r.write(9, "x")            # unissued must not poison the mark
    assert r.write(1, "ok")


def test_resources_share_store_independently():
    store = Store()
    s = svc(store)
    issue(s, 2)
    r1, r2 = Resource(store, "one"), Resource(store, "two")
    assert r1.write(2, "a")
    assert r2.write(1, "b")               # separate high mark
    assert not r1.write(1, "c")
    assert r2.read() == "b" and r1.read() == "a"
    assert Resource(store, "one").read() == "a"      # second object, same store


def test_write_with_competing_higher_token_between_load_and_cas():
    store = Store()
    s, r = svc(store), Resource(store, "r")
    issue(s, 2)
    hooked = Hook(store, "resource:r", after_load=lambda: Resource(store, "r").write(2, "new"))
    assert not Resource(hooked, "r").write(1, "old")
    assert r.read() == "new"


def test_two_writes_read_same_version():
    store = Store()
    s = svc(store)
    issue(s, 2)
    gate = SameVersion(store, "resource:r")
    out = {}
    ts = [threading.Thread(target=lambda t=t: out.__setitem__(t, Resource(gate, "r").write(t, "v%d" % t)))
          for t in (1, 2)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    assert out[2] is True
    assert Resource(store, "r").read() == "v2"       # token 2 never overwritten by 1


def test_token_issued_during_write_is_fine():
    store = Store()
    s = svc(store)
    s.acquire("a", 0)
    r = Resource(Hook(store, "resource:r", after_load=lambda: svc(store).acquire("b", 9)), "r")
    assert r.write(1, "x")


# --- observation test against the P spec ---
class Rec:
    def __init__(self, store):
        self.s, self.ev = LockService(store, TTL), []
        self.store = store

    def acquire(self, c, now):
        t = self.s.acquire(c, now)
        self.ev.append({"event": "eAcquired", "payload": {"client": c, "now": now, "token": -1 if t is None else t}})
        return t

    def release(self, c, tok, now):
        ok = self.s.release(c, tok, now)
        self.ev.append({"event": "eReleased", "payload": {"client": c, "token": tok, "now": now, "ok": ok}})
        return ok

    def write(self, name, tok, v):
        ok = Resource(self.store, name).write(tok, v)
        self.ev.append({"event": "eWritten", "payload": {"res": name, "token": tok, "ok": ok}})
        return ok


def test_observed_history_matches_spec():
    r = Rec(Store())
    r.write("r", 1, "x")
    r.acquire("a", 0); r.acquire("a", 1); r.acquire("b", 1)
    r.release("b", 1, 1); r.release("a", 1, 2)
    r.acquire("b", 2); r.write("r", 2, "y"); r.write("r", 1, "z"); r.write("r", 3, "q")
    r.write("s", 2, "w"); r.write("r", 2, "again"); r.write("r", 0, "w")
    r.release("b", 2, 3); r.acquire("a", 3)
    check_events(r.ev, "ObservedContract", spec_files=["Observed.p"], strict=True)


def test_observation_spec_rejects_bad_history():
    ev = [{"event": "eAcquired", "payload": {"client": "a", "now": 0, "token": 1}},
          {"event": "eAcquired", "payload": {"client": "b", "now": 1, "token": 2}}]
    with pytest.raises(Exception):
        check_events(ev, "ObservedContract", spec_files=["Observed.p"], strict=True)
