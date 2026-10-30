"""Lock service with fencing tokens. All state lives in the Store."""

LOCK_KEY = "lock"


def _lock_state(raw):
    # issued: highest token ever issued (never decreases).
    return raw or {"issued": 0, "holder": None, "token": 0, "expiry": 0}


class LockService:
    def __init__(self, store, ttl):
        self.store, self.ttl = store, ttl

    def acquire(self, client, now):
        while True:
            version, raw = self.store.load(LOCK_KEY)
            s = _lock_state(raw)
            if s["holder"] == client:
                return s["token"]
            if s["holder"] is not None and now < s["expiry"]:
                return s["token"] if s["holder"] == client else None
            token = s["issued"] + 1
            new = {"issued": token, "holder": client, "token": token,
                   "expiry": now + self.ttl}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client, token, now):
        while True:
            version, raw = self.store.load(LOCK_KEY)
            s = _lock_state(raw)
            if not (s["holder"] == client and s["token"] == token
                    and now < s["expiry"]):
                return False
            new = dict(s, holder=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name):
        self.store, self.key = store, "res:" + name

    def write(self, token, value):
        _, raw = self.store.load(LOCK_KEY)
        if not (1 <= token <= _lock_state(raw)["issued"]):
            return False
        while True:
            version, cur = self.store.load(self.key)
            if cur is not None and token < cur["max"]:
                return False
            if self.store.compare_swap(self.key, version,
                                       {"max": token, "value": value}):
                return True

    def read(self):
        _, cur = self.store.load(self.key)
        return None if cur is None else cur["value"]
