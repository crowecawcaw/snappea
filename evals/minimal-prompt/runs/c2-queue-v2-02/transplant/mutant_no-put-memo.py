"""Shared bounded queue over a versioned single-record store.

The record is {"items": [...], "puts": {id: bool}, "takes": {id: int|None}}.
Result memos live in the same record as the items, so one compare_swap commits
the queue change and the memo together. Every call is load -> decide -> CAS,
retried from a fresh load when the CAS loses.
"""


class Queue:
    def __init__(self, store, capacity=2):
        if capacity < 1:
            raise ValueError("capacity must be a positive integer")
        self._store = store
        self._capacity = capacity

    @staticmethod
    def _fresh(record):
        if record is None:
            return {"items": [], "puts": {}, "takes": {}}
        return record

    def put(self, request_id, item):
        while True:
            version, record = self._store.load()
            record = self._fresh(record)
            ok = len(record["items"]) < self._capacity
            if ok:
                record["items"].append(item)
            record["puts"][request_id] = ok  # rejections are memoized too
            if self._store.compare_swap(version, record):
                return ok

    def take(self, request_id):
        while True:
            version, record = self._store.load()
            record = self._fresh(record)
            if request_id in record["takes"]:
                return record["takes"][request_id]
            result = record["items"].pop(0) if record["items"] else None
            record["takes"][request_id] = result  # empty results are memoized too
            if self._store.compare_swap(version, record):
                return result

    def snapshot(self):
        _, record = self._store.load()
        return list(self._fresh(record)["items"])
