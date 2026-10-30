"""Unique usernames over a per-key load/compare-and-swap store.

Two kinds of record: ("n", name) -> owner user, ("u", user) -> name. Only the
name record is contended. A name is claimed by CAS from free to the user; the
user's own record is written only by that user (calls per user never overlap).
Rename claims the new name, points the user at it, then frees the old one, so a
name is never held by two users; the two views agree once no call is running.
See Names.p.
"""

import re

_NAME = re.compile(r"[a-z0-9-]{1,16}")


def _norm(name):
    """Lower-cased name, or None if invalid. ASCII only, checked before lowering."""
    if not isinstance(name, str) or not name.isascii():
        return None
    name = name.lower()
    return name if _NAME.fullmatch(name) else None


class Registry:
    def __init__(self, store):
        self._store = store

    def _claim(self, name, user):
        """Make `user` the holder of `name` if it is free (or already theirs)."""
        key = ("n", name)
        while True:
            version, holder = self._store.load(key)
            if holder == user:
                return True
            if holder is not None:
                return False
            if self._store.compare_swap(key, version, user):
                return True

    def _release(self, name, user):
        key = ("n", name)
        while True:
            version, holder = self._store.load(key)
            if holder != user:
                return
            if self._store.compare_swap(key, version, None):
                return

    def _point(self, user, name):
        key = ("u", user)
        while True:
            version, _ = self._store.load(key)
            if self._store.compare_swap(key, version, name):
                return

    def register(self, user, name):
        name = _norm(name)
        if name is None:
            return False
        _, current = self._store.load(("u", user))
        if current is not None and current != name:
            return False
        if not self._claim(name, user):
            return False
        if current is None:
            self._point(user, name)
        return True

    def rename(self, user, new_name):
        new_name = _norm(new_name)
        if new_name is None:
            return False
        _, old = self._store.load(("u", user))
        if old == new_name:
            return True
        if not self._claim(new_name, user):
            return False
        self._point(user, new_name)
        self._release(old, user)
        return True

    def owner(self, name):
        name = _norm(name)
        if name is None:
            return None
        return self._store.load(("n", name))[1]

    def name_of(self, user):
        return self._store.load(("u", user))[1]
