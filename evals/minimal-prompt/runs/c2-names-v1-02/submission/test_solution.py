import random
import threading
from pathlib import Path

import pytest
from snappea import check_spec

import solution
from runtime import Store
from solution import Registry

SPEC = str(Path(__file__).with_name("NamesObs.p"))


def observed():
    return check_spec("NamesObs", spec_files=[SPEC], strict=True, check_hot=True,
                      unknown_events="error")


def test_register_rules_and_case():
    with observed() as rec:
        r = Registry(Store())
        assert r.register("u1", "Bob")
        assert r.owner("bob") == "u1" and r.owner("BOB") == "u1"
        assert r.name_of("u1") == "bob"
        assert r.register("u1", "bob")          # same name again
        assert not r.register("u1", "alice")    # user already has a name (name free)
        assert r.owner("alice") is None and r.name_of("u1") == "bob"
        assert not r.register("u2", "BOB")      # name held by other
        assert r.name_of("u2") is None and r.owner("bob") == "u1"
        assert r.register("u2", "alice")
        from snappea import emit
        emit("eQuiescent")
    assert rec.count("eClaimed") == 2


def test_invalid_names_change_nothing():
    r = Registry(Store())
    assert r.register("u1", "ok")
    for bad in ["", "a" * 17, "a_b", "a b", "é", "a.b", None, "x\n"]:
        assert not r.register("u2", bad)
        assert r.name_of("u2") is None
        assert not r.rename("u1", bad)
        assert r.name_of("u1") == "ok" and r.owner("ok") == "u1"
        assert r.owner(bad) is None
    assert r.register("u2", "a" * 16) and r.register("u3", "0-9")
    assert r.owner("A" * 16) == "u2"


def test_rename_rules():
    with observed():
        r = Registry(Store())
        assert not r.rename("u1", "x")           # no name yet
        assert r.name_of("u1") is None and r.owner("x") is None
        assert r.register("u1", "a") and r.register("u2", "b")
        assert r.rename("u1", "A")               # same name
        assert not r.rename("u1", "b")           # held by other
        assert r.name_of("u1") == "a" and r.owner("a") == "u1" and r.owner("b") == "u2"
        assert r.rename("u1", "c")
        assert r.owner("a") is None and r.owner("c") == "u1" and r.name_of("u1") == "c"
        assert r.register("u3", "a")             # old name freed
        assert r.rename("u2", "a") is False
        assert r.rename("u3", "B") is False      # b still held by u2
        from snappea import emit
        emit("eQuiescent")


def _invariant(r, users, names):
    for u in users:
        n = r.name_of(u)
        if n is not None:
            assert r.owner(n) == u
    for n in names:
        o = r.owner(n)
        if o is not None:
            assert r.name_of(o) == n


@pytest.mark.parametrize("seed", range(6))
def test_concurrent_stress_observed(seed):
    users = [f"u{i}" for i in range(6)]
    names = ["a", "b", "c", "d"]
    with observed():
        store = Store()
        regs = [Registry(store) for _ in users]  # many registries, one store
        def work(i):
            rnd = random.Random(seed * 100 + i)
            for _ in range(60):
                n = rnd.choice(names)
                if rnd.random() < 0.5:
                    regs[i].register(users[i], n)
                else:
                    regs[i].rename(users[i], n.upper())
        ts = [threading.Thread(target=work, args=(i,)) for i in range(len(users))]
        [t.start() for t in ts]; [t.join() for t in ts]
        _invariant(regs[0], users, names)
        held = [regs[0].name_of(u) for u in users if regs[0].name_of(u)]
        assert len(held) == len(set(held))
        from snappea import emit
        emit("eQuiescent")


def test_contended_register_exactly_one_wins():
    store = Store()
    users = [f"u{i}" for i in range(8)]
    res = {}
    bar = threading.Barrier(len(users))
    def go(u):
        bar.wait()
        res[u] = Registry(store).register(u, "same")
    ts = [threading.Thread(target=go, args=(u,)) for u in users]
    [t.start() for t in ts]; [t.join() for t in ts]
    winners = [u for u in users if res[u]]
    assert len(winners) == 1 and Registry(store).owner("same") == winners[0]
    assert all(Registry(store).name_of(u) is None for u in users if u != winners[0])


def test_swap_race_old_name_reusable():
    # u1 renames away from "a" while u2 tries to take "a": either order is legal,
    # never both holding a name that disagrees with owner().
    for _ in range(50):
        r = Registry(Store())
        assert r.register("u1", "a")
        out = {}
        t1 = threading.Thread(target=lambda: out.update(a=r.rename("u1", "b")))
        t2 = threading.Thread(target=lambda: out.update(b=r.register("u2", "a")))
        t1.start(); t2.start(); t1.join(); t2.join()
        assert out["a"] is True
        _invariant(r, ["u1", "u2"], ["a", "b"])
        assert r.name_of("u1") == "b"
        assert r.name_of("u2") == ("a" if out["b"] else None)


class BrokenRegistry(Registry):
    """Defect: release old name before claiming new (another user can steal it,
    and a failed claim loses the old name)."""
    def rename(self, user, new_name):
        new_name = solution._norm(new_name)
        version, current = self._store.load("u:" + user)
        if new_name is None or current is None:
            return False
        self._release(user, current)
        if not self._claim(user, new_name):
            return False
        self._set_user(user, version, new_name)
        return True


def test_broken_variant_is_caught():
    r = BrokenRegistry(Store())
    assert r.register("u1", "a") and r.register("u2", "b")
    assert not r.rename("u1", "b")
    with pytest.raises(AssertionError):  # the defect loses u1's old name
        assert r.name_of("u1") == "a" and r.owner("a") == "u1"


def test_broken_variant_violates_monitor():
    from snappea import SpecViolation, emit
    with pytest.raises(SpecViolation):
        with observed():
            r = BrokenRegistry(Store())
            r.register("u1", "a"); r.register("u2", "b")
            r.rename("u1", "b")
            emit("eQuiescent")
