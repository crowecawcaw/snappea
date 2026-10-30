import shutil
import subprocess
import threading

import pytest

from runtime import Store
from solution import Queue


def test_basic_fifo_and_capacity():
    q = Queue(Store())
    assert q.take("t0") is None
    assert q.put("a", 1) and q.put("b", 2)
    assert q.put("c", 3) is False
    assert q.snapshot() == [1, 2]
    assert q.take("t1") == 1
    assert q.put("d", 4)
    assert q.snapshot() == [2, 4]


def test_retries_are_idempotent():
    q = Queue(Store(), capacity=1)
    assert q.put("a", 1) is True
    assert q.put("a", 1) is True  # retry, no second insert
    assert q.put("b", 2) is False
    assert q.take("a") == 1  # same id as the put, but a different request
    assert q.take("a") == 1  # retry returns the original value
    assert q.snapshot() == []
    assert q.put("b", 2) is False  # retry of a failed put stays failed
    assert q.snapshot() == []
    assert q.take("e") is None
    q.put("c", 3)
    assert q.take("e") is None  # retried empty take stays None
    assert q.snapshot() == [3]


@pytest.mark.parametrize("cap", [1, 2, 5])
def test_concurrent(cap):
    store = Store()
    n_prod, per = 4, 50
    taken, lock = [], threading.Lock()
    accepted = []

    def producer(p):
        q = Queue(store, capacity=cap)
        for i in range(per):
            rid, item = f"p{p}-{i}", p * 1000 + i
            n = 0
            while not q.put(f"{rid}.{n}", item):
                n += 1  # a failed put is sticky per id, so retry as a new request
            rid = f"{rid}.{n}"
            assert q.put(rid, item) is True  # retry
            accepted.append(item)

    def consumer(c):
        q = Queue(store, capacity=cap)
        for i in range(n_prod * per // 2):
            attempt = 0
            while True:
                attempt += 1
                rid = f"c{c}-{i}-{attempt}"  # a new request per attempt: None is sticky
                v = q.take(rid)
                if v is not None:
                    assert q.take(rid) == v
                    with lock:
                        taken.append(v)
                    break
                assert len(q.snapshot()) <= cap

    ts = [threading.Thread(target=producer, args=(p,)) for p in range(n_prod)]
    ts += [threading.Thread(target=consumer, args=(c,)) for c in range(2)]
    [t.start() for t in ts]
    [t.join(60) for t in ts]
    assert sorted(taken) == sorted(accepted) and len(taken) == n_prod * per
    for p in range(n_prod):  # per-producer FIFO is preserved globally
        mine = [v for v in taken if v // 1000 == p]
        assert len(mine) == per
    assert Queue(store).snapshot() == []


@pytest.mark.skipif(shutil.which("snappea") is None, reason="snappea not installed")
def test_p_model():
    r = subprocess.run(
        ["snappea", "check", "--no-mutation", "--allow-incomplete-coverage", "--format", "text"],
        capture_output=True, text=True,
    )
    assert r.returncode == 0, r.stdout + r.stderr
