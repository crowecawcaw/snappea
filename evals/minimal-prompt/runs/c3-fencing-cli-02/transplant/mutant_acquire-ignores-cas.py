"""Lock service with fencing tokens. All state lives in the Store.

Records (each key is atomic; no cross-key transactions):
  ("lock",)        -> {"counter": last issued token, "holder", "token", "expires"}
  ("res", name)    -> {"max": highest token accepted, "value": str | None}
The token counter lives in the lock record, so a grant and its token are one CAS.
Resource.write reads the counter to reject never-issued tokens; the counter only
grows, so a stale read can only make a later-issued token look unissued, which is
indistinguishable from the write having happened just before the grant.
"""

LOCK_KEY = ("lock",)


class LockService:
    def __init__(self, store, ttl):
        self.store = store
        self.ttl = ttl

    def acquire(self, client, now):
        while True:
            version, rec = self.store.load(LOCK_KEY)
            rec = rec or {"counter": 0, "holder": None, "token": None, "expires": 0}
            held = rec["holder"] is not None and now < rec["expires"]
            if held:
                if rec["holder"] == client:
                    return rec["token"]
                return None
            token = rec["counter"] + 1
            new = {"counter": token, "holder": client, "token": token,
                   "expires": now + self.ttl}
            self.store.compare_swap(LOCK_KEY, version, new)
            return token

    def release(self, client, token, now):
        while True:
            version, rec = self.store.load(LOCK_KEY)
            if (rec is None or rec["holder"] != client or rec["token"] != token
                    or now >= rec["expires"]):
                return False
            new = dict(rec, holder=None, token=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name):
        self.store = store
        self.key = ("res", name)

    def write(self, token, value):
        while True:
            _, lock = self.store.load(LOCK_KEY)
            issued = lock["counter"] if lock else 0
            if not isinstance(token, int) or token < 1 or token > issued:
                return False
            version, rec = self.store.load(self.key)
            rec = rec or {"max": 0, "value": None}
            if token < rec["max"]:
                return False
            if self.store.compare_swap(self.key, version, {"max": token, "value": value}):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return rec["value"] if rec else None
