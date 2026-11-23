---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - How do I force the interleavings the model explored in implementation tests?
  - Which retry, boundary and precondition tests should the implementation have?
  - How do I show that my implementation tests can fail?
---

# Test the implementation against what the model explored

Force each interleaving the model found; random threads almost never hit a race.
Wrap the real store in the test, not the implementation.

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

- Same version catches ignored CAS results, deciding twice from one read, lost
  updates. Run both calls in a `ThreadPoolExecutor(2)`; submit both before waiting.
- Competitor between read and write catches retrying with the stale value.
- Assert both results and the state afterwards via the public API.

## Replay, limits, preconditions

- Retry each operation that remembers a result after another request changed the
  state: an accepted one whose effect others undid (no second effect); a rejected one
  after the reason went away (still rejected); through a second object on the same
  store (the memo must live in the store).
- Each limit just below, at and just above. Each precondition false on its own.
- Check real calls against the P spec: `snappea guide python`.
  Go, Java, Rust: https://snappea.dev/docs/go.html, https://snappea.dev/docs/java.html,
  https://snappea.dev/docs/rust.html.

## Prove the tests can fail

Make one deliberate bug at a time: ignore the CAS result; reuse the old value on
retry; drop the memo for rejections; keep the memo in the object; flip `<` and `<=`;
skip a precondition; remove the timeout or lease expiry. Each must fail a test; if
not, add the test. Restore the code.

Runnable: `snappea guide example forced-interleaving` (these wrappers),
`snappea guide example cas-queue` (model, Python queue, pytest regressions).
