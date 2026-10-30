"""Unique usernames over a per-key CAS store.

Protocol (modeled in Names.p): claim the name record, then point the user record
at it, then free the old name record. A name record only ever changes to a new
owner when it is free, so two users can never both hold a name. Calls of one user
never overlap, so the user-record CAS cannot lose.
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
        """Make `name` held by user; False if another user holds it."""
        key = ("name", name)
        while True:
            ver, holder = self._s.load(key)
            if holder == user:
                return True
            if holder is not None:
                return False
            if self._s.compare_swap(key, ver, user):
                return True

    def _set_user(self, user, name):
        key = ("user", user)
        while True:  # only this user's (non-overlapping) calls write it
            ver, _ = self._s.load(key)
            if self._s.compare_swap(key, ver, name):
                return

    def _release(self, user, name):
        key = ("name", name)
        while True:
            ver, holder = self._s.load(key)
            if holder != user or self._s.compare_swap(key, ver, None):
                return

    def register(self, user, name):
        name = _norm(name)
        if name is None:
            return False
        current = self.name_of(user)
        if current is not None:
            return current == name
        if not self._claim(user, name):
            return False
        self._set_user(user, name)
        return True

    def rename(self, user, new_name):
        new_name = _norm(new_name)
        if new_name is None:
            return False
        old = self.name_of(user)
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
        return self._s.load(("name", name))[1]

    def name_of(self, user):
        return self._s.load(("user", user))[1]
