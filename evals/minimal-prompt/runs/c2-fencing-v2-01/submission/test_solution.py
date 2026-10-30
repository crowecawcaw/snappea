import threading
import pytest
from snappea import check_events

from runtime import Store
from solution import LockService, Resource


class HookStore:
    """Wraps a Store; one-shot hooks run right after a matching load (before the
    caller decides) so a competitor can interleave between load and CAS."""

    def __init__(self, inner=None):
        self.inner = inner or Store()
        self.after_load = []  # list of (key, fn)
        self.log = []

    def load(self, key):
        result = self.inner.load(key)
        self.log.append(("load", key))
        for i, (k, fn) in enumerate(self.after_load):
            if k == key:
                del self.after_load[i]
                fn()
                break
        return result

    def compare_swap(self, key, version, value):
        self.log.append(("cas", key))
        return self.inner.compare_swap(key, version, value)


def svc(store=None, ttl=3):
    store = store or Store()
    return store, LockService(store, ttl)


# ---- basic contract ----

def test_acquire_release_and_tokens_increase():
    _, a = svc()
    t1 = a.acquire("a", 0)
    assert t1 is not None and t1 >= 1
    assert a.acquire("a", 1) == t1          # holder gets same token back
    assert a.acquire("b", 1) is None
    assert a.release("a", t1, 2) is True
    assert a.release("a", t1, 2) is False   # already released
    t2 = a.acquire("b", 2)
    assert t2 > t1
    assert a.release("b", t2, 2)
    assert a.acquire("a", 2) > t2


def test_expiry_boundary():
    _, a = svc(ttl=3)
    t1 = a.acquire("a", 10)
    assert a.acquire("b", 12) is None       # now < now0+ttl
    t2 = a.acquire("b", 13)                 # now == expiry: expired
    assert t2 is not None and t2 > t1


def test_release_boundary_and_preconditions():
    _, a = svc(ttl=3)
    t = a.acquire("a", 0)
    assert a.release("b", t, 1) is False    # wrong client only
    assert a.release("a", t + 1, 1) is False  # wrong token only
    assert a.release("a", t, 3) is False    # exactly at expiry: too late
    assert a.release("a", t, 2) is True     # one before expiry
    t = a.acquire("a", 5)
    assert a.release("a", t, 8) is False
    assert a.acquire("b", 8) is not None


def test_release_of_expired_does_not_free_a_new_holder():
    _, a = svc(ttl=3)
    t1 = a.acquire("a", 0)
    t2 = a.acquire("b", 3)
    assert a.release("a", t1, 4) is False   # stale token
    assert a.acquire("a", 4) is None        # b still holds
    assert a.release("b", t2, 4) is True


def test_out_of_order_times():
    _, a = svc(ttl=3)
    t1 = a.acquire("a", 10)
    assert a.acquire("b", 5) is None        # unexpired for any now < 13
    assert a.acquire("a", 5) == t1
    t2 = a.acquire("b", 13)
    assert t2 > t1
    assert a.acquire("a", 1) is None        # earlier time cannot take b's lease


def test_tokens_survive_release_and_expiry():
    store, a = svc(ttl=1)
    seen = []
    for i in range(5):
        seen.append(a.acquire("c%d" % (i % 2), i * 2))
        if i % 2:
            a.release("c%d" % (i % 2), seen[-1], i * 2)
    assert seen == sorted(set(seen))
    # a second service object on the same store continues the sequence
    b = LockService(store, 1)
    assert b.acquire("z", 100) > seen[-1]


def test_acquire_retry_after_others_changed_state():
    _, a = svc(ttl=3)
    t1 = a.acquire("a", 0)
    assert a.acquire("a", 1) == t1
    t2 = a.acquire("b", 3)
    assert a.acquire("a", 4) is None        # no stale memo of a's old token
    assert a.acquire("b", 4) == t2


# ---- fencing ----

def test_fencing_rules():
    store, a = svc(ttl=3)
    r = Resource(store, "r")
    assert r.read() is None
    assert r.write(0, "x") is False         # never issued
    assert r.write(1, "x") is False         # not yet issued
    t1 = a.acquire("a", 0)
    t2 = a.acquire("b", 3)
    assert r.write(t2 + 1, "x") is False    # never issued (just above last)
    assert r.write(-1, "x") is False
    assert r.read() is None
    assert r.write(t2, "new") is True
    assert r.write(t1, "old") is False      # lower than used
    assert r.read() == "new"
    assert r.write(t2, "again") is True     # equal token is allowed
    assert r.read() == "again"


