"""Correct lock service used to calibrate hidden tests and agent tests."""

LOCK = "lock"


class LockService:
    def __init__(self, store, ttl):
        if ttl <= 0:
            raise ValueError("ttl must be positive")
        self.store = store
        self.ttl = ttl

    def _load(self):
        version, record = self.store.load(LOCK)
        if record is None:
            record = {"issued": 0, "holder": None, "token": 0, "expires": 0}
        return version, record

    def acquire(self, client, now):
        while True:
            version, record = self._load()
            held = record["holder"] is not None and now < record["expires"]
            if held:
                return record["token"] if record["holder"] == client else None
            record["issued"] += 1
            record.update(holder=client, token=record["issued"], expires=now + self.ttl)
            if self.store.compare_swap(LOCK, version, record):
                return record["token"]

    def release(self, client, token, now):
        while True:
            version, record = self._load()
            if not (record["holder"] == client and record["token"] == token and now < record["expires"]):
                return False
            record["holder"] = None
            if self.store.compare_swap(LOCK, version, record):
                return True


class Resource:
    def __init__(self, store, name):
        self.store = store
        self.key = "resource:" + name

    def write(self, token, value):
        _, lock = self.store.load(LOCK)
        issued = lock["issued"] if lock else 0
        if not 1 <= token <= issued:
            return False
        while True:
            version, record = self.store.load(self.key)
            fence = record["fence"] if record else 0
            if token < fence:
                return False
            if self.store.compare_swap(self.key, version, {"fence": token, "value": value}):
                return True

    def read(self):
        _, record = self.store.load(self.key)
        return record["value"] if record else None
