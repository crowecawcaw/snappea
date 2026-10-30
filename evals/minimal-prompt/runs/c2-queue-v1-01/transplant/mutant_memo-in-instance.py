from runtime import Store


class Queue:
    """Bounded FIFO over a shared Store using load/compare_swap retry loops.

    Record: {"items": [...], "done": {"put:<id>": bool, "take:<id>": int|None}}
    The result of each request is committed atomically with its effect, so
    retries (even racing ones) return the original result without re-applying.
    """

    def __init__(self, store, capacity=2):
        self.store = store
        self.capacity = capacity
        self._done = {}

    def _apply(self, key, op):
        while True:
            version, rec = self.store.load()
            if rec is None:
                rec = {"items": [], "done": {}}
            if key in self._done:
                return self._done[key]
            result = op(rec["items"])
            self._done[key] = result
            if self.store.compare_swap(version, rec):
                return result

    def put(self, request_id, item):
        def op(items):
            if len(items) >= self.capacity:
                return False
            items.append(item)
            return True
        return self._apply("put:" + request_id, op)

    def take(self, request_id):
        def op(items):
            return items.pop(0) if items else None
        return self._apply("take:" + request_id, op)

    def snapshot(self):
        _, rec = self.store.load()
        return [] if rec is None else list(rec["items"])
