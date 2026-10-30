"""Lock service with fencing tokens, built on runtime.Store (load + CAS)."""

LOCK_KEY = "lock"
# lock record: {"issued": last token issued, "holder": client|None,
#               "token": lease token, "expires": expiry time}


class LockService:
    def __init__(self, store, ttl: int):
        self.store = store
        self.ttl = ttl

    def acquire(self, client: str, now: int):
        while True:
            version, rec = self.store.load(LOCK_KEY)
            rec = rec or {"issued": 0, "holder": None, "token": 0, "expires": 0}
            if rec["holder"] is not None and now < rec["expires"]:
                if rec["holder"] == client:
                    return rec["token"]
                return None
            token = rec["issued"] + 1
            new = {"issued": token, "holder": client, "token": token,
                   "expires": now + self.ttl}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client: str, token: int, now: int) -> bool:
        while True:
            version, rec = self.store.load(LOCK_KEY)
            if (rec is None or rec["holder"] != client or rec["token"] != token
                    or now >= rec["expires"]):
                return False
            new = dict(rec, holder=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name: str):
        self.store = store
        self.key = "resource:" + name

    def write(self, token: int, value: str) -> bool:
        _, lock = self.store.load(LOCK_KEY)
        issued = lock["issued"] if lock else 0
        if isinstance(token, bool) or not isinstance(token, int) \
                or token < 1 or token > issued:
            return False
        while True:
            version, rec = self.store.load(self.key)
            high, _val = rec if rec else (0, None)
            if token < high:
                return False
            if self.store.compare_swap(self.key, version, (token, value)):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return rec[1] if rec else None
