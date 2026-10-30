"""Unique usernames over a per-key load/compare-and-swap store. See DESIGN.md.

Two kinds of record, no cross-key transaction:
  ("name", n) -> owning user or None      (None = free; the record is never deleted)
  ("user", u) -> that user's name or None
Only the user's own calls write ("user", u), and those never overlap.
"""

import re

_VALID = re.compile(r"[a-z0-9-]{0,16}", re.ASCII | re.IGNORECASE)


def _norm(name):
    """Lower-cased name, or None if invalid (ASCII only, so no Unicode case folding)."""
    if not isinstance(name, str) or _VALID.fullmatch(name) is None:
        return None
    return name.lower()


class Registry:
    def __init__(self, store):
        self.store = store

    def _claim(self, user, name):
        """Take a free name for user. Retries lost races; False if someone holds it."""
        key = ("name", name)
        while True:
            version, holder = self.store.load(key)
            if holder is not None:
                return holder == user
            if self.store.compare_swap(key, version, user):
                return True

    def _set_user(self, user, name):
        key = ("user", user)
        while True:  # only this user writes the record, so one retry at most
            version, _ = self.store.load(key)
            if self.store.compare_swap(key, version, name):
                return

    def _release(self, user, name):
        key = ("name", name)
        while True:
            version, holder = self.store.load(key)
            if holder != user:
                return
            if self.store.compare_swap(key, version, None):
                return

    def register(self, user, name):
        name = _norm(name)
        if name is None:
            return False
        _, current = self.store.load(("user", user))
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
        _, old = self.store.load(("user", user))
        if old is None:
            return False
        if old == new_name:
            return True
        if not self._claim(user, new_name):
            return False
        self._set_user(user, new_name)
        self._release(user, old)
        return True

    def owner(self, name):
        name = _norm(name)
        if name is None:
            return None
        return self.store.load(("name", name))[1]

    def name_of(self, user):
        return self.store.load(("user", user))[1]
