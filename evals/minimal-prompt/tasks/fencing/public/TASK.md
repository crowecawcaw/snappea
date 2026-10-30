# Lock service with fencing tokens

Implement `LockService` and `Resource` in `solution.py`, keeping all state in
`runtime.Store` (see `runtime.py`).

```python
LockService(store, ttl: int)
acquire(client: str, now: int) -> int | None
release(client: str, token: int, now: int) -> bool

Resource(store, name: str)
write(token: int, value: str) -> bool
read() -> str | None
```

- `acquire` grants the lock when no one holds an unexpired lease, and returns a fencing
  token. A lease granted at `now` expires at `now + ttl`; it is expired for any
  `now >= now + ttl`. Tokens are integers that strictly increase over the life of the
  store. If `client` already holds the unexpired lease, `acquire` returns that lease's
  token. Otherwise it returns `None`.
- `release` ends the lease if `client` holds the lease with `token` and it is unexpired at
  `now`, and returns whether it did.
- A `Resource` is protected by fencing: `write` rejects (returns `False`, no effect) a
  token that was never issued, or a token lower than one already used for an accepted
  write to that resource. Otherwise it stores `value` and returns `True`.
- Many `LockService` and `Resource` objects in many threads may share one `Store`; all
  `LockService` objects for a store use the same `ttl`. Callers supply `now`, and calls
  from different clients may arrive with out-of-order times.

Use P to model the design before implementing it, and to test the implementation.
`snappea` and the `pysnappea` Python package are installed. Keep your P files and
tests in this directory; the tests must run with `python -m pytest`.
