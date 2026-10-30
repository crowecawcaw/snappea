"""Tests for solution.py. Observation tests run the Fencing monitor (Fencing.p) over
events the real implementation emits; concurrency tests use independent Python oracles
(emission order across threads is not commit order, so no monitor there)."""
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import pytest
from snappea import SpecViolation, check_spec

from runtime import Store
import solution
from solution import LockService, Resource

SPEC = str(Path(__file__).with_name("Fencing.p"))
TTL = 5


def observe():
    return check_spec("Fencing", spec_files=[SPEC], strict=True, check_hot=True,
                      unknown_events="error")


def test_acquire_expiry_release_and_tokens_observed():
    with observe() as rec:
        lk = LockService(Store(), TTL)
        t1 = lk.acquire("a", 10)
        assert t1 == 1
        assert lk.acquire("a", 12) == t1            # holder gets same token
        assert lk.acquire("b", 14) is None          # still unexpired at expiry-1
        assert lk.acquire("b", 15) == 2             # expired exactly at now+ttl
        assert lk.acquire("a", 16) is None          # a's old lease gone; b holds
        assert lk.release("a", t1, 16) is False     # stale holder
        assert lk.acquire("b", 16) == 2
        assert lk.release("b", 1, 16) is False      # wrong token
        assert lk.release("c", 2, 16) is False      # wrong client
        assert lk.acquire("b", 16) == 2             # failed releases had no effect
        assert lk.release("b", 2, 19) is True       # unexpired at expiry-1
        assert lk.release("b", 2, 19) is False      # already released
        assert lk.acquire("c", 20) == 3             # free after release; token increases
        assert lk.release("c", 3, 25) is False      # expired at exactly now+ttl
        assert lk.acquire("c", 25) == 4             # expired lease: new token, even same client
    assert rec.count("eLockOp") >= 12


def test_out_of_order_times():
    with observe():
        lk = LockService(Store(), TTL)
        assert lk.acquire("a", 100) == 1
        assert lk.acquire("b", 50) is None          # earlier clock still sees lease
        assert lk.release("b", 1, 50) is False
        assert lk.release("a", 1, 99) is True       # earlier now than grant: unexpired
        assert lk.acquire("b", 40) == 2
        assert lk.acquire("a", 100) == 3            # b's lease (expiry 45) is expired at 100


def test_out_of_order_times_expected_values():
    lk = LockService(Store(), TTL)
    assert lk.acquire("b", 40) == 1
    assert lk.acquire("a", 100) == 2     # now 100 >= 45: expired from a's viewpoint
    assert lk.acquire("c", 41) is None   # a's lease expires at 105


def test_write_fencing_observed():
    with observe() as rec:
        store = Store()
        lk = LockService(store, TTL)
        res = Resource(store, "r")
        assert res.write(1, "never issued") is False        # no token issued yet
        assert res.read() is None
        t1 = lk.acquire("a", 0)
        assert res.write(0, "zero") is False
        assert res.write(-1, "neg") is False
        assert res.write(t1 + 1, "future") is False         # not yet issued
        assert res.read() is None
        assert res.write(t1, "v1") is True
        assert res.write(t1, "v1b") is True                 # equal token is allowed
        t2 = lk.acquire("b", 5)
        assert t2 == 2
        assert res.write(t2, "v2") is True
        assert res.write(t1, "stale") is False              # stale vs newer live state
        assert res.read() == "v2"
        assert res.write(99, "bogus") is False
        assert res.read() == "v2"
        assert res.write(t2, "v2b") is True                 # still ok after rejects
        assert res.read() == "v2b"
    assert rec.count("eResOp") >= 10


def test_resources_are_independent_and_token_survives_release():
    store = Store()
    lk = LockService(store, TTL)
    r1, r2 = Resource(store, "r1"), Resource(store, "r2")
    t1 = lk.acquire("a", 0)
    assert lk.release("a", t1, 1)
    t2 = lk.acquire("a", 1)
    assert t2 > t1
    assert r1.write(t2, "x")
    assert r2.write(t1, "y")                 # r2 never saw a higher token
    assert not r1.write(t1, "z")
    assert (r1.read(), r2.read()) == ("x", "y")


