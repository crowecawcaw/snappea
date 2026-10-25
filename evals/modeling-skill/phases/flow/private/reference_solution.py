"""Private correct implementation used to calibrate implementation tests."""


class Queue:
    def __init__(self, store, capacity=2):
        if capacity <= 0:
            raise ValueError("capacity must be positive")
        self.store = store
        self.capacity = capacity

    @staticmethod
    def _empty_record():
        return {"items": [], "puts": {}, "takes": {}}

    def put(self, request_id, item):
        while True:
            version, record = self.store.load()
            if record is None:
                record = self._empty_record()
            result = record["puts"].get(request_id)
            if result is not None:
                return result[1]
            accepted = len(record["items"]) < self.capacity
            if accepted:
                record["items"].append(item)
            record["puts"][request_id] = [item, accepted]
            if self.store.compare_swap(version, record):
                return accepted

    def take(self, request_id):
        while True:
            version, record = self.store.load()
            if record is None:
                record = self._empty_record()
            if request_id in record["takes"]:
                return record["takes"][request_id]
            if record["items"]:
                result = record["items"].pop(0)
            else:
                result = None
            record["takes"][request_id] = result
            if self.store.compare_swap(version, record):
                return result

    def snapshot(self):
        _, record = self.store.load()
        return [] if record is None else list(record["items"])
