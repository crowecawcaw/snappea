"""Unique usernames on a per-key load/compare-and-swap store (see Names.p, DESIGN.md).

Two kinds of record: ("user", u) -> name or None, ("name", n) -> owner or None.
Writes to different keys are not atomic together, so a name is claimed first, then
the user record is published, then the old name is released. A name record is only
changed by a claimant (None -> user) or by its holder (user -> None); a user record
only by that user, whose calls never overlap.
"""

import re

_VALID = re.compile(r"[a-z0-9-]{1,16}")


def _normal(name):
    """Lower-cased name, or None if invalid."""
    if not isinstance(name, str) or not name.isascii():
        return None
    name = name.lower()
    return name if _VALID.fullmatch(name) else None


class Registry:
    def __init__(self, store):
        self._store = store  # all state lives in the store

    @staticmethod
    def _ukey(user):
        return ("user", user)

    @staticmethod
    def _nkey(name):
        return ("name", name)

    def _claim(self, user, name):
        """Make `user` the holder of `name`; False if someone else holds it."""
        key = self._nkey(name)
        while True:
            version, holder = self._store.load(key)
            if holder == user:
                return True
            if holder is not None:
                return False
            if self._store.compare_swap(key, version, user):
                return True

    def _publish(self, user, name):
        key = self._ukey(user)
        while True:
            version, _ = self._store.load(key)
            if self._store.compare_swap(key, version, name):
                return

    def _release(self, user, name):
        key = self._nkey(name)
        while True:
            version, holder = self._store.load(key)
            if holder != user:
                return
            if self._store.compare_swap(key, version, None):
                return

    def register(self, user, name):
        name = _normal(name)
        if name is None:
            return False
        current = self._store.load(self._ukey(user))[1]
        if current == name:
            return True
        if current is not None:
            return False
        if not self._claim(user, name):
            return False
        self._publish(user, name)
        return True

    def rename(self, user, new_name):
        new_name = _normal(new_name)
        if new_name is None:
            return False
        current = self._store.load(self._ukey(user))[1]
        if current is None:
            return False
        if current == new_name:
            return True
        if not self._claim(user, new_name):
            return False
        self._publish(user, new_name)  # new name is now both claimed and published
        self._release(user, current)
        return True

    def owner(self, name):
        name = _normal(name)
        if name is None:
            return None
        return self._store.load(self._nkey(name))[1]

    def name_of(self, user):
        return self._store.load(self._ukey(user))[1]
