import itertools
import random
import threading

import pytest
from snappea import check_spec, emit

from runtime import Store
from solution import LockService, Resource

TTL = 10


def mk():
    s = Store()
    return s, LockService(s, TTL), Resource(s, "r")


def test_acquire_basic_and_reentrant():
    _, ls, _ = mk()
    t = ls.acquire("a", 0)
    assert t is not None
    assert ls.acquire("a", 5) == t
    assert ls.acquire("b", 5) is None


def test_expiry_boundary():
    _, ls, _ = mk()
    t = ls.acquire("a", 0)
    assert ls.acquire("b", TTL - 1) is None
    t2 = ls.acquire("b", TTL)  # expired at now+ttl exactly
    assert t2 is not None and t2 > t
    # the same client re-acquiring after its own expiry gets a new token
    assert ls.acquire("b", 3 * TTL) > t2


def test_release_preconditions_each():
    _, ls, _ = mk()
    t = ls.acquire("a", 0)
    assert not ls.release("b", t, 1)          # wrong client
    assert not ls.release("a", t + 1, 1)      # wrong token
    assert not ls.release("a", t, TTL)        # expired at boundary
    assert ls.acquire("b", 1) is None         # still held by a
    assert ls.release("a", t, TTL - 1)
    assert not ls.release("a", t, TTL - 1)    # already released
    t2 = ls.acquire("b", 2)
    assert t2 > t


def test_stale_release_does_not_free_newer_lease():
    _, ls, _ = mk()
    t1 = ls.acquire("a", 0)
    t2 = ls.acquire("b", TTL)
    assert not ls.release("a", t1, TTL + 1)
    assert ls.acquire("c", TTL + 1) is None
    assert ls.release("b", t2, TTL + 1)


def test_out_of_order_times():
    _, ls, _ = mk()
    t = ls.acquire("a", 100)
    assert ls.acquire("b", 50) is None   # earlier time: still unexpired
    assert ls.acquire("a", 50) == t


def test_tokens_increase_across_release():
    _, ls, _ = mk()
    seen = []
    for i in range(5):
        t = ls.acquire("a", i * 100)
        seen.append(t)
        if i % 2:
            assert ls.release("a", t, i * 100)
    assert seen == sorted(set(seen))


def test_resource_fencing():
    s, ls, r = mk()
    assert r.read() is None
    assert not r.write(1, "x")      # never issued
    assert r.read() is None
    t1 = ls.acquire("a", 0)
    t2 = ls.acquire("b", TTL)
    assert not r.write(0, "z") and not r.write(-1, "z")
    assert not r.write(t2 + 1, "z")  # not yet issued
    assert r.write(t2, "new") and r.read() == "new"
    assert not r.write(t1, "old") and r.read() == "new"
    assert r.write(t2, "again") and r.read() == "again"  # equal token ok
    assert not r.write(t2 + 1, "future")
    assert r.read() == "again"


def test_resources_independent():
    s, ls, r = mk()
    r2 = Resource(s, "other")
    t1 = ls.acquire("a", 0)
    t2 = ls.acquire("b", TTL)
    assert r.write(t2, "x")
    assert r2.write(t1, "y") and r2.read() == "y"   # r's max does not affect r2


def test_empty_string_value():
    _, ls, r = mk()
    t = ls.acquire("a", 0)
    assert r.write(t, "") and r.read() == ""


class Observed:
    """Wraps real calls and emits what actually happened."""

    def __init__(self):
        self.s, self.ls, self.r = mk()
        self.seen = set()

    def acquire(self, c, now):
        t = self.ls.acquire(c, now)
        if t is not None and t not in self.seen:
            self.seen.add(t)
            emit("eIssued", token=t)
        return t

    def write(self, tok, v):
        ok = self.r.write(tok, v)
        emit("eWrote", token=tok, ok=ok)
        return ok


@pytest.mark.parametrize("seed", range(5))
def test_observed_history_matches_monitor(seed):
    rnd = random.Random(seed)
    with check_spec("FencedWrites", spec_files=["obs/Observe.p"], strict=True,
                    unknown_events="error"):
        o = Observed()
        now = 0
        toks = [0, 1]
        for _ in range(60):
            now += rnd.choice([0, 3, 6, 11])
            t = o.acquire(rnd.choice("abc"), now)
            if t:
                toks.append(t)
            o.write(rnd.choice(toks) + rnd.choice([0, 0, 1]), "v")


def test_concurrent_acquire_single_winner_and_unique_tokens():
    for _ in range(20):
        s = Store()
        ls = LockService(s, 1000)
        out = []
        bar = threading.Barrier(8)

        def go(i):
            bar.wait()
            out.append(ls.acquire(f"c{i}", 0))

        th = [threading.Thread(target=go, args=(i,)) for i in range(8)]
        [t.start() for t in th]
        [t.join() for t in th]
        assert len([x for x in out if x is not None]) == 1


def test_concurrent_fenced_writes_never_regress():
    s = Store()
    ls = LockService(s, 1)
    r = Resource(s, "r")
    toks = [ls.acquire("a", 0)]
    for i in range(1, 30):
        toks.append(ls.acquire("a", i * 2))
    accepted = []
    lock = threading.Lock()

    def writer(seq):
        for t in seq:
            if r.write(t, str(t)):
                with lock:
                    accepted.append(t)
            v = r.read()
            assert v is None or int(v) >= 0

    seqs = [random.Random(i).sample(toks, len(toks)) for i in range(6)]
    th = [threading.Thread(target=writer, args=(q,)) for q in seqs]
    [t.start() for t in th]
    [t.join() for t in th]
    assert r.read() == str(max(toks))
    assert not r.write(min(toks), "late")
    assert r.read() == str(max(toks))
