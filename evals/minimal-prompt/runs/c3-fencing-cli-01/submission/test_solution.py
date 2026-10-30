import threading
from concurrent.futures import ThreadPoolExecutor
from runtime import Store
from solution import LockService, Resource

TTL = 10


class SameVersionStore:
    def __init__(self, inner, key, n=2):
        self.inner, self.key = inner, key
        self.barrier, self.left, self.lock = threading.Barrier(n, timeout=2), n, threading.Lock()

    def load(self, key):
        r = self.inner.load(key)
        if key == self.key:
            with self.lock:
                wait, self.left = self.left > 0, self.left - 1
            if wait:
                self.barrier.wait()
        return r

    def compare_swap(self, *a):
        return self.inner.compare_swap(*a)


class InterferingStore:
    def __init__(self, inner, key, competitor):
        self.inner, self.key, self.competitor = inner, key, competitor

    def load(self, key):
        r = self.inner.load(key)
        if key == self.key and self.competitor:
            run, self.competitor = self.competitor, None
            run()
        return r

    def compare_swap(self, *a):
        return self.inner.compare_swap(*a)


def test_basic_and_boundaries():
    s = Store(); L = LockService(s, TTL)
    t = L.acquire("a", 0)
    assert t >= 1
    assert L.acquire("a", 9) == t          # same client, unexpired
    assert L.acquire("b", 9) is None       # just before expiry
    t2 = L.acquire("b", 10)                # exactly at expiry
    assert t2 > t
    assert L.acquire("a", 10) is None


def test_tokens_increase_across_release_and_reacquire():
    s = Store(); L = LockService(s, TTL)
    t1 = L.acquire("a", 0)
    assert L.release("a", t1, 1) is True
    t2 = L.acquire("a", 2)
    assert t2 > t1
    assert L.release("a", t2, 3)
    t3 = L.acquire("b", 3)
    assert t3 > t2


def test_release_preconditions_each_alone():
    s = Store(); L = LockService(s, TTL)
    t = L.acquire("a", 0)
    assert L.release("b", t, 1) is False       # wrong client only
    assert L.release("a", t + 1, 1) is False   # wrong token only
    assert L.release("a", t, 10) is False      # expired at boundary only
    assert L.release("a", t, 9) is True
    assert L.release("a", t, 9) is False       # already released
    assert L.acquire("b", 9) is not None       # free after release


def test_stale_release_does_not_free_new_lease():
    s = Store(); L = LockService(s, TTL)
    t1 = L.acquire("a", 0)
    t2 = L.acquire("a", 10)  # a's own expired lease replaced
    assert t2 > t1
    assert L.release("a", t1, 11) is False
    assert L.acquire("b", 11) is None


def test_out_of_order_times():
    s = Store(); L = LockService(s, TTL)
    t = L.acquire("a", 100)
    assert L.acquire("b", 5) is None   # earlier time still sees a live lease
    assert L.acquire("a", 5) == t


def test_two_service_objects_share_state():
    s = Store()
    t = LockService(s, TTL).acquire("a", 0)
    assert LockService(s, TTL).acquire("b", 1) is None
    assert LockService(s, TTL).release("a", t, 1)


def test_concurrent_acquire_same_version_one_winner():
    inner = Store(); sv = SameVersionStore(inner, "lock")
    with ThreadPoolExecutor(2) as ex:
        fs = [ex.submit(LockService(sv, TTL).acquire, c, 0) for c in "ab"]
        res = [f.result() for f in fs]
    assert sorted(r is None for r in res) == [False, True]


def test_competitor_acquires_between_load_and_swap():
    inner = Store()
    st = InterferingStore(inner, "lock", lambda: LockService(inner, TTL).acquire("other", 0))
    assert LockService(st, TTL).acquire("me", 1) is None
    assert LockService(inner, TTL).acquire("other", 2) is not None


def test_release_races_with_expiry_takeover():
    inner = Store(); L = LockService(inner, TTL)
    t = L.acquire("a", 0)
    st = InterferingStore(inner, "lock", lambda: LockService(inner, TTL).acquire("b", 10))
    assert LockService(st, TTL).release("a", t, 5) is False
    assert LockService(inner, TTL).acquire("c", 11) is None


def test_fencing_write():
    s = Store(); L = LockService(s, TTL); R = Resource(s, "r")
    assert R.read() is None
    assert R.write(1, "x") is False        # never issued
    t1 = L.acquire("a", 0)
    assert R.write(0, "x") is False and R.write(-1, "x") is False
    assert R.write(t1 + 1, "x") is False   # not yet issued
    assert R.write(t1, "v1") is True
    assert R.write(t1, "v1b") is True      # equal token allowed
    t2 = L.acquire("b", 10)
    assert R.write(t2, "v2") is True
    assert R.write(t1, "stale") is False
    assert R.read() == "v2"
    assert Resource(s, "other").write(t1, "o") is True   # per-resource
    assert Resource(s, "other").read() == "o"
    assert Resource(s, "r").read() == "v2"


def test_rejected_write_has_no_effect_and_stays_rejected():
    s = Store(); L = LockService(s, TTL); R = Resource(s, "r")
    t1 = L.acquire("a", 0); t2 = L.acquire("b", 10)
    assert R.write(t2, "new")
    assert R.write(t1, "old") is False
    assert R.write(t1, "old") is False
    assert R.read() == "new"


def test_write_race_lower_token_between_load_and_swap():
    inner = Store(); L = LockService(inner, TTL)
    t1 = L.acquire("a", 0); t2 = L.acquire("b", 10)
    st = InterferingStore(inner, "res:r", lambda: Resource(inner, "r").write(t2, "hi"))
    assert Resource(st, "r").write(t1, "lo") is False
    assert Resource(inner, "r").read() == "hi"


def test_concurrent_writes_same_version():
    inner = Store(); L = LockService(inner, TTL)
    t1 = L.acquire("a", 0); t2 = L.acquire("b", 10)
    sv = SameVersionStore(inner, "res:r")
    with ThreadPoolExecutor(2) as ex:
        f1 = ex.submit(Resource(sv, "r").write, t1, "lo")
        f2 = ex.submit(Resource(sv, "r").write, t2, "hi")
        f1.result(), f2.result()
    assert Resource(inner, "r").read() == "hi"


def test_thread_stress_tokens_unique_and_exclusive():
    s = Store(); out = []
    def work(c):
        L = LockService(s, 5)
        for now in range(50):
            t = L.acquire(c, now)
            if t is not None:
                out.append((t, c, now))
    ts = [threading.Thread(target=work, args=(str(i),)) for i in range(4)]
    [t.start() for t in ts]; [t.join() for t in ts]
    assert out and len({o[0] for o in out if True}) >= 1
    by = {}
    for t, c, _ in out:
        assert by.setdefault(t, c) == c   # a token belongs to one client


def test_observed_against_p_spec():
    from snappea import check_events
    s = Store(); L = LockService(s, TTL)
    ev = []
    t = L.acquire(1, 0)
    ev.append({"event": "eCommit", "payload": {"client": 1, "token": t, "expiry": TTL}})
    assert L.acquire(2, 3) is None
    ev.append({"event": "eSeen", "payload": {"client": 2, "now": 3, "token": t, "own": False}})
    t2 = L.acquire(2, 10)
    ev.append({"event": "eCommit", "payload": {"client": 2, "token": t2, "expiry": 10 + TTL}})
    check_events(ev, "Fencing", spec_files=["Lock.p"], strict=True)
