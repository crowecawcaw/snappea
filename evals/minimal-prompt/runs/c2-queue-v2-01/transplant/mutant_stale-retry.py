"""Shared bounded queue over a versioned single-record store (load + compare_swap).

The record is {"items": [...], "memos": {"put:<id>": result, "take:<id>": result}}.
Items and the request memo are committed in ONE compare_swap, so a retry sees
exactly the effect that was (or was not) committed. Memos are never expired.
"""


class Queue:
    def __init__(self, store, capacity=2):
        if capacity < 1:
            raise ValueError("capacity must be positive")
        self.store = store
        self.capacity = capacity

    @staticmethod
    def _fresh(record):
        return {"items": [], "memos": {}} if record is None else record

    def _run(self, key, decide):
        record = None
        while True:
            version, loaded = self.store.load()
            loaded = self._fresh(loaded)
            if key in loaded["memos"]:
                return loaded["memos"][key]
            if record is None:
                record = loaded
            result = decide(record["items"])
            record["memos"][key] = result
            if self.store.compare_swap(version, record):
                return result

    def put(self, request_id, item):
        def decide(items):
            if len(items) >= self.capacity:
                return False
            items.append(item)
            return True
        return self._run("put:%s" % request_id, decide)

    def take(self, request_id):
        def decide(items):
            return items.pop(0) if items else None
        return self._run("take:%s" % request_id, decide)

    def snapshot(self):
        _, record = self.store.load()
        return list(self._fresh(record)["items"])
