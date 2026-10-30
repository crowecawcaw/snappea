"""Lock service with fencing tokens. All state lives in the shared Store.

Keys: "lock" -> (last, holder, token, expires); "res:<name>" -> (max_token, value).
Every update is a load + compare_swap retry loop on a single key.
"""

LOCK_KEY = "lock"
_FRESH = {"last": 0, "holder": None, "token": 0, "expires": 0}


def _held(rec, now):
    return rec["holder"] is not None and now < rec["expires"]


class LockService:
    def __init__(self, store, ttl):
        self.store = store
        self.ttl = ttl

    def acquire(self, client, now):
        while True:
            version, rec = self.store.load(LOCK_KEY)
            rec = rec or _FRESH
            if _held(rec, now):
                return rec["token"] if rec["holder"] == client else None
            token = rec["last"] + 1
            new = {"last": token, "holder": client, "token": token, "expires": now + self.ttl}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client, token, now):
        while True:
            version, rec = self.store.load(LOCK_KEY)
            rec = rec or _FRESH
            if not (_held(rec, now) and rec["holder"] == client and rec["token"] == token):
                return False
            new = dict(rec, holder=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name):
        self.store = store
        self.key = "res:" + name

    def write(self, token, value):
        # `last` only grows, and the caller got its token before calling, so a
        # read of it here is enough to tell "never issued" from "issued".
        _, lock = self.store.load(LOCK_KEY)
        last = lock["last"] if lock else 0
        if not isinstance(token, int) or token < 1 or token > last:
            return False
        while True:
            version, rec = self.store.load(self.key)
            high = rec["max"] if rec else 0
            if token < high:
                return False
            if self.store.compare_swap(self.key, version, {"max": token, "value": value}):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return rec["value"] if rec else None