def test_fencing_per_resource_and_shared_objects():
    store, a = svc(ttl=3)
    r1, r2, r1b = Resource(store, "r1"), Resource(store, "r2"), Resource(store, "r1")
    t1 = a.acquire("a", 0)
    t2 = a.acquire("b", 3)
    assert r1.write(t2, "hi")
    assert r2.write(t1, "low-but-fresh")    # r2 never saw t2
    assert r1b.write(t1, "x") is False      # state lives in the store
    assert r1b.read() == "hi" and r2.read() == "low-but-fresh"
    # lock key and resource keys do not collide
    assert a.acquire("c", 3) is None


def test_lower_token_still_issued_is_accepted_if_nothing_higher_used():
    store, a = svc(ttl=3)
    r = Resource(store, "r")
    t1 = a.acquire("a", 0)
    a.acquire("b", 3)
    assert r.write(t1, "stale-but-unfenced") is True   # contract: only used tokens fence


# ---- forced interleavings ----

def test_two_acquires_read_same_version():
    store = HookStore()
    _, a = svc(store)
    results = {}
    barrier = threading.Barrier(2)
    inner_load = store.load
    count = {"n": 0}
    lock = threading.Lock()

    def load(key):
        v = inner_load(key)
        with lock:
            count["n"] += 1
            first_two = count["n"] <= 2
        if first_two:
            barrier.wait(timeout=5)
        return v
    store.load = load

    def run(name):
        results[name] = a.acquire(name, 0)
    ts = [threading.Thread(target=run, args=(n,)) for n in "xy"]
    [t.start() for t in ts]
    [t.join(10) for t in ts]
    assert sorted(v is None for v in results.values()) == [False, True]
    winner = [n for n, v in results.items() if v is not None][0]
    assert a.acquire(winner, 1) == results[winner]


def test_competitor_between_load_and_cas_in_acquire():
    store = HookStore()
    _, a = svc(store)
    other = LockService(store.inner, 3)
    won = {}
    store.after_load.append(("lock", lambda: won.update(t=other.acquire("rival", 0))))
    assert a.acquire("me", 0) is None       # must re-read and re-decide
    assert won["t"] is not None
    assert other.acquire("rival", 1) == won["t"]


def test_competitor_between_load_and_cas_in_release():
    store = HookStore()
    _, a = svc(store)
    t = a.acquire("me", 0)
    other = LockService(store.inner, 3)
    # Competing actions between load and CAS: holder is released and a rival takes over.
    def interfere():
        assert other.release("me", t, 1)
        assert other.acquire("rival", 1) > t
    store.after_load.append(("lock", interfere))
    assert a.release("me", t, 1) is False   # must re-decide on the new state
    assert other.acquire("me", 1) is None   # rival still holds


def test_two_releases_same_lease():
    store = HookStore()
    _, a = svc(store)
    t = a.acquire("me", 0)
    other = LockService(store.inner, 3)
    res = {}
    store.after_load.append(("lock", lambda: res.update(inner=other.release("me", t, 1))))
    outer = a.release("me", t, 1)
    assert [res["inner"], outer] == [True, False]


def test_write_competitor_between_load_and_cas():
    store = HookStore()
    s, a = svc(store)
    t1 = a.acquire("a", 0)
    t2 = a.acquire("b", 3)
    r = Resource(store, "r")
    rival = Resource(store.inner, "r")
    store.after_load.append(("res:r", lambda: rival.write(t2, "hi")))
    assert r.write(t1, "lo") is False       # re-reads, sees higher token
    assert r.read() == "hi"


def test_write_competitor_lower_token_then_higher():
    store = HookStore()
    s, a = svc(store)
    t1 = a.acquire("a", 0)
    t2 = a.acquire("b", 3)
    r = Resource(store, "r")
    rival = Resource(store.inner, "r")
    store.after_load.append(("res:r", lambda: rival.write(t1, "lo")))
    assert r.write(t2, "hi") is True
    assert r.read() == "hi"


