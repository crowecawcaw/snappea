import random
import threading

from runtime import Store
from solution import Registry


def test_basic():
    r = Registry(Store())
    assert r.register("u1", "Bob") and r.name_of("u1") == "bob"
    assert r.owner("BOB") == "u1"
    assert not r.register("u2", "bob")
    assert r.register("u1", "bob")
    assert not r.register("u1", "other") and r.name_of("u1") == "bob"
    assert not r.rename("u2", "x")
    assert r.register("u2", "x")
    assert not r.rename("u2", "bob") and r.name_of("u2") == "x"
    assert r.rename("u1", "bob") and r.rename("u1", "y")
    assert r.owner("bob") is None and r.owner("y") == "u1"
    assert r.register("u3", "bob")


def test_invalid():
    r = Registry(Store())
    for bad in ["", "a" * 17, "a_b", "é", "a b", None]:
        assert not r.register("u", bad)
        assert r.owner(bad) is None
    assert r.name_of("u") is None
    assert r.register("u", "a" * 16)
    assert not r.rename("u", "bad!") and r.name_of("u") == "a" * 16


def test_concurrent_stress():
    store = Store()
    users = [f"u{i}" for i in range(6)]
    names = ["a", "B", "c", "d"]

    def work(u, seed):
        rnd = random.Random(seed)
        r = Registry(store)
        for _ in range(300):
            n = rnd.choice(names)
            (r.register if rnd.random() < 0.3 else r.rename)(u, n)

    ts = [threading.Thread(target=work, args=(u, i)) for i, u in enumerate(users)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    r = Registry(store)
    held = {}
    for u in users:
        n = r.name_of(u)
        if n is not None:
            assert n not in held
            held[n] = u
    for n in ["a", "b", "c", "d"]:
        assert r.owner(n) == held.get(n)


def test_contended_register_one_winner():
    for _ in range(50):
        store = Store()
        res = []
        ts = [threading.Thread(target=lambda u=u: res.append(Registry(store).register(u, "x")))
              for u in "abcd"]
        [t.start() for t in ts]
        [t.join() for t in ts]
        assert res.count(True) == 1
