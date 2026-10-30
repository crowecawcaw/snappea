# Test the implementation against what the model explored

The model checks every ordering of its clients. The implementation's tests should make
the same orderings happen on purpose. Starting threads and hoping rarely does: a race
window of a few microseconds is almost never hit.

## Force the interleaving

Wrap the real store in the test; the implementation does not change.

```python
import threading

class SameVersionStore:
    """The first two loads wait for each other, so both callers read the same
    version before either writes. One compare-and-swap must then fail."""
    def __init__(self, inner):
        self.inner, self.barrier, self.waiting = inner, threading.Barrier(2, timeout=2), 2
        self.lock = threading.Lock()
    def load(self, *key):
        result = self.inner.load(*key)
        with self.lock:
            wait, self.waiting = self.waiting > 0, self.waiting - 1
        if wait:
            self.barrier.wait()
        return result
    def compare_swap(self, *args):
        return self.inner.compare_swap(*args)

class InterferingStore:
    """Runs `competitor` once, between this caller's load and its compare-and-swap.
    The competitor uses the inner store, so it is not interrupted itself."""
    def __init__(self, inner, competitor):
        self.inner, self.competitor = inner, competitor
    def load(self, *key):
        result = self.inner.load(*key)
        if self.competitor:
            run, self.competitor = self.competitor, None
            run()
        return result
    def compare_swap(self, *args):
        return self.inner.compare_swap(*args)

def test_competing_spend_between_load_and_swap():
    inner = Store()
    sneaky = InterferingStore(inner, lambda: Wallet(inner).spend("other", 6))
    assert Wallet(sneaky).spend("mine", 6) is False  # must reload and re-decide
    assert Wallet(inner).balance() == 4
```

- Same version: catches ignoring the compare-and-swap result, deciding twice from one
  read, lost updates. Run the two calls in a `ThreadPoolExecutor(2)`; submit both
  before waiting on either result.
- Competitor between read and write: catches retrying with the stale value, or
  re-checking only the version.
- Assert both returned results and the state afterwards, read through the public API.
  With several keys, arm the wrapper only for the key under test.

## Replay requests after the state changed

For every operation that remembers a result, retry it after a different request
changed the state: an accepted request after its effect was undone by others (no
second effect); a rejected or empty result after the reason went away (still rejected
or empty); through a second object on the same store (the memo must live in the
store); the same request ID for two different operation kinds, if they are separate.

## Boundaries and preconditions

- Test each value exactly at, just below and just above each limit.
- For each acceptance condition, make only that condition false. When two conditions
  fail at once, one check can hide a missing other check.

## Check real calls against the P spec

```python
from snappea import check_events   # pip install pysnappea

events = []
ok = wallet.spend("a", 7)
events.append({"event": "eSpent", "payload": {"id": "a", "amount": 7, "ok": ok}})
events.append({"event": "eBalance", "payload": {"balance": wallet.balance()}})
check_events(events, "WalletContract", spec_files=["Wallet.p"], strict=True)
```

- Record only what really happened: actual results and public reads, in the order the
  calls returned. Under real concurrency, return order is not commit order; use the
  monitor on sequential or forced orderings, and plain assertions for thread stress.
  Do not add locks to the implementation to make events line up.
- Keep spec monitors in their own .p file so tests can load just the spec.
- Application code can instead call `snappea.emit(...)`; tests then check with
  `snappea.check_spec(...)`. For logs from a running system, use `snappea observe`.

## Prove the tests can fail

Make one deliberate bug at a time and run the tests: ignore the compare-and-swap
result; reuse the old value on retry; drop the memo for rejected results; keep the memo
in the object; flip `<` and `<=` at each limit; skip each precondition. Every bug should
fail some test. If one passes, add the test that would have caught it. Restore the code.
