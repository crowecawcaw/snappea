import random
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest

from runtime import Store
from solution import Registry


class HookStore:
    """Real store with hooks: after_load(key) runs after a load returns (outside any
    lock); the hook may call the inner store or other Registries to interleave."""

    def __init__(self, inner, after_load=None, before_cas=None):
        self.inner, self.after_load, self.before_cas = inner, after_load, before_cas

    def load(self, key):
        result = self.inner.load(key)
        if self.after_load:
            self.after_load(key, result)
        return result

    def compare_swap(self, key, version, value):
        if self.before_cas:
            self.before_cas(key, version, value)
        return self.inner.compare_swap(key, version, value)


def once(fn, match):
    """Wrap hook fn so it runs a single time, for the first key/value where match is true."""
    state = {"done": False}

    def hook(key, *rest):
        if not state["done"] and match(key, *rest):
            state["done"] = True
            fn()
    return hook


def is_name(n):
    return lambda key, *rest: key == ("name", n)


def check_consistent(reg, users, names):
    for u in users:
        n = reg.name_of(u)
        if n is not None:
            assert reg.owner(n) == u
    for n in names:
        o = reg.owner(n)
        if o is not None:
            assert reg.name_of(o) == n


# ---- basic contract ----

def test_basic_register_and_lookup():
    r = Registry(Store())
    assert r.owner("bob") is None and r.name_of("u") is None
    assert r.register("u", "Bob") is True
    assert r.name_of("u") == "bob"
    assert r.owner("BOB") == "u" and r.owner("bob") == "u"
    assert r.register("u", "bob") is True          # same name again
    assert r.register("u", "alice") is False       # user already has a name
    assert r.name_of("u") == "bob" and r.owner("alice") is None
    assert r.register("v", "BoB") is False         # name taken, case-insensitively
    assert r.name_of("v") is None and r.owner("bob") == "u"


def test_rename():
    r = Registry(Store())
    assert r.rename("u", "x") is False             # no name yet
    assert r.name_of("u") is None and r.owner("x") is None
    r.register("u", "a")
    r.register("v", "b")
    assert r.rename("u", "B") is False             # taken
    assert (r.name_of("u"), r.owner("a"), r.owner("b")) == ("a", "u", "v")
    assert r.rename("u", "A") is True              # own name
    assert r.rename("u", "c") is True
    assert r.owner("a") is None and r.owner("c") == "u" and r.name_of("u") == "c"
    assert r.register("w", "a") is True            # old name is free
    assert r.rename("v", "a") is False
    assert r.rename("v", "bad name") is False and r.name_of("v") == "b"


@pytest.mark.parametrize("name,valid", [
    ("a", True), ("0", True), ("-", True), ("a" * 16, True), ("A-1" * 5 + "Z", True),
    ("", False), ("a" * 17, False), ("a_b", False), ("a b", False), ("a.b", False),
    ("a\n", False), ("é", False), ("K", False), ("İ", False), (None, False), (5, False),
])
def test_validation_boundaries(name, valid):
    r = Registry(Store())
    assert r.register("u", name) is valid
    if valid:
        assert r.name_of("u") == name.lower()
        assert r.owner(name) == "u"
    else:
        assert r.name_of("u") is None and r.owner(name) is None
    # rename uses the same rule; an invalid target changes nothing
    r2 = Registry(Store())
    r2.register("v", "keep")
    assert r2.rename("v", name) is valid
    if valid:
        assert r2.name_of("v") == name.lower() and r2.owner("keep") is None
    else:
        assert r2.name_of("v") == "keep" and r2.owner("keep") == "v"


def test_invalid_does_not_free_or_take():
    r = Registry(Store())
    r.register("u", "x")
    assert r.rename("u", "x" * 17) is False
    assert r.name_of("u") == "x" and r.owner("x") == "u"
    assert r.register("w", "") is False and r.owner("") is None


def test_state_lives_in_store_not_object():
    s = Store()
    a, b = Registry(s), Registry(s)
    assert a.register("u", "n") is True
    assert b.owner("n") == "u" and b.name_of("u") == "n"
    assert b.register("v", "n") is False
    assert b.rename("u", "m") is True
    assert a.owner("n") is None and a.owner("m") == "u"
    assert a.register("v", "n") is True


# ---- forced interleavings (scenarios from DESIGN.md) ----

