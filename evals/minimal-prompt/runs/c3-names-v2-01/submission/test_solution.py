"""Tests for Registry: boundaries, forced interleavings from the P model, and
observation of real calls against the NamesContract spec in Names.p."""
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest
from snappea import check_events

from runtime import Store
from solution import Registry


class HookStore:
    """Real store; `after(op, key)` runs after each real load/compare_swap returns
    (before the caller sees the result), so a competitor can run in that window.
    Competitors must use the inner store/another Registry on the inner store."""

    def __init__(self, inner=None):
        self.inner = inner or Store()
        self.hooks = []  # callables (op, key, result) -> None

    def load(self, key):
        result = self.inner.load(key)
        for h in list(self.hooks):
            h("load", key, result)
        return result

    def compare_swap(self, key, version, value):
        result = self.inner.compare_swap(key, version, value)
        for h in list(self.hooks):
            h("cas", key, result)
        return result


def once(fn, op, key):
    """Hook: run fn the first time (op, key) happens."""
    state = {"done": False}

    def hook(o, k, _result):
        if o == op and k == key and not state["done"]:
            state["done"] = True
            fn()
    return hook


def consistent(reg, users, names):
    for u in users:
        n = reg.name_of(u)
        if n is not None:
            assert reg.owner(n) == u
    for n in names:
        o = reg.owner(n)
        if o is not None:
            assert reg.name_of(o) == n


# ---- contract, boundaries ----------------------------------------------------------

def test_basic_register_rename_release():
    r = Registry(Store())
    assert r.register("u1", "alice") is True
    assert r.owner("alice") == "u1" and r.name_of("u1") == "alice"
    assert r.register("u2", "alice") is False
    assert r.register("u1", "alice") is True  # same name again
    assert r.register("u1", "bob") is False  # already has a name
    assert r.name_of("u1") == "alice"
    assert r.rename("u1", "carol") is True
    assert r.owner("alice") is None and r.owner("carol") == "u1"
    assert r.register("u2", "alice") is True  # old name is free
    assert r.rename("u1", "carol") is True  # same name
    assert r.rename("u1", "alice") is False  # held by u2
    assert r.name_of("u1") == "carol" and r.owner("alice") == "u2"
    assert r.rename("nobody", "zed") is False  # no name yet
    assert r.name_of("nobody") is None and r.owner("zed") is None


def test_case_insensitive_and_lowercase_storage():
    r = Registry(Store())
    assert r.register("u1", "BoB") is True
    assert r.name_of("u1") == "bob"
    assert r.owner("BOB") == "u1" and r.owner("bob") == "u1"
    assert r.register("u2", "bOb") is False
    assert r.register("u1", "BOB") is True
    assert r.rename("u1", "Bob") is True
    assert r.rename("u1", "NEW-1") is True
    assert r.name_of("u1") == "new-1"
    assert r.owner("bob") is None


@pytest.mark.parametrize("name,valid", [
    ("a", True), ("a" * 16, True), ("a" * 17, False), ("", False),
    ("0-9", True), ("-", True), ("a_b", False), ("a b", False), ("é", False),
    ("a.b", False), ("a\n", False), ("A" * 16, True), ("A" * 17, False),
])
def test_name_validity_boundaries(name, valid):
    r = Registry(Store())
    assert r.register("u1", name) is valid
    if valid:
        assert r.name_of("u1") == name.lower()
        assert r.owner(name) == "u1"
    else:
        assert r.name_of("u1") is None


def test_invalid_names_change_nothing():
    store = Store()
    r = Registry(store)
    assert r.register("u1", "ok") is True
    assert r.register("u2", "bad name") is False
    assert r.rename("u1", "x" * 17) is False
    assert r.rename("u1", "") is False
    assert r.rename("u1", "a_b") is False
    assert r.owner("bad name") is None and r.owner("") is None and r.owner("a_b") is None
    assert r.name_of("u1") == "ok" and r.owner("ok") == "u1" and r.name_of("u2") is None
    # u1 who has a name registering an invalid one: still nothing changes
    assert r.register("u1", "bad!") is False
    assert r.name_of("u1") == "ok"


def test_state_lives_in_store_not_object():
    store = Store()
    assert Registry(store).register("u1", "x") is True
    other = Registry(store)
    assert other.owner("x") == "u1" and other.name_of("u1") == "x"
    assert other.register("u2", "x") is False
    assert other.rename("u1", "y") is True
    assert Registry(store).owner("x") is None


# ---- forced interleavings (P: tcSameName, tcSwap, tcHandoff) ------------------------

def test_two_users_read_same_free_name_one_wins():
    inner = Store()
    hs = HookStore(inner)
    barrier = threading.Barrier(2, timeout=3)
    count = {"n": 0}
    lock = threading.Lock()

    def hook(op, key, _r):
        if op == "load" and key == "name:x":
            with lock:
                count["n"] += 1
                first_two = count["n"] <= 2
            if first_two:
                barrier.wait()
    hs.hooks.append(hook)
    a, b = Registry(hs), Registry(hs)
    with ThreadPoolExecutor(2) as pool:
        fa = pool.submit(a.register, "ua", "x")
        fb = pool.submit(b.register, "ub", "x")
        results = sorted([fa.result(5), fb.result(5)])
    assert results == [False, True]
    fresh = Registry(inner)
    winner = fresh.owner("x")
    assert winner in ("ua", "ub")
    loser = "ub" if winner == "ua" else "ua"
    assert fresh.name_of(winner) == "x" and fresh.name_of(loser) is None
    consistent(fresh, ["ua", "ub"], ["x"])


