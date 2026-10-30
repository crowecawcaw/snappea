"""Hidden implementation tests. Race tests force two calls to read the same version of a key."""
import threading
from concurrent.futures import ThreadPoolExecutor

from runtime import Store
from solution import LockService, Resource


class RaceStore:
    """Real Store whose first two loads of an armed key wait for each other."""

    def __init__(self):
        self.inner = Store()
        self.lock = threading.Lock()
        self.barrier = threading.Barrier(2, timeout=2)
        self.key = None
        self.pending = 0

    def arm(self, key_part):
        self.key, self.pending = key_part, 2

    def load(self, key):
        result = self.inner.load(key)
        with self.lock:
            wait = self.pending > 0 and self.key in str(key)
            self.pending -= wait
        if wait:
            try:
                self.barrier.wait()
            except threading.BrokenBarrierError:
                pass
        return result

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


def race(f, g):
    with ThreadPoolExecutor(2) as pool:
        a, b = pool.submit(f), pool.submit(g)
        return a.result(timeout=5), b.result(timeout=5)


def test_grant_and_exclusion():
    locks = LockService(Store(), ttl=10)
    t = locks.acquire("a", 0)
    assert isinstance(t, int)
    assert locks.acquire("b", 5) is None
    assert locks.acquire("a", 5) == t


def test_expiry_boundary():
    locks = LockService(Store(), ttl=10)
    t1 = locks.acquire("a", 0)
    assert locks.acquire("b", 9) is None
    t2 = locks.acquire("b", 10)
    assert t2 is not None and t2 > t1


def test_expired_holder_gets_new_token():
    locks = LockService(Store(), ttl=10)
    t1 = locks.acquire("a", 0)
    t2 = locks.acquire("a", 10)
    assert t2 is not None and t2 > t1


def test_tokens_increase_across_release():
    store = Store()
    locks = LockService(store, ttl=10)
    t1 = locks.acquire("a", 0)
    assert locks.release("a", t1, 1)
    t2 = LockService(store, ttl=10).acquire("b", 2)
    assert t2 > t1


def test_release_rules():
    locks = LockService(Store(), ttl=10)
    t1 = locks.acquire("a", 0)
    assert locks.release("b", t1, 1) is False
    assert locks.release("a", t1 + 1, 1) is False
    assert locks.release("a", t1, 10) is False
    t2 = locks.acquire("a", 10)
    assert locks.release("a", t1, 11) is False
    assert locks.acquire("b", 12) is None
    assert locks.release("a", t2, 12) is True
    assert locks.acquire("b", 12) is not None


def test_out_of_order_time_does_not_expire():
    locks = LockService(Store(), ttl=10)
    locks.acquire("a", 100)
    assert locks.acquire("b", 50) is None


def test_fencing_rejects_stale_and_unissued():
    store = Store()
    locks, res = LockService(store, ttl=10), Resource(store, "r")
    t1 = locks.acquire("a", 0)
    assert res.write(t1 + 1, "never issued") is False
    t2 = locks.acquire("b", 10)
    assert res.write(t2, "b1") is True
    assert res.write(t1, "stale") is False
    assert res.write(t2, "b2") is True
    assert res.read() == "b2"


def test_old_token_accepted_until_newer_write():
    store = Store()
    locks, res = LockService(store, ttl=10), Resource(store, "r")
    t1 = locks.acquire("a", 0)
    locks.acquire("b", 10)
    assert res.write(t1, "a-late") is True
    assert res.read() == "a-late"


def test_resources_fence_independently():
    store = Store()
    locks = LockService(store, ttl=10)
    t1 = locks.acquire("a", 0)
    t2 = locks.acquire("b", 10)
    assert Resource(store, "x").write(t2, "x") is True
    assert Resource(store, "y").write(t1, "y") is True
    assert Resource(store, "y").read() == "y" and Resource(store, "x").read() == "x"


def test_unwritten_resource_reads_none():
    assert Resource(Store(), "r").read() is None


def test_racing_acquires_one_winner():
    store = RaceStore()
    a, b = LockService(store, ttl=10), LockService(store, ttl=10)
    store.arm("lock")
    r = race(lambda: a.acquire("a", 0), lambda: b.acquire("b", 0))
    assert sum(x is not None for x in r) == 1


class InterferingStore:
    """Real Store that runs `hook` once, right after the first load of a matching key."""

    def __init__(self, inner, key_part, hook):
        self.inner, self.key_part, self.hook = inner, key_part, hook

    def load(self, key):
        result = self.inner.load(key)
        if self.hook and self.key_part in str(key):
            hook, self.hook = self.hook, None
            hook()
        return result

    def compare_swap(self, key, version, value):
        return self.inner.compare_swap(key, version, value)


def test_write_loses_to_newer_token_between_read_and_swap():
    store = Store()
    locks = LockService(store, ttl=10)
    t1 = locks.acquire("a", 0)
    t2 = locks.acquire("b", 10)
    sneaky = InterferingStore(store, "r", lambda: Resource(store, "r").write(t2, "new"))
    assert Resource(sneaky, "r").write(t1, "old") is False
    assert Resource(store, "r").read() == "new"


def test_write_retries_after_equal_token_interference():
    store = Store()
    t = LockService(store, ttl=10).acquire("a", 0)
    sneaky = InterferingStore(store, "r", lambda: Resource(store, "r").write(t, "first"))
    assert Resource(sneaky, "r").write(t, "second") is True
    assert Resource(store, "r").read() == "second"


def test_acquire_loses_to_racing_acquire_between_read_and_swap():
    store = Store()
    got = []
    sneaky = InterferingStore(store, "lock", lambda: got.append(LockService(store, 10).acquire("b", 0)))
    assert LockService(sneaky, 10).acquire("a", 0) is None
    assert got[0] is not None
