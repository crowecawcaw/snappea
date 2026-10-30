"""Lock service with fencing tokens. All state lives in runtime.Store.

Two independent keys, each updated only by load + compare_swap:
  "lock"        : {"last", "holder", "token", "expires"}  (last = highest token issued)
  "res:<name>"  : {"max", "value"}                         (max = highest accepted token)
"""

_LOCK = "lock"


def _lock_state(store):
    version, rec = store.load(_LOCK)
    if rec is None:
        rec = {"last": 0, "holder": None, "token": 0, "expires": 0}
    return version, rec


class LockService:
    def __init__(self, store, ttl):
        self.store = store
        self.ttl = ttl

    def acquire(self, client, now):
        while True:
            version, rec = _lock_state(self.store)
            if rec["holder"] is not None and now <= rec["expires"]:
                # Read-only outcome: linearized at the load.
                return rec["token"] if rec["holder"] == client else None
            token = rec["last"] + 1
            new = {"last": token, "holder": client, "token": token,
                   "expires": now + self.ttl}
            if self.store.compare_swap(_LOCK, version, new):
                return token

    def release(self, client, token, now):
        while True:
            version, rec = _lock_state(self.store)
            if not (rec["holder"] is not None and rec["holder"] == client
                    and rec["token"] == token and now <= rec["expires"]):
                return False
            new = dict(rec, holder=None)
            if self.store.compare_swap(_LOCK, version, new):
                return True


class Resource:
    def __init__(self, store, name):
        self.store = store
        self.key = "res:" + name

    def write(self, token, value):
        # Tokens are only ever issued upward, so "issued" never becomes false.
        _, lock = _lock_state(self.store)
        if not (isinstance(token, int) and 1 <= token <= lock["last"]):
            return False
        while True:
            version, rec = self.store.load(self.key)
            if rec is None:
                rec = {"max": 0, "value": None}
            if token < rec["max"]:
                return False
            if self.store.compare_swap(self.key, version, {"max": token, "value": value}):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return None if rec is None else rec["value"]
