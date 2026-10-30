"""Lock service with fencing tokens on top of runtime.Store (per-key load + CAS).

Design (modeled in Fencing.p):
  * One lock record under "lock": {last, holder, token, expiry}. `last` is the highest
    token ever issued; it is never reset, so tokens strictly increase. Every change is
    a load -> compute -> CAS retry loop.
  * One record per resource under "res:<name>": {max, value}; max = highest accepted token.
  * Resource.write checks "was issued" against the lock record's `last` (monotonic, so a
    slightly stale read can only under-report tokens issued after the caller's own
    acquire returned, which cannot affect a token that caller received), then CASes the
    resource record against the version it read.
Events are emitted after the operation's linearization point and never raise.
"""

try:
    from snappea import emit as _emit
except Exception:  # pragma: no cover - snappea is optional at runtime
    _emit = None

LOCK_KEY = "lock"


def _emit_safe(name, **fields):
    if _emit is None:
        return
    try:
        _emit(name, **fields)
    except Exception:
        pass


def _lock_record(value):
    return value or {"last": 0, "holder": None, "token": 0, "expiry": 0}


class LockService:
    def __init__(self, store, ttl: int):
        self.store = store
        self.ttl = ttl
        _emit_safe("eConfig", ttl=ttl)

    def _held(self, rec, now):
        """True when rec holds a lease that is unexpired at `now`."""
        return rec["holder"] is not None and now < rec["expiry"]

    def acquire(self, client: str, now: int):
        while True:
            version, value = self.store.load(LOCK_KEY)
            rec = _lock_record(value)
            if self._held(rec, now):
                if rec["holder"] == client:
                    _emit_safe("eLockOp", kind=1, who=client, now=now, token=rec["token"])
                    return rec["token"]
                _emit_safe("eLockOp", kind=2, who=client, now=now, token=-1)
                return None
            token = rec["last"] + 1
            new = {"last": token, "holder": client, "token": token, "expiry": now + self.ttl}
            if self.store.compare_swap(LOCK_KEY, version, new):
                _emit_safe("eLockOp", kind=0, who=client, now=now, token=token)
                return token

    def release(self, client: str, token: int, now: int) -> bool:
        while True:
            version, value = self.store.load(LOCK_KEY)
            rec = _lock_record(value)
            if not (self._held(rec, now) and rec["holder"] == client and rec["token"] == token):
                _emit_safe("eLockOp", kind=4, who=client, now=now, token=token)
                return False
            new = dict(rec, holder=None)
            if self.store.compare_swap(LOCK_KEY, version, new):
                _emit_safe("eLockOp", kind=3, who=client, now=now, token=token)
                return True


class Resource:
    def __init__(self, store, name: str):
        self.store = store
        self.key = "res:" + name

    def write(self, token: int, value: str) -> bool:
        while True:
            _, lock = self.store.load(LOCK_KEY)
            issued = _lock_record(lock)["last"]
            version, res = self.store.load(self.key)
            high = res["max"] if res else 0
            if not (1 <= token <= issued and token >= high):
                _emit_safe("eResOp", token=token, ok=False)
                return False
            if self.store.compare_swap(self.key, version, {"max": token, "value": value}):
                _emit_safe("eResOp", token=token, ok=True)
                return True

    def read(self):
        _, res = self.store.load(self.key)
        return res["value"] if res else None
