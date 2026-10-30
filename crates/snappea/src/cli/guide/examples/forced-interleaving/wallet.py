"""Idempotent wallet over a store with atomic load and compare-and-swap only."""

from threading import Lock


class Store:
    def __init__(self):
        self._version, self._value, self._lock = 0, None, Lock()

    def load(self):
        with self._lock:
            return self._version, self._value

    def compare_swap(self, version, value):
        with self._lock:
            if version != self._version:
                return False
            self._version, self._value = version + 1, value
            return True


class Wallet:
    """spend(request_id, amount) succeeds iff the balance covers it. A retry with the
    same request_id returns the first result and has no further effect."""

    def __init__(self, store, initial=10):
        self.store, self.initial = store, initial

    def spend(self, request_id, amount):
        while True:
            version, record = self.store.load()
            record = record or {"balance": self.initial, "results": {}}
            if request_id in record["results"]:
                return record["results"][request_id]
            ok = amount <= record["balance"]
            new = {"balance": record["balance"] - (amount if ok else 0),
                   "results": {**record["results"], request_id: ok}}
            if self.store.compare_swap(version, new):
                return ok

    def balance(self):
        _, record = self.store.load()
        return record["balance"] if record else self.initial