def test_competitor_takes_name_between_load_and_cas_in_register():
    inner = Store()
    hs = HookStore(inner)
    hs.hooks.append(once(lambda: Registry(inner).register("rival", "x"), "load", "name:x"))
    r = Registry(hs)
    assert r.register("me", "x") is False  # must reload and see the rival
    assert r.owner("x") == "rival" and r.name_of("me") is None
    consistent(r, ["me", "rival"], ["x"])


def test_competitor_takes_target_between_load_and_cas_in_rename():
    inner = Store()
    assert Registry(inner).register("me", "a") is True
    hs = HookStore(inner)
    hs.hooks.append(once(lambda: Registry(inner).register("rival", "b"), "load", "name:b"))
    r = Registry(hs)
    assert r.rename("me", "b") is False
    assert r.name_of("me") == "a" and r.owner("a") == "me" and r.owner("b") == "rival"
    consistent(r, ["me", "rival"], ["a", "b"])


def test_swap_attempt_fails_for_both():
    inner = Store()
    base = Registry(inner)
    assert base.register("u1", "a") and base.register("u2", "b")
    hs = HookStore(inner)
    barrier = threading.Barrier(2, timeout=3)
    seen = set()
    lock = threading.Lock()

    def hook(op, key, _r):
        if op == "load" and key in ("name:a", "name:b"):
            with lock:
                fresh = key not in seen
                seen.add(key)
            if fresh:
                barrier.wait()
    hs.hooks.append(hook)
    with ThreadPoolExecutor(2) as pool:
        f1 = pool.submit(Registry(hs).rename, "u1", "b")
        f2 = pool.submit(Registry(hs).rename, "u2", "a")
        assert (f1.result(5), f2.result(5)) == (False, False)
    assert base.name_of("u1") == "a" and base.name_of("u2") == "b"
    assert base.owner("a") == "u1" and base.owner("b") == "u2"


def test_old_name_not_free_until_released_then_free():
    inner = Store()
    assert Registry(inner).register("u1", "a") is True
    observed = {}
    hs = HookStore(inner)

    def mid_rename():
        # u1's user record already says "b"; its old name is not yet released.
        observed["mid"] = Registry(inner).register("u2", "a")
    hs.hooks.append(once(mid_rename, "cas", "user:u1"))
    assert Registry(hs).rename("u1", "b") is True
    assert observed["mid"] is False  # either outcome keeps the invariant; this one is the algorithm's
    assert Registry(inner).register("u2", "a") is True  # retry after release works
    consistent(Registry(inner), ["u1", "u2"], ["a", "b"])


def test_rival_claims_old_name_right_after_release():
    inner = Store()
    assert Registry(inner).register("u1", "a") is True
    hs = HookStore(inner)
    out = {}
    hs.hooks.append(once(lambda: out.setdefault("r", Registry(inner).register("u2", "a")), "cas", "name:a"))
    # first cas on name:a during rename is the release (the claim is on name:b)
    assert Registry(hs).rename("u1", "b") is True
    assert out["r"] is True
    r = Registry(inner)
    assert r.owner("a") == "u2" and r.owner("b") == "u1" and r.name_of("u1") == "b"
    consistent(r, ["u1", "u2"], ["a", "b"])


def test_rename_to_name_released_by_other_user_concurrently():
    inner = Store()
    base = Registry(inner)
    assert base.register("u1", "a") and base.register("u2", "b")
    hs = HookStore(inner)
    # while u1 is moving a -> c, u2 moves b -> a (a may or may not be free yet)
    hs.hooks.append(once(lambda: base.rename("u2", "a"), "cas", "user:u1"))
    assert Registry(hs).rename("u1", "c") is True
    assert base.owner("c") == "u1"
    consistent(base, ["u1", "u2"], ["a", "b", "c"])
    assert base.name_of("u2") in ("a", "b")


def test_cas_conflict_on_name_record_is_retried_not_ignored():
    inner = Store()
    hs = HookStore(inner)
    calls = {"n": 0}

    def bump():
        # change the record's version without changing who holds it
        v, _ = inner.load("name:x")
        assert inner.compare_swap("name:x", v, None)
        calls["n"] += 1
    hs.hooks.append(once(bump, "load", "name:x"))
    r = Registry(hs)
    assert r.register("u1", "x") is True
    assert calls["n"] == 1
    assert r.owner("x") == "u1" and r.name_of("u1") == "x"


