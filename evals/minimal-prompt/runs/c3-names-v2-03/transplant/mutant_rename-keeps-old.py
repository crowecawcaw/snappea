"""Unique usernames over a per-key load/compare-and-swap store.

Keys: "name:<n>" -> holder user (None = free); "user:<u>" -> name (None = none).
No cross-key transaction exists, so a name is claimed with one CAS on its key
first; the user's record and the release of an old name follow.  Calls for one
user never overlap, so only that user writes its own record or releases a name
it holds.  See DESIGN.md.
"""

import re

_VALID = re.compile(r"[a-z0-9-]{1,16}")


def _norm(name):
    """Return the lower-case name, or None if it is not a valid name."""
    # ASCII check first: str.lower() maps some non-ASCII letters (Kelvin sign) to a-z.
    if not isinstance(name, str) or not name.isascii():
        return None
    name = name.lower()
    # fullmatch: '$' would accept a trailing newline.
    return name if _VALID.fullmatch(name) else None


class Registry:
    def __init__(self, store):
        self._store = store

    def _holder(self, name):
        return self._store.load("name:" + name)

    def _claim(self, user, name):
        """Take a free name with one CAS. False if somebody holds it."""
        version, holder = self._holder(name)
        if holder is not None:
            return False
        return self._store.compare_swap("name:" + name, version, user)

    def _set_user(self, user, name):
        key = "user:" + user
        while True:  # only this user writes the record; the loop is defensive
            version, _ = self._store.load(key)
            if self._store.compare_swap(key, version, name):
                return

    def _release(self, user, name):
        key = "name:" + name
        while True:
            version, holder = self._store.load(key)
            if holder != user:
                return
            if self._store.compare_swap(key, version, None):
                return

    def register(self, user, name):
        name = _norm(name)
        if name is None:
            return False
        _, current = self._store.load("user:" + user)
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
        _, old = self._store.load("user:" + user)
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
        return self._holder(name)[1]

    def name_of(self, user):
        return self._store.load("user:" + user)[1]
