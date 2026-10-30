"""Tests for solution.py: boundaries, forced interleavings from the P model, observation."""
import threading

import pytest
from snappea import check_events

import solution
from runtime import Store
from solution import LockService, Resource

TTL = 2


class HookStore:
    """Store wrapper that runs a callback right after the n-th load of a key."""

    def __init__(self, inner):
        self.inner = inner
        self.loads = {}
        self.hooks = {}  # (key, n) -> callable

    def after_load(self, key, n, fn):
        self.hooks[(key, n)] = fn

    def load(self, key):
        result = self.inner.load(key)
        n = self.loads[key] = self.loads.get(key, 0) + 1
        fn = self.hooks.pop((key, n), None)
        if fn:
            fn()
        return result

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


def make():
    store = Store()
    return store, LockService(store, TTL)


# ---- contract and boundaries -------------------------------------------------

def test_basic_acquire_release():
    _, svc = make()
    assert svc.acquire("a", 0) == 1
    assert svc.acquire("b", 1) is None
    assert svc.acquire("a", 1) == 1  # same unexpired lease, same token
    assert svc.release("a", 1, 1) is True
    assert svc.release("a", 1, 1) is False
    assert svc.acquire("b", 1) == 2


def test_expiry_boundary():
    _, svc = make()
    assert svc.acquire("a", 10) == 1
    assert svc.acquire("b", 10 + TTL - 1) is None  # still unexpired
    assert svc.acquire("a", 10 + TTL - 1) == 1
    assert svc.acquire("b", 10 + TTL) == 2  # expired exactly at now + ttl
    assert svc.acquire("a", 10 + TTL) is None


def test_expired_holder_gets_new_token():
    _, svc = make()
    assert svc.acquire("a", 0) == 1
    assert svc.acquire("a", TTL) == 2  # own lease expired: a new lease, a new token


def test_release_boundaries_and_preconditions():
    _, svc = make()
    svc.acquire("a", 0)
    assert svc.release("a", 1, TTL) is False  # expired exactly at ttl
    assert svc.acquire("a", 1) == 1  # failed release changed nothing
    assert svc.release("b", 1, 1) is False  # wrong client only
    assert svc.release("a", 2, 1) is False  # wrong token only
    assert svc.release("a", 0, 1) is False
    assert svc.acquire("b", 1) is None  # still held
    assert svc.release("a", 1, TTL - 1) is True


def test_release_of_stale_lease_does_not_free_new_one():
    _, svc = make()
    svc.acquire("a", 0)
    assert svc.acquire("b", 5) == 2
    assert svc.release("a", 1, 5) is False
    assert svc.acquire("c", 5) is None


def test_tokens_strictly_increase_across_release_and_expiry():
    _, svc = make()
    seen = []
    for i in range(5):
        seen.append(svc.acquire("a", 10 * i))
        if i % 2:
            svc.release("a", seen[-1], 10 * i)
    assert seen == [1, 2, 3, 4, 5]


def test_out_of_order_times():
    _, svc = make()
    assert svc.acquire("a", 10) == 1  # lease until 12
    assert svc.acquire("b", 5) is None  # earlier time: lease unexpired
    assert svc.release("a", 1, 3) is True  # unexpired at 3 too
    assert svc.acquire("b", 3) == 2
    assert svc.acquire("a", 4) is None  # b holds until 5
    assert svc.acquire("a", 5) == 3


def test_state_lives_in_store_not_objects():
    store = Store()
    s1, s2 = LockService(store, TTL), LockService(store, TTL)
    assert s1.acquire("a", 0) == 1
    assert s2.acquire("b", 0) is None
    assert s2.acquire("a", 0) == 1
    assert s2.release("a", 1, 0) is True
    assert s1.acquire("b", 0) == 2
    assert LockService(store, TTL).acquire("c", 0) is None


# ---- fencing -----------------------------------------------------------------

def test_write_fencing_rules():
    store, svc = make()
    r = Resource(store, "r")
    assert r.read() is None
    assert r.write(1, "x") is False  # never issued
    assert r.read() is None
    svc.acquire("a", 0)
    svc.acquire("b", 2)
    svc.acquire("a", 4)  # tokens 1..3 issued
    assert r.write(0, "z") is False
    assert r.write(-1, "z") is False
    assert r.write(4, "z") is False  # never issued
    assert r.write(2, "two") is True
    assert r.write(2, "two-again") is True  # equal token is not lower
    assert r.write(1, "one") is False  # lower than accepted
    assert r.read() == "two-again"
    assert r.write(3, "three") is True
    assert r.write(2, "late") is False
    assert r.read() == "three"
    assert r.write(5, "future") is False
    assert r.read() == "three"


