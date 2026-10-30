"""Unique usernames over a Store with per-key compare-and-swap.

Keys: ("user", u) holds u's name or None; ("name", n) holds n's owner or None.
The store has no cross-key transactions, so the name key is the single point of
agreement. Order of writes: claim the new name key, set the user record, release the
old name key. The user record is only written by that user's calls, which never
overlap. See model/names.p.
"""

import re

from snappea import emit

_VALID = re.compile(r"[a-z0-9-]{1,16}")


def _norm(name):
    """Lower-cased valid name, or None."""
    if not isinstance(name, str):
        return None
    name = name.lower()
    return name if _VALID.fullmatch(name) else None


class Registry:
    def __init__(self, store):
        self._store = store

    def _claim(self, user, name):
        """Take the name key for user; False if another user holds it."""
        key = ("name", name)
        while True:
            version, holder = self._store.load(key)
            if holder is not None:
                return holder == user
            if self._store.compare_swap(key, version, user):
                return True

    def _release(self, user, name):
        key = ("name", name)
        while True:
            version, holder = self._store.load(key)
            if holder != user:
                return
            if self._store.compare_swap(key, version, None):
                return

    def _set_user(self, user, name):
        key = ("user", user)
        while True:
            version, _ = self._store.load(key)
            if self._store.compare_swap(key, version, name):
                break
        emit("eHold", user=user, name=name)

    def register(self, user, name):
        name = _norm(name)
        if name is None:
            return False
        _, current = self._store.load(("user", user))
        if current == name:
            return True
        if current is not None:
            return False
        if not self._claim(user, name):
            return False
        self._set_user(user, name)
        return True

    def rename(self, user, new_name):
        new_name = _norm(new_name)
        if new_name is None:
            return False
        _, old = self._store.load(("user", user))
        if old is None:
            return False
        if old == new_name:
            return True
        if not self._claim(user, new_name):
            return False
        self._set_user(user, new_name)
        return True

    def owner(self, name):
        name = _norm(name)
        if name is None:
            return None
        return self._store.load(("name", name))[1]

    def name_of(self, user):
        return self._store.load(("user", user))[1]
