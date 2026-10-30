"""Hidden implementation tests. Race tests force overlapping calls by different users."""
import threading
from concurrent.futures import ThreadPoolExecutor

from runtime import Store
from solution import Registry


class SameVersionStore:
    """The first two loads of an armed key wait for each other."""

    def __init__(self, inner, key_part):
        self.inner, self.key_part = inner, key_part
        self.barrier, self.waiting, self.lock = threading.Barrier(2, timeout=2), 2, threading.Lock()

    def load(self, key):
        result = self.inner.load(key)
        with self.lock:
            wait = self.waiting > 0 and self.key_part in str(key)
            self.waiting -= wait
        if wait:
            try:
                self.barrier.wait()
            except threading.BrokenBarrierError:
                pass
        return result

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


class InterferingStore:
    """Runs `hook` once, right after the first load of a matching key."""

    def __init__(self, inner, key_part, hook):
        self.inner, self.key_part, self.hook = inner, key_part, hook

    def load(self, key):
        result = self.inner.load(key)
        if self.hook and self.key_part in str(key):
            hook, self.hook = self.hook, None
            hook()
        return result

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


def consistent(reg, users, names):
    for u in users:
        n = reg.name_of(u)
        if n is not None:
            assert reg.owner(n) == u, (u, n)
    for n in names:
        o = reg.owner(n)
        if o is not None:
            assert reg.name_of(o) == n, (n, o)


def test_register_and_lookup():
    reg = Registry(Store())
    assert reg.register("ann", "ann1") is True
    assert reg.owner("ann1") == "ann" and reg.name_of("ann") == "ann1"
    assert reg.owner("nobody") is None and reg.name_of("bob") is None
    assert reg.register("bob", "ann1") is False
    consistent(reg, ["ann", "bob"], ["ann1"])


def test_case_insensitive():
    reg = Registry(Store())
    assert reg.register("ann", "Bob-1") is True
    assert reg.name_of("ann") == "bob-1" and reg.owner("BOB-1") == "ann"
    assert reg.register("bob", "bob-1") is False
    assert reg.register("ann", "BOB-1") is True


def test_name_validity():
    reg = Registry(Store())
    assert reg.register("a", "x" * 16) is True
    assert reg.register("b", "y" * 17) is False
    assert reg.register("c", "") is False
    assert reg.register("d", "under_score") is False
    assert reg.owner("y" * 17) is None
    assert reg.rename("a", "z" * 17) is False and reg.name_of("a") == "x" * 16


def test_register_rules():
    reg = Registry(Store())
    assert reg.register("ann", "a")
    assert reg.register("ann", "a") is True
    assert reg.register("ann", "b") is False
    assert reg.name_of("ann") == "a" and reg.owner("b") is None
    assert reg.register("bob", "b") is True
    consistent(reg, ["ann", "bob"], ["a", "b"])


def test_rename_frees_old_name():
    store = Store()
    reg = Registry(store)
    reg.register("ann", "old")
    assert reg.rename("ann", "new") is True
    assert reg.name_of("ann") == "new" and reg.owner("new") == "ann" and reg.owner("old") is None
    assert Registry(store).register("bob", "old") is True
    consistent(reg, ["ann", "bob"], ["old", "new"])


def test_rename_to_taken_changes_nothing():
    reg = Registry(Store())
    reg.register("ann", "a"), reg.register("bob", "b")
    assert reg.rename("ann", "b") is False
    assert reg.name_of("ann") == "a" and reg.owner("a") == "ann" and reg.owner("b") == "bob"


def test_rename_to_own_name():
    reg = Registry(Store())
    reg.register("ann", "a")
    assert reg.rename("ann", "A") is True
    assert reg.owner("a") == "ann" and reg.name_of("ann") == "a"
    assert reg.register("bob", "a") is False


def test_rename_without_name():
    reg = Registry(Store())
    assert reg.rename("ann", "a") is False
    assert reg.name_of("ann") is None and reg.owner("a") is None
    assert reg.register("bob", "a") is True


def test_racing_registers_one_winner():
    store = SameVersionStore(Store(), "x")
    a, b = Registry(store), Registry(store)
    with ThreadPoolExecutor(2) as pool:
        fa, fb = pool.submit(a.register, "ann", "x"), pool.submit(b.register, "bob", "x")
        results = [fa.result(timeout=5), fb.result(timeout=5)]
    assert sorted(results) == [False, True]
    winner = "ann" if results[0] else "bob"
    assert a.owner("x") == winner
    consistent(a, ["ann", "bob"], ["x"])


def test_register_wins_between_rename_read_and_write():
    inner = Store()
    Registry(inner).register("ann", "a")
    sneaky = InterferingStore(inner, "x", lambda: Registry(inner).register("bob", "x"))
    assert Registry(sneaky).rename("ann", "x") is False
    reg = Registry(inner)
    assert reg.owner("x") == "bob" and reg.name_of("ann") == "a" and reg.owner("a") == "ann"
