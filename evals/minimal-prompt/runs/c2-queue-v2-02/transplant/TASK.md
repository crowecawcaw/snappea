# Shared bounded queue

Implement `Queue` in `solution.py`, keeping all state in `runtime.Store` (see `runtime.py`).

```python
Queue(store, capacity=2)
put(request_id: str, item: int) -> bool   # False when the queue is full
take(request_id: str) -> int | None       # removes the oldest item; None when empty
snapshot() -> list[int]                   # current contents, oldest first
```

- Many `Queue` objects in many threads may share one `Store`; they all act on one
  queue with the same capacity (any positive integer).
- Clients retry after timeouts. Repeating a `put` or `take` with the same `request_id`
  and arguments returns the original result and has no further effect. A `put` and a
  `take` with the same `request_id` are different requests.

Use P to model the design before implementing it, and to test the implementation.
`snappea` and the `pysnappea` Python package are installed. Keep your P files and
tests in this directory; the tests must run with `python -m pytest`.