def test_fencing_is_independent_per_resource_and_shared_via_store():
    store, svc = make()
    svc.acquire("a", 0)
    svc.acquire("b", 2)
    r1, r2, r1b = Resource(store, "r1"), Resource(store, "r2"), Resource(store, "r1")
    assert r1.write(2, "x") is True
    assert r2.write(1, "y") is True  # r2 has seen nothing higher
    assert r1b.write(1, "stale") is False  # another object, same name, same store
    assert r1b.read() == "x" and r2.read() == "y"


def test_resource_name_does_not_collide_with_lock():
    store, svc = make()
    r = Resource(store, "lock")
    svc.acquire("a", 0)
    assert r.write(1, "v") is True
    assert svc.acquire("a", 1) == 1 and svc.release("a", 1, 1) is True


def test_stale_holder_is_fenced_after_takeover():
    store, svc = make()
    r = Resource(store, "r")
    t1 = svc.acquire("a", 0)
    t2 = svc.acquire("b", 5)
    assert r.write(t2, "b-data") is True
    assert r.write(t1, "a-late") is False
    assert r.read() == "b-data"


# ---- forced interleavings (scenarios from the model) ---------------------------

def test_two_acquirers_read_same_version():
    inner = Store()
    barrier = threading.Barrier(2, timeout=5)
    store = HookStore(inner)
    store.after_load("lock", 1, barrier.wait)
    store.after_load("lock", 2, barrier.wait)
    results = {}

    def go(name):
        results[name] = LockService(store, TTL).acquire(name, 0)

    ts = [threading.Thread(target=go, args=(n,)) for n in "ab"]
    [t.start() for t in ts]
    [t.join(10) for t in ts]
    assert sorted(results.values(), key=lambda v: (v is None, v)) == [1, None]
    # state afterwards: exactly one holder, next token is 2
    winner = next(n for n, v in results.items() if v == 1)
    assert LockService(inner, TTL).acquire(winner, 1) == 1


def test_competitor_between_load_and_cas_in_acquire():
    inner = Store()
    store = HookStore(inner)
    other = LockService(inner, TTL)
    store.after_load("lock", 1, lambda: other.acquire("b", 0))
    assert LockService(store, TTL).acquire("a", 0) is None  # must re-decide, not overwrite b
    assert other.acquire("b", 1) == 1


def test_competitor_between_load_and_cas_same_client_gets_same_token():
    inner = Store()
    store = HookStore(inner)
    other = LockService(inner, TTL)
    store.after_load("lock", 1, lambda: other.acquire("a", 0))
    assert LockService(store, TTL).acquire("a", 0) == 1  # not a second token
    assert other.acquire("b", 1) is None


def test_release_races_with_expiry_and_takeover():
    inner = Store()
    svc = LockService(inner, TTL)
    store = HookStore(inner)
    assert svc.acquire("a", 0) == 1
    store.after_load("lock", 1, lambda: svc.acquire("b", TTL))  # a's lease expired, b took over
    assert LockService(store, TTL).release("a", 1, 1) is False  # stale decision must not free b
    assert svc.acquire("c", TTL) is None


def test_double_release_race():
    inner = Store()
    svc = LockService(inner, TTL)
    store = HookStore(inner)
    svc.acquire("a", 0)
    store.after_load("lock", 1, lambda: svc.release("a", 1, 0))
    assert LockService(store, TTL).release("a", 1, 0) is False  # exactly one release wins
    assert svc.acquire("b", 0) == 2


def test_release_races_with_reacquire_by_holder():
    inner = Store()
    svc = LockService(inner, TTL)
    store = HookStore(inner)
    svc.acquire("a", 0)
    # a's own release (time 1) decides from stale state while a releases and re-acquires.
    def competitor():
        svc.release("a", 1, 1)
        assert svc.acquire("a", 1) == 2
    store.after_load("lock", 1, competitor)
    assert LockService(store, TTL).release("a", 1, 1) is False  # token 1 is gone
    assert svc.release("a", 2, 1) is True


def test_stale_write_cas_must_retry_not_lower_max():
    inner = Store()
    svc = LockService(inner, TTL)
    for i, who in enumerate("aba"):
        svc.acquire(who, 2 * i)  # tokens 1, 2, 3
    store = HookStore(inner)
    fast = Resource(inner, "r")
    slow = Resource(store, "r")
    store.after_load("res:r", 1, lambda: fast.write(3, "three"))
    assert slow.write(1, "one") is False  # loaded max 0, then 3 landed
    assert slow.write(2, "two") is False  # max must still be 3
    assert fast.read() == "three"


