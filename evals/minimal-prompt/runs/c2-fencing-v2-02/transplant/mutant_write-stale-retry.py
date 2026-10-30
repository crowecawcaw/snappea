"""Lock service with fencing tokens, built on runtime.Store (load / compare_swap per key).

Keys: "lock" -> {"last", "holder", "token", "exp"}  (last never decreases: token source)
      "resource:<name>" -> {"high", "value"}
"""

LOCK_KEY = "lock"


class LockService:
    def __init__(self, store, ttl):
        self.store = store
        self.ttl = ttl

    def acquire(self, client, now):
        while True:
            version, rec = self.store.load(LOCK_KEY)
            rec = rec or {"last": 0, "holder": None, "token": 0, "exp": 0}
            if rec["holder"] is not None and now < rec["exp"]:
                return rec["token"] if rec["holder"] == client else None
            token = rec["last"] + 1
            new = {"last": token, "holder": client, "token": token, "exp": now + self.ttl}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client, token, now):
        while True:
            version, rec = self.store.load(LOCK_KEY)
            if (rec is None or rec["holder"] != client or rec["token"] != token
                    or now >= rec["exp"]):
                return False
            new = dict(rec, holder=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name):
        self.store = store
        self.key = "resource:" + name

    def write(self, token, value):
        # `last` only grows, so a token seen as issued stays issued.
        _, lock = self.store.load(LOCK_KEY)
        if lock is None or not (1 <= token <= lock["last"]):
            return False
        version, rec = self.store.load(self.key)
        high = rec["high"] if rec else 0
        while True:
            if token < high:
                return False
            if self.store.compare_swap(self.key, version, {"high": token, "value": value}):
                return True
            version, _ = self.store.load(self.key)

    def read(self):
        _, rec = self.store.load(self.key)
        return rec["value"] if rec else None
