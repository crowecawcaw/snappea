"""Correct registry: claim the new name, then point the user at it, then free the old."""
import re

VALID = re.compile(r"[a-z0-9-]{1,16}")


def norm(name):
    if not isinstance(name, str):
        return None
    name = name.lower()
    return name if VALID.fullmatch(name) else None


class Registry:
    def __init__(self, store):
        self.store = store

    def _claim(self, user, name):
        """Make `user` the holder of `name` unless someone else holds it."""
        while True:
            version, holder = self.store.load("name:" + name)
            if holder is not None:
                return holder == user
            if self.store.compare_swap("name:" + name, version, user):
                return True

    def _free(self, user, name):
        while True:
            version, holder = self.store.load("name:" + name)
            if holder != user:
                return
            if self.store.compare_swap("name:" + name, version, None):
                return

    def register(self, user, name):
        name = norm(name)
        if name is None:
            return False
        version, current = self.store.load("user:" + user)
        if current is not None:
            return current == name
        if not self._claim(user, name):
            return False
        if self.store.compare_swap("user:" + user, version, name):
            return True
        self._free(user, name)
        return False

    def rename(self, user, new_name):
        new = norm(new_name)
        if new is None:
            return False
        version, old = self.store.load("user:" + user)
        if old is None:
            return False
        if old == new:
            return True
        if not self._claim(user, new):
            return False
        if not self.store.compare_swap("user:" + user, version, new):
            self._free(user, new)
            return False
        self._free(user, old)
        return True

    def owner(self, name):
        name = norm(name)
        if name is None:
            return None
        holder = self.store.load("name:" + name)[1]
        if holder is None:
            return None
        # a claim whose user record does not point back is not yet (or no longer) held
        return holder if self.store.load("user:" + holder)[1] == name else None

    def name_of(self, user):
        return self.store.load("user:" + user)[1]
