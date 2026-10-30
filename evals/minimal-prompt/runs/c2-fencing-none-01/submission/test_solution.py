import threading

import pytest
from snappea import check_spec

from runtime import Store
from solution import LockService, Resource

TTL = 5


def mk():
    s = Store()
    return s, LockService(s, TTL), Resource(s, "r")


def test_basic_lease():
    s, L, R = mk()
    t = L.acquire("a", 0)
    assert t == 1
    assert L.acquire("a", 4) == t          # same lease
    assert L.acquire("b", 4) is None
    assert L.acquire("b", 5) == 2          # expired at now+ttl exactly
    assert L.acquire("a", 6) is None


def test_release():
    s, L, R = mk()
    t = L.acquire("a", 0)
    assert not L.release("b", t, 1)
    assert not L.release("a", t + 1, 1)
    assert not L.release("a", t, 5)        # expired
    assert L.release("a", t, 4)
    assert not L.release("a", t, 4)
    assert L.acquire("b", 4) == t + 1


def test_tokens_increase_across_expiry_and_release():
    s, L, R = mk()
    seen = []
    for i in range(5):
        t = L.acquire("a", i * 10)
        seen.append(t)
        if i % 2:
            L.release("a", t, i * 10 + 1)
    assert seen == sorted(set(seen)) and len(seen) == 5


def test_fencing():
    s, L, R = mk()
    assert not R.write(1, "x")             # never issued
    t1 = L.acquire("a", 0)
    t2 = L.acquire("b", 10)
    assert R.write(t2, "new")
    assert not R.write(t1, "stale")
    assert R.read() == "new"
    assert R.write(t2, "again")            # equal token ok
    assert not R.write(t2 + 1, "future") and not R.write(0, "z") and not R.write(-1, "z")
    assert R.read() == "again"
    other = Resource(s, "other")
    assert other.read() is None and other.write(t1, "ok")


def test_out_of_order_times():
    s, L, R = mk()
    assert L.acquire("a", 10) == 1
    assert L.acquire("b", 3) is None       # earlier clock still sees unexpired lease
    assert L.release("a", 1, 3)


def test_spec_sequence():
    s, L, R = mk()
    with check_spec("LockSafety", "Fencing", spec_files=["model/lock.p"], strict=False):
        t1 = L.acquire("a", 0)
        L.acquire("b", 1)
        t2 = L.acquire("b", 5)
        R.write(t2, "v")
        R.write(t1, "stale")
        L.release("b", t2, 6)
        t3 = L.acquire("a", 6)
        R.write(t3, "w")


def test_threads():
    s = Store()
    ttl = 3
    results = []
    lk = threading.Lock()

    def worker(i):
        L, R = LockService(s, ttl), Resource(s, "r")
        for n in range(200):
            now = (n * 7 + i) % 40
            t = L.acquire(f"c{i}", now)
            if t is not None:
                ok = R.write(t, f"c{i}")
                with lk:
                    results.append((t, now, f"c{i}", ok))
                if n % 3 == 0:
                    L.release(f"c{i}", t, now)

    th = [threading.Thread(target=worker, args=(i,)) for i in range(8)]
    [x.start() for x in th]
    [x.join() for x in th]
    # a token belongs to exactly one client
    owner = {}
    for t, _, c, _ in results:
        assert owner.setdefault(t, c) == c
    _, rec = s.load("lock")
    assert rec["last"] == max(owner)
    assert len(owner) == rec["last"]       # no skipped or duplicated tokens
    _, res = s.load("res:r")
    assert res["max"] <= rec["last"]


def test_spec_catches_stale_write():
    from snappea import emit, expect_violation
    with expect_violation("Fencing", spec_files=["model/lock.p"]):
        emit("eIssued", 1)
        emit("eIssued", 2)
        emit("eWriteOk", 2)
        emit("eWriteOk", 1)
