"""Lock service with fencing tokens. All state lives in runtime.Store.

Primitives: per-key atomic load and compare_swap. Every operation is a
load / decide / compare_swap loop that re-reads when the swap loses.

Key "lock": {holder, token, expiry, last}. `last` is the highest token ever
issued; it survives release and expiry so tokens strictly increase.
Key "res:<name>": (max token used by an accepted write, value).
"""

LOCK_KEY = "lock"
_FREE = {"holder": None, "token": 0, "expiry": 0, "last": 0}


def _load_lock(store):
    version, rec = store.load(LOCK_KEY)
    return version, (rec if rec is not None else dict(_FREE))


class LockService:
    def __init__(self, store, ttl: int):
        self.store = store
        self.ttl = ttl

    def acquire(self, client: str, now: int):
        while True:
            version, rec = _load_lock(self.store)
            if rec["holder"] is not None and now < rec["expiry"]:
                if rec["holder"] != client:
                    return None
            token = rec["last"] + 1
            new = {"holder": client, "token": token, "expiry": now + self.ttl, "last": token}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client: str, token: int, now: int) -> bool:
        while True:
            version, rec = _load_lock(self.store)
            if not (rec["holder"] == client and rec["token"] == token and now < rec["expiry"]):
                return False
            new = dict(rec, holder=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name: str):
        self.store = store
        self.key = "res:" + name

    def write(self, token: int, value: str) -> bool:
        # `last` only grows, so a token that is issued in this read stays issued.
        _, lock = _load_lock(self.store)
        if token < 1 or token > lock["last"]:
            return False
        while True:
            version, rec = self.store.load(self.key)
            max_used, _ = rec if rec is not None else (0, None)
            if token < max_used:
                return False
            if self.store.compare_swap(self.key, version, (token, value)):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return None if rec is None else rec[1]
