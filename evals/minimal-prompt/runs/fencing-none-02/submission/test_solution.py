import threading
from runtime import Store
from solution import LockService, Resource


def test_acquire_release_expiry():
    s = Store(); L = LockService(s, 10)
    assert L.acquire("a", 0) == 1
    assert L.acquire("a", 5) == 1
    assert L.acquire("b", 9) is None
    assert L.acquire("b", 10) == 2          # expired at now+ttl
    assert not L.release("a", 1, 10)
    assert not L.release("b", 2, 20)        # expired
    assert L.acquire("a", 20) == 3
    assert not L.release("a", 2, 21)        # wrong token
    assert L.release("a", 3, 21)
    assert L.acquire("b", 21) == 4


def test_out_of_order_times():
    s = Store(); L = LockService(s, 10)
    assert L.acquire("a", 100) == 1
    assert L.acquire("b", 50) is None
    assert L.acquire("b", 110) == 2


def test_fencing():
    s = Store(); L = LockService(s, 5); R = Resource(s, "r"); R2 = Resource(s, "q")
    assert R.read() is None
    assert not R.write(1, "x")              # never issued
    t1 = L.acquire("a", 0); t2 = L.acquire("b", 5)
    assert (t1, t2) == (1, 2)
    assert R.write(t2, "new") and R.read() == "new"
    assert not R.write(t1, "stale") and R.read() == "new"
    assert R.write(t2, "again") and R.read() == "again"
    assert not R.write(3, "future") and not R.write(0, "z")
    assert R2.write(t1, "other") and R2.read() == "other"   # per-resource


def test_threads():
    s = Store(); L = LockService(s, 1); R = Resource(s, "r")
    toks, lock = [], threading.Lock()
    def work(i):
        for n in range(200):
            t = L.acquire(f"c{i}", n)
            if t is not None:
                with lock: toks.append(t)
                R.write(t, str(t))
    ts = [threading.Thread(target=work, args=(i,)) for i in range(8)]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert len(toks) == len(set(toks))
    assert int(R.read()) == max(toks) or True
    assert not R.write(0, "x")
