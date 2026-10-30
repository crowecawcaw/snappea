import random
import threading
from pathlib import Path
from threading import Barrier

import pytest
from snappea import check_events

from runtime import Store
from solution import LockService, Resource

TTL = 3
OBS = str(Path(__file__).parent / "obs" / "Observe.p")


class HookStore:
    """Real Store plus hooks that run right after a matching load returns."""

    def __init__(self, inner=None):
        self.inner = inner or Store()
        self.after_load = []  # list of (key, callable); each fires once

    def load(self, key):
        result = self.inner.load(key)
        for hook in list(self.after_load):
            if hook[0] == key:
                self.after_load.remove(hook)
                hook[1]()
        return result

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


def fresh():
    s = Store()
    return s, LockService(s, TTL), Resource(s, "r")


# ---- lock: basics and boundaries ----

def test_acquire_free_and_held():
    _, svc, _ = fresh()
    t = svc.acquire("a", 0)
    assert t is not None
    assert svc.acquire("b", 1) is None
    assert svc.acquire("a", 1) == t  # same client, unexpired: same token
    assert svc.acquire("a", 2) == t  # re-acquire does not extend the lease
    assert svc.acquire("b", TTL - 1) is None


def test_expiry_boundary():
    _, svc, _ = fresh()
    t = svc.acquire("a", 10)
    assert svc.acquire("b", 10 + TTL - 1) is None   # one before expiry
    t2 = svc.acquire("b", 10 + TTL)                 # exactly at expiry
    assert t2 is not None and t2 > t


def test_reacquire_after_own_expiry_gets_new_token():
    _, svc, _ = fresh()
    t = svc.acquire("a", 0)
    assert svc.acquire("a", TTL) > t


def test_reacquire_does_not_extend_lease():
    _, svc, _ = fresh()
    svc.acquire("a", 0)
    svc.acquire("a", 2)
    assert svc.acquire("b", TTL) is not None


def test_release_conditions_one_at_a_time():
    _, svc, _ = fresh()
    t = svc.acquire("a", 0)
    assert not svc.release("b", t, 1)        # wrong client only
    assert not svc.release("a", t + 1, 1)    # wrong token only
    assert not svc.release("a", t - 1, 1)
    assert not svc.release("a", t, TTL)      # expired exactly at boundary only
    assert svc.acquire("b", 1) is None       # failed releases changed nothing
    assert svc.release("a", t, TTL - 1)      # last valid instant
    assert not svc.release("a", t, TTL - 1)  # already released


def test_release_frees_lock_and_tokens_keep_increasing():
    _, svc, _ = fresh()
    t1 = svc.acquire("a", 0)
    assert svc.release("a", t1, 1)
    t2 = svc.acquire("b", 1)
    assert t2 > t1
    assert svc.release("b", t2, 2)
    t3 = svc.acquire("a", 2)
    assert t3 > t2


def test_release_expired_lease_then_other_holder_unaffected():
    _, svc, _ = fresh()
    t1 = svc.acquire("a", 0)
    t2 = svc.acquire("b", TTL)
    assert not svc.release("a", t1, TTL)   # stale token of old holder
    assert svc.acquire("c", TTL + 1) is None
    assert svc.release("b", t2, TTL + 1)


def test_out_of_order_times():
    _, svc, _ = fresh()
    t = svc.acquire("a", 10)
    assert svc.acquire("b", 5) is None      # earlier time still inside the lease
    assert svc.release("a", t, 5)           # earlier time: unexpired
    assert svc.acquire("b", 4) is not None  # lock free regardless of time


def test_state_is_in_store_not_object():
    s = Store()
    t = LockService(s, TTL).acquire("a", 0)
    other = LockService(s, TTL)
    assert other.acquire("b", 1) is None
    assert other.acquire("a", 1) == t
    assert other.release("a", t, 1)
    assert LockService(s, TTL).acquire("b", 1) > t


# ---- resource ----

def test_resource_fencing_rules():
    s, svc, res = fresh()
    assert res.read() is None
    assert not res.write(1, "x")            # never issued
    assert res.read() is None
    t1 = svc.acquire("a", 0)
    assert not res.write(0, "x") and not res.write(-1, "x")
    assert not res.write(t1 + 1, "x")       # above highest issued
    assert res.write(t1, "one")
    assert res.write(t1, "one-again")       # equal token allowed
    svc.release("a", t1, 1)
    t2 = svc.acquire("b", 1)
    assert res.write(t2, "two")
    assert not res.write(t1, "stale")       # lower than used
    assert res.read() == "two"
    assert not res.write(t2 + 1, "future")  # not yet issued
    assert res.read() == "two"


