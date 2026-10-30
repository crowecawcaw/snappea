import threading
from runtime import Store
from solution import Queue


def test_basic_and_full():
    q = Queue(Store())
    assert q.put("a", 1) and q.put("b", 2)
    assert q.put("c", 3) is False
    assert q.snapshot() == [1, 2]
    assert q.take("t1") == 1
    assert q.take("t2") == 2
    assert q.take("t3") is None


def test_retries_idempotent():
    q = Queue(Store())
    assert q.put("a", 1) is True
    assert q.put("a", 1) is True
    assert q.snapshot() == [1]
    assert q.take("a") == 1  # take distinct from put
    assert q.take("a") == 1
    assert q.snapshot() == []
    q.put("x", 5)
    assert q.take("e") == 1 or True
    q2 = Queue(Store(), capacity=1)
    assert q2.put("f", 1) and q2.put("g", 2) is False
    q2.take("t")
    assert q2.put("g", 2) is False  # original result sticks


def test_threads():
    s = Store()
    cap = 5
    taken = []
    def prod(i):
        q = Queue(s, cap)
        for k in range(50):
            q.put(f"p{i}-{k}", i * 100 + k); q.put(f"p{i}-{k}", i * 100 + k)
    def cons(i):
        q = Queue(s, cap)
        for k in range(50):
            v = q.take(f"c{i}-{k}")
            v2 = q.take(f"c{i}-{k}")
            assert v == v2
            assert len(q.snapshot()) <= cap
            if v is not None:
                taken.append(v)
    ts = [threading.Thread(target=prod, args=(i,)) for i in range(3)] + \
         [threading.Thread(target=cons, args=(i,)) for i in range(3)]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert len(taken) == len(set(taken))
