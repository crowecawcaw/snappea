"""Tests for the real LockService/Resource: scenarios from DESIGN.md, forced
interleavings, and an observation test checked against Observe.p."""
import threading
from concurrent.futures import ThreadPoolExecutor

from runtime import Store
from solution import LockService, Resource

TTL = 5


class Hook:
    """Store wrapper: runs `before_swap` once, right before the first compare_swap
    (i.e. between the caller's load and its swap)."""

    def __init__(self, inner, before_swap=None):
        self.inner, self.before_swap = inner, before_swap

    def load(self, key):
        return self.inner.load(key)

    def compare_swap(self, key, version, value):
        if self.before_swap:
            run, self.before_swap = self.before_swap, None
            run()
        return self.inner.compare_swap(key, version, value)


class SameVersion:
    """First n loads of `key` wait for each other so all callers see one version."""

    def __init__(self, inner, key, n=2):
        self.inner, self.key, self.n = inner, key, n
        self.barrier = threading.Barrier(n, timeout=3)
        self.lock, self.left = threading.Lock(), n

    def load(self, key):
        r = self.inner.load(key)
        if key == self.key:
            with self.lock:
                wait, self.left = self.left > 0, self.left - 1
            if wait:
                self.barrier.wait()
        return r

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


def svc(store=None):
    return LockService(store or Store(), TTL)


# --- acquire / release basics and boundaries ---

def test_first_grant_and_exclusion():
    s = svc()
    t = s.acquire("a", 0)
    assert isinstance(t, int) and t >= 1
    assert s.acquire("b", 1) is None


def test_reacquire_returns_same_token_without_extending():
    s = svc()
    t = s.acquire("a", 0)
    assert s.acquire("a", 3) == t
    assert s.acquire("b", TTL - 1) is None      # not extended by the re-acquire
    assert s.acquire("b", TTL) is not None


def test_expiry_boundary_acquire():
    s = svc()
    t = s.acquire("a", 10)
    assert s.acquire("b", 10 + TTL - 1) is None
    t2 = s.acquire("b", 10 + TTL)
    assert t2 is not None and t2 > t


def test_expired_same_client_gets_new_token():
    s = svc()
    t = s.acquire("a", 0)
    t2 = s.acquire("a", TTL)
    assert t2 > t


def test_release_boundaries_and_ownership():
    s = svc()
    t = s.acquire("a", 0)
    assert not s.release("b", t, 1)
    assert not s.release("a", t + 1, 1)
    assert not s.release("a", t, TTL)           # expired exactly at now+ttl
    assert s.acquire("b", TTL - 1) is None      # failed releases had no effect
    assert s.release("a", t, TTL - 1)
    assert not s.release("a", t, TTL - 1)       # already released
    assert s.acquire("b", 1) is not None


def test_tokens_strictly_increase_across_release_and_expiry():
    s = svc()
    seen = []
    now = 0
    for i in range(6):
        c = "ab"[i % 2]
        t = s.acquire(c, now)
        seen.append(t)
        if i % 2:
            assert s.release(c, t, now)
        now += TTL
    assert seen == sorted(set(seen))


def test_out_of_order_times():
    s = svc()
    t = s.acquire("a", 10)
    assert s.acquire("b", 2) is None            # earlier now: lease still unexpired
    assert s.release("a", t, 3)                 # earlier now than the grant is unexpired
    t2 = s.acquire("b", 1)
    assert t2 > t


def test_release_then_other_client_gets_larger_token():
    s = svc()
    t = s.acquire("a", 0)
    assert s.release("a", t, 1)
    assert s.acquire("b", 1) > t


def test_two_service_objects_share_state():
    store = Store()
    a, b = svc(store), svc(store)
    t = a.acquire("x", 0)
    assert b.acquire("y", 1) is None
    assert b.acquire("x", 1) == t
    assert b.release("x", t, 2)
    assert a.acquire("y", 2) > t


# --- forced interleavings ---

def test_same_version_acquire_only_one_wins():
    store = SameVersion(Store(), "lock")
    a, b = svc(store), svc(store)
    with ThreadPoolExecutor(2) as pool:
        fa, fb = pool.submit(a.acquire, "a", 0), pool.submit(b.acquire, "b", 0)
        ra, rb = fa.result(5), fb.result(5)
    assert sorted([ra is None, rb is None]) == [False, True]


def test_competitor_wins_between_load_and_swap():
    inner = Store()
    other = svc(inner)
    s = svc(Hook(inner, lambda: other.acquire("rival", 0)))
    assert s.acquire("me", 0) is None           # must reload, see the rival's lease
    assert other.acquire("rival", 1) is not None


def test_competitor_expires_between_load_and_swap_gets_fresh_token():
    inner = Store()
    first = svc(inner).acquire("a", 0)
    # b loads the expired lease, then c takes it; b must retry and fail.
    c_tok = []
    s = svc(Hook(inner, lambda: c_tok.append(svc(inner).acquire("c", TTL))))
    assert s.acquire("b", TTL) is None
    assert c_tok[0] > first


def test_release_races_with_competing_release_and_acquire():
    inner = Store()
    t = svc(inner).acquire("a", 0)
    # between a's load and swap, a's lease is released and b takes it.
    def interfere():
        svc(inner).release("a", t, 1)
        svc(inner).acquire("b", 1)
    s = svc(Hook(inner, interfere))
    assert s.release("a", t, 2) is False
    assert svc(inner).acquire("c", 2) is None   # b still holds