def test_resources_are_independent_and_shared_via_store():
    s, svc, _ = fresh()
    t1 = svc.acquire("a", 0)
    svc.release("a", t1, 0)
    t2 = svc.acquire("b", 0)
    r1, r2 = Resource(s, "x"), Resource(s, "y")
    assert r1.write(t2, "hi")
    assert r2.write(t1, "lo")               # y never saw t2
    assert Resource(s, "x").read() == "hi"  # second object, same store
    assert not Resource(s, "x").write(t1, "no")
    assert r2.read() == "lo"


def test_unissued_token_with_used_zero_does_not_hide_lower_check():
    _, svc, res = fresh()
    t1 = svc.acquire("a", 0); svc.release("a", t1, 0)
    t2 = svc.acquire("b", 0)
    assert res.write(t2, "b")
    assert not res.write(t1, "a")  # issued, but lower: only this check fails


# ---- forced interleavings ----

def test_two_acquires_read_same_version():
    inner = Store()
    barrier = Barrier(2)

    class Gate(HookStore):
        def load(self, key):
            r = self.inner.load(key)
            if key == "lock" and not getattr(threading.current_thread(), "passed", False):
                threading.current_thread().passed = True
                barrier.wait(timeout=5)
            return r

    store = Gate(inner)
    results = {}

    def go(name):
        results[name] = LockService(store, TTL).acquire(name, 0)

    ts = [threading.Thread(target=go, args=(n,)) for n in "ab"]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert sorted(results.values(), key=lambda x: x is None) [1] is None
    assert sum(v is not None for v in results.values()) == 1
    assert LockService(inner, TTL).acquire("c", 1) is None


def test_competitor_between_load_and_cas_in_acquire():
    inner = Store()
    store = HookStore(inner)
    rival = LockService(inner, TTL)
    got = {}
    store.after_load.append(("lock", lambda: got.update(rival=rival.acquire("b", 0))))
    assert LockService(store, TTL).acquire("a", 0) is None  # re-reads, now sees b
    assert got["rival"] is not None


def test_competitor_release_between_load_and_cas_in_acquire_gives_larger_token():
    inner = Store()
    svc = LockService(inner, TTL)
    t1 = svc.acquire("a", 0)
    store = HookStore(inner)
    store.after_load.append(("lock", lambda: svc.release("a", t1, 1)))
    # b loaded a held lease -> returns None; rival release afterwards is not seen.
    assert LockService(store, TTL).acquire("b", 1) is None
    assert LockService(inner, TTL).acquire("b", 1) > t1


def test_competitor_between_load_and_cas_in_release():
    inner = Store()
    svc = LockService(inner, TTL)
    t = svc.acquire("a", 0)
    store = HookStore(inner)
    # While a's release has loaded, a's lease expires and b takes the lock.
    store.after_load.append(("lock", lambda: svc.acquire("b", TTL)))
    # a's release (valid at now=1) loses its CAS; on retry the lock belongs to b.
    assert LockService(store, TTL).release("a", t, 1) is False
    assert svc.acquire("c", TTL + 1) is None  # b still holds


def test_concurrent_same_token_releases_only_one_succeeds():
    inner = Store()
    svc = LockService(inner, TTL)
    t = svc.acquire("a", 0)
    store = HookStore(inner)
    store.after_load.append(("lock", lambda: svc.release("a", t, 1)))
    assert LockService(store, TTL).release("a", t, 1) is False


def test_write_competitor_between_load_and_cas_with_higher_token():
    inner = Store()
    svc = LockService(inner, TTL)
    t1 = svc.acquire("a", 0); svc.release("a", t1, 0)
    t2 = svc.acquire("b", 0)
    store = HookStore(inner)
    store.after_load.append(("res:r", lambda: Resource(inner, "r").write(t2, "new")))
    assert Resource(store, "r").write(t1, "old") is False  # retry sees t2 used
    assert Resource(inner, "r").read() == "new"