def test_two_users_read_same_free_name_one_wins():
    inner = Store()
    barrier = threading.Barrier(2, timeout=5)
    seen = set()
    lock = threading.Lock()

    def sync(key, result):
        if key == ("name", "n"):
            with lock:
                first = threading.get_ident() not in seen and len(seen) < 2
                seen.add(threading.get_ident())
            if first:
                barrier.wait()

    r = Registry(HookStore(inner, after_load=sync))
    with ThreadPoolExecutor(2) as ex:
        f1, f2 = ex.submit(r.register, "u1", "n"), ex.submit(r.register, "u2", "n")
        results = sorted([f1.result(timeout=10), f2.result(timeout=10)])
    assert results == [False, True]
    plain = Registry(inner)
    winner = plain.owner("n")
    assert winner in ("u1", "u2")
    assert plain.name_of(winner) == "n"
    loser = "u2" if winner == "u1" else "u1"
    assert plain.name_of(loser) is None


def test_competitor_registers_name_between_load_and_cas():
    inner = Store()
    other = Registry(inner)
    hook = once(lambda: other.register("rival", "n"), is_name("n"))
    r = Registry(HookStore(inner, after_load=hook))
    assert r.register("u", "n") is False           # must re-load, see the rival
    assert r.name_of("u") is None and other.owner("n") == "rival"
    assert other.name_of("rival") == "n"


def test_competitor_takes_rename_target_between_load_and_cas():
    inner = Store()
    other = Registry(inner)
    r = Registry(HookStore(inner, after_load=once(lambda: other.register("rival", "t"), is_name("t"))))
    r.register("u", "old")
    assert r.rename("u", "t") is False
    assert r.name_of("u") == "old" and r.owner("old") == "u"   # nothing changed
    assert r.owner("t") == "rival"


def test_competitor_rename_into_same_target_one_wins():
    inner = Store()
    other = Registry(inner)
    other.register("rival", "r0")
    base = Registry(inner)
    base.register("u", "u0")
    hook = once(lambda: other.rename("rival", "t"), is_name("t"))
    r = Registry(HookStore(inner, after_load=hook))
    assert r.rename("u", "t") is False
    assert base.owner("t") == "rival" and base.name_of("u") == "u0"
    assert base.owner("u0") == "u" and base.owner("r0") is None


def test_name_released_by_rename_is_claimable_and_not_double_owned():
    inner = Store()
    r = Registry(inner)
    r.register("u", "a")
    # v reads "a" as taken, then u renames away; v's earlier answer was False.
    hook = once(lambda: r.rename("u", "b"), is_name("a"))
    v = Registry(HookStore(inner, after_load=hook))
    assert v.register("v", "a") is False
    assert r.owner("a") is None and r.name_of("v") is None
    assert v.register("v", "a") is True
    check_consistent(r, ["u", "v"], ["a", "b"])


def test_swap_cycle_with_both_reading_before_either_writes():
    inner = Store()
    r = Registry(inner)
    r.register("u1", "a")
    r.register("u2", "b")
    barrier = threading.Barrier(2, timeout=5)
    waited = set()
    lock = threading.Lock()

    def sync(key, result):
        if key[0] == "name":
            with lock:
                first = threading.get_ident() not in waited
                waited.add(threading.get_ident())
            if first:
                barrier.wait()

    h = Registry(HookStore(inner, after_load=sync))
    with ThreadPoolExecutor(2) as ex:
        f1, f2 = ex.submit(h.rename, "u1", "b"), ex.submit(h.rename, "u2", "a")
        a, b = f1.result(timeout=10), f2.result(timeout=10)
    assert (a, b) == (False, False)               # each target is held by the other
    assert (r.name_of("u1"), r.name_of("u2")) == ("a", "b")
    assert (r.owner("a"), r.owner("b")) == ("u1", "u2")


def test_rename_to_own_name_after_losing_nothing():
    inner = Store()
    r = Registry(inner)
    r.register("u", "a")
    assert r.rename("u", "a") is True and r.owner("a") == "u"
    assert r.rename("u", "b") and r.rename("u", "a")
    assert r.owner("b") is None and r.owner("a") == "u" and r.name_of("u") == "a"


# ---- thread stress with end-state invariants ----

def test_stress_unique_and_consistent():
    inner = Store()
    users = [f"u{i}" for i in range(8)]
    names = [f"n{i}" for i in range(5)]
    errors = []

    def worker(u, seed):
        rnd = random.Random(seed)
        r = Registry(inner)
        try:
            for _ in range(300):
                n = rnd.choice(names)
                if rnd.random() < 0.5:
                    r.register(u, n)
                else:
                    r.rename(u, n)
                r.owner(rnd.choice(names))
        except Exception as e:  # pragma: no cover
            errors.append(e)

    threads = [threading.Thread(target=worker, args=(u, i)) for i, u in enumerate(users)]
    [t.start() for t in threads]
    [t.join() for t in threads]
    assert not errors
    r = Registry(inner)
    check_consistent(r, users, names)
    held = [r.name_of(u) for u in users if r.name_of(u)]
    assert len(held) == len(set(held))