def test_many_threads_stress_invariant():
    store = Store()
    users = [f"u{i}" for i in range(8)]
    names = ["a", "b", "c", "d"]

    def work(u):
        reg = Registry(store)
        for i in range(60):
            n = names[(i * 3 + len(u) + int(u[1:])) % 4]
            (reg.register if i % 2 == 0 else reg.rename)(u, n)
    with ThreadPoolExecutor(8) as pool:
        for f in [pool.submit(work, u) for u in users]:
            f.result(30)
    reg = Registry(store)
    consistent(reg, users, names)
    held = [reg.name_of(u) for u in users if reg.name_of(u)]
    assert len(held) == len(set(held))


# ---- observation: real calls against the P spec -------------------------------------

USERS = {"u1": 2, "u2": 3}
NAMES = {"a": 0, "b": 1}


class Recorder:
    def __init__(self, reg):
        self.reg, self.events = reg, []

    def call(self, kind, user, name, invoke=True):
        if invoke:
            self.events.append({"event": "eInvoked", "payload": None})
        fn = self.reg.register if kind == 0 else self.reg.rename
        ok = fn(user, name)
        self.events.append({"event": "eReturned", "payload": {
            "user": USERS[user], "kind": kind, "name": NAMES[name], "ok": ok}})
        return ok

    def finish(self, reg=None):
        reg = reg or self.reg
        for u, i in USERS.items():
            n = reg.name_of(u)
            self.events.append({"event": "eFinalName", "payload": {
                "user": i, "name": NAMES[n] if n else -1}})
        for n, i in NAMES.items():
            o = reg.owner(n)
            self.events.append({"event": "eFinalOwner", "payload": {
                "name": i, "owner": USERS[o] if o else -1}})
        calls = sum(e["event"] == "eReturned" for e in self.events)
        self.events.append({"event": "eFinished", "payload": calls})
        check_events(self.events, "NamesContract", spec_files=["Names.p"], strict=True)


def test_observed_sequential_history_matches_spec():
    rec = Recorder(Registry(Store()))
    assert rec.call(0, "u1", "a") is True
    assert rec.call(0, "u2", "a") is False
    assert rec.call(0, "u1", "b") is False
    assert rec.call(1, "u2", "b") is False  # u2 has no name
    assert rec.call(0, "u2", "b") is True
    assert rec.call(1, "u2", "a") is False
    assert rec.call(1, "u1", "a") is True
    assert rec.call(1, "u1", "b") is False
    rec.finish()


def test_observed_release_history_matches_spec():
    rec = Recorder(Registry(Store()))
    assert rec.call(0, "u1", "a")
    assert rec.call(1, "u1", "b")
    assert rec.call(0, "u2", "a")
    rec.finish()


def test_observed_forced_race_matches_spec():
    inner = Store()
    hs = HookStore(inner)
    barrier = threading.Barrier(2, timeout=3)
    count = {"n": 0}
    lock = threading.Lock()

    def hook(op, key, _r):
        if op == "load" and key == "name:a":
            with lock:
                count["n"] += 1
                first_two = count["n"] <= 2
            if first_two:
                barrier.wait()
    hs.hooks.append(hook)
    rec = Recorder(Registry(hs))
    rec.events += [{"event": "eInvoked", "payload": None}] * 2  # both calls overlap
    with ThreadPoolExecutor(2) as pool:
        fs = [pool.submit(rec.call, 0, u, "a", False) for u in USERS]
        assert sorted(f.result(5) for f in fs) == [False, True]
    rec.finish(Registry(inner))


# ---- robustness: a version bump on a record we write must be retried ----------------

def _bump(inner, key):
    """Rewrite a record with its own value: new version, same content."""
    v, val = inner.load(key)
    assert inner.compare_swap(key, v, val)


def test_user_record_version_bump_is_retried_on_register():
    inner = Store()
    hs = HookStore(inner)
    hs.hooks.append(once(lambda: _bump(inner, "user:u1"), "cas", "name:x"))
    r = Registry(hs)
    # bump lands after the claim, before the user-record load/cas: retried by version
    hs.hooks.append(once(lambda: _bump(inner, "user:u1"), "load", "user:u1"))
    assert r.register("u1", "x") is True
    assert r.name_of("u1") == "x" and r.owner("x") == "u1"


def test_user_record_version_bump_is_retried_on_rename():
    inner = Store()
    assert Registry(inner).register("u1", "a")
    hs = HookStore(inner)
    loads = {"n": 0}

    def hook(op, key, _r):
        if op == "load" and key == "user:u1":
            loads["n"] += 1
            if loads["n"] == 2:  # the load inside _set_user (the first read the name)
                _bump(inner, "user:u1")
    hs.hooks.append(hook)
    r = Registry(hs)
    assert r.rename("u1", "b") is True
    assert r.name_of("u1") == "b" and r.owner("b") == "u1" and r.owner("a") is None


def test_release_retried_when_old_name_record_version_changes():
    inner = Store()
    assert Registry(inner).register("u1", "a")
    hs = HookStore(inner)
    n = {"loads": 0}

    def hook(op, key, _r):
        if op == "load" and key == "name:a":
            n["loads"] += 1
            _bump(inner, "name:a") if n["loads"] == 1 else None
    hs.hooks.append(hook)
    assert Registry(hs).rename("u1", "b") is True
    assert Registry(inner).owner("a") is None
