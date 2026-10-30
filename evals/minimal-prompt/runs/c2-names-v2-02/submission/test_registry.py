import threading
from concurrent.futures import ThreadPoolExecutor

import pytest
from snappea import check_events

from runtime import Store
from solution import Registry


class Hook:
    """Store wrapper: run callbacks right after the n-th matching load, and/or
    make loads of a key wait on a barrier so two callers read the same version."""

    def __init__(self, inner):
        self.inner = inner
        self.after_load = []   # (predicate(key), callback) one-shot
        self.barrier_keys = {}  # key -> Barrier
        self.waited = {}
        self.limit = {}   # key -> how many loads of it meet at the barrier

    def load(self, key):
        result = self.inner.load(key)
        b = self.barrier_keys.get(key)
        if b is not None and self.waited.get(key, 0) < self.limit.get(key, b.parties):
            self.waited[key] = self.waited.get(key, 0) + 1   # only the first reads meet
            b.wait(timeout=5)
        for i, (pred, cb) in enumerate(self.after_load):
            if pred(key):
                del self.after_load[i]
                cb()
                break
        return result

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


def consistent(reg, users, names):
    for u in users:
        n = reg.name_of(u)
        if n is not None:
            assert reg.owner(n) == u
    for n in names:
        o = reg.owner(n)
        if o is not None:
            assert reg.name_of(o) == n


# ---- sequential contract ----

def test_basic_and_case():
    r = Registry(Store())
    assert r.register("a", "Bob")
    assert r.owner("BOB") == "a" and r.name_of("a") == "bob"
    assert not r.register("b", "bOb")
    assert r.register("a", "BOB")           # same name again
    assert not r.register("a", "other")      # already has a name
    assert r.name_of("a") == "bob" and r.owner("other") is None
    assert r.name_of("nobody") is None and r.owner("zzz") is None


def test_rename():
    r = Registry(Store())
    assert not r.rename("a", "x")            # no name yet
    assert r.name_of("a") is None and r.owner("x") is None
    r.register("a", "one")
    r.register("b", "two")
    assert not r.rename("a", "TWO")
    assert r.name_of("a") == "one" and r.owner("one") == "a" and r.owner("two") == "b"
    assert r.rename("a", "ONE")              # own name
    assert r.rename("a", "three")
    assert r.owner("one") is None and r.owner("three") == "a"
    assert r.register("c", "one")            # old name is free
    assert r.rename("b", "one") is False


def test_validation_boundaries():
    r = Registry(Store())
    assert r.register("a", "x" * 16)
    assert not r.register("b", "x" * 17)
    assert not r.register("b", "")
    assert r.register("b", "-")
    assert r.register("c", "a-9")
    for bad in ["a b", "a_b", "é", "a\n", "K", "ａ", "a.b", None, 5]:
        assert not r.register("d", bad), bad
        assert r.owner(bad) is None
        assert r.name_of("d") is None
    assert not r.rename("a", "y" * 17) and r.name_of("a") == "x" * 16
    assert not r.rename("a", "bad name") and r.name_of("a") == "x" * 16
    assert r.owner("y" * 17) is None and r.owner("") is None


def test_invalid_rename_keeps_name():
    r = Registry(Store())
    r.register("a", "ok")
    assert not r.rename("a", "NOT OK")
    assert r.name_of("a") == "ok" and r.owner("ok") == "a"


def test_two_registries_share_store():
    s = Store()
    r1, r2 = Registry(s), Registry(s)
    assert r1.register("a", "n")
    assert not r2.register("b", "N")
    assert r2.owner("n") == "a" and r2.name_of("a") == "n"
    assert r2.rename("a", "m") and r1.register("b", "n")


def test_state_lives_in_store():
    s = Store()
    Registry(s).register("a", "n")
    fresh = Registry(s)
    assert fresh.name_of("a") == "n" and fresh.owner("n") == "a"


# ---- forced interleavings (scenarios from DESIGN.md) ----

