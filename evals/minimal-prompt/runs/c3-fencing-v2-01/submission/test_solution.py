import threading
from concurrent.futures import ThreadPoolExecutor

import pytest
from snappea import check_events

from runtime import Store
from solution import LockService, Resource

TTL = 10


class SameVersionStore:
    """First two loads of `key` wait for each other: both callers decide from one version."""

    def __init__(self, inner, key):
        self.inner, self.key = inner, key
        self.barrier, self.waiting, self.lock = threading.Barrier(2, timeout=3), 2, threading.Lock()

    def load(self, key):
        result = self.inner.load(key)
        if key == self.key:
            with self.lock:
                wait, self.waiting = self.waiting > 0, self.waiting - 1
            if wait:
                self.barrier.wait()
        return result

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


class InterferingStore:
    """Runs `competitor` once, right after the first load of `key` (before this caller's CAS)."""

    def __init__(self, inner, key, competitor):
        self.inner, self.key, self.competitor = inner, key, competitor

    def load(self, key):
        result = self.inner.load(key)
        if key == self.key and self.competitor:
            run, self.competitor = self.competitor, None
            run()
        return result

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


def two(f, g):
    with ThreadPoolExecutor(2) as pool:
        a, b = pool.submit(f), pool.submit(g)
        return a.result(timeout=5), b.result(timeout=5)


# ---- basic contract and boundaries

def test_acquire_release_basics_and_increasing_tokens():
    s = Store()
    svc = LockService(s, TTL)
    assert svc.acquire("a", 0) == 1
    assert svc.acquire("a", 5) == 1          # same holder, unexpired: same token
    assert svc.acquire("b", 5) is None
    assert svc.release("a", 1, 5) is True
    assert svc.release("a", 1, 5) is False   # already released
    assert svc.acquire("b", 5) == 2
    assert svc.release("b", 2, 6) is True
    assert svc.acquire("a", 6) == 3          # tokens never reused after release


def test_expiry_boundary():
    svc = LockService(Store(), TTL)
    assert svc.acquire("a", 3) == 1          # expires at 13
    assert svc.acquire("b", 12) is None      # just before
    assert svc.acquire("b", 13) == 2         # exactly at expiry: expired
    assert svc.acquire("a", 13) is None


def test_reacquire_after_own_expiry_gets_new_token():
    svc = LockService(Store(), TTL)
    assert svc.acquire("a", 0) == 1
    assert svc.acquire("a", 10) == 2


def test_release_boundaries_and_preconditions():
    svc = LockService(Store(), TTL)
    t = svc.acquire("a", 0)
    assert svc.release("b", t, 1) is False       # wrong client only
    assert svc.release("a", t + 1, 1) is False   # wrong token only
    assert svc.release("a", t - 1, 1) is False
    assert svc.release("a", t, 10) is False      # exactly expired
    assert svc.release("a", t, 9) is True        # just before
    assert svc.acquire("b", 9) == 2


def test_release_does_not_free_a_newer_lease():
    svc = LockService(Store(), TTL)
    t1 = svc.acquire("a", 0)
    t2 = svc.acquire("b", 10)
    assert svc.release("a", t1, 11) is False
    assert svc.acquire("c", 11) is None and svc.acquire("b", 11) == t2


def test_out_of_order_times():
    svc = LockService(Store(), TTL)
    assert svc.acquire("a", 100) == 1
    assert svc.acquire("b", 50) is None          # earlier time: lease still unexpired
    assert svc.release("a", 1, 50) is True
    assert svc.acquire("b", 20) == 2             # expires 30
    assert svc.acquire("a", 29) is None
    assert svc.acquire("a", 30) == 3


def test_state_lives_in_store_not_object():
    s = Store()
    assert LockService(s, TTL).acquire("a", 0) == 1
    assert LockService(s, TTL).acquire("b", 1) is None
    assert LockService(s, TTL).acquire("a", 1) == 1
    assert LockService(s, TTL).release("a", 1, 2) is True
    assert LockService(s, TTL).acquire("b", 2) == 2
    Resource(s, "r").write(2, "x")
    assert Resource(s, "r").write(1, "y") is False
    assert Resource(s, "r").read() == "x"


# ---- fencing

