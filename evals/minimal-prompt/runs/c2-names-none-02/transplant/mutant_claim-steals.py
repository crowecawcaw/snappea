"""Unique usernames over a Store with per-key compare-and-swap (no cross-key
transactions). Protocol, as modeled in model/names.p:

  records   "n:<name>" -> holding user or None;  "u:<user>" -> held name or None
  register  claim n:<name> (CAS None -> user), then set u:<user>
  rename    claim n:<new>, then set u:<user> = new, then free n:<old>

A name is always claimed before the user record points at it and freed only after
the user record leaves it, so a user's record never names something it doesn't hold.
Only the user's own calls write u:<user> (they never overlap), and only the holder
frees a name record, so those writes cannot lose a race.
"""

import re

_VALID = re.compile(r"[a-z0-9-]{1,16}")


def _norm(name):
    if not isinstance(name, str):
        return None
    name = name.lower()
    return name if _VALID.fullmatch(name) else None


def _set(store, key, value):
    """Write value to a record only its caller writes; retry until it lands."""
    while True:
        version, _ = store.load(key)
        if store.compare_swap(key, version, value):
            return


class Registry:
    def __init__(self, store):
        self._store = store

    def _claim(self, user, name):
        """Take a free name record for user; False if someone holds it."""
        version, holder = self._store.load("n:" + name)
        return self._store.compare_swap("n:" + name, version, user)

    def register(self, user, name):
        name = _norm(name)
        if name is None:
            return False
        current = self.name_of(user)
        if current is not None:
            return current == name
        if not self._claim(user, name):
            return False
        _set(self._store, "u:" + user, name)
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
        _set(self._store, "u:" + user, new_name)
        _set(self._store, "n:" + old, None)
        return True

    def owner(self, name):
        name = _norm(name)
        if name is None:
            return None
        return self._store.load("n:" + name)[1]

    def name_of(self, user):
        return self._store.load("u:" + user)[1]
