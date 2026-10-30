"""Bounded shared queue over runtime.Store: one record, whole-record CAS.

The record holds the queue contents and a memo of every request's result, so a
retry (same request_id) returns the original result without further effect.
"""


class Queue:
    def __init__(self, store, capacity=2):
        if capacity < 1:
            raise ValueError("capacity must be positive")
        self.store = store
        self.capacity = capacity

    def _apply(self, kind, request_id, arg, step):
        key = f"{kind}:{request_id}"
        while True:
            version, rec = self.store.load()
            rec = rec or {"items": [], "memo": {}}
            if key in rec["memo"]:
                return rec["memo"][key]
            result = step(rec["items"])
            rec["memo"][key] = result
            if self.store.compare_swap(version, rec):
                return result

    def put(self, request_id, item):
        def step(items):
            if len(items) >= self.capacity:
                return False
            items.append(item)
            return True
        return self._apply("put", request_id, item, step)

    def take(self, request_id):
        def step(items):
            return items.pop(0) if items else None
        return self._apply("take", request_id, None, step)

    def snapshot(self):
        _, rec = self.store.load()
        return list(rec["items"]) if rec else []
