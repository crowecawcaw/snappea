# Design a fenced lease

Design and model a service that protects one stored integer with a lease. Two clients may interleave storage calls. Time is a logical clock; no wall-clock delays.

## Contract

- Initially: time 0, value 0, no lease. Client names are nonempty and never `-`; TTLs are positive; clock advances are nonnegative.
- `acquire(client, ttl)` succeeds only without an active lease. Return a fresh positive integer token greater than every previously issued token, with expiry `now + ttl`; otherwise return `-1`. Even the current owner cannot reacquire an active lease.
- A lease is active exactly when `now < expires_at`.
- `write(client, token, value)` changes the stored value and returns true only for the active owner and token. Rejected writes leave state unchanged.
- `release(client, token)` returns true and removes the lease only for the active owner and token; otherwise false, without changing state.
- `advance(ticks)` is an atomic test-control operation, serialized between other public calls, and advances logical time. `read()` returns the stored value. `peek()` returns the active `(owner, token, expires_at)` or `("-", -1, -1)`.
- Storage offers atomic `read` and versioned compare-and-swap of **one record**, not an atomic read/decide/write operation. Assume reliable storage; omit process crashes. Choose and justify which fields the record contains.

## Deliver

- `model.p`: executable design with state-owning machines and competing clients.
- `monitor.p`: independent `spec LeaseContract`, using the event declarations in `events.p`. Keep event declarations in `events.p`; private model events belong in `model.p`.
- `tests.p`: test cases selecting the design and monitor. `snappea check events.p monitor.p model.p tests.p` must run them.
- `DESIGN.md`: brief design, actual atomic boundary, modeled bounds/omissions, check results.

The supplied events describe public calls and their actual results in **linearization order**, not return-message order. Announce each event at the modeled operation's linearization point. The monitor checks the external contract, including permitted choices such as nonconsecutive increasing tokens; additional algorithm-specific invariants may live in the model. It must also work on those histories without your model. No Python implementation is requested.
