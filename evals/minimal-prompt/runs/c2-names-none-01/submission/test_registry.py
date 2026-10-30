import random
import threading

import pytest

from runtime import Store
from solution import Registry


def test_basic():
    r = Registry(Store())
    assert r.register("a", "Bob")
    assert r.owner("bob") == "a" and r.owner("BOB") == "a"
    assert r.name_of("a") == "bob"
    assert r.register("a", "bob")
    assert not r.register("a", "other")
    assert not r.register("b", "BOB")
    assert r.name_of("b") is None
    assert r.register("b", "x")
    assert not r.rename("b", "bob")
    assert r.rename("b", "bob") is False and r.name_of("b") == "x"
    assert r.rename("a", "BOB")
    assert r.rename("a", "z")
    assert r.owner("bob") is None
    assert r.rename("b", "bob") and r.owner("x") is None
    assert not r.rename("c", "q")  # no name yet


@pytest.mark.parametrize("bad", ["", "a" * 17, "a_b", "a b", "é", "a.b", None])
def test_invalid(bad):
    r = Registry(Store())
    r.register("a", "ok")
    assert not r.register("b", bad)
    assert not r.rename("a", bad)
    assert r.owner(bad) is None
    assert r.name_of("a") == "ok" and r.name_of("b") is None
    assert r.register("c", "a" * 16) and r.register("d", "0-9")


def test_threads_consistent():
    store = Store()
    users = [f"u{i}" for i in range(8)]
    names = [f"n{i}" for i in range(5)]
    errors = []

    def work(u, seed):
        rnd = random.Random(seed)
        reg = Registry(store)
        for _ in range(300):
            n = rnd.choice(names + [n.upper() for n in names])
            (reg.register if rnd.random() < 0.4 else reg.rename)(u, n)

    ts = [threading.Thread(target=work, args=(u, i)) for i, u in enumerate(users)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    reg = Registry(store)
    held = {u: reg.name_of(u) for u in users}
    vals = [n for n in held.values() if n]
    assert len(vals) == len(set(vals))
    for u, n in held.items():
        if n:
            assert reg.owner(n) == u
    for n in names:
        o = reg.owner(n)
        assert o is None or held[o] == n


def test_spec_sees_events_under_threads():
    from snappea import check_spec

    store = Store()
    with check_spec("UniqueNames", strict=True):
        ts = [threading.Thread(target=lambda u=u: [
            (Registry(store).register(u, "a"), Registry(store).rename(u, "b"),
             Registry(store).rename(u, "A")) for _ in range(50)]) for u in "wxyz"]
        [t.start() for t in ts]
        [t.join() for t in ts]


def test_spec_catches_duplicate_holder():
    from snappea import check_events

    with pytest.raises(AssertionError):
        check_events([
            {"event": "eHold", "user": "a", "name": "x"},
            {"event": "eHold", "user": "b", "name": "x"},
        ], "UniqueNames")