def test_concurrent_acquire_stress_tokens_unique_and_one_holder():
    store = Store()
    results = []
    def work(i):
        results.append(svc(store).acquire(f"c{i}", 0))
    threads = [threading.Thread(target=work, args=(i,)) for i in range(16)]
    [t.start() for t in threads]
    [t.join() for t in threads]
    assert len([r for r in results if r is not None]) == 1


# --- Resource fencing ---

def issue(store, n):
    """Issue tokens 1..n through the real service; returns them."""
    s = svc(store)
    out = []
    for i in range(n):
        t = s.acquire("c", i * TTL)
        out.append(t)
    return out


def test_never_issued_tokens_rejected():
    store = Store()
    r = Resource(store, "r")
    assert not r.write(1, "x")                   # nothing issued yet
    t = issue(store, 1)[0]
    for bad in (0, -1, t + 1, t + 100):
        assert not r.write(bad, "bad")
    assert r.read() is None
    assert r.write(t, "ok") and r.read() == "ok"


def test_lower_token_rejected_equal_and_higher_accepted():
    store = Store()
    t1, t2, t3 = issue(store, 3)
    r = Resource(store, "r")
    assert r.write(t2, "two")
    assert not r.write(t1, "one")
    assert r.read() == "two"
    assert r.write(t2, "two-again")               # equal token is accepted
    assert r.write(t3, "three")
    assert not r.write(t2, "stale")
    assert r.read() == "three"


def test_rejected_write_has_no_effect_on_max():
    store = Store()
    t1, t2 = issue(store, 2)
    r = Resource(store, "r")
    assert r.write(t1, "a")
    assert not r.write(t2 + 5, "never issued")    # must not raise max
    assert r.write(t2, "b")


def test_resources_are_independent_and_share_through_store():
    store = Store()
    t1, t2 = issue(store, 2)
    assert Resource(store, "a").write(t2, "A")
    assert Resource(store, "b").write(t1, "B")
    assert Resource(store, "a").read() == "A"     # second object, same store
    assert Resource(store, "b").read() == "B"
    assert Resource(store, "never").read() is None


def test_resource_name_does_not_collide_with_lock():
    store = Store()
    t = issue(store, 1)[0]
    r = Resource(store, "lock")
    assert r.write(t, "v") and r.read() == "v"
    assert svc(store).acquire("n", 10 * TTL) > t


def test_competing_higher_write_between_load_and_swap():
    inner = Store()
    t1, t2 = issue(inner, 2)
    other = Resource(inner, "r")
    low = Resource(Hook(inner, lambda: other.write(t2, "high")), "r")
    assert low.write(t1, "low") is False          # must reload and see max = t2
    assert other.read() == "high"


def test_same_version_writes_lower_cannot_overwrite_higher():
    base = Store()
    t1, t2 = issue(base, 2)
    store = SameVersion(base, "res:r")
    a, b = Resource(store, "r"), Resource(store, "r")
    with ThreadPoolExecutor(2) as pool:
        fl, fh = pool.submit(a.write, t1, "low"), pool.submit(b.write, t2, "high")
        rl, rh = fl.result(5), fh.result(5)
    assert rh is True
    assert Resource(base, "r").read() == "high"


def test_stress_final_value_is_highest_token():
    store = Store()
    toks = issue(store, 6)
    def w(t):
        Resource(store, "r").write(t, f"v{t}")
    threads = [threading.Thread(target=w, args=(t,)) for t in toks * 3]
    [t.start() for t in threads]
    [t.join() for t in threads]
    assert Resource(store, "r").read() == f"v{toks[-1]}"


# --- observation test: real calls checked against Observe.p ---

class Recorder:
    def __init__(self, store, ttl=TTL):
        self.svc, self.store, self.events, self.ids = LockService(store, ttl), store, [], {}

    def cid(self, c):
        return self.ids.setdefault(c, len(self.ids))

    def acquire(self, c, now):
        r = self.svc.acquire(c, now)
        self.events.append({"event": "eObsAcquire", "payload": {"cid": self.cid(c), "now": now, "result": -1 if r is None else r}})
        return r

    def release(self, c, token, now):
        r = self.svc.release(c, token, now)
        self.events.append({"event": "eObsRelease", "payload": {"cid": self.cid(c), "token": token, "now": now, "result": r}})
        return r

    def write(self, name, token, value):
        r = Resource(self.store, name).write(token, value)
        self.events.append({"event": "eObsWrite", "payload": {"res": self.cid("res:" + name), "token": token, "ok": r}})
        return r


def test_observed_history_matches_spec():
    from snappea import check_events
    # ttl 2 matches Observe.p
    rec = Recorder(Store(), ttl=2)
    t1 = rec.acquire("a", 0)
    assert rec.acquire("b", 1) is None
    assert rec.acquire("a", 1) == t1
    assert rec.write("r", t1, "x")
    assert not rec.write("r", t1 + 1, "never issued")
    assert not rec.release("b", t1, 1)
    assert not rec.release("a", t1 + 1, 1)
    assert not rec.release("a", t1, 2)            # boundary: expired
    t2 = rec.acquire("b", 2)
    assert t2 > t1
    assert rec.write("r", t2, "y")
    assert not rec.write("r", t1, "stale")
    assert rec.write("s", t1, "other resource")
    assert rec.release("b", t2, 3)
    assert not rec.release("b", t2, 3)
    t3 = rec.acquire("a", 0)                      # out-of-order time
    assert t3 > t2
    assert rec.acquire("b", 1) is None
    check_events(rec.events, "ObservedContract", spec_files=["Observe.p"], strict=True)
