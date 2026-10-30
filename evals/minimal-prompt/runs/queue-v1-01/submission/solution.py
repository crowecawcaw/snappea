"""Shared bounded queue over a versioned single-record Store.

Record: {"items": [...], "puts": {rid: [item, result]}, "takes": {rid: result}}.
Every operation, including rejected ones (full / empty), is one load + compare_swap
that also records the result under its request id, so a retry returns the original
result even after the queue has changed. The result log grows with the number of
distinct request ids (it cannot be pruned without a client-side retry horizon).
"""

from snappea import emit


class Queue:
    def __init__(self, store, capacity=2):
        if capacity < 1:
            raise ValueError("capacity must be positive")
        self.store = store
        self.capacity = capacity

    @staticmethod
    def _fresh(rec):
        if rec is None:
            return {"items": [], "puts": {}, "takes": {}}
        return rec

    def put(self, request_id, item):
        while True:
            version, rec = self.store.load()
            rec = self._fresh(rec)
            prior = rec["puts"].get(request_id)
            if prior is not None and prior[0] == item:
                return prior[1]  # retry: original result, no effect
            ok = len(rec["items"]) < self.capacity
            if ok:
                rec["items"].append(item)
            rec["puts"][request_id] = [item, ok]
            if self.store.compare_swap(version, rec):
                emit("eCommit", kind=0, rid=request_id, arg=item, result=int(ok))
                return ok

    def take(self, request_id):
        while True:
            version, rec = self.store.load()
            rec = self._fresh(rec)
            if request_id in rec["takes"]:
                return rec["takes"][request_id]
            result = rec["items"].pop(0) if rec["items"] else None
            rec["takes"][request_id] = result
            if self.store.compare_swap(version, rec):
                emit("eCommit", kind=1, rid=request_id, arg=0,
                     result=-1 if result is None else result)
                return result

    def snapshot(self):
        _, rec = self.store.load()
        return list(self._fresh(rec)["items"])
