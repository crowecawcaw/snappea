import threading
import pytest
from runtime import Store
from solution import Queue


def test_basic_fifo_and_full():
    q = Queue(Store())
    assert q.take("t0") is None
    assert q.put("a", 1) and q.put("b", 2)
    assert q.put("c", 3) is False
    assert q.snapshot() == [1, 2]          # rejected put had no effect
    assert q.take("t1") == 1 and q.take("t2") == 2 and q.take("t3") is None
    assert q.snapshot() == []


def test_zero_and_negative_items():
    q = Queue(Store(), capacity=3)
    for i, v in enumerate([0, -1, 5]):
        assert q.put(f"p{i}", v)
    assert [q.take(f"t{i}") for i in range(3)] == [0, -1, 5]


@pytest.mark.parametrize("cap", [1, 2, 5])
def test_capacity(cap):
    q = Queue(Store(), capacity=cap)
    assert [q.put(f"p{i}", i) for i in range(cap + 2)] == [True] * cap + [False] * 2


def test_put_retry_idempotent_after_other_progress():
    s = Store(); q = Queue(s); r = Queue(s)
    assert q.put("a", 1)
    assert r.put("b", 2)
    assert r.take("t") == 1
    assert q.put("a", 1) is True            # original result, no second insert
    assert q.snapshot() == [2]
    assert q.put("c", 3) and q.put("d", 4) is False
    assert q.put("d", 4) is False
    assert r.take("t2") == 2
    assert q.put("d", 4) is False           # original False even though room now
    assert q.snapshot() == [3]


def test_take_retry_idempotent():
    s = Store(); q = Queue(s)
    assert q.take("t") is None
    q.put("a", 1)
    assert q.take("t") is None              # original None, item stays
    assert q.snapshot() == [1]
    assert q.take("u") == 1
    q.put("b", 2)
    assert q.take("u") == 1                 # original result
    assert q.snapshot() == [2]


def test_put_and_take_same_id_distinct():
    q = Queue(Store())
    assert q.put("x", 7)
    assert q.take("x") == 7
    assert q.snapshot() == []


def test_shared_store_many_queue_objects():
    s = Store()
    assert Queue(s).put("a", 1)
    assert Queue(s).put("b", 2)
    assert Queue(s).put("c", 3) is False
    assert Queue(s).snapshot() == [1, 2]


def test_concurrent_conservation_with_retries():
    for cap in (1, 2, 3):
        s = Store()
        n = 6
        puts, takes = {}, {}
        def worker(i):
            q = Queue(s, cap)
            for rep in range(2):            # duplicate delivery
                r = q.put(f"p{i}", i)
                assert puts.setdefault(i, r) == r
            for rep in range(2):
                r = q.take(f"t{i}")
                assert takes.setdefault(i, r) == r
        ts = [threading.Thread(target=worker, args=(i,)) for i in range(n)]
        [t.start() for t in ts]; [t.join() for t in ts]
        ok = [i for i, r in puts.items() if r]
        got = [r for r in takes.values() if r is not None]
        left = Queue(s, cap).snapshot()
        assert len(ok) >= 1 and len(ok) <= n
        assert sorted(got + left) == sorted(ok)   # nothing lost or duplicated
        assert len(left) <= cap


def test_concurrent_capacity_never_exceeded():
    s = Store(); cap = 3
    res = []
    def w(i):
        res.append(Queue(s, cap).put(f"p{i}", i))
    ts = [threading.Thread(target=w, args=(i,)) for i in range(10)]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert sum(res) == cap and len(Queue(s, cap).snapshot()) == cap
