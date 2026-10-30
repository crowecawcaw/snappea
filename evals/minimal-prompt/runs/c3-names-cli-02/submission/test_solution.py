import random
import threading
from concurrent.futures import ThreadPoolExecutor

from runtime import Store
from solution import Registry


class Interfere:
    """Run `competitor` once, right after the first load of `key`, before the caller's CAS."""
    def __init__(self, inner, key, competitor):
        self.inner, self.key, self.competitor = inner, key, competitor

    def load(self, key):
        r = self.inner.load(key)
        if key == self.key and self.competitor:
            run, self.competitor = self.competitor, None
            run()
        return r

    def compare_swap(self, *a):
        return self.inner.compare_swap(*a)


class SameVersion:
    """The first two loads of `key` wait for each other, so both read the same version."""
    def __init__(self, inner, key):
        self.inner, self.key = inner, key
        self.barrier, self.left, self.lock = threading.Barrier(2, timeout=5), 2, threading.Lock()

    def load(self, key):
        r = self.inner.load(key)
        if key == self.key:
            with self.lock:
                wait, self.left = self.left > 0, self.left - 1
            if wait:
                self.barrier.wait()
        return r

    def compare_swap(self, *a):
        return self.inner.compare_swap(*a)


def consistent(reg, users, names):
    for n in names:
        o = reg.owner(n)
        assert o is None or reg.name_of(o) == n
    for u in users:
        n = reg.name_of(u)
        assert n is None or reg.owner(n) == u


def test_basic_and_case():
    r = Registry(Store())
    assert r.register("u", "Bob") is True
    assert r.name_of("u") == "bob" and r.owner("BOB") == "u"
    assert r.register("u", "bob") is True       # same name again
    assert r.register("u", "alice") is False    # already has a name
    assert r.register("v", "BOB") is False      # taken
    assert r.register("v", "alice") is True
    assert r.owner("zed") is None and r.name_of("w") is None


def test_invalid_names():
    r = Registry(Store())
    for bad in ["", "a" * 17, "a_b", "a b", "é", "a.b", None, 5]:
        assert r.register("u", bad) is False
        assert r.owner(bad) is None
    assert r.name_of("u") is None
    assert r.register("u", "a" * 16) is True
    assert r.register("v", "0-9") is True
    assert r.rename("u", "x" * 17) is False and r.name_of("u") == "a" * 16
    assert r.rename("u", "bad!") is False and r.name_of("u") == "a" * 16


def test_rename():
    r = Registry(Store())
    assert r.rename("u", "x") is False          # no name yet
    assert r.name_of("u") is None and r.owner("x") is None
    r.register("u", "a"); r.register("v", "b")
    assert r.rename("u", "A") is True           # own name
    assert r.rename("u", "b") is False          # held by another
    assert r.name_of("u") == "a" and r.owner("a") == "u" and r.owner("b") == "v"
    assert r.rename("u", "c") is True
    assert r.owner("a") is None and r.owner("c") == "u" and r.name_of("u") == "c"
    assert r.register("w", "a") is True         # old name is free
    assert r.rename("u", "A") is False          # now taken


def test_two_users_same_version_register():
    inner = Store()
    s = SameVersion(inner, ("name", "n"))
    with ThreadPoolExecutor(2) as ex:
        fs = [ex.submit(Registry(s).register, u, "n") for u in ("a", "b")]
        res = sorted(f.result() for f in fs)
    assert res == [False, True]
    reg = Registry(inner)
    consistent(reg, "ab", ["n"])
    assert reg.owner("n") in ("a", "b")


def test_competitor_takes_name_between_load_and_cas():
    inner = Store()
    s = Interfere(inner, ("name", "n"), lambda: Registry(inner).register("other", "n"))
    assert Registry(s).register("me", "n") is False
    assert Registry(inner).name_of("me") is None
    assert Registry(inner).owner("n") == "other"


def test_competitor_registers_name_during_rename():
    inner = Store()
    reg = Registry(inner)
    reg.register("me", "old")
    s = Interfere(inner, ("name", "new"), lambda: Registry(inner).register("other", "new"))
    assert Registry(s).rename("me", "new") is False
    assert reg.name_of("me") == "old" and reg.owner("old") == "me" and reg.owner("new") == "other"


def test_old_name_taken_right_after_rename_frees_it():
    inner = Store()
    reg = Registry(inner)
    reg.register("me", "old")
    assert reg.rename("me", "new")
    assert Registry(inner).register("x", "old") is True
    assert reg.rename("me", "old") is False
    consistent(reg, ["me", "x"], ["old", "new"])
    assert reg.owner("new") == "me"


def test_swap_names_concurrently_never_duplicates():
    # a: x->y, b: y->x at once; at most one may succeed and state stays consistent.
    for _ in range(50):
        inner = Store()
        reg = Registry(inner)
        reg.register("a", "x"); reg.register("b", "y")
        with ThreadPoolExecutor(2) as ex:
            f1 = ex.submit(Registry(inner).rename, "a", "y")
            f2 = ex.submit(Registry(inner).rename, "b", "x")
            f1.result(); f2.result()
        consistent(reg, "ab", ["x", "y"])
        assert reg.name_of("a") != reg.name_of("b")


def test_stress_threads():
    inner = Store()
    users = [f"u{i}" for i in range(6)]
    names = ["a", "b", "c", "d"]

    def work(u):
        rnd = random.Random(u)
        reg = Registry(inner)
        for _ in range(300):
            n = rnd.choice(names)
            rnd.choice([reg.register, reg.rename])(u, n)
    with ThreadPoolExecutor(6) as ex:
        list(ex.map(work, users))
    reg = Registry(inner)
    consistent(reg, users, names)
    held = [reg.name_of(u) for u in users if reg.name_of(u)]
    assert len(held) == len(set(held)) <= len(names)


def test_p_spec_sequential():
    from snappea import check_events
    reg = Registry(Store())
    # names a=0,b=1 ; users 0,1 map to "u0","u1" (see Names.p eResult)
    steps = [(0, 0, 0), (1, 0, 0), (0, 1, 1), (1, 1, 1), (1, 2, 0), (0, 2, 1)]
    events = []
    for user, op, name in steps:
        fn = reg.rename if op == 1 else reg.register
        ok = fn(f"u{user}", "ab"[name])
        events.append({"event": "eResult", "payload": {"user": user, "op": op, "name": name, "ok": ok}})
    check_events(events, "Sequential", spec_files=["Names.p"], strict=True)