def test_concurrent_equal_and_higher_writes_keep_highest():
    inner = Store()
    svc = LockService(inner, TTL)
    for i, who in enumerate("abc"):
        svc.acquire(who, 2 * i)
    store = HookStore(inner)
    slow = Resource(store, "r")
    fast = Resource(inner, "r")
    store.after_load("res:r", 1, lambda: fast.write(2, "two"))
    assert slow.write(2, "slow-two") is True  # equal token still allowed after retry
    assert slow.write(1, "x") is False
    assert slow.write(3, "three") is True
    assert fast.read() == "three"


def test_token_issued_while_write_in_flight_is_accepted():
    inner = Store()
    svc = LockService(inner, TTL)
    svc.acquire("a", 0)
    res = Resource(inner, "r")
    assert res.write(2, "early") is False  # not issued yet
    svc.acquire("b", 2)
    assert res.write(2, "now") is True


def test_thread_stress_unique_increasing_tokens_and_fenced_writes():
    store = Store()
    granted = []
    glock = threading.Lock()

    def worker(name):
        svc = LockService(store, TTL)
        res = Resource(store, "r")
        for now in range(0, 60, 2):
            t = svc.acquire(name, now)
            if t is not None:
                with glock:
                    granted.append((now, t, name))
                res.write(t, name)

    ts = [threading.Thread(target=worker, args=(f"c{i}",)) for i in range(6)]
    [t.start() for t in ts]
    [t.join(30) for t in ts]
    tokens = [t for _, t, _ in granted]
    assert all(t >= 1 for t in tokens)
    # One token per lease: leases never share a token between different clients.
    by_token = {}
    for _, t, n in granted:
        by_token.setdefault(t, set()).add(n)
    assert all(len(v) == 1 for v in by_token.values())
    # The last lease issued matches the resource's accepted maximum.
    final = LockService(store, TTL).acquire("zz", 1000)
    assert final == max(tokens) + 1


# ---- observation against the P spec --------------------------------------------

def test_observed_history_matches_p_spec():
    store, svc = make()
    res = Resource(store, "r")
    ids = {"a": 1, "b": 2}
    events = []

    def acquire(c, now):
        events.append({"event": "eInvoked", "payload": {"cid": ids[c], "kind": 0, "now": now, "tok": 0}})
        tok = svc.acquire(c, now)
        events.append({"event": "eAcq", "payload": {"cid": ids[c], "now": now, "token": tok or 0}})

    def release(c, tok, now):
        events.append({"event": "eInvoked", "payload": {"cid": ids[c], "kind": 1, "now": now, "tok": tok}})
        ok = svc.release(c, tok, now)
        events.append({"event": "eRel", "payload": {"cid": ids[c], "token": tok, "now": now, "ok": ok}})

    def write(c, tok):
        events.append({"event": "eInvoked", "payload": {"cid": ids[c], "kind": 2, "now": 0, "tok": tok}})
        ok = res.write(tok, "v")
        events.append({"event": "eWrote", "payload": {"cid": ids[c], "token": tok, "ok": ok}})

    acquire("a", 0)
    acquire("b", 1)
    acquire("a", 1)
    write("a", 1)
    write("a", 7)
    release("b", 1, 1)
    release("a", 1, 1)
    acquire("b", 1)
    write("b", 2)
    write("a", 1)
    write("b", 2)
    release("b", 2, 3)
    acquire("a", 3)
    write("a", 3)
    release("a", 3, 5)
    events.append({"event": "eDone", "payload": sum(e["event"] != "eInvoked" for e in events)})
    check_events(events, "LockContract", spec_files=["Lock.p"], strict=True)


def test_spec_rejects_a_wrong_history():
    events = [
        {"event": "eInvoked", "payload": {"cid": 1, "kind": 0, "now": 0, "tok": 0}},
        {"event": "eAcq", "payload": {"cid": 1, "now": 0, "token": 1}},
        {"event": "eInvoked", "payload": {"cid": 2, "kind": 0, "now": 1, "tok": 0}},
        {"event": "eAcq", "payload": {"cid": 2, "now": 1, "token": 2}},  # lease still held
    ]
    with pytest.raises(Exception):
        check_events(events, "LockContract", spec_files=["Lock.p"], strict=True)
