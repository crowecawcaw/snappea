# Build a shared FIFO queue

Read `runtime.py` and `events.p`. Model the queue contract before implementing it.

Implement `solution.Queue(store, capacity=2)` with synchronous methods:

- `put(request_id, item: int) -> bool` appends when there is room; otherwise returns
  `False`.
- `take(request_id) -> int | None` removes and returns the oldest item, or returns
  `None` when empty.
- `snapshot() -> list[int]` returns the queue's current contents in FIFO order.

All Queue instances sharing a Store use the same capacity. State must live in the Store;
instances may overlap, and correctness cannot depend on a Queue object's cache. A successful or
rejected `put` is memoized by string request ID and item: retrying that ID with the same item
returns its original result without another effect. `take` results, including `None`,
are memoized by request ID so a retry never removes a later item. Put and take IDs use
separate namespaces. Calls with the same ID and same arguments are the required retry
case. The queue starts empty; capacity is positive. The supplied observation schema
describes capacity-2 histories; the implementation must support any positive capacity.

Submit `model.p`, `monitor.p`, `tests.p`, `DESIGN.md`, `solution.py`, and a saved
`test_observation.py`. The model should represent competing clients and the actual
atomic Store operation. The monitor spec is named `QueueContract` and uses the
declarations in `events.p`; observation events must come from calls to the submitted
implementation and their public results.
Record serial calls in invocation order; concurrent event append order alone does not
establish the operation's linearization order.

Run model checks from this directory with:

```sh
snappea check events.p monitor.p model.p tests.p
```

Run the saved Python tests with:

```sh
python -m unittest -v
```