def test_write_fencing_rules():
    s = Store()
    svc = LockService(s, TTL)
    r = Resource(s, "r")
    assert r.read() is None
    assert r.write(1, "never") is False         # nothing issued yet
    t1 = svc.acquire("a", 0)
    assert r.write(0, "z") is False and r.write(-1, "z") is False
    assert r.write(t1 + 1, "future") is False   # not issued yet
    assert r.write(t1, "one") and r.read() == "one"
    t2 = svc.acquire("b", 10)
    assert r.write(t2, "two") and r.read() == "two"
    assert r.write(t1, "stale") is False and r.read() == "two"   # lower than used
    assert r.write(t2, "two-again") is True and r.read() == "two-again"  # equal ok
    assert r.write(t2 + 1, "future") is False and r.read() == "two-again"


def test_issued_but_unused_lower_token_still_accepted_on_other_resource():
    s = Store()
    svc = LockService(s, TTL)
    t1 = svc.acquire("a", 0)
    t2 = svc.acquire("b", 10)
    assert Resource(s, "x").write(t2, "x2")
    assert Resource(s, "y").write(t1, "y1") is True     # per-resource fencing
    assert Resource(s, "x").write(t1, "x1") is False
    assert Resource(s, "x").read() == "x2" and Resource(s, "y").read() == "y1"


def test_released_token_never_issued_check_uses_counter():
    s = Store()
    svc = LockService(s, TTL)
    t = svc.acquire("a", 0)
    svc.release("a", t, 1)
    assert Resource(s, "r").write(t, "v") is True       # issued, even if released
    assert Resource(s, "r").write(t + 1, "v") is False


# ---- forced interleavings

def test_two_acquires_read_same_version_one_wins():
    inner = Store()
    st = SameVersionStore(inner, "lock")
    a, b = LockService(st, TTL), LockService(st, TTL)
    ra, rb = two(lambda: a.acquire("a", 0), lambda: b.acquire("b", 0))
    assert sorted(map(str, (ra, rb))) == ["1", "None"]
    assert LockService(inner, TTL).acquire("a", 1) == (1 if ra == 1 else None)


def test_same_client_double_acquire_same_version_same_token():
    st = SameVersionStore(Store(), "lock")
    a, b = LockService(st, TTL), LockService(st, TTL)
    assert two(lambda: a.acquire("a", 0), lambda: b.acquire("a", 0)) == (1, 1)


def test_competitor_acquires_between_load_and_cas():
    inner = Store()
    st = InterferingStore(inner, "lock", lambda: LockService(inner, TTL).acquire("b", 0))
    assert LockService(st, TTL).acquire("a", 0) is None       # must re-read and see b
    assert LockService(inner, TTL).acquire("b", 1) == 1


def test_competitor_acquires_after_expiry_between_load_and_cas():
    inner = Store()
    LockService(inner, TTL).acquire("old", 0)
    st = InterferingStore(inner, "lock", lambda: LockService(inner, TTL).acquire("b", 10))
    assert LockService(st, TTL).acquire("a", 10) is None
    assert LockService(inner, TTL).acquire("b", 11) == 2


def test_release_races_with_expiry_takeover():
    inner = Store()
    svc = LockService(inner, TTL)
    t = svc.acquire("a", 0)
    # a's release (at now=5) loads the lease; b can't take over unexpired lease, but a
    # competing release+reacquire by a changes the token under it.
    def competitor():
        assert svc.release("a", t, 5)
        assert svc.acquire("a", 5) == 2
    st = InterferingStore(inner, "lock", competitor)
    assert LockService(st, TTL).release("a", t, 6) is False   # token 1 is gone
    assert svc.acquire("a", 6) == 2


def test_concurrent_releases_only_one_succeeds():
    inner = Store()
    t = LockService(inner, TTL).acquire("a", 0)
    st = SameVersionStore(inner, "lock")
    a, b = LockService(st, TTL), LockService(st, TTL)
    assert sorted(two(lambda: a.release("a", t, 1), lambda: b.release("a", t, 1))) == [False, True]


def test_resource_lower_token_loses_cas_race_and_is_rejected():
    inner = Store()
    svc = LockService(inner, TTL)
    t1, t2 = svc.acquire("a", 0), None
    t2 = svc.acquire("b", 10)
    st = InterferingStore(inner, "res:r", lambda: Resource(inner, "r").write(t2, "new"))
    assert Resource(st, "r").write(t1, "old") is False        # retry must re-check
    assert Resource(inner, "r").read() == "new"


