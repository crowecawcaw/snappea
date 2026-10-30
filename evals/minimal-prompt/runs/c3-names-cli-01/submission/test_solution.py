import threading
from concurrent.futures import ThreadPoolExecutor

import pytest
from runtime import Store
from solution import Registry
from snappea import check_events


class Interfere:
    """Runs `competitor` once, right after the first load of a key matching `when`."""

    def __init__(self, inner, competitor, when=lambda key: True):
        self.inner, self.competitor, self.when = inner, competitor, when

    def load(self, key):
        r = self.inner.load(key)
        if self.competitor and self.when(key):
            run, self.competitor = self.competitor, None
            run()
        return r

    def compare_swap(self, *a):
        return self.inner.compare_swap(*a)


def consistent(reg, users, names):
    for u in users:
        n = reg.name_of(u)
        if n is not None:
            assert reg.owner(n) == u
    for n in names:
        o = reg.owner(n)
        if o is not None:
            assert reg.name_of(o) == n


def test_basic():
    r = Registry(Store())
    assert r.register("a", "Bob") is True
    assert r.owner("bob") == "a" and r.owner("BOB") == "a" and r.name_of("a") == "bob"
    assert r.register("a", "bob") is True
    assert r.register("a", "other") is False
    assert r.register("b", "BOB") is False
    assert r.name_of("b") is None
    assert r.owner("zzz") is None


def test_invalid_names():
    r = Registry(Store())
    for bad in ["", "a" * 17, "a b", "a_b", "é", "a\n", "K", None, 5]:
        assert r.register("u", bad) is False
        assert r.owner(bad) is None
    assert r.name_of("u") is None
    assert r.register("u", "a" * 16) is True
    assert r.register("v", "0-9") is True
    assert r.rename("u", "bad name") is False and r.name_of("u") == "a" * 16


def test_rename():
    r = Registry(Store())
    assert r.rename("a", "x") is False
    r.register("a", "x")
    r.register("b", "y")
    assert r.rename("a", "y") is False
    assert r.name_of("a") == "x" and r.owner("x") == "a" and r.owner("y") == "b"
    assert r.rename("a", "X") is True
    assert r.rename("a", "z") is True
    assert r.owner("x") is None and r.owner("z") == "a" and r.name_of("a") == "z"
    assert r.register("b", "x") is False  # b holds y
    assert r.rename("b", "x") is True
    assert r.owner("y") is None


def test_second_object_shares_store():
    s = Store()
    Registry(s).register("a", "n")
    r2 = Registry(s)
    assert r2.owner("n") == "a" and r2.register("b", "n") is False


def test_competitor_claims_between_load_and_swap_register():
    s = Store()
    sneaky = Interfere(s, lambda: Registry(s).register("other", "n"), lambda k: k == ("n", "n"))
    assert Registry(sneaky).register("me", "n") is False
    assert Registry(s).owner("n") == "other" and Registry(s).name_of("me") is None


def test_competitor_claims_between_load_and_swap_rename():
    s = Store()
    r = Registry(s)
    r.register("me", "old")
    sneaky = Interfere(s, lambda: r.register("other", "new"), lambda k: k == ("n", "new"))
    assert Registry(sneaky).rename("me", "new") is False
    assert r.name_of("me") == "old" and r.owner("old") == "me" and r.owner("new") == "other"


def test_old_name_reusable_after_rename():
    s = Store()
    r = Registry(s)
    r.register("a", "n")
    r.rename("a", "m")
    assert r.register("b", "n") is True
    assert r.rename("a", "n") is False
    consistent(r, "ab", ["n", "m"])


def test_same_name_race_one_winner():
    for _ in range(50):
        s = Store()
        barrier = threading.Barrier(8)

        def go(i):
            barrier.wait()
            return Registry(s).register(f"u{i}", "name")

        with ThreadPoolExecutor(8) as ex:
            res = list(ex.map(go, range(8)))
        assert res.count(True) == 1
        assert sum(Registry(s).name_of(f"u{i}") == "name" for i in range(8)) == 1
        consistent(Registry(s), [f"u{i}" for i in range(8)], ["name"])


def test_rename_swap_race():
    for _ in range(100):
        s = Store()
        r = Registry(s)
        r.register("a", "x")
        r.register("b", "y")
        barrier = threading.Barrier(2)

        def go(u, n):
            barrier.wait()
            return Registry(s).rename(u, n)

        with ThreadPoolExecutor(2) as ex:
            f1, f2 = ex.submit(go, "a", "y"), ex.submit(go, "b", "x")
            f1.result(), f2.result()
        consistent(r, "ab", ["x", "y"])
        assert r.name_of("a") != r.name_of("b")


def test_stress_invariants():
    s = Store()
    users = [f"u{i}" for i in range(6)]
    names = [f"n{i}" for i in range(4)]

    def work(u):
        r = Registry(s)
        import random
        rnd = random.Random(u)
        for _ in range(300):
            n = rnd.choice(names)
            (r.register if rnd.random() < 0.5 else r.rename)(u, n)

    with ThreadPoolExecutor(6) as ex:
        list(ex.map(work, users))
    r = Registry(s)
    consistent(r, users, names)
    held = [r.name_of(u) for u in users if r.name_of(u)]
    assert len(held) == len(set(held))


def test_observed_events_match_spec():
    r = Registry(Store())
    ev = []

    def rec(op, user, name, ok):
        ev.append({"event": "eRes", "payload": {"op": op, "user": user, "name": name, "ok": ok}})

    # model names/users are ints 0/1
    names = {"a": 0, "b": 1}
    for op, u, n in [(0, 0, "a"), (0, 0, "b"), (1, 0, "b"), (0, 1, "b"), (1, 1, "a"), (1, 0, "a"), (0, 1, "a")]:
        f = r.register if op == 0 else r.rename
        rec(op, u, names[n], f(f"u{u}", n))
    check_events(ev, "Exact", spec_files=["Names.p"], strict=True)
