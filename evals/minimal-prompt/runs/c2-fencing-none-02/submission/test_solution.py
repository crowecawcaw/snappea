import threading
from runtime import Store
from solution import LockService, Resource


def test_basic_and_expiry():
    s = Store(); L = LockService(s, 10)
    assert L.acquire("a", 0) == 1
    assert L.acquire("a", 5) == 1
    assert L.acquire("b", 9) is None
    assert L.acquire("b", 10) == 2
    assert not L.release("a", 1, 10)
    assert L.release("b", 2, 11) and not L.release("b", 2, 11)
    assert L.acquire("a", 11) == 3


def test_release_expired():
    s = Store(); L = LockService(s, 5)
    t = L.acquire("a", 0)
    assert not L.release("a", t, 5)


def test_fencing():
    s = Store(); L = LockService(s, 5); R = Resource(s, "r"); R2 = Resource(s, "q")
    assert R.read() is None
    assert not R.write(1, "x") and not R.write(0, "x")
    t1 = L.acquire("a", 0); t2 = L.acquire("b", 5)
    assert not R.write(t2 + 1, "x")
    assert R.write(t2, "new") and R.write(t2, "new2")
    assert not R.write(t1, "old") and R.read() == "new2"
    assert R2.write(t1, "q") and R2.read() == "q"


def test_threads_unique_tokens():
    s = Store(); L = LockService(s, 1)
    out = []
    def w(i):
        for n in range(50):
            t = L.acquire(f"c{i}", n * 2)
            if t is not None: out.append((i, n, t))
    th = [threading.Thread(target=w, args=(i,)) for i in range(8)]
    [x.start() for x in th]; [x.join() for x in th]
    toks = [t for _, _, t in out]
    assert len(toks) == len(set(toks))


def test_spec():
    from snappea import check_spec
    with check_spec("FencingMonotonic"):
        s = Store(); L = LockService(s, 5); R = Resource(s, "r")
        t1 = L.acquire("a", 0); t2 = L.acquire("b", 5)
        R.write(t2, "x"); R.write(t1, "y"); R.write(t2, "z")
