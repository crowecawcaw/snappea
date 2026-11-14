"""Two-head publication through immutable blobs and one durable root CAS.

Each request increments both heads exactly once and retains its original result.
This is a bounded teaching protocol, not an unbounded blob store or garbage collector.
"""
from copy import deepcopy
from hashlib import sha256
import json
from threading import Lock


class BlobStore:
    def __init__(self, max_record_bytes=16 * 1024):
        self.blobs = {}
        self.version, self.root = 0, None
        self.lock, self.hook = Lock(), None
        self.max_record_bytes = max_record_bytes

    def _after(self, operation):
        if self.hook:
            self.hook(operation)

    def read_root(self):
        with self.lock:
            snapshot = self.version, self.root
        self._after('read-root')
        return snapshot

    def read_blob(self, key):
        with self.lock:
            return deepcopy(self.blobs[key])

    def stage(self, value):
        encoded = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
        if len(encoded) > self.max_record_bytes:
            raise ValueError('immutable record exceeds max_record_bytes')
        key = sha256(encoded).hexdigest()
        with self.lock:
            if key in self.blobs:
                assert self.blobs[key] == value, 'content hash collision'
            else:
                self.blobs[key] = deepcopy(value)
        self._after('stage')  # a crash here preserves the staged record
        return key

    def compare_root(self, version, root):
        with self.lock:
            if self.version != version:
                return False
            assert root in self.blobs
            self.version, self.root = version + 1, root
        self._after('root-cas')  # a crash here loses the response, not the commit
        return True


class Publisher:
    def __init__(self, store):
        self.store = store

    def _snapshot(self):
        version, root = self.store.read_root()
        if root is None:
            return version, [0, 0], {}
        manifest = self.store.read_blob(root)
        heads = [self.store.read_blob(key) for key in manifest['heads']]
        return version, heads, manifest['outcomes']

    def heads(self):
        return self._snapshot()[1]

    def publish(self, request_id):
        while True:
            version, heads, outcomes = self._snapshot()
            if request_id in outcomes:
                return outcomes[request_id]
            # Rebuild all version-dependent state after a failed root CAS.
            candidate = [value + 1 for value in heads]
            refs = [self.store.stage(value) for value in candidate]
            result = candidate[0]
            manifest = self.store.stage(dict(heads=refs, outcomes={**outcomes, request_id: result}))
            if self.store.compare_root(version, manifest):
                return result
