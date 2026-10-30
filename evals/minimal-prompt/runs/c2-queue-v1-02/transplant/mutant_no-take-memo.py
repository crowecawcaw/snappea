"""Shared bounded queue kept in one versioned runtime.Store record.

Record: {"items": [int], "memo": {"put:<id>"/"take:<id>": [arg, result]}}.
Queue contents and the request memo commit in one compare_swap, so a retry
sees either both the effect and its memo or neither.
"""


class Queue:
    def __init__(self, store, capacity=2):
        if capacity < 1:
            raise ValueError("capacity must be positive")
        self._store = store
        self._capacity = capacity

    def _apply(self, key, arg, op):
        while True:
            version, rec = self._store.load()
            if rec is None:
                rec = {"items": [], "memo": {}}
            hit = rec["memo"].get(key)
            if hit is not None and not key.startswith("take:"):
                if hit[0] != arg:
                    raise ValueError("request_id %r reused with different arguments" % key)
                return hit[1]
            result = op(rec["items"])
            rec["memo"][key] = [arg, result]
            if self._store.compare_swap(version, rec):
                return result

    def put(self, request_id, item):
        def op(items):
            if len(items) >= self._capacity:
                return False
            items.append(item)
            return True
        return self._apply("put:%s" % (request_id,), item, op)

    def take(self, request_id):
        return self._apply("take:%s" % (request_id,), None,
                           lambda items: items.pop(0) if items else None)

    def snapshot(self):
        _, rec = self._store.load()
        return [] if rec is None else list(rec["items"])
