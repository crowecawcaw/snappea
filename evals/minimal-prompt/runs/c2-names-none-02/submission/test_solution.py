import random
import subprocess
import threading
from pathlib import Path

from runtime import Store
from solution import Registry


def test_model_checks():
    """The P model of the protocol passes (skipped if snappea is not on PATH)."""
    try:
        r = subprocess.run(["snappea", "check", "--no-mutation", "--format", "text"],
                           cwd=Path(__file__).parent / "model", capture_output=True, text=True)
    except FileNotFoundError:
        return
    assert r.returncode == 0, r.stdout + r.stderr


def test_basics():
    r = Registry(Store())
    assert r.register("u1", "Bob")
    assert r.owner("BOB") == "u1" and r.name_of("u1") == "bob"
    assert r.register("u1", "bob")
    assert not r.register("u1", "alice")
    assert not r.register("u2", "bob")
    assert r.register("u2", "al-1")
    assert not r.rename("u2", "bob")
    assert r.rename("u2", "bob2") and r.owner("al-1") is None
    assert r.rename("u2", "BOB2")
    assert not r.rename("u3", "zed")
    assert r.rename("u1", "al-1") and r.register("u3", "bob")


def test_invalid():
    r = Registry(Store())
    for bad in ["", "a" * 17, "a b", "a_b", "é", "a.b"]:
        assert not r.register("u", bad)
        assert r.owner(bad) is None
    assert r.name_of("u") is None
    assert r.register("u", "a" * 16)
    assert not r.rename("u", "bad name")
    assert r.name_of("u") == "a" * 16


def test_concurrent_stress():
    store = Store()
    names = ["n%d" % i for i in range(6)]
    users = ["u%d" % i for i in range(8)]

    def work(user, seed):
        rnd = random.Random(seed)
        reg = Registry(store)
        for _ in range(300):
            n = rnd.choice(names).upper() if rnd.random() < .3 else rnd.choice(names)
            (reg.register if rnd.random() < .4 else reg.rename)(user, n)

    ts = [threading.Thread(target=work, args=(u, i)) for i, u in enumerate(users)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    reg = Registry(store)
    held = {u: reg.name_of(u) for u in users if reg.name_of(u)}
    assert len(set(held.values())) == len(held)
    for u, n in held.items():
        assert reg.owner(n) == u
    for n in names:
        o = reg.owner(n)
        assert o is None or held[o] == n
