"""Unique usernames on a per-key compare-and-swap store.

Records: ("name", n) -> holder or None; ("user", u) -> name or None.
The name record is the claim: a CAS on it decides who gets a name. A user's own
record is only written by that user's calls, which never overlap.
Rename claims the new name, points the user at it, then frees the old name, so
a name is never held by two users and never lost.
"""
import re

_VALID = re.compile(r"[a-z0-9-]{1,16}")


def _norm(name):
    if not isinstance(name, str):
        return None
    name = name.lower()
    return name if _VALID.fullmatch(name) else None


class Registry:
    def __init__(self, store):
        self._s = store

    def _claim(self, user, name):
        """Take `name` if free. A failed CAS means the record changed: reload."""
        key = ("name", name)
        while True:
            version, holder = self._s.load(key)
            if holder is not None:
                return False
            if self._s.compare_swap(key, version, user):
                return True

    def _set(self, key, value):
        # Only this user's calls write their record, and they never overlap.
        version, _ = self._s.load(key)
        assert self._s.compare_swap(key, version, value), "record changed under its owner"

    def _release(self, name):
        # We hold the name, so nobody else can change it.
        key = ("name", name)
        version, _ = self._s.load(key)
        assert self._s.compare_swap(key, version, None), "held name changed under its holder"

    def register(self, user, name):
        name = _norm(name)
        if name is None:
            return False
        _, current = self._s.load(("user", user))
        if current is not None:
            return current == name
        if not self._claim(user, name):
            return False
        self._set(("user", user), name)
        return True

    def rename(self, user, new_name):
        new_name = _norm(new_name)
        if new_name is None:
            return False
        _, current = self._s.load(("user", user))
        if current is None:
            return False
        if current == new_name:
            return True
        self._release(current)
        if not self._claim(user, new_name):
            return False
        self._set(("user", user), new_name)
        return True

    def owner(self, name):
        name = _norm(name)
        if name is None:
            return None
        return self._s.load(("name", name))[1]

    def name_of(self, user):
        return self._s.load(("user", user))[1]
