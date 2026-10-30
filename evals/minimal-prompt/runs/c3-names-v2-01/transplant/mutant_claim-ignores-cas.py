"""Unique usernames on a per-key load/compare-and-swap store. See DESIGN.md and Names.p.

Two records per entity: "name:<n>" holds the owner (or None), "user:<u>" holds the
user's name (or None). Only single-key operations are atomic, so the name record is
the arbiter of uniqueness: a name is taken by one compare-and-swap on its record.
"""
import re

_VALID = re.compile(r"[a-z0-9-]{1,16}")


def _normalize(name):
    if not isinstance(name, str):
        return None
    name = name.lower()
    return name if _VALID.fullmatch(name) else None


def _name_key(name):
    return "name:" + name


def _user_key(user):
    return "user:" + user


class Registry:
    def __init__(self, store):
        self._store = store

    def _claim(self, name, user):
        """Take the name record for user; False if another user holds it."""
        key = _name_key(name)
        while True:
            version, holder = self._store.load(key)
            if holder is not None:
                return False
            self._store.compare_swap(key, version, user)
            return True

    def _set_user(self, user, name):
        key = _user_key(user)
        while True:
            version, _ = self._store.load(key)
            if self._store.compare_swap(key, version, name):
                return

    def _release(self, name):
        # Only the holder changes a held name record, so this does not lose to a rival.
        key = _name_key(name)
        while True:
            version, _ = self._store.load(key)
            if self._store.compare_swap(key, version, None):
                return

    def register(self, user, name):
        name = _normalize(name)
        if name is None:
            return False
        current = self.name_of(user)
        if current == name:
            return True
        if current is not None:
            return False
        if not self._claim(name, user):
            return False
        self._set_user(user, name)
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
        if not self._claim(new_name, user):
            return False
        self._set_user(user, new_name)
        self._release(current)
        return True

    def owner(self, name):
        name = _normalize(name)
        if name is None:
            return None
        return self._store.load(_name_key(name))[1]

    def name_of(self, user):
        return self._store.load(_user_key(user))[1]
