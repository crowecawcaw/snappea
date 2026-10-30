"""Unique usernames on a per-key load/compare-and-swap store.

Records: "n:<name>" -> owner (None if free), "u:<user>" -> name (None if none).
There are no cross-key transactions, so a change touches keys in an order that
keeps one invariant at every step: a name's record is only ever set by the user
that claimed it from free, and released by that same user.

  register: claim name (CAS free -> user), then set the user's record.
  rename:   claim new name, set user's record, then release the old name.

Claim comes before publish, release comes last, so no name is ever held by two
users, and when calls finish owner(n) == u exactly when name_of(u) == n.
Calls for one user never overlap, so the user's record is only changed by that
user's own call; only name records are contended (retry on a lost CAS).
"""

import re
import threading

from snappea import emit

_VALID = re.compile(r"[a-z0-9-]{1,17}")
# Serializes "commit + emit" so observation order matches commit order.
# It is held only around one CAS and one best-effort emit.
_EMIT_LOCK = threading.Lock()


def _norm(name):
    if not isinstance(name, str):
        return None
    name = name.lower()
    return name if _VALID.fullmatch(name) else None


class Registry:
    def __init__(self, store):
        self._store = store

    def _cas(self, key, version, value, event, **fields):
        with _EMIT_LOCK:
            ok = self._store.compare_swap(key, version, value)
            if ok:
                try:
                    emit(event, **fields)
                except Exception:  # emission is best-effort
                    pass
        return ok

    def _claim(self, user, name):
        """Claim a free name for user; False if someone holds it."""
        while True:
            version, holder = self._store.load("n:" + name)
            if holder is not None:
                return False
            if self._cas("n:" + name, version, user, "eClaimed", name=name, user=user):
                return True

    def _set_user(self, user, version, name):
        ok = self._cas("u:" + user, version, name, "eUserSet", user=user, name=name)
        assert ok, "user record changed during a call for that user"

    def _release(self, user, name):
        version, holder = self._store.load("n:" + name)
        assert holder == user
        ok = self._cas("n:" + name, version, None, "eReleased", name=name, user=user)
        assert ok, "held name changed under its holder"

    def register(self, user, name):
        name = _norm(name)
        if name is None:
            return False
        version, current = self._store.load("u:" + user)
        if current == name:
            return True
        if current is not None:
            return False
        if not self._claim(user, name):
            return False
        self._set_user(user, version, name)
        return True

    def rename(self, user, new_name):
        new_name = _norm(new_name)
        if new_name is None:
            return False
        version, current = self._store.load("u:" + user)
        if current is None:
            return False
        if current == new_name:
            return True
        if not self._claim(user, new_name):
            return False
        self._set_user(user, version, new_name)
        self._release(user, current)
        return True

    def owner(self, name):
        name = _norm(name)
        if name is None:
            return None
        return self._store.load("n:" + name)[1]

    def name_of(self, user):
        return self._store.load("u:" + user)[1]
