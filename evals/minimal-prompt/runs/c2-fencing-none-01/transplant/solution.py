"""Lock service with fencing tokens, over runtime.Store.

All lock state is one record ("lock"), changed only by compare-and-swap, so
concurrent acquire/release calls are linearised by the store. Each resource is
one record too ("res:<name>"). Events are emitted for the P specs in model/;
emit is best-effort and never raises into application code.
"""

try:
    from snappea import emit as _emit
except Exception:  # snappea not installed: instrumentation off
    _emit = None

LOCK_KEY = "lock"


def _emit_safe(event, payload=None, *, store):
    if _emit is None:
        return
    try:
        _emit(event, payload, key=id(store)) if not isinstance(payload, dict) else _emit(
            event, key=id(store), **payload)
    except Exception:
        pass


def _fresh():
    # last: highest token ever issued; holder/token/expiry describe the current lease.
    return {"last": 0, "holder": None, "token": 0, "expiry": 0}


class LockService:
    def __init__(self, store, ttl):
        self._store = store
        self._ttl = ttl

    def acquire(self, client, now):
        while True:
            version, rec = self._store.load(LOCK_KEY)
            rec = rec or _fresh()
            if rec["holder"] is not None and now < rec["expiry"]:
                if rec["holder"] == client:
                    return rec["token"]
                return None
            token = rec["last"] + 1
            new = {"last": token, "holder": client, "token": token, "expiry": now + self._ttl}
            if self._store.compare_swap(LOCK_KEY, version, new):
                _emit_safe("eIssued", token, store=self._store)
                _emit_safe("eGrant", {"client": client, "token": token, "now": now,
                                      "expiry": new["expiry"]}, store=self._store)
                return token

    def release(self, client, token, now):
        while True:
            version, rec = self._store.load(LOCK_KEY)
            if (rec is None or rec["holder"] != client or rec["token"] != token
                    or not now < rec["expiry"]):
                return False
            new = dict(rec, holder=None)
            if self._store.compare_swap(LOCK_KEY, version, new):
                _emit_safe("eReleased", token, store=self._store)
                return True


class Resource:
    def __init__(self, store, name):
        self._store = store
        self._key = "res:" + name

    def write(self, token, value):
        if isinstance(token, bool) or not isinstance(token, int):
            return False
        while True:
            # Tokens only grow, so a stale read of "last" can only under-accept
            # tokens that were issued after the read; re-read each attempt.
            _, lock = self._store.load(LOCK_KEY)
            if token < 1 or token > (lock["last"] if lock else 0):
                return False
            version, rec = self._store.load(self._key)
            rec = rec or {"max": 0, "value": None}
            if token < rec["max"]:
                return False
            if self._store.compare_swap(self._key, version, {"max": token, "value": value}):
                _emit_safe("eWriteOk", token, store=self._store)
                return True

    def read(self):
        _, rec = self._store.load(self._key)
        return rec["value"] if rec else None
