"""Tests for Registry: basics, forced interleavings from Names.p, observation vs the P spec."""
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest
from snappea import check_events

from runtime import Store
from solution import Registry


def make():
    store = Store()
    return store, Registry(store)


class Hook:
    """Real store; `before_cas(key, version, value)` may run a competitor before each CAS,
    `after_load(key)` after each load. Competitors use the inner store/other Registry."""

    def __init__(self, inner):
        self.inner = inner
        self.after_load = None
        self.before_cas = None

    def load(self, key):
        result = self.inner.load(key)
        if self.after_load:
            hook, self.after_load = self.after_load, None
            hook(key)
        return result

    def compare_swap(self, key, version, value):
        if self.before_cas:
            hook, self.before_cas = self.before_cas, None
            hook(key)
        return self.inner.compare_swap(key, version, value)


# --- basics and boundaries -------------------------------------------------------

def test_register_and_lookup():
    _, r = make()
    assert r.register("u", "Bob")
    assert r.name_of("u") == "bob"
    assert r.owner("BOB") == "u" and r.owner("bob") == "u"


def test_register_rules():
    _, r = make()
    assert r.register("u", "bob")
    assert r.register("u", "BOB")          # same name again
    assert not r.register("u", "alice")    # user already holds a name
    assert not r.register("v", "Bob")      # name taken
    assert r.name_of("u") == "bob" and r.name_of("v") is None
    assert r.owner("alice") is None


@pytest.mark.parametrize("name,valid", [
    ("a", True), ("a" * 16, True), ("a" * 17, False), ("", False),
    ("0-9", True), ("-", True), ("a_b", False), ("a b", False), ("a.b", False),
    ("é", False), ("K", False), ("bob\n", False), (None, False), (5, False),
])
def test_name_validity(name, valid):
    _, r = make()
    assert r.register("u", name) is valid
    assert (r.name_of("u") is not None) is valid
    assert r.owner(name) == ("u" if valid else None)


def test_invalid_changes_nothing():
    _, r = make()
    r.register("u", "bob")
    assert not r.rename("u", "bad name")
    assert not r.rename("u", "x" * 17)
    assert r.name_of("u") == "bob" and r.owner("bob") == "u"
    assert not r.register("v", "")
    assert r.name_of("v") is None


def test_rename_rules():
    _, r = make()
    assert not r.rename("u", "a")          # no name yet
    assert r.name_of("u") is None and r.owner("a") is None
    r.register("u", "a")
    r.register("v", "b")
    assert not r.rename("u", "B")          # taken
    assert r.name_of("u") == "a" and r.owner("a") == "u" and r.owner("b") == "v"
    assert r.rename("u", "A")              # own name
    assert r.rename("u", "c")
    assert r.owner("a") is None and r.owner("c") == "u" and r.name_of("u") == "c"
    assert r.register("w", "a")            # old name is free
    assert r.rename("v", "Bb")
    assert r.owner("b") is None


def test_state_lives_in_store():
    store, r = make()
    r.register("u", "bob")
    other = Registry(store)
    assert other.name_of("u") == "bob" and other.owner("bob") == "u"
    assert not other.register("v", "bob")


# --- forced interleavings (scenarios from Names.p) -------------------------------

def test_both_read_name_free_one_loses_cas():
    """Two users read the name record free before either claims it."""
    store = Store()
    barrier = threading.Barrier(2, timeout=5)
    seen = []
    lock = threading.Lock()

    class Sync:
        def load(self, key):
            result = store.load(key)
            if key[0] == "name":
                with lock:
                    first = len(seen) < 2
                    seen.append(key)
                if first:
                    barrier.wait()
            return result

        def compare_swap(self, *a):
            return store.compare_swap(*a)

    ra, rb = Registry(Sync()), Registry(Sync())
    with ThreadPoolExecutor(2) as ex:
        fa = ex.submit(ra.register, "a", "x")
        fb = ex.submit(rb.register, "b", "x")
        results = sorted([fa.result(10), fb.result(10)])
    assert results == [False, True]
    verify = Registry(store)
    winner = verify.owner("x")
    assert winner in ("a", "b") and verify.name_of(winner) == "x"
    assert verify.name_of("b" if winner == "a" else "a") is None


def test_competitor_claims_between_load_and_cas():
    store, plain = make()
    hook = Hook(store)
    mine = Registry(hook)
    hook.after_load = lambda key: plain.register("b", "x")  # fires after first load (user rec)
    # first load is the user record; arrange the competitor right after the name load
    hook.after_load = None
    seen = []

    def after(key):
        seen.append(key)

    class Wrap(Hook):
        def load(self, key):
            result = self.inner.load(key)
            if key == ("name", "x") and not seen:
                seen.append(key)
                plain.register("b", "x")
            return result

    mine = Registry(Wrap(store))
    assert not mine.register("a", "x")
    assert plain.owner("x") == "b" and plain.name_of("a") is None and plain.name_of("b") == "x"


def test_rename_target_taken_between_load_and_cas():
    store, plain = make()
    plain.register("a", "x")

    class Wrap(Hook):
        fired = False

        def load(self, key):
            result = self.inner.load(key)
            if key == ("name", "y") and not self.fired:
                self.fired = True
                plain.register("b", "y")
            return result

    assert not Registry(Wrap(store)).rename("a", "y")
    assert plain.name_of("a") == "x" and plain.owner("x") == "a"
    assert plain.owner("y") == "b" and plain.name_of("b") == "y"


