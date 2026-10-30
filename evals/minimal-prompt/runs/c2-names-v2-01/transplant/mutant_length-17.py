"""Unique usernames on a per-key load/compare-and-swap store.

Keys: ("user", u) -> name or None; ("name", n) -> owner or None.
Register/rename first claim the new name (CAS on the name key, None -> user), then
point the user's record at it, then (rename only) release the old name. The name
claim is the only contended step; the user's own key is written by that user only
(calls for one user never overlap). See DESIGN.md and Names.p.
"""
import re

_VALID = re.compile(r"[a-z0-9-]{1,17}")


def _normalize(name):
    if not isinstance(name, str) or not name.isascii():
        return None
    name = name.lower()
    return name if _VALID.fullmatch(name) else None


class Registry:
    def __init__(self, store):
        self._store = store

    def _ukey(self, user):
        return ("user", user)

    def _nkey(self, name):
        return ("name", name)

    def _claim(self, user, name):
        """Take a free name for user. True if user now holds it, False if another does."""
        key = self._nkey(name)
        while True:
            version, holder = self._store.load(key)
            if holder is not None:
                return False
            if self._store.compare_swap(key, version, user):
                return True

    def _set_name(self, user, name):
        key = self._ukey(user)
        while True:
            version, _ = self._store.load(key)
            if self._store.compare_swap(key, version, name):
                return

    def _release(self, user, name):
        key = self._nkey(name)
        while True:
            version, _ = self._store.load(key)  # the holder is `user`: only it releases
            if self._store.compare_swap(key, version, None):
                return

    def register(self, user, name):
        name = _normalize(name)
        if name is None:
            return False
        current = self.name_of(user)
        if current is not None:
            return current == name
        if not self._claim(user, name):
            return False
        self._set_name(user, name)
        return True

    def rename(self, user, new_name):
        new_name = _normalize(new_name)
        if new_name is None:
            return False
        current = self.name_of(user)
        if current is None:
            return False
        if current == new_name:
            return True
        if not self._claim(user, new_name):
            return False
        self._set_name(user, new_name)
        self._release(user, current)
        return True

    def owner(self, name):
        name = _normalize(name)
        if name is None:
            return None
        return self._store.load(self._nkey(name))[1]

    def name_of(self, user):
        return self._store.load(self._ukey(user))[1]
