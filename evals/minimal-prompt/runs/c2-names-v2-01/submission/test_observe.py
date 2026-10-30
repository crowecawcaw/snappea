"""Observation tests: run real Registry calls (with forced interleavings) and check the
recorded history against the P spec `Contract` in Spec.p (via snappea.check_events)."""
import threading

import pytest
from snappea import check_events

from runtime import Store
from solution import Registry

NAMES = {"a": 0, "b": 1}
USERS = {"u0": 0, "u1": 1, "u2": 2}


class Recorder:
    """Store wrapper: records every successful write as eWrote, in commit order, and
    lets a hook run after a load (to force a competing call between load and CAS)."""

    def __init__(self, events, after_load=None):
        self.inner, self.events, self.after_load = Store(), events, after_load
        self.lock = threading.Lock()

    @staticmethod
    def _code(key, value):
        if value is None:
            return -1
        return NAMES[value] if key[0] == "user" else USERS[value]

    @staticmethod
    def _key(key):
        return USERS[key[1]] if key[0] == "user" else 3 + NAMES[key[1]]

    def load(self, key):
        result = self.inner.load(key)
        if self.after_load:
            self.after_load(key, result)
        return result

    def compare_swap(self, key, version, value):
        with self.lock:
            old = self.inner.load(key)[1]
            ok = self.inner.compare_swap(key, version, value)
            if ok:
                self.events.append({"event": "eWrote", "payload": {
                    "key": self._key(key), "old": self._code(key, old), "nv": self._code(key, value)}})
            return ok


class Observed:
    """Registry wrapper recording each call's actual arguments and result."""

    def __init__(self, store, events):
        self.reg, self.events = Registry(store), events

    def _rec(self, user, is_reg, name, ok):
        self.events.append({"event": "eReturned", "payload": {
            "user": USERS[user], "reg": is_reg, "name": NAMES[name.lower()], "ok": ok}})
        return ok

    def register(self, user, name):
        return self._rec(user, True, name, self.reg.register(user, name))

    def rename(self, user, name):
        return self._rec(user, False, name, self.reg.rename(user, name))


def finish(events, rec):
    """Quiescent snapshot read through the public API, then check the whole history."""
    r = Registry(rec.inner)
    users = {i: (NAMES[r.name_of(u)] if r.name_of(u) else -1) for u, i in USERS.items()}
    names = {i: (USERS[r.owner(n)] if r.owner(n) else -1) for n, i in NAMES.items()}
    events.append({"event": "eSnapshot", "payload": {"users": users, "names": names}})
    events.append({"event": "eDone", "payload": None})
    check_events(events, "Contract", spec_files=["Spec.p"], strict=True)


def setup(hook=None):
    events = []
    rec = Recorder(events)
    obs = Observed(rec, events)
    if hook:
        rec.after_load = hook(obs)
    return events, rec, obs


def once(fn, match):
    state = {"done": False}

    def hook(key, result):
        if not state["done"] and match(key):
            state["done"] = True
            fn()
    return hook


def test_sequential_history():
    events, rec, obs = setup()
    assert obs.register("u0", "A") and not obs.register("u1", "a")
    assert not obs.register("u0", "b") and obs.register("u0", "a")
    assert obs.rename("u0", "b") and obs.register("u1", "a") and not obs.rename("u1", "b")
    assert not obs.rename("u2", "a") and obs.register("u2", "A") is False
    finish(events, rec)


def test_competitor_between_load_and_cas():
    events, rec, obs = setup(lambda o: once(lambda: o.register("u1", "a"), lambda k: k == ("name", "a")))
    assert obs.register("u0", "a") is False
    finish(events, rec)


def test_swap_race():
    events, rec, obs = setup()
    assert obs.register("u0", "a") and obs.register("u1", "b")
    rec.after_load = once(lambda: obs.rename("u1", "a"), lambda k: k == ("name", "b"))
    assert obs.rename("u0", "b") is False        # u1 holds b; u1's own swap also fails
    finish(events, rec)


def test_release_visible_to_later_claim():
    events, rec, obs = setup()
    assert obs.register("u0", "a")
    # u1 reads name a as taken; right after, u0 moves off it.
    rec.after_load = once(lambda: obs.rename("u0", "b"), lambda k: k == ("name", "a"))
    assert obs.register("u1", "a") is False
    assert obs.register("u1", "a") is True
    assert obs.rename("u0", "a") is False
    finish(events, rec)


def test_rename_target_taken_between_load_and_cas():
    events, rec, obs = setup()
    assert obs.register("u0", "a")
    rec.after_load = once(lambda: obs.register("u2", "b"), lambda k: k == ("name", "b"))
    assert obs.rename("u0", "b") is False
    assert obs.rename("u0", "a") is True
    finish(events, rec)


def test_observation_rejects_a_wrong_history():
    """The spec itself must flag a bad history (guards against a vacuous check)."""
    events = [
        {"event": "eWrote", "payload": {"key": 3, "old": -1, "nv": 0}},
        {"event": "eReturned", "payload": {"user": 1, "reg": True, "name": 0, "ok": True}},
    ]
    with pytest.raises(Exception):
        check_events(events, "Contract", spec_files=["Spec.p"], strict=True)
