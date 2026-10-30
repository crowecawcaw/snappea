LOCK_KEY = "lock"


def _lock_state(store):
    version, st = store.load(LOCK_KEY)
    if st is None:
        st = {"last": 0, "holder": None, "token": None, "expiry": None}
    return version, st


class LockService:
    def __init__(self, store, ttl):
        self.store = store
        self.ttl = ttl

    def acquire(self, client, now):
        while True:
            version, st = _lock_state(self.store)
            held = st["holder"] is not None and now < st["expiry"]
            if held:
                return st["token"] if st["holder"] == client else None
            token = st["last"] + 1
            new = {"last": token, "holder": client, "token": token,
                   "expiry": now + self.ttl}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client, token, now):
        while True:
            version, st = _lock_state(self.store)
            if not (st["holder"] == client and st["token"] == token
                    and now < st["expiry"]):
                return False
            new = dict(st, holder=None, expiry=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name):
        self.store = store
        self.key = "resource:" + name

    def write(self, token, value):
        if isinstance(token, bool) or not isinstance(token, int):
            return False
        while True:
            _, lock = _lock_state(self.store)  # issued tokens are 1..last
            if not (1 <= token <= lock["last"]):
                return False
            version, st = self.store.load(self.key)
            if st is None:
                st = {"max": 0, "value": None}
            if token < st["max"]:
                return False
            if self.store.compare_swap(self.key, version,
                                       {"max": token, "value": value}):
                return True

    def read(self):
        _, st = self.store.load(self.key)
        return None if st is None else st["value"]
