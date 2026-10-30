import subprocess, threading
from runtime import Store
from solution import LockService, Resource


def test_model():
    r = subprocess.run(["snappea", "check", "Lock.p", "--no-mutation", "--timeout", "20s"],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stdout + r.stderr


def test_lease_basics():
    s = Store(); L = LockService(s, 10)
    assert L.acquire("a", 0) == 1
    assert L.acquire("a", 5) == 1
    assert L.acquire("b", 9) is None
    assert L.release("b", 1, 9) is False
    assert L.release("a", 2, 9) is False
    assert L.release("a", 1, 10) is False
    assert L.acquire("b", 10) == 2
    assert L.release("a", 1, 11) is False
    assert L.release("b", 2, 11) is True
    assert L.release("b", 2, 11) is False
    assert L.acquire("a", 11) == 3
    assert L.acquire("a", 21) == 4


def test_fencing():
    s = Store(); L = LockService(s, 10); R = Resource(s, "r"); R2 = Resource(s, "r2")
    assert R.read() is None
    assert not R.write(1, "x") and not R.write(0, "x") and not R.write(-1, "x")
    assert (L.acquire("a", 0), L.acquire("b", 10)) == (1, 2)
    assert not R.write(3, "no")
    assert R.write(2, "new") and R.read() == "new"
    assert not R.write(1, "old") and R.read() == "new"
    assert R.write(2, "again") and R.read() == "again"
    assert R2.write(1, "other") and R2.read() == "other"


def test_concurrent():
    s = Store(); got = []
    def w(i):
        got.append(LockService(s, 5).acquire("c%d" % i, 0))
    ts = [threading.Thread(target=w, args=(i,)) for i in range(16)]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert got.count(None) == 15 and got.count(1) == 1
    R = Resource(s, "r"); res = []
    L = LockService(s, 1)
    for n in range(2, 6): L.acquire("z", 100 * n)
    def x(t):
        res.append((t, R.write(t, str(t))))
    ts = [threading.Thread(target=x, args=(t,)) for t in range(1, 6)]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert dict(res)[5] and R.read() == "5"