def test_two_register_same_name_read_same_version():
    h = Hook(Store())
    h.barrier_keys[("name", "n")] = threading.Barrier(2)
    ra, rb = Registry(h), Registry(h)
    with ThreadPoolExecutor(2) as ex:
        fa = ex.submit(ra.register, "a", "n")
        fb = ex.submit(rb.register, "b", "n")
        results = sorted([fa.result(10), fb.result(10)])
    assert results == [False, True]           # exactly one winner
    winner = ra.owner("n")
    assert winner in ("a", "b") and ra.name_of(winner) == "n"
    loser = "b" if winner == "a" else "a"
    assert ra.name_of(loser) is None
    consistent(ra, "ab", ["n"])


def test_competitor_takes_name_between_read_and_claim():
    s = Store()
    h = Hook(s)
    other = Registry(s)
    r = Registry(h)
    h.after_load.append((lambda k: k == ("name", "n"), lambda: other.register("b", "n")))
    assert r.register("a", "n") is False      # must re-read after the CAS fails
    assert r.name_of("a") is None and r.owner("n") == "b"
    consistent(r, "ab", ["n"])


def test_rename_loses_race_for_new_name():
    s = Store()
    h = Hook(s)
    other = Registry(s)
    r = Registry(h)
    r.register("a", "old")
    h.after_load.append((lambda k: k == ("name", "new"), lambda: other.register("b", "new")))
    assert r.rename("a", "new") is False
    assert r.name_of("a") == "old" and r.owner("old") == "a" and r.owner("new") == "b"


def test_name_freed_while_claiming_is_taken():
    s = Store()
    other = Registry(s)
    other.register("b", "n")
    h = Hook(s)
    r = Registry(h)
    # the first read sees b holding n; rejection is correct at that time
    assert r.register("a", "n") is False
    other.rename("b", "m")
    assert r.register("a", "n") is True        # retry after the state changed
    assert r.owner("m") == "b" and r.owner("n") == "a"


def test_name_freed_and_retaken_between_read_and_cas():
    s = Store()
    other = Registry(s)
    other.register("b", "n")
    other.rename("b", "m")                      # name:n is free at version 2
    h = Hook(s)
    r = Registry(h)
    # after a reads name:n (free), b takes it again then c... a's CAS must fail
    h.after_load.append((lambda k: k == ("name", "n"), lambda: other.rename("b", "n")))
    assert r.register("a", "n") is False
    assert r.owner("n") == "b" and r.name_of("a") is None


def test_two_renames_to_each_others_old_names():
    s = Store()
    r = Registry(s)
    r.register("a", "x")
    r.register("b", "y")
    h = Hook(s)
    both = threading.Barrier(2)       # a's claim read of y meets b's claim read of x
    for k in (("name", "x"), ("name", "y")):
        h.barrier_keys[k] = both
        h.limit[k] = 1
    h.waited = {("name", "x"): 0, ("name", "y"): 0}
    ra, rb = Registry(h), Registry(h)
    with ThreadPoolExecutor(2) as ex:
        fa = ex.submit(ra.rename, "a", "y")
        fb = ex.submit(rb.rename, "b", "x")
        res = (fa.result(10), fb.result(10))
    assert res == (False, False)
    assert r.name_of("a") == "x" and r.name_of("b") == "y"
    consistent(r, "ab", ["x", "y"])


def test_rename_chain_frees_old_name_for_another():
    s = Store()
    r = Registry(s)
    r.register("a", "x")
    assert r.rename("a", "y")
    assert r.register("b", "x")
    assert r.rename("a", "x") is False
    assert r.name_of("a") == "y" and r.owner("y") == "a" and r.owner("x") == "b"