def test_write_unissued_token_never_stored_even_if_issued_later_in_race():
    store = HookStore()
    s, a = svc(store)
    r = Resource(store, "r")
    t_future = 1
    # a token is issued right after the writer's lock read: the write was already unissued
    store.after_load.append(("lock", lambda: a.acquire("a", 0)))
    assert r.write(t_future, "early") is False
    assert r.read() is None
    assert r.write(t_future, "later") is True


def test_threads_exactly_one_winner_and_monotone_tokens():
    store = Store()
    a = LockService(store, 5)
    out = []
    def run(i):
        out.append(a.acquire("c%d" % i, 0))
    ts = [threading.Thread(target=run, args=(i,)) for i in range(16)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    assert len([x for x in out if x is not None]) == 1


# ---- observation against the P spec ----

class Observed:
    """Runs real calls and records what happened as events for Contract."""

    def __init__(self, client_count, total_ops, ttl=2):  # Fencing.p models ttl=2
        self.store = Store()
        self.svc = LockService(self.store, ttl)
        self.events = [{"event": "eStart", "payload": {"clients": client_count, "ops": total_ops}}]
        self.n = {}

    def _rec(self):
        v, r = self.store.load("lock")
        r = r or {"last": 0, "holder": None, "token": 0, "expires": 0}
        h = -1 if r["holder"] is None else int(r["holder"])
        return v, {"ver": v, "holder": h, "tok": r["token"], "last": r["last"], "exp": r["expires"]}

    def lock(self, who, kind, now, arg=0):
        ver, _ = self._rec()
        k = self.n.get(who, 0); self.n[who] = k + 1
        op = who * 10 + k
        if kind == 0:
            got = self.svc.acquire(str(who), now); result = got or 0
        else:
            result = int(self.svc.release(str(who), arg, now))
        ver2, after = self._rec()
        self.events.append({"event": "eLockOp", "payload": {
            "op": op, "kind": kind, "who": who, "now": now, "arg": arg, "result": result,
            "ver": ver, "committed": ver2 != ver, "after": after}})
        self.events.append({"event": "eReturned", "payload": {"op": op, "result": result}})
        return result

    def write(self, who, name, token, value="v"):
        lver, _ = self._rec()
        k = self.n.get(who, 0); self.n[who] = k + 1
        op = who * 10 + k
        key = "res:%d" % name
        rver = self.store.load(key)[0]
        ok = Resource(self.store, "%d" % name).write(token, value)
        committed = self.store.load(key)[0] != rver
        self.events.append({"event": "eWriteOp", "payload": {
            "op": op, "name": name, "token": token, "result": ok, "lver": lver,
            "rver": rver, "committed": committed}})
        self.events.append({"event": "eReturned", "payload": {"op": op, "result": int(ok)}})
        return ok

    def finish(self, clients):
        self.events[0]["payload"]["ops"] = sum(e["event"] == "eReturned" for e in self.events)
        self.events += [{"event": "eClientDone", "payload": None}] * clients
        check_events(self.events, "Contract", spec_files=["Fencing.p"], strict=True)


def test_observation_matches_contract():
    o = Observed(2, 14)
    t1 = o.lock(1, 0, 0)
    assert o.lock(2, 0, 1) == 0
    assert o.lock(1, 0, 1) == t1
    assert o.write(1, 0, t1)
    assert o.write(1, 0, 0) is False
    assert o.write(1, 0, t1 + 5) is False
    t2 = o.lock(2, 0, 3)               # expiry boundary
    assert o.lock(1, 1, 3, t1) == 0    # stale release
    assert o.write(2, 0, t1) is True   # equal to the highest used token
    assert o.write(2, 0, t2) is True
    assert o.write(1, 0, t1) is False  # now lower than used
    o.finish(2)


def test_observation_rejects_a_wrong_history():
    o = Observed(1, 1)
    o.lock(1, 0, 0)
    # tamper: claim a rival got the lock while it was held
    ev = [dict(e) for e in o.events]
    ev.append({"event": "eLockOp", "payload": {"op": 99, "kind": 0, "who": 2, "now": 1, "arg": 0,
               "result": 2, "ver": 1, "committed": True,
               "after": {"ver": 2, "holder": 2, "tok": 2, "last": 2, "exp": 4}}})
    with pytest.raises(Exception):
        check_events(ev, "Contract", spec_files=["Fencing.p"], strict=True)
