# Add tests for a stable priority queue

Read `queue_service.py`, `events.p`, and `contract.p`. Add a saved Python unittest module
named `test_queue.py` that checks the service and validates observed operations
with the P monitor. Do not modify the supplied files.

`Service` stores unique string item IDs with signed integer priorities. IDs are
nonempty and cannot be `"-"`. Lower numbers have higher priority. `submit(id,
priority)` returns `True` for a new ID and `False` for an ID already queued; a
rejected duplicate has no effect. `pop()` returns the queued ID with the lowest
priority, choosing the earliest admission among ties, or `None` when empty.
`reprioritize(id, priority)` changes an existing item's priority and returns
`True`, while preserving its admission order; it returns `False` for a missing
ID. `cancel(id)` removes an existing item and returns `True`, or returns `False`
for a missing ID. Cancelling and submitting that ID again gives it a fresh
admission order.

The service has no instrumentation. Capture actual public calls and their
results in the test wrapper. Encode an empty `pop()` result as `"-"` in `ePop`.
Use `eSubmit(item_id, priority, accepted)`, `eReprioritize(item_id, priority,
accepted)`, and `eCancel(item_id, accepted)` for the other calls. Pass the full
history to `check_events` with `events.p` and `contract.p`, strict checking, and
unknown events as errors. Run the saved tests from this directory with
`python -m unittest -v`.
