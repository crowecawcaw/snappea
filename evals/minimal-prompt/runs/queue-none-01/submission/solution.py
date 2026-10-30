"""Bounded queue over a versioned compare-and-swap Store."""


class Queue:
    def __init__(self, store, capacity=2):
        self._store = store
        self._cap = capacity

    @staticmethod
    def _fresh(rec):
        return rec if rec is not None else {"items": [], "puts": {}, "takes": {}}

    def _update(self, fn):
        while True:
            version, rec = self._store.load()
            rec = self._fresh(rec)
            result = fn(rec)
            if result[1] is False:  # no change needed
                return result[0]
            if self._store.compare_swap(version, rec):
                return result[0]

    def put(self, request_id, item):
        def fn(rec):
            seen = rec["puts"].get(request_id)
            if seen is not None:
                return seen[1], False
            ok = len(rec["items"]) < self._cap
            if ok:
                rec["items"].append(item)
            rec["puts"][request_id] = [item, ok]
            return ok, True
        return self._update(fn)

    def take(self, request_id):
        def fn(rec):
            if request_id in rec["takes"]:
                return rec["takes"][request_id], False
            res = rec["items"].pop(0) if rec["items"] else None
            rec["takes"][request_id] = res
            return res, True
        return self._update(fn)

    def snapshot(self):
        _, rec = self._store.load()
        return list(self._fresh(rec)["items"])
