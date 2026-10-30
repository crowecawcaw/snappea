"""Shared bounded queue on a versioned compare-and-swap Store.

The whole queue (items plus per-request results) is one record, so every
operation is a single atomic compare_swap, retried on contention.
Results are remembered per (kind, request_id), making retries idempotent.
"""


class Queue:
    def __init__(self, store, capacity=2):
        if capacity < 1:
            raise ValueError("capacity must be positive")
        self._store = store
        self._capacity = capacity

    @staticmethod
    def _fresh(rec):
        if rec is None:
            return {"items": [], "put": {}, "take": {}}
        return rec

    def _update(self, kind, request_id, op):
        while True:
            version, rec = self._store.load()
            rec = self._fresh(rec)
            done = rec[kind]
            if kind != "take" and request_id in done:
                return done[request_id]
            result = op(rec["items"])
            if kind != "take":
                done[request_id] = result
            if self._store.compare_swap(version, rec):
                return result

    def put(self, request_id, item):
        def op(items):
            if len(items) >= self._capacity:
                return False
            items.append(item)
            return True
        return self._update("put", request_id, op)

    def take(self, request_id):
        return self._update("take", request_id,
                            lambda items: items.pop(0) if items else None)

    def snapshot(self):
        _, rec = self._store.load()
        return list(self._fresh(rec)["items"])
