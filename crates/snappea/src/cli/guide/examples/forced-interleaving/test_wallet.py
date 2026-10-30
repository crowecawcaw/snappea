"""Turn model interleavings into deterministic tests, and check real calls against the spec."""
import threading
from concurrent.futures import ThreadPoolExecutor

from snappea import check_events

from wallet import Store, Wallet


class SameVersionStore:
    """Real store; the first two loads wait for each other, so both callers read the
    same version before either writes. One compare-and-swap must then fail."""

    def __init__(self, inner):
        self.inner, self.barrier, self.waiting = inner, threading.Barrier(2, timeout=2), 2
        self.lock = threading.Lock()

    def load(self):
        result = self.inner.load()
        with self.lock:
            wait, self.waiting = self.waiting > 0, self.waiting - 1
        if wait:
            self.barrier.wait()
        return result

    def compare_swap(self, version, value):
        return self.inner.compare_swap(version, value)


class InterferingStore:
    """Real store that runs `competitor` once, between this caller's load and its
    compare-and-swap. The competitor uses the inner store, so it is not interrupted."""

    def __init__(self, inner, competitor):
        self.inner, self.competitor = inner, competitor

    def load(self):
        result = self.inner.load()
        if self.competitor:
            run, self.competitor = self.competitor, None
            run()
        return result

    def compare_swap(self, version, value):
        return self.inner.compare_swap(version, value)


class Recorder:
    """Wraps real calls; records actual arguments and results as spec events."""

    def __init__(self, wallet):
        self.wallet, self.events = wallet, []

    def spend(self, request_id, amount):
        ok = self.wallet.spend(request_id, amount)
        self.events.append({"event": "eSpent", "payload": {"id": request_id, "amount": amount, "ok": ok}})
        self.events.append({"event": "eBalance", "payload": {"balance": self.wallet.balance()}})
        return ok


def test_both_read_same_version_only_one_spend_fits():
    store = SameVersionStore(Store())
    a, b = Wallet(store), Wallet(store)
    with ThreadPoolExecutor(2) as pool:
        futures = [pool.submit(a.spend, "a", 7), pool.submit(b.spend, "b", 7)]
        results = sorted(f.result(timeout=5) for f in futures)
    assert results == [False, True]
    assert a.balance() == 3


def test_competing_spend_between_load_and_swap():
    inner = Store()
    sneaky = InterferingStore(inner, lambda: Wallet(inner).spend("other", 6))
    assert Wallet(sneaky).spend("mine", 6) is False  # must reload and see balance 4
    assert Wallet(inner).balance() == 4


def test_rejected_retry_stays_rejected_after_state_changes():
    store = Store()
    first, second = Wallet(store), Wallet(store)
    rec = Recorder(first)
    rec.spend("big", 8)
    rec.spend("too-much", 5)        # rejected: balance is 2
    rec.spend("big", 8)             # retry: same result, no effect
    rec.wallet = second             # a second object on the same store
    rec.spend("too-much", 5)        # retry through another object: still rejected
    check_events(rec.events, "WalletContract", spec_files=["Wallet.p"], strict=True)