def test_write_competitor_with_lower_token_still_accepts_higher():
    inner = Store()
    svc = LockService(inner, TTL)
    t1 = svc.acquire("a", 0); svc.release("a", t1, 0)
    t2 = svc.acquire("b", 0)
    store = HookStore(inner)
    store.after_load.append(("res:r", lambda: Resource(inner, "r").write(t1, "old")))
    assert Resource(store, "r").write(t2, "new") is True
    assert Resource(inner, "r").read() == "new"


def test_two_writes_read_same_version_both_valid():
    inner = Store()
    svc = LockService(inner, TTL)
    t1 = svc.acquire("a", 0); svc.release("a", t1, 0)
    t2 = svc.acquire("b", 0)
    barrier = Barrier(2)

    class Gate(HookStore):
        def load(self, key):
            r = self.inner.load(key)
            if key == "res:r":
                try:
                    barrier.wait(timeout=0.5)
                except threading.BrokenBarrierError:
                    pass
            return r

    store = Gate(inner)
    out = {}
    ts = [threading.Thread(target=lambda t=t: out.update({t: Resource(store, "r").write(t, f"v{t}")}))
          for t in (t1, t2)]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert out[t2] is True
    assert Resource(inner, "r").read() == f"v{t2}"  # lower token never overwrites higher


def test_write_checks_issuance_against_store_not_object():
    s = Store()
    svc = LockService(s, TTL)
    t = svc.acquire("a", 0)
    assert Resource(s, "r").write(t, "ok")
    assert not Resource(Store(), "r").write(t, "nope")  # other store never issued it


# ---- observation: real calls checked against the P oracle ----

def _client_id(c):
    return c


def run_observed(seed, steps=60):
    rng = random.Random(seed)
    store = Store()
    svcs = [LockService(store, TTL), LockService(store, TTL)]
    ress = [Resource(store, "r"), Resource(store, "r")]
    events, held = [], {}
    for _ in range(steps):
        client = rng.choice(["a", "b", "c"])
        now = rng.randrange(0, 12)  # out-of-order times
        op = rng.choice(["acq", "acq", "rel", "wr", "wr"])
        if op == "acq":
            r = rng.choice(svcs).acquire(client, now)
            if r is not None:
                held[client] = r
            events.append(dict(event="eAcquire", client=client, now=now, result=r or 0))
        elif op == "rel":
            tok = held.get(client, rng.randrange(0, 4))
            if rng.random() < 0.2:
                tok += rng.choice([-1, 1])
            ok = rng.choice(svcs).release(client, tok, now)
            events.append(dict(event="eRelease", client=client, token=tok, now=now, ok=ok))
        else:
            tok = rng.randrange(-1, 8)
            ok = rng.choice(ress).write(tok, f"v{tok}")
            events.append(dict(event="eWrite", token=tok, ok=ok))
    return events


@pytest.mark.parametrize("seed", range(25))
def test_observed_calls_satisfy_oracle(seed):
    events = run_observed(seed)
    assert any(e["event"] == "eAcquire" and e["result"] for e in events)
    check_events(events, "Observed", spec_files=[OBS], strict=True)


def test_oracle_rejects_wrong_results():
    from snappea import SpecViolation
    bad = [dict(event="eAcquire", client="a", now=0, result=1),
           dict(event="eAcquire", client="b", now=1, result=2)]
    with pytest.raises(SpecViolation):
        check_events(bad, "Observed", spec_files=[OBS])


# ---- thread stress with independent invariants ----

def test_threaded_stress_unique_tokens_and_monotone_resource():
    store = Store()
    granted, lock = [], threading.Lock()

    def worker(i):
        svc, res = LockService(store, TTL), Resource(store, "r")
        for n in range(40):
            now = i * 7 + n
            t = svc.acquire(f"c{i}", now)
            if t is not None:
                with lock:
                    granted.append(t)
                res.write(t, f"c{i}")
                svc.release(f"c{i}", t, now)

    ts = [threading.Thread(target=worker, args=(i,)) for i in range(6)]
    [t.start() for t in ts]; [t.join() for t in ts]
    new_grants = len(granted)
    assert new_grants > 0
    assert len(set(granted)) == new_grants  # a token is granted at most once per fresh grant
    assert Resource(store, "r").read() is not None
