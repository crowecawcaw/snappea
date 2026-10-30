# Test the implementation against what the model explored

The model checks every ordering of its clients. The implementation's tests should
make the same orderings happen on purpose. Starting threads and hoping rarely does:
a race window of a few microseconds is almost never hit.

Runnable example: [forced-interleaving](../examples/forced-interleaving/test_wallet.py).

## Force the interleaving

Wrap the real store in the test; the implementation does not change.

- **Both read the same version.** The first two `load` calls wait on a
  `threading.Barrier(2)`, then return. Both callers decide from the same state, and one
  compare-and-swap must fail and retry. Catches: ignoring the compare-and-swap result,
  deciding twice from one read, lost updates.
- **A competitor between read and write.** Right after this caller's first `load`,
  run a complete competing call against the inner store. This caller's
  compare-and-swap fails; on retry it must re-read and re-decide (now maybe reject).
  Catches: retrying with the stale value, re-checking only the version.

Assert both the returned results and the state afterwards, read through the public
API. With per-key stores, arm the wrapper only for the key under test.

## Replay requests after the state changed

For every operation that remembers a result, retry it after a different request
changed the state:

- retry an accepted request after its effect was undone by others (no second effect);
- retry a rejected or empty result after the reason went away (still rejected/empty);
- retry through a second object on the same store (the memo must live in the store);
- use the same request ID for two different operation kinds, if they are separate.

## Boundaries and preconditions

- Test each value exactly at, just below and just above each limit (capacity, expiry
  time, token order).
- For each acceptance condition, make only that condition false. When two conditions
  fail at once, one check can hide a missing other check.

## Check real calls against the P spec

Record actual arguments and results of real calls as events, then check them with the
same spec monitor the model uses:

```python
from snappea import check_events

events = []
ok = wallet.spend("a", 7)
events.append({"event": "eSpent", "payload": {"id": "a", "amount": 7, "ok": ok}})
events.append({"event": "eBalance", "payload": {"balance": wallet.balance()}})
check_events(events, "WalletContract", spec_files=["Wallet.p"], strict=True)
```

- Record in the test (as above) or `emit` from the application inside
  `snappea.check_spec(...)`; see [implementation](implementation.md).
- Record only what really happened: actual results and public reads, in the order the
  calls returned. Under real concurrency, return order is not commit order; use the
  monitor on sequential or forced orderings, and plain assertions for thread stress.
- Keep spec monitors in their own `.p` file so tests can load just the spec.

## Prove the tests can fail

Make one deliberate bug at a time in a copy of the implementation and run the tests:

```sh
cp solution.py /tmp/good.py
sed -i 's/if self.store.compare_swap(version, new):/if self.store.compare_swap(version, new) or True:/' solution.py
python -m pytest -q; cp /tmp/good.py solution.py
```

Try: ignore the compare-and-swap result; reuse the old value on retry; drop the memo
for rejected results; keep the memo in the object; flip `<` and `<=` at each limit;
skip each precondition. Every bug should fail some test. If one passes, add the test
that would have caught it.