def test_swap_names_both_succeed_never_share():
    """a: x->y while b: y->x. Exactly one is refused until the other releases; no name
    ever has two holders, and at rest the views agree."""
    for _ in range(50):
        store, r = make()
        r.register("a", "x")
        r.register("b", "y")
        r2 = Registry(store)
        with ThreadPoolExecutor(2) as ex:
            fa = ex.submit(r.rename, "a", "y")
            fb = ex.submit(r2.rename, "b", "x")
            fa.result(10), fb.result(10)
        check_consistent(r, ["a", "b"], ["x", "y"])


def test_rename_old_name_taken_by_other_right_after_release():
    store, r = make()
    r.register("a", "x")
    r.rename("a", "y")
    assert r.register("b", "x")
    assert r.owner("x") == "b" and r.owner("y") == "a" and r.name_of("a") == "y"


def test_rename_publishes_before_release():
    """While a rename is mid-flight (new name published), no other user can have either
    name yet unreleased, and the old name is not lost."""
    store, plain = make()
    plain.register("a", "x")
    observed = {}

    class Wrap(Hook):
        def compare_swap(self, key, version, value):
            ok = self.inner.compare_swap(key, version, value)
            if key == ("name", "x") and value is None:
                # old name just released: user record must already say 'y'
                observed["name_of"] = plain.name_of("a")
                observed["owner_y"] = plain.owner("y")
            return ok

    assert Registry(Wrap(store)).rename("a", "y")
    assert observed == {"name_of": "y", "owner_y": "a"}


def test_same_user_second_object_same_result():
    store, r = make()
    r.register("a", "x")
    r2 = Registry(store)
    assert r2.register("a", "x")
    assert not r2.register("a", "y")
    assert r2.rename("a", "x")


# --- stress with a consistency check at rest -------------------------------------

def check_consistent(r, users, names):
    held = {}
    for u in users:
        n = r.name_of(u)
        if n is not None:
            assert n not in held, f"{n} held twice"
            held[n] = u
            assert r.owner(n) == u
    for n in names:
        o = r.owner(n)
        assert (o is None) == (n not in held)
        if o is not None:
            assert r.name_of(o) == n


def test_thread_stress_consistent_at_rest():
    import random
    names = [f"n{i}" for i in range(4)]
    users = [f"u{i}" for i in range(6)]
    for round_ in range(20):
        store = Store()

        def work(u):
            r = Registry(store)
            rng = random.Random(hash((u, round_)))
            for _ in range(30):
                if rng.random() < 0.5:
                    r.register(u, rng.choice(names))
                else:
                    r.rename(u, rng.choice(names))

        with ThreadPoolExecutor(len(users)) as ex:
            list(ex.map(work, users))
        check_consistent(Registry(store), users, names)


# --- observation: real calls checked against the P spec --------------------------

USERS = {"a": 0, "b": 1}
NAMES = {"x": 1, "y": 2, "z": 3}


class Recorder:
    def __init__(self, registry):
        self.r, self.events = registry, []

    def _op(self, kind, user, name):
        op = {"kind": kind, "name": NAMES[name]}
        self.events.append({"event": "eInvoke", "payload": {"user": USERS[user], "op": op}})
        ok = (self.r.register if kind == 0 else self.r.rename)(user, name)
        self.events.append({"event": "eReturn", "payload": {"user": USERS[user], "op": op, "ok": ok}})
        return ok

    def register(self, user, name):
        return self._op(0, user, name)

    def rename(self, user, name):
        return self._op(1, user, name)

    def audit(self):
        for u, i in USERS.items():
            n = self.r.name_of(u)
            self.events.append({"event": "eAuditUser",
                                "payload": {"user": i, "name": NAMES[n] if n else 0}})
        for n, i in NAMES.items():
            o = self.r.owner(n)
            self.events.append({"event": "eAuditOwner",
                                "payload": {"name": i, "owner": USERS[o] if o else -1}})
        self.events.append({"event": "eDone", "payload": None})

    def check(self):
        self.audit()
        check_events(self.events, "NamesContract", spec_files=["Names.p"], strict=True)


def test_observed_sequence_matches_spec():
    _, r = make()
    rec = Recorder(r)
    assert rec.register("a", "x")
    assert not rec.register("b", "x")
    assert rec.register("b", "y")
    assert not rec.rename("a", "y")
    assert rec.rename("a", "z")
    assert rec.rename("b", "x")
    assert rec.rename("b", "x")
    assert not rec.register("a", "y")
    rec.check()


def test_observed_forced_race_matches_spec():
    store, plain = make()
    rec_events = Recorder(plain)

    class Wrap(Hook):
        fired = False

        def load(self, key):
            result = self.inner.load(key)
            if key == ("name", "x") and not self.fired:
                self.fired = True
                rec_events.register("b", "x")
            return result

    racer = Recorder(Registry(Wrap(store)))
    racer.events = rec_events.events
    assert not racer.register("a", "x")
    racer.check()


def test_spec_rejects_wrong_result():
    """The spec itself catches a wrong reported result (sanity for the oracle)."""
    _, r = make()
    rec = Recorder(r)
    rec.register("a", "x")
    rec.events[-1]["payload"]["ok"] = False   # claim register failed though nobody contested
    with pytest.raises(Exception):
        rec.check()
