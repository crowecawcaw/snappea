"""Realistic faulty queues: (name, defect, [(old, new), ...]) edits to reference.py."""

MUTANTS = [
    ("lifo", "takes the newest item",
     [('record["items"].pop(0)', 'record["items"].pop()')]),
    ("capacity-off-by-one", "accepts one item past capacity",
     [('len(record["items"]) < self.capacity', 'len(record["items"]) <= self.capacity')]),
    ("no-put-memo", "a retried put appends again",
     [('            if key in record["puts"]:\n                return record["puts"][key]\n', '')]),
    ("no-take-memo", "a retried take removes another item",
     [('            if request_id in record["takes"]:\n                return record["takes"][request_id]\n', '')]),
    ("rejection-not-memoized", "a retried rejected put succeeds once space frees",
     [('            record["puts"][key] = accepted', '            if accepted:\n                record["puts"][key] = accepted')]),
    ("empty-take-not-memoized", "a retried empty take removes a later item",
     [('            record["takes"][request_id] = result', '            if result is not None:\n                record["takes"][request_id] = result')]),
    ("shared-namespace", "a take with a put's request id returns the put's result",
     [('key = repr((request_id, item))', 'key = request_id'),
      ('record["takes"]', 'record["puts"]')]),
    ("ignore-cas-put", "a put that loses a compare-and-swap still reports success",
     [('            if self.store.compare_swap(version, record):\n                return accepted',
       '            self.store.compare_swap(version, record)\n            return accepted')]),
    ("ignore-cas-take", "a take that loses a compare-and-swap still returns an item",
     [('            if self.store.compare_swap(version, record):\n                return result',
       '            self.store.compare_swap(version, record)\n            return result')]),
    ("memo-in-instance", "retry memos live in the Queue object, not the Store",
     [('        self.capacity = capacity\n', '        self.capacity = capacity\n        self.memo = {}\n'),
      ('            if key in record["puts"]:\n                return record["puts"][key]\n',
       '            if key in self.memo:\n                return self.memo[key]\n'),
      ('            if self.store.compare_swap(version, record):\n                return accepted',
       '            if self.store.compare_swap(version, record):\n                self.memo[key] = accepted\n                return accepted'),
      ('            record["puts"][key] = accepted\n', '')]),
    ("stale-retry", "after a lost compare-and-swap, reuses the already-modified record",
     [('    def take(self, request_id):\n        while True:\n            version, record = self._load()',
       '    def take(self, request_id):\n        version, record = self._load()\n        while True:\n            version = self.store.load()[0]')]),
]


def apply(source, edits):
    for old, new in edits:
        if source.count(old) < 1:
            raise ValueError(f"edit does not apply: {old[:60]!r}")
        source = source.replace(old, new)
    return source