def test_concurrent_stress_invariant():
    s = Store()
    names = ["n%d" % i for i in range(4)]
    users = ["u%d" % i for i in range(6)]

    def work(u):
        reg = Registry(s)
        for i in range(200):
            n = names[(i * 7 + hash(u)) % 4]
            if i % 3 == 0:
                reg.register(u, n)
            else:
                reg.rename(u, n)

    with ThreadPoolExecutor(6) as ex:
        list(ex.map(work, users))
    r = Registry(s)
    consistent(r, users, names)
    held = [r.name_of(u) for u in users if r.name_of(u)]
    assert len(held) == len(set(held))


# ---- observation against the P spec ----

class Recorder:
    """Emits the spec's events at the real store's commit points."""

    NAMES = {"a": 0, "b": 1}
    USERS = {"u0": 0, "u1": 1}

    def __init__(self):
        self.inner = Store()
        self.events = []

    def load(self, key):
        return self.inner.load(key)

    def compare_swap(self, key, version, value):
        _, prev = self.inner.load(key)
        ok = self.inner.compare_swap(key, version, value)
        if ok:
            kind, k = key
            if kind == "name" and value is not None:
                self.ev("eClaimed", user=self.USERS[value], name=self.NAMES[k])
            elif kind == "name":
                self.ev("eReleased", user=self.USERS[prev], name=self.NAMES[k])
            else:
                self.ev("eCommitted", user=self.USERS[k], name=self.NAMES[value])
        return ok

    def ev(self, evname, **payload):
        self.events.append({"event": evname, "payload": payload})


class Observed:
    """Runs real calls and records the spec's op events around the commit events."""

    def __init__(self):
        self.rec = Recorder()
        self.reg = Registry(self.rec)

    def op(self, kind, user, name):
        reg, rec = self.reg, self.rec
        cur = reg.name_of(user)
        holder = reg.owner(name)
        # the call probes the name only when it reaches the claim step
        probes = holder not in (None, user) and (
            (kind == 0 and cur is None) or (kind == 1 and cur not in (None, name)))
        ok = reg.register(user, name) if kind == 0 else reg.rename(user, name)
        if probes:
            rec.ev("eClaimRejected", user=Recorder.USERS[user], name=Recorder.NAMES[name])
        rec.ev("eOpReturned", user=Recorder.USERS[user], kind=kind,
               name=Recorder.NAMES[name], ok=ok)
        return ok

    def finish(self):
        rec, reg = self.rec, self.reg
        rec.ev("eClientDone")
        rec.ev("eClientDone")
        for u, ui in Recorder.USERS.items():
            owners = [reg.owner(x) for x in ("a", "b")]
            rec.ev("eFinal", user=ui,
                   ownerOf0=Recorder.USERS.get(owners[0], -1),
                   ownerOf1=Recorder.USERS.get(owners[1], -1),
                   nameOf=Recorder.NAMES.get(reg.name_of(u), -1))
        check_events(rec.events, "UniqueNames", spec_files=["Names.p"], strict=True)


def test_observation_register_rename():
    o = Observed()
    assert o.op(0, "u0", "a")
    assert o.op(0, "u1", "a") is False
    assert o.op(1, "u0", "b")
    assert o.op(0, "u1", "a")
    assert o.op(0, "u1", "a")               # holds it already
    assert o.op(1, "u1", "a")               # rename to own name
    o.finish()


def test_observation_rejections():
    o = Observed()
    assert o.op(1, "u0", "a") is False       # rename without a name
    assert o.op(0, "u0", "a") and o.op(0, "u1", "b")
    assert o.op(1, "u1", "a") is False       # new name held
    assert o.op(0, "u0", "b") is False       # already has another name
    o.finish()


def test_observation_spec_rejects_bad_history():
    o = Observed()
    o.op(0, "u0", "a")
    o.rec.ev("eClientDone")
    o.rec.ev("eClientDone")
    o.rec.ev("eFinal", user=0, ownerOf0=0, ownerOf1=-1, nameOf=1)   # wrong read
    o.rec.ev("eFinal", user=1, ownerOf0=0, ownerOf1=-1, nameOf=-1)
    with pytest.raises(Exception):
        check_events(o.rec.events, "UniqueNames", spec_files=["Names.p"], strict=True)
