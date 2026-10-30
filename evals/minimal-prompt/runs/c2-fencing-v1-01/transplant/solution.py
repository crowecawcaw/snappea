"""Lock service with fencing tokens on a CAS-only Store.

Two records, no cross-key transactions:
  "lock"      -> {"last": int, "holder": str|None, "token": int, "expiry": int}
  "res:<name>"-> {"max": int, "value": str|None}
`last` only grows, so it is both the token counter and the bound on issued tokens.
"""

LOCK_KEY = "lock"
_EMPTY = {"last": 0, "holder": None, "token": 0, "expiry": 0}


def _lock(store):
    version, rec = store.load(LOCK_KEY)
    return version, (rec if rec is not None else dict(_EMPTY))


class LockService:
    def __init__(self, store, ttl: int):
        self.store = store
        self.ttl = ttl

    def acquire(self, client, now):
        while True:
            version, rec = _lock(self.store)
            if rec["holder"] is not None and now < rec["expiry"]:
                return rec["token"] if rec["holder"] == client else None
            token = rec["last"] + 1
            new = {"last": token, "holder": client, "token": token,
                   "expiry": now + self.ttl}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client, token, now):
        while True:
            version, rec = _lock(self.store)
            if not (rec["holder"] == client and rec["token"] == token
                    and now < rec["expiry"]):
                return False
            new = dict(rec, holder=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name):
        self.store = store
        self.key = "res:" + name

    def write(self, token, value):
        while True:
            version, rec = self.store.load(self.key)
            if rec is None:
                rec = {"max": 0, "value": None}
            if token < rec["max"]:
                return False
            # Read issued bound after the resource: `last` only grows, so a
            # token issued before this write began is always <= last.
            _, lock = _lock(self.store)
            if token < 1 or token > lock["last"]:
                return False
            if self.store.compare_swap(self.key, version,
                                       {"max": token, "value": value}):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return None if rec is None else rec["value"]
