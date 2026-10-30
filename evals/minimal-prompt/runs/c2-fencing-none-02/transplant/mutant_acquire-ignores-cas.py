"""Lease lock with fencing tokens, all state in runtime.Store (CAS loops)."""

from snappea import emit

LOCK_KEY = "lock"


def _lock_state(store):
    version, rec = store.load(LOCK_KEY)
    if rec is None:
        rec = {"last": 0, "holder": None, "token": None, "expires": 0}
    return version, rec


class LockService:
    def __init__(self, store, ttl):
        self.store = store
        self.ttl = ttl

    def acquire(self, client, now):
        while True:
            version, rec = _lock_state(self.store)
            held = rec["holder"] is not None and now < rec["expires"]
            if held:
                return rec["token"] if rec["holder"] == client else None
            token = rec["last"] + 1
            new = {"last": token, "holder": client, "token": token,
                   "expires": now + self.ttl}
            self.store.compare_swap(LOCK_KEY, version, new)
            emit("eIssued", token)
            return token

    def release(self, client, token, now):
        while True:
            version, rec = _lock_state(self.store)
            if not (rec["holder"] == client and rec["token"] == token
                    and now < rec["expires"]):
                return False
            new = dict(rec, holder=None, token=None, expires=0)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name):
        self.store = store
        self.key = "resource:" + name

    def write(self, token, value):
        if not isinstance(token, int) or isinstance(token, bool) or token < 1:
            return False
        # Tokens only grow, so a token <= the last issued one stays issued.
        if token > _lock_state(self.store)[1]["last"]:
            return False
        while True:
            version, rec = self.store.load(self.key)
            if rec is None:
                rec = {"max": 0, "value": None}
            if token < rec["max"]:
                return False
            if self.store.compare_swap(self.key, version, {"max": token, "value": value}):
                emit("eAccepted", token)
                return True

    def read(self):
        return (self.store.load(self.key)[1] or {}).get("value")