def test_separate_service_objects_share_state():
    store = Store()
    a, b = LockService(store, TTL), LockService(store, TTL)
    t = a.acquire("x", 0)
    assert b.acquire("y", 1) is None
    assert b.acquire("x", 1) == t
    assert b.release("x", t, 2)
    assert a.acquire("y", 2) == t + 1


class _StrictlyLaterExpiry(LockService):
    """Defective: treats a lease as live through now == expiry."""
    def _held(self, rec, now):
        return rec["holder"] is not None and now <= rec["expiry"]


class _ReusesTokens(LockService):
    """Defective: forgets `last` on release so tokens restart."""
    def release(self, client, token, now):
        ok = super().release(client, token, now)
        if ok:
            self.store.compare_swap(solution.LOCK_KEY, *self._bump())
        return ok

    def _bump(self):
        v, _ = self.store.load(solution.LOCK_KEY)
        return v, {"last": 0, "holder": None, "token": 0, "expiry": 0}


def test_monitor_rejects_off_by_one_expiry():
    with pytest.raises(SpecViolation):
        with observe():
            lk = _StrictlyLaterExpiry(Store(), TTL)
            lk.acquire("a", 0)
            lk.acquire("b", TTL)   # must be granted at exactly now+ttl


def test_monitor_rejects_token_reuse():
    with pytest.raises(SpecViolation):
        with observe():
            lk = _ReusesTokens(Store(), TTL)
            t = lk.acquire("a", 0)
            lk.release("a", t, 1)
            lk.acquire("b", 1)


def test_emit_failures_never_break_application(monkeypatch):
    def boom(*a, **k):
        raise RuntimeError("emit broke")
    monkeypatch.setattr(solution, "_emit", boom)
    store = Store()
    lk = LockService(store, TTL)
    t = lk.acquire("a", 0)
    assert t == 1 and Resource(store, "r").write(t, "v")


# ---- concurrency (independent oracles) ----

def test_concurrent_acquire_single_winner_per_lease():
    for _ in range(20):
        store = Store()
        n = 8
        barrier = threading.Barrier(n)

        def go(i):
            lk = LockService(store, 100)
            barrier.wait(timeout=5)
            return lk.acquire(f"c{i}", 0)

        with ThreadPoolExecutor(n) as ex:
            results = list(ex.map(go, range(n)))
        winners = [r for r in results if r is not None]
        assert winners == [1]


def test_concurrent_lease_cycles_unique_increasing_tokens():
    store = Store()
    granted = []
    lock = threading.Lock()
    clock = iter(range(1, 10_000))

    def worker(i):
        lk = LockService(store, 1)
        for _ in range(50):
            now = next(clock)
            t = lk.acquire(f"c{i}", now)
            if t is not None:
                with lock:
                    granted.append(t)
                lk.release(f"c{i}", t, now)

    with ThreadPoolExecutor(6) as ex:
        list(ex.map(worker, range(6)))
    # Every grant of a fresh lease has a distinct token (reacquire impossible here
    # because each release/expiry precedes the next acquire by the same client).
    assert len(granted) == len(set(granted))
    assert max(granted) == len(granted) or max(granted) >= len(granted)
    lk = LockService(store, 1)
    assert lk.acquire("z", 10**6) > max(granted)


def test_concurrent_writes_keep_highest_token_and_its_value():
    for _ in range(20):
        store = Store()
        lk = LockService(store, 1)
        tokens = []
        for i in range(6):
            tokens.append(lk.acquire(f"c{i}", i * 10))   # each expires before next
        res = Resource(store, "r")
        barrier = threading.Barrier(len(tokens))
        results = {}

        def go(t):
            barrier.wait(timeout=5)
            results[t] = res.write(t, f"v{t}")

        with ThreadPoolExecutor(len(tokens)) as ex:
            list(ex.map(go, tokens))
        assert results[max(tokens)] is True
        assert res.read() == f"v{max(tokens)}"
        # accepted tokens, in any order, must all be <= winner and the final
        # write rejects any lower token
        assert res.write(min(tokens), "late") is False
        assert res.read() == f"v{max(tokens)}"
