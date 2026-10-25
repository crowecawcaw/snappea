# Add tests for a lease service

Read `lease.py`, `events.p`, and `lease_contract.p`. Add a saved
Python unittest module named `test_lease.py` that checks the service and validates
observed operations with the P monitor. Do not modify the supplied files. Test the contract; token allocation may vary while remaining strictly increasing.

`Service` manages one resource, initially integer value `0`, with a deterministic
clock. Clients are nonempty strings other than `"-"`; TTLs are positive integers,
ticks are nonnegative integers, and values are integers. `acquire(client, ttl)` returns
a monotonically increasing token if no lease is active, or `None` otherwise. A lease
is inactive when `expires_at <= now`; its owner cannot acquire again while it is
active. `write(client, token, value)` succeeds only for the active owner and token.
`release(client, token)` clears only that matching lease. `advance(ticks)` increases
the clock, `read()` returns the value, and `peek()` returns `(owner, token, expires_at)`
for an active lease or `None` otherwise.

The service has no instrumentation. Capture actual public calls and their results in
the test wrapper. Use these event fields:

- `eAcquire(client, ttl, token)`; encode a `None` result as token `-1`.
- `eWrite(client, token, value, accepted)`.
- `eRelease(client, token, accepted)`.
- `eAdvance(ticks)`.
- `eRead(value)`.
- `ePeek(owner, token, expires_at)`; encode `None` as `("-", -1, -1)`.

Run the saved tests from this directory with `python -m unittest -v`.
