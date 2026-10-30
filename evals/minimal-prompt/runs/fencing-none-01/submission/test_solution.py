import threading
from runtime import Store
from solution import LockService, Resource


def test_basic_lease_and_expiry():
    s = Store(); l = LockService(s, 10)
    t = l.acquire("a", 0)
    assert t == 1 and l.acquire("a", 5) == 1 and l.acquire("b", 9) is None
    t2 = l.acquire("b", 10)
    assert t2 > t
    assert not l.release("a", t, 11)


def test_release():
    s = Store(); l = LockService(s, 10)
    t = l.acquire("a", 0)
    assert not l.release("b", t, 1) and not l.release("a", t + 5, 1)
    assert not l.release("a", t, 10)
    assert l.release("a", t, 9)
    assert l.acquire("b", 9) == t + 1


def test_out_of_order_time():
    s = Store(); l = LockService(s, 10)
    t = l.acquire("a", 100)
    assert l.acquire("b", 50) is None
    assert l.acquire("a", 50) == t


def test_fencing():
    s = Store(); l = LockService(s, 5); r = Resource(s, "r"); r2 = Resource(s, "r2")
    t1 = l.acquire("a", 0); t2 = l.acquire("b", 5)
    assert not r.write(99, "x") and not r.write(0, "x")
    assert r.write(t2, "new") and not r.write(t1, "old")
    assert r.write(t2, "again") and r.read() == "again"
    assert r2.write(t1, "ok") and r2.read() == "ok" and Resource(s, "z").read() is None


def test_threads_unique_tokens():
    s = Store(); l = LockService(s, 1)
    out = []
    def w(i):
        t = LockService(s, 1).acquire("c%d" % i, i * 1000)
        out.append(t)
    th = [threading.Thread(target=w, args=(i,)) for i in range(30)]
    [x.start() for x in th]; [x.join() for x in th]
    assert len(set(out)) == 30
