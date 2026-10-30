import threading
from runtime import Store
from solution import Queue


def test_basic():
    q = Queue(Store(), 2)
    assert q.put("a", 1) and q.put("b", 2)
    assert q.put("c", 3) is False
    assert q.snapshot() == [1, 2]
    assert q.take("t1") == 1
    assert q.take("t2") == 2
    assert q.take("t3") is None


def test_idempotent():
    q = Queue(Store(), 2)
    assert q.put("a", 1)
    assert q.put("a", 1)
    assert q.snapshot() == [1]
    assert q.take("a") == 1  # take distinct from put
    assert q.take("a") == 1
    assert q.snapshot() == []
    assert q.put("x", 5) and q.put("y", 6) and q.put("z", 7) is False
    assert q.put("z", 7) is False
    q.take("t")
    assert q.put("z", 7) is False  # original result retained


def test_capacity_any():
    q = Queue(Store(), 5)
    assert all(q.put(str(i), i) for i in range(5))
    assert not q.put("n", 9)


def test_threads():
    s = Store()
    got, lock = [], threading.Lock()

    def prod(i):
        q = Queue(s, 3)
        for k in range(50):
            a = 0
            while not q.put(f"p{i}-{k}-{a}", i * 100 + k):
                a += 1

    def cons(i):
        q = Queue(s, 3)
        n = a = 0
        while n < 50:
            a += 1
            r = q.take(f"c{i}-{a}")
            if r is not None:
                with lock:
                    got.append(r)
                n += 1

    ts = [threading.Thread(target=prod, args=(i,)) for i in range(2)] + \
         [threading.Thread(target=cons, args=(i,)) for i in range(2)]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert sorted(got) == sorted(i * 100 + k for i in range(2) for k in range(50))
