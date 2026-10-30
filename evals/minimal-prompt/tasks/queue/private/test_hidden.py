"""Hidden implementation tests. Each race test forces two calls to read the same version."""
import threading
from concurrent.futures import ThreadPoolExecutor

import pytest

from runtime import Store
from solution import Queue


class RaceStore:
    """Real Store whose first two loads after arm() wait for each other."""

    def __init__(self):
        self.inner = Store()
        self.lock = threading.Lock()
        self.barrier = threading.Barrier(2, timeout=2)
        self.pending = 0

    def arm(self):
        self.pending = 2

    def load(self):
        result = self.inner.load()
        with self.lock:
            wait = self.pending > 0
            self.pending -= wait
        if wait:
            try:
                self.barrier.wait()
            except threading.BrokenBarrierError:
                pass
        return result

    def compare_swap(self, version, record):
        return self.inner.compare_swap(version, record)


def race(store, f, g):
    store.arm()
    with ThreadPoolExecutor(2) as pool:
        a, b = pool.submit(f), pool.submit(g)
        return a.result(timeout=5), b.result(timeout=5)


def test_fifo_order_and_capacity():
    q = Queue(Store())
    assert q.put("p1", 10) and q.put("p2", 20)
    assert q.put("p3", 30) is False
    assert q.snapshot() == [10, 20]
    assert q.take("t1") == 10 and q.take("t2") == 20
    assert q.take("t3") is None
    assert q.snapshot() == []


def test_capacity_three():
    q = Queue(Store(), capacity=3)
    assert all(q.put(f"p{i}", i) for i in range(3))
    assert q.put("p4", 4) is False
    assert q.snapshot() == [0, 1, 2]


def test_retried_put_after_take_has_no_effect():
    q = Queue(Store())
    assert q.put("same", 7) and q.take("t") == 7
    assert q.put("same", 7) is True
    assert q.snapshot() == []


def test_retried_rejected_put_stays_rejected():
    q = Queue(Store())
    q.put("a", 1), q.put("b", 2)
    assert q.put("full", 3) is False
    assert q.take("t") == 1
    assert q.put("full", 3) is False
    assert q.snapshot() == [2]


def test_retried_empty_take_stays_empty():
    q = Queue(Store())
    assert q.take("empty") is None
    assert q.put("later", 8)
    assert q.take("empty") is None
    assert q.snapshot() == [8]


def test_retried_take_returns_same_item():
    q = Queue(Store())
    q.put("a", 1), q.put("b", 2)
    assert q.take("t") == 1
    assert q.take("t") == 1
    assert q.snapshot() == [2]


def test_put_and_take_ids_are_separate():
    q = Queue(Store())
    assert q.put("x", 4) and q.take("x") == 4
    assert q.put("y", 5)
    assert q.take("x") == 4
    assert q.snapshot() == [5]


def test_retry_through_another_instance():
    store = Store()
    a, b = Queue(store), Queue(store)
    assert a.put("p", 1) and b.take("t") == 1
    assert b.put("p", 1) is True and a.take("t") == 1
    assert a.snapshot() == []


def test_racing_puts_both_land():
    store = RaceStore()
    a, b = Queue(store), Queue(store)
    assert race(store, lambda: a.put("a", 1), lambda: b.put("b", 2)) == (True, True)
    assert sorted(a.snapshot()) == [1, 2]


def test_racing_puts_for_last_slot():
    store = RaceStore()
    a, b = Queue(store), Queue(store)
    a.put("first", 0)
    r = race(store, lambda: a.put("a", 1), lambda: b.put("b", 2))
    assert sorted(r) == [False, True]
    assert len(a.snapshot()) == 2


def test_racing_takes_get_different_items():
    store = RaceStore()
    a, b = Queue(store), Queue(store)
    a.put("p1", 1), a.put("p2", 2)
    assert sorted(race(store, lambda: a.take("x"), lambda: b.take("y"))) == [1, 2]
    assert a.snapshot() == []


def test_racing_take_and_put():
    store = RaceStore()
    a, b = Queue(store), Queue(store)
    a.put("p1", 1), a.put("p2", 2)
    got, put = race(store, lambda: a.take("t"), lambda: b.put("p3", 3))
    assert got == 1
    assert a.snapshot() == ([2, 3] if put else [2])
    assert a.take("t") == 1