def test_resource_higher_token_after_competitor_with_lower_token():
    inner = Store()
    svc = LockService(inner, TTL)
    t1 = svc.acquire("a", 0)
    t2 = svc.acquire("b", 10)
    st = InterferingStore(inner, "res:r", lambda: Resource(inner, "r").write(t1, "old"))
    assert Resource(st, "r").write(t2, "new") is True
    assert Resource(inner, "r").read() == "new"


def test_resource_writes_same_version_higher_token_wins():
    inner = Store()
    svc = LockService(inner, TTL)
    t1 = svc.acquire("a", 0)
    t2 = svc.acquire("b", 10)
    st = SameVersionStore(inner, "res:r")
    r1, r2 = Resource(st, "r"), Resource(st, "r")
    res = two(lambda: r1.write(t1, "old"), lambda: r2.write(t2, "new"))
    assert res[1] is True
    assert Resource(inner, "r").read() == "new"


def test_token_checked_against_counter_not_stale_object():
    # token issued by another LockService object after the Resource was created
    s = Store()
    r = Resource(s, "r")
    assert r.write(1, "x") is False
    LockService(s, TTL).acquire("a", 0)
    assert r.write(1, "x") is True


# ---- thread stress (plain assertions; return order is not commit order)

def test_threads_tokens_unique_and_mutually_exclusive():
    s = Store()
    got = []
    def work(i):
        svc = LockService(s, TTL)
        for _ in range(200):
            t = svc.acquire(f"c{i}", 0)       # now fixed: lease never expires
            if t is not None:
                got.append((i, t))
    with ThreadPoolExecutor(8) as pool:
        list(pool.map(work, range(8)))
    assert len({t for _, t in got}) == 1       # one lease, one token; only holder re-acquires
    assert len({i for i, _ in got}) == 1


def test_threads_expiring_leases_tokens_unique():
    s = Store()
    tokens = []
    def work(i):
        svc = LockService(s, 1)
        for n in range(200):
            t = svc.acquire(f"c{i}", n)
            if t is not None:
                tokens.append(t)
    with ThreadPoolExecutor(6) as pool:
        list(pool.map(work, range(6)))
    r = Resource(s, "r")
    assert r.write(max(tokens), "last")


# ---- observation against Spec.p (SeqContract)

class Recorder:
    def __init__(self, store):
        self.store, self.events = store, []
        self.svc = LockService(store, 2)    # the model uses ttl 2
        self.res = Resource(store, "r")

    def acquire(self, c, now):
        t = self.svc.acquire(c, now)
        self.events.append({"event": "eAcquired", "payload": {"client": c, "now": now, "token": t or 0}})
        return t

    def release(self, c, token, now):
        ok = self.svc.release(c, token, now)
        self.events.append({"event": "eReleased", "payload": {"client": c, "token": token, "now": now, "ok": ok}})
        return ok

    def write(self, token):
        ok = self.res.write(token, "v")
        self.events.append({"event": "eWritten", "payload": {"token": token, "ok": ok}})
        return ok


def check(rec):
    check_events(rec.events, "SeqContract", spec_files=["Spec.p"], strict=True)


def test_observation_scripted_history():
    r = Recorder(Store())
    r.write(1)                      # never issued
    r.acquire(1, 0)                 # token 1, expires 2
    r.acquire(1, 1)                 # same token
    r.acquire(2, 1)                 # None
    r.write(1); r.write(2)
    r.acquire(2, 2)                 # exactly expired: token 2
    r.write(1)                      # stale
    r.write(2)
    r.release(1, 1, 2)              # not holder anymore
    r.release(2, 2, 3)              # ok, unexpired
    r.release(2, 2, 3)              # twice
    r.acquire(1, 0)                 # out-of-order time, lease released: token 3
    r.release(1, 3, 2)              # exactly expired
    r.write(3); r.write(4)
    check(r)


def test_observation_random_histories_match_spec():
    import random
    for seed in range(40):
        rnd = random.Random(seed)
        r = Recorder(Store())
        for _ in range(25):
            op = rnd.choice("arw")
            if op == "a":
                r.acquire(rnd.choice([1, 2]), rnd.randrange(6))
            elif op == "r":
                r.release(rnd.choice([1, 2]), rnd.randrange(5), rnd.randrange(6))
            else:
                r.write(rnd.randrange(5))
        check(r)


def test_observation_rejects_wrong_result():
    r = Recorder(Store())
    r.acquire(1, 0)
    r.events.append({"event": "eAcquired", "payload": {"client": 2, "now": 0, "token": 2}})
    with pytest.raises(Exception):
        check(r)
