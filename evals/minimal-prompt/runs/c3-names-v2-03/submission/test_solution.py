"""Forced interleavings from the P model (Names.p), validation, and observation."""
import random
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest
from snappea import check_events

from runtime import Store
from solution import Registry


class HookStore:
    """Real store with hooks run around load/compare_swap on chosen keys."""

    def __init__(self, inner=None):
        self.inner = inner or Store()
        self.after_load = None   # f(key) called after a load returns
        self.before_cas = None   # f(key, value) called before a CAS
        self.log = []            # (key, old_value, new_value) of successful CAS

    def load(self, key):
        result = self.inner.load(key)
        hook = self.after_load
        if hook:
            hook(key)
        return result

    def compare_swap(self, key, version, value):
        hook = self.before_cas
        if hook:
            hook(key, value)
        old = self.inner.load(key)[1]
        ok = self.inner.compare_swap(key, version, value)
        if ok:
            self.log.append((key, old, value))
        return ok


def on_first(key, fn):
    """Hook that runs fn once, right after the first load of `key`."""
    done = []

    def hook(k):
        if k == key and not done:
            done.append(1)
            fn()
    return hook


def consistent(store, users, names):
    """owner(n) == u exactly when name_of(u) == n."""
    r = Registry(store)
    for n in names:
        for u in users:
            assert (r.owner(n) == u) == (r.name_of(u) == n), (n, u)


# ---- validation -----------------------------------------------------------

@pytest.mark.parametrize("bad", ["", "a" * 17, "a b", "a_b", "é", "a\n", "K", "A!", " ", None, 5])
def test_invalid_names_change_nothing(bad):
    store = Store()
    r = Registry(store)
    assert r.register("u", "ok") is True
    assert r.register("v", bad) is False
    assert r.rename("u", bad) is False
    assert r.owner(bad) is None
    assert r.name_of("v") is None and r.name_of("u") == "ok" and r.owner("ok") == "u"


def test_length_boundaries_and_charset():
    r = Registry(Store())
    assert r.register("a", "x") is True            # 1
    assert r.register("b", "a" * 16) is True       # 16
    assert r.register("c", "a" * 17) is False      # 17
    assert r.register("d", "0-9-z") is True
    assert r.owner("A" * 16) == "b"
    assert r.name_of("b") == "a" * 16


def test_case_insensitive_and_lowercase_storage():
    r = Registry(Store())
    assert r.register("u", "Bob") is True
    assert r.name_of("u") == "bob" and r.owner("BOB") == "u"
    assert r.register("v", "bOb") is False
    assert r.register("u", "BOB") is True
    assert r.register("u", "other") is False
    assert r.rename("u", "BoB") is True
    assert r.rename("u", "Carl") is True
    assert r.owner("bob") is None and r.name_of("u") == "carl"
    assert r.register("v", "bob") is True


# ---- sequential contract -----------------------------------------------------

def test_each_precondition_alone():
    r = Registry(Store())
    assert r.rename("u", "a") is False               # no name yet
    assert r.name_of("u") is None and r.owner("a") is None
    assert r.register("u", "a") is True
    assert r.register("u", "b") is False             # user has a name
    assert r.register("v", "a") is False             # name taken
    assert r.rename("v", "b") is False               # v has no name
    assert r.register("v", "b") is True
    assert r.rename("v", "a") is False               # taken by u; v keeps b
    assert r.name_of("v") == "b" and r.owner("b") == "v" and r.owner("a") == "u"
    assert r.rename("v", "b") is True
    assert r.rename("u", "c") is True
    assert r.register("w", "a") is True              # old name is free
    consistent(r._store, ["u", "v", "w"], ["a", "b", "c"])


# ---- forced interleavings ----------------------------------------------------

def test_two_users_read_same_free_name_one_wins():
    inner = Store()
    barrier = threading.Barrier(2, timeout=3)
    store = HookStore(inner)
    store.after_load = lambda key: barrier.wait() if key == "name:x" else None
    a, b = Registry(store), Registry(store)
    with ThreadPoolExecutor(2) as pool:
        fa, fb = pool.submit(a.register, "ua", "x"), pool.submit(b.register, "ub", "x")
        results = sorted([fa.result(5), fb.result(5)])
    store.after_load = None
    assert results == [False, True]
    consistent(inner, ["ua", "ub"], ["x"])
    winner = a.owner("x")
    assert winner in ("ua", "ub") and a.name_of(winner) == "x"
    loser = "ub" if winner == "ua" else "ua"
    assert a.name_of(loser) is None


def test_competitor_between_load_and_cas_forces_reject():
    store = HookStore()
    r = Registry(store)
    rival = Registry(store.inner)
    store.after_load = on_first("name:x", lambda: rival.register("rival", "x"))
    assert r.register("me", "x") is False
    store.after_load = None
    assert r.owner("x") == "rival" and r.name_of("me") is None
    assert r.register("me", "y") is True            # same user can still register


