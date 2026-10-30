import threading
from collections import Counter

from runtime import Store
from solution import Queue


def test_basic_fifo_and_full():
    q = Queue(Store())
    assert q.put("a", 1) and q.put("b", 2)
    assert q.put("c", 3) is False
    assert q.snapshot() == [1, 2]
    assert q.take("t1") == 1 and q.take("t2") == 2
    assert q.take("t3") is None


def test_retries_idempotent():
    q = Queue(Store(), capacity=1)
    assert q.put("a", 1) is True
    assert q.put("a", 1) is True      # retry: no second insert
    assert q.put("b", 2) is False
    assert q.take("t") == 1
    assert q.take("t") == 1           # retry returns original
    assert q.put("b", 2) is False     # retry of failed put stays False
    assert q.snapshot() == []
    assert q.take("u") is None


def test_put_take_same_id_distinct():
    q = Queue(Store())
    assert q.put("x", 5)
    assert q.take("x") == 5


def test_shared_store_capacity():
    s = Store()
    a, b = Queue(s, 3), Queue(s, 3)
    for i in range(3):
        assert (a if i % 2 else b).put(f"p{i}", i)
    assert a.put("p9", 9) is False
    assert b.snapshot() == [0, 1, 2]


def test_threads_conserve_items():
    s = Store()
    cap, n = 4, 200
    taken, accepted = [], []
    lock = threading.Lock()

    def producer(k):
        q = Queue(s, cap)
        for i in range(n):
            item = k * 1000 + i
            rid = f"p{item}"
            ok = q.put(rid, item)
            assert q.put(rid, item) == ok
            if ok:
                with lock:
                    accepted.append(item)

    def consumer(k):
        q = Queue(s, cap)
        for i in range(n * 2):
            v = q.take(f"t{k}-{i}")
            assert q.take(f"t{k}-{i}") == v
            if v is not None:
                with lock:
                    taken.append(v)
            assert len(q.snapshot()) <= cap

    ts = [threading.Thread(target=producer, args=(k,)) for k in range(3)]
    ts += [threading.Thread(target=consumer, args=(k,)) for k in range(2)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    rest = Queue(s, cap).snapshot()
    assert Counter(taken + rest) == Counter(accepted)
    assert rest == sorted(rest, key=lambda x: (x % 1000)) or len(rest) <= cap
