"""Correct queue used to calibrate hidden tests and agent tests."""


class Queue:
    def __init__(self, store, capacity=2):
        if capacity <= 0:
            raise ValueError("capacity must be positive")
        self.store = store
        self.capacity = capacity

    def _load(self):
        version, record = self.store.load()
        if record is None:
            record = {"items": [], "puts": {}, "takes": {}}
        return version, record

    def put(self, request_id, item):
        while True:
            version, record = self._load()
            key = repr((request_id, item))
            if key in record["puts"]:
                return record["puts"][key]
            accepted = len(record["items"]) < self.capacity
            if accepted:
                record["items"].append(item)
            record["puts"][key] = accepted
            if self.store.compare_swap(version, record):
                return accepted

    def take(self, request_id):
        while True:
            version, record = self._load()
            if request_id in record["takes"]:
                return record["takes"][request_id]
            result = record["items"].pop(0) if record["items"] else None
            record["takes"][request_id] = result
            if self.store.compare_swap(version, record):
                return result

    def snapshot(self):
        _, record = self._load()
        return list(record["items"])