def test_competitor_between_load_and_cas_in_rename():
    store = HookStore()
    r = Registry(store)
    assert r.register("me", "old")
    rival = Registry(store.inner)
    store.after_load = on_first("name:new", lambda: rival.register("rival", "new"))
    assert r.rename("me", "new") is False
    store.after_load = None
    assert r.name_of("me") == "old" and r.owner("old") == "me" and r.owner("new") == "rival"
    consistent(store.inner, ["me", "rival"], ["old", "new"])


def test_swap_rename_with_competitor_after_claim():
    # Between rename's claim of the new name and its release of the old name, a
    # second user registering the old name fails (the old name is still held);
    # afterwards it succeeds.
    store = HookStore()
    r = Registry(store)
    assert r.register("me", "old")
    rival = Registry(store.inner)
    seen = []
    store.before_cas = lambda key, value: seen.append(rival.register("rival", "old")) if (
        key == "name:old" and value is None and not seen) else None
    assert r.rename("me", "new") is True
    store.before_cas = None
    assert seen == [False]                 # still held by me when the rival looked
    assert rival.register("rival", "old") is True
    consistent(store.inner, ["me", "rival"], ["old", "new"])
    assert r.name_of("me") == "new" and r.owner("old") == "rival"


def test_same_user_second_object_sees_state():
    inner = Store()
    a, b = Registry(inner), Registry(inner)
    assert a.register("u", "n") is True
    assert b.register("u", "n") is True          # retry through another object
    assert b.register("u", "m") is False
    assert b.rename("u", "m") is True
    assert a.rename("u", "m") is True            # repeated rename: same result
    assert a.owner("n") is None and b.owner("m") == "u"


def test_state_lives_in_store_only():
    inner = Store()
    Registry(inner).register("u", "n")
    fresh = Registry(inner)
    assert fresh.owner("n") == "u" and fresh.name_of("u") == "n"


def test_stress_threads_keep_invariant():
    rng = random.Random(7)
    inner = Store()
    users = [f"u{i}" for i in range(6)]
    names = [f"n{i}" for i in range(4)]
    plans = {u: [(rng.choice(["register", "rename"]), rng.choice(names)) for _ in range(40)] for u in users}

    def run(u):
        r = Registry(inner)
        for op, n in plans[u]:
            getattr(r, op)(u, n)

    with ThreadPoolExecutor(len(users)) as pool:
        list(pool.map(run, users))
    consistent(inner, users, names)
    held = [Registry(inner).name_of(u) for u in users]
    held = [h for h in held if h]
    assert len(held) == len(set(held))


# ---- observation against the P spec ---------------------------------------------

IDS = {"u1": 1, "u2": 2, "a": 3, "b": 4, "c": 5, "d": 6}


class Observed:
    """Records actual calls, the store's name transitions, and the final state as
    events for the Contract monitor in Names.p."""

    def __init__(self, contended):
        self.store = HookStore()
        self.reg = Registry(self.store)
        self.events = []
        self.contended = contended
        self.seen = 0

    def _flush_holds(self):
        for key, old, new in self.store.log[self.seen:]:
            if key.startswith("name:"):
                self.events.append({"event": "eHold", "payload": {
                    "name": IDS[key[5:]], "before": IDS[old] if old else 0, "after": IDS[new] if new else 0}})
        self.seen = len(self.store.log)

    def call(self, op, user, name):
        ok = getattr(self.reg, op)(user, name)
        self._flush_holds()
        self.events.append({"event": "eReturned", "payload": {
            "user": IDS[user], "isRegister": op == "register", "name": IDS[name],
            "ok": ok, "contended": self.contended}})
        return ok

    def check(self):
        names, users = {}, {}
        for n in ("a", "b", "c", "d"):
            holder = self.reg.owner(n)
            if self.store.inner.load("name:" + n)[0]:
                names[IDS[n]] = IDS[holder] if holder else 0
        for u in ("u1", "u2"):
            name = self.reg.name_of(u)
            if self.store.inner.load("user:" + u)[0]:
                users[IDS[u]] = IDS[name] if name else 0
        self.events.append({"event": "eFinal", "payload": {"names": names, "users": users}})
        check_events(self.events, "Contract", spec_files=["Names.p"], strict=True, unknown_events="error")


def test_observed_sequence_matches_spec():
    o = Observed(contended=False)
    assert o.call("register", "u1", "a") is True
    assert o.call("rename", "u1", "c") is True
    assert o.call("register", "u2", "a") is True
    assert o.call("rename", "u2", "d") is True
    o.check()


def test_observed_contention_matches_spec():
    o = Observed(contended=True)
    assert o.call("register", "u1", "a") is True
    assert o.call("register", "u2", "a") is False
    assert o.call("rename", "u2", "b") is False
    assert o.call("register", "u2", "b") is True
    o.check()


def test_observation_rejects_a_wrong_history():
    o = Observed(contended=False)
    assert o.call("register", "u1", "a") is True
    assert o.call("register", "u2", "b") is True
    o.events[-1]["payload"]["ok"] = False   # an uncontended register cannot fail
    with pytest.raises(Exception):
        o.check()
