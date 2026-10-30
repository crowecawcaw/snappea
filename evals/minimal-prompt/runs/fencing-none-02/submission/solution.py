"""Lock service with fencing tokens, on top of runtime.Store (CAS per key)."""

_LOCK_KEY = "lock"


def _lock_state(store):
    version, st = store.load(_LOCK_KEY)
    if st is None:
        st = {"counter": 0, "holder": None, "token": None, "expiry": 0}
    return version, st


class LockService:
    def __init__(self, store, ttl: int):
        self.store = store
        self.ttl = ttl

    def acquire(self, client: str, now: int):
        while True:
            version, st = _lock_state(self.store)
            if st["holder"] is not None and now < st["expiry"]:
                if st["holder"] == client:
                    return st["token"]
                return None
            token = st["counter"] + 1
            new = {"counter": token, "holder": client, "token": token,
                   "expiry": now + self.ttl}
            if self.store.compare_swap(_LOCK_KEY, version, new):
                return token

    def release(self, client: str, token: int, now: int) -> bool:
        while True:
            version, st = _lock_state(self.store)
            if not (st["holder"] == client and st["token"] == token
                    and now < st["expiry"]):
                return False
            new = dict(st, holder=None, token=None)
            if self.store.compare_swap(_LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name: str):
        self.store = store
        self.key = "resource:" + name

    def write(self, token: int, value: str) -> bool:
        while True:
            _, st = _lock_state(self.store)  # counter only grows
            if not (isinstance(token, int) and 1 <= token <= st["counter"]):
                return False
            version, rec = self.store.load(self.key)
            if rec is None:
                rec = {"max": 0, "value": None}
            if token < rec["max"]:
                return False
            if self.store.compare_swap(self.key, version,
                                       {"max": token, "value": value}):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return None if rec is None else rec["value"]
