"""Lock service with fencing tokens over a Store with per-key atomic load/CAS.

Keys (all state lives in the store):
  "lock"        -> {"counter": last token issued, "lease": None | {holder, token, expires}}
  "res:<name>"  -> {"max_used": highest token accepted, "value": last stored value}
Every update is a load -> decide -> compare_swap loop that re-decides after a lost CAS.
Modeled in Model.p / Spec.p.
"""

LOCK_KEY = "lock"


def _is_token(token):
    return isinstance(token, int) and not isinstance(token, bool)


class LockService:
    def __init__(self, store, ttl):
        self.store, self.ttl = store, ttl

    def acquire(self, client, now):
        while True:
            version, rec = self.store.load(LOCK_KEY)
            rec = rec or {"counter": 0, "lease": None}
            lease = rec["lease"]
            if lease is not None and now < lease["expires"]:
                return lease["token"] if lease["holder"] == client else None
            token = rec["counter"] + 1
            new = {"counter": token,
                   "lease": {"holder": client, "token": token, "expires": now + self.ttl}}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client, token, now):
        while True:
            version, rec = self.store.load(LOCK_KEY)
            lease = rec["lease"] if rec else None
            if (lease is None or lease["holder"] != client or lease["token"] != token
                    or not now < lease["expires"]):
                return False
            if self.store.compare_swap(LOCK_KEY, version, {"counter": rec["counter"], "lease": None}):
                return True


class Resource:
    def __init__(self, store, name):
        self.store, self.key = store, "res:" + name

    def write(self, token, value):
        if not _is_token(token) or token < 1:
            return False
        # Issued tokens only grow, so one read of the counter is a valid check.
        _, lock = self.store.load(LOCK_KEY)
        if token > (lock["counter"] if lock else 0):
            return False
        while True:
            version, rec = self.store.load(self.key)
            if rec is not None and token < rec["max_used"]:
                return False
            if self.store.compare_swap(self.key, version, {"max_used": token, "value": value}):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return rec["value"] if rec else None
