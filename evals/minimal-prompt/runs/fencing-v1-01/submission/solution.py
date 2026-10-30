LOCK_KEY = "lock"


def _lock_state(store):
    version, rec = store.load(LOCK_KEY)
    if rec is None:
        rec = {"last": 0, "holder": None, "token": 0, "exp": 0}
    return version, rec


class LockService:
    def __init__(self, store, ttl: int):
        self.store = store
        self.ttl = ttl

    def acquire(self, client, now):
        while True:
            version, rec = _lock_state(self.store)
            if rec["holder"] is not None and now < rec["exp"]:
                return rec["token"] if rec["holder"] == client else None
            token = rec["last"] + 1
            new = {"last": token, "holder": client, "token": token, "exp": now + self.ttl}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client, token, now):
        while True:
            version, rec = _lock_state(self.store)
            if not (rec["holder"] == client and rec["token"] == token and now < rec["exp"]):
                return False
            new = dict(rec, holder=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name):
        self.store = store
        self.key = "resource:" + name

    def write(self, token, value):
        while True:
            _, lock = _lock_state(self.store)  # issued tokens only grow
            if not (isinstance(token, int) and 1 <= token <= lock["last"]):
                return False
            version, rec = self.store.load(self.key)
            high = rec["high"] if rec else 0
            if token < high:
                return False
            if self.store.compare_swap(self.key, version, {"high": token, "value": value}):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return rec["value"] if rec else None
