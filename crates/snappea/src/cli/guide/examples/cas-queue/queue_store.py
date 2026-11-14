"""A shared FIFO with durable, operation-specific request deduplication."""


class Queue:
    def __init__(self, store, capacity=2):
        if not isinstance(capacity, int) or isinstance(capacity, bool) or capacity <= 0:
            raise ValueError("capacity must be a positive integer")
        self._store = store
        while True:
            version, record = store.load()
            if record is not None:
                if record["capacity"] != capacity:
                    raise ValueError("queues sharing a store must have the same capacity")
                return
            record = {"capacity": capacity, "items": [], "puts": {}, "takes": {}}
            if store.compare_swap(version, record):
                return

    def put(self, request_id: str, item: int) -> bool:
        while True:
            version, record = self._store.load()
            if request_id in record["puts"]:
                original_item, result = record["puts"][request_id]
                if item != original_item:
                    raise ValueError("put request_id reused with a different item")
                return result
            result = len(record["items"]) < record["capacity"]
            if result:
                record["items"].append(item)
            # Full results must be committed too: later retries remain rejected.
            record["puts"][request_id] = (item, result)
            if self._store.compare_swap(version, record):
                return result

    def take(self, request_id: str) -> int | None:
        while True:
            version, record = self._store.load()
            if request_id in record["takes"]:
                return record["takes"][request_id]
            result = record["items"].pop(0) if record["items"] else None
            # An empty take is a completed request, not a future subscription.
            record["takes"][request_id] = result
            if self._store.compare_swap(version, record):
                return result

    def snapshot(self) -> list[int]:
        _, record = self._store.load()
        return record["items"]  # load() has already made an isolated deep copy.
