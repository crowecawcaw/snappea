from runtime import Store


class Queue:
    """Bounded FIFO sharing all state through a versioned Store.

    Record: {"items": [...], "puts": {key: bool}, "takes": {rid: int|None}}.
    Every operation is one load / compare_swap loop, so a request's effect and
    its recorded result land atomically; retries replay the recorded result.
    """

    def __init__(self, store, capacity=2):
        self.store = store
        self.capacity = capacity

    def _update(self, fn):
        while True:
            version, rec = self.store.load()
            if rec is None:
                rec = {"items": [], "puts": {}, "takes": {}}
            result = fn(rec)
            if self.store.compare_swap(version, rec):
                return result

    def put(self, request_id, item):
        key = repr((request_id, item))

        def fn(rec):
            if key in rec["puts"]:
                return rec["puts"][key]
            ok = len(rec["items"]) < self.capacity
            if ok:
                rec["items"].append(item)
            rec["puts"][key] = ok
            return ok

        return self._update(fn)

    def take(self, request_id):
        def fn(rec):
            if request_id in rec["takes"]:
                return rec["takes"][request_id]
            val = rec["items"].pop() if rec["items"] else None
            rec["takes"][request_id] = val
            return val

        return self._update(fn)

    def snapshot(self):
        _, rec = self.store.load()
        return [] if rec is None else list(rec["items"])
