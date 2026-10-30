"""Bounded queue over a versioned single-record Store (optimistic CAS loop)."""


class Queue:
    def __init__(self, store, capacity=2):
        self._store = store
        self._capacity = capacity

    def _update(self, kind, request_id, arg, apply):
        key = f"{kind}:{request_id}"
        while True:
            version, rec = self._store.load()
            if rec is None:
                rec = {"items": [], "done": {}}
            prior = rec["done"].get(key)
            if prior is not None and prior[0] == arg:
                return prior[1]
            result = apply(rec["items"])
            rec["done"][key] = [arg, result]
            if self._store.compare_swap(version, rec):
                return result

    def put(self, request_id, item):
        def apply(items):
            if len(items) >= self._capacity:
                return False
            items.append(item)
            return True

        return self._update("put", request_id, item, apply)

    def take(self, request_id):
        return self._update(
            "take", request_id, None, lambda items: items.pop(0) if items else None
        )

    def snapshot(self):
        _, rec = self._store.load()
        return list(rec["items"]) if rec else []
