import threading
from runtime import Store
from solution import LockService, Resource


def make(ttl=10):
    s = Store()
    return s, LockService(s, ttl), Resource(s, "r")


def test_acquire_basics_and_expiry_boundary():
    _, l, _ = make()
    t1 = l.acquire("a", 0)
    assert t1 is not None
    assert l.acquire("a", 5) == t1          # same holder, same token, no extension
    assert l.acquire("b", 9) is None
    assert l.acquire("a", 9) == t1
    t2 = l.acquire("b", 10)                  # expired exactly at now+ttl
    assert t2 is not None and t2 > t1


def test_release_preconditions():
    _, l, _ = make()
    t = l.acquire("a", 0)
    assert not l.release("b", t, 1)          # wrong client
    assert not l.release("a", t + 1, 1)      # wrong token
    assert not l.release("a", t, 10)         # expired
    assert l.acquire("b", 5) is None         # failed releases had no effect
    assert l.release("a", t, 9)
    assert not l.release("a", t, 9)          # already released
    t2 = l.acquire("b", 9)
    assert t2 > t                            # tokens increase after release


def test_stale_release_does_not_free_new_lease():
    _, l, _ = make()
    t1 = l.acquire("a", 0)
    t2 = l.acquire("a", 10)
    assert t2 > t1
    assert not l.release("a", t1, 11)
    assert l.acquire("b", 11) is None


def test_tokens_increase_with_out_of_order_times():
    _, l, _ = make()
    t1 = l.acquire("a", 100)
    assert l.release("a", t1, 100)
    t2 = l.acquire("b", 0)
    assert t2 > t1


def test_fencing():
    _, l, r = make()
    assert r.read() is None
    assert not r.write(1, "x")               # never issued
    assert not r.write(0, "x") and not r.write(-1, "x")
    t1 = l.acquire("a", 0)
    t2 = l.acquire("b", 10)
    assert r.write(t2, "new")
    assert not r.write(t1, "old")            # stale token
    assert r.read() == "new"
    assert r.write(t2, "again")              # same token ok
    assert not r.write(t2 + 1, "future")     # not issued
    assert r.read() == "again"
    assert r.write(t2, "")                   # empty value
    assert r.read() == ""


def test_resources_independent():
    s, l, r = make()
    r2 = Resource(s, "other")
    t1 = l.acquire("a", 0)
    t2 = l.acquire("b", 10)
    assert r.write(t2, "x")
    assert r2.write(t1, "y")                 # high-water mark is per resource


def test_concurrent_acquire_single_winner():
    for _ in range(50):
        s = Store()
        res = []
        def go(i):
            res.append(LockService(s, 100).acquire(f"c{i}", 0))
        ts = [threading.Thread(target=go, args=(i,)) for i in range(8)]
        [t.start() for t in ts]; [t.join() for t in ts]
        assert len([x for x in res if x is not None]) == 1


def test_concurrent_writes_monotonic():
    s = Store()
    l = LockService(s, 1)
    toks = [l.acquire("c", 10 * i) for i in range(5)]
    r = Resource(s, "r")
    ts = [threading.Thread(target=r.write, args=(t, str(t))) for t in toks * 4]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert r.read() == str(max(toks))
