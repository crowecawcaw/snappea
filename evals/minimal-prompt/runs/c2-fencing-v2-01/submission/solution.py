"""Lock service with fencing tokens. All state lives in runtime.Store.

Keys: "lock"  -> {"last": highest token issued, "holder", "token", "expires"}
      "res:<name>" -> {"max": highest token used by an accepted write, "value"}
Each operation is load -> decide -> compare_swap, retried when the swap loses.
Only per-key load and compare_swap are atomic; see DESIGN.md.
"""

LOCK_KEY = "lock"
_EMPTY_LOCK = {"last": 0, "holder": None, "token": 0, "expires": 0}


class LockService:
    def __init__(self, store, ttl: int):
        self.store = store
        self.ttl = ttl

    def _load(self):
        version, rec = self.store.load(LOCK_KEY)
        return version, (rec if rec is not None else dict(_EMPTY_LOCK))

    def acquire(self, client: str, now: int):
        while True:
            version, rec = self._load()
            if rec["holder"] is not None and now < rec["expires"]:
                # Unexpired lease: the holder gets its own token back, others None.
                return rec["token"] if rec["holder"] == client else None
            token = rec["last"] + 1  # last survives release, so tokens never repeat
            new = {"last": token, "holder": client, "token": token,
                   "expires": now + self.ttl}
            if self.store.compare_swap(LOCK_KEY, version, new):
                return token

    def release(self, client: str, token: int, now: int) -> bool:
        while True:
            version, rec = self._load()
            if not (rec["holder"] == client and rec["token"] == token
                    and now < rec["expires"]):
                return False
            new = dict(rec, holder=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                return True


class Resource:
    def __init__(self, store, name: str):
        self.store = store
        self.key = "res:" + name

    def write(self, token: int, value: str) -> bool:
        if isinstance(token, bool) or not isinstance(token, int):
            return False
        while True:
            # Issued tokens are 1..last and last only grows, so one read suffices.
            _, lock = self.store.load(LOCK_KEY)
            last = lock["last"] if lock is not None else 0
            if token < 1 or token > last:
                return False
            version, rec = self.store.load(self.key)
            if rec is not None and token < rec["max"]:
                return False
            if self.store.compare_swap(self.key, version, {"max": token, "value": value}):
                return True

    def read(self):
        _, rec = self.store.load(self.key)
        return None if rec is None else rec["value"]
