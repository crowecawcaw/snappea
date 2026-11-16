"""Hidden scenarios for the distributed lock task.

Scenarios use only node ids and times (never message contents), so they work with any
protocol. Faults are triggered by what the solution does (for example "once c0 holds the
lock, restart s1"), so they do not depend on the solution's timeouts or lease length.
"""
import random

import pytest

from sim import HOLD_MAX, LIVENESS, MAX_DELAY, Sim, healthy, lossy
from solution import LockClient, LockServer

START = 200   # first requests come after any start-up grace period a design may use


def make(servers=3, clients=2, seed=0, policy=healthy):
    return Sim(LockServer, LockClient, servers=servers, clients=clients, seed=seed, policy=policy)


def when(sim, predicate, action, until=10_000):
    """Run action(sim) at the first tick where predicate(sim) holds (checked every tick)."""
    fired = []

    def poll(s):
        if fired:
            return
        if predicate(s):
            fired.append(s.time)
            action(s)
        elif s.time < until:
            s.at(s.time + 1, poll)
    sim.at(sim.time, poll)
    return fired


def holding(sim, client, resource="r"):
    return any(h.client == client and h.resource == resource and h.end is None for h in sim.holds)


def acquired_count(sim, client, resource="r"):
    return sum(h.client == client and h.resource == resource for h in sim.holds)


def first_acquire(sim, client, after=0, resource="r"):
    times = [h.start for h in sim.holds if h.client == client and h.resource == resource and h.start >= after]
    return min(times) if times else None


def lock_if_idle(sim, client, mode, hold, at, resource="r"):
    """At tick `at`, lock unless the client is down, holds or is requesting the resource
    (the application never does that; see TASK.md)."""
    def go(s):
        last = s.__dict__.setdefault("_lock_if_idle", {})
        if last.get((client, resource)) == s.time:   # another lock_if_idle already fired this tick
            return
        if s.up(client) and (client, resource) not in s.requests and not holding(s, client, resource):
            last[(client, resource)] = s.time
            s.lock(client, resource, mode, hold)
    sim.at(at, go)


def shaped(rules, default=healthy):
    """A policy from rules (src, dst, t0, t1, delays): messages from src to dst sent in
    [t0, t1) get `delays` (a list: [] drops, two entries duplicate). '*' matches any
    node. A delay may be a callable of the send time."""
    def policy(sim, env):
        for src, dst, t0, t1, delays in rules:
            if src in ("*", env.src) and dst in ("*", env.dst) and t0 <= env.sent_at < t1:
                return [d(env.sent_at) if callable(d) else d for d in delays]
        return default(sim, env)
    return policy


def no_violations(sim):
    assert sim.violations == [], "\n".join(sim.violations[:3] + sim.log[-40:])


# ---------------------------------------------------------------- basic behavior

def test_single_writer_acquires_and_releases():
    s = make()
    s.lock("c0", "r", "write", hold=10, at=START)
    s.lock("c1", "r", "write", hold=10, at=START + 40)
    s.run_until(START + 2 * LIVENESS)
    no_violations(s)
    assert first_acquire(s, "c0") is not None and first_acquire(s, "c0") <= START + LIVENESS
    assert first_acquire(s, "c1") is not None


@pytest.mark.parametrize("servers", [3, 4, 5])
def test_competing_writers_take_turns(servers):
    for seed in range(5):
        s = make(servers=servers, clients=3, seed=seed)
        for c in ("c0", "c1", "c2"):
            s.lock(c, "r", "write", hold=HOLD_MAX, at=START)
        s.run_until(START + 1500)
        no_violations(s)
        assert all(acquired_count(s, c) == 1 for c in ("c0", "c1", "c2")), (seed, s.log[-30:])


@pytest.mark.parametrize("servers", [3, 4, 5])
def test_readers_share(servers):
    s = make(servers=servers)
    s.lock("c0", "r", "read", hold=HOLD_MAX, at=START)
    when(s, lambda s: holding(s, "c0"), lambda s: s.lock("c1", "r", "read", hold=5))
    s.run_until(START + 2 * LIVENESS)
    no_violations(s)
    c0 = s.acquisitions("r")[0]
    c1 = [h for h in s.holds if h.client == "c1"]
    assert c1 and c1[0].start < c0.end, "a reader waited for another reader"


def test_independent_resources():
    s = make()
    s.lock("c0", "a", "write", hold=HOLD_MAX, at=START)
    when(s, lambda s: holding(s, "c0", "a"), lambda s: s.lock("c1", "b", "write", hold=5))
    s.run_until(START + 2 * LIVENESS)
    no_violations(s)
    a = s.acquisitions("a")[0]
    b = s.acquisitions("b")
    assert b and b[0].start < a.end, "a lock on one resource blocked another resource"


# ---------------------------------------------------------------- quorum intersection

def test_even_cluster_writers_need_overlapping_quorums():
    # N=4: c0 reaches only s0,s1 quickly and c1 only s2,s3. Two writers of 2 servers each
    # would both hold the lock.
    for seed in range(3):
        rules = [("c0", "s2", 0, START + 5, [300]), ("c0", "s3", 0, START + 5, [300]),
                 ("c1", "s0", 0, START + 5, [300]), ("c1", "s1", 0, START + 5, [300])]
        s = make(servers=4, seed=seed, policy=shaped(rules))
        s.lock("c0", "r", "write", hold=HOLD_MAX, at=START)
        s.lock("c1", "r", "write", hold=HOLD_MAX, at=START)
        s.run_until(START + 1200)
        no_violations(s)


@pytest.mark.parametrize("servers", [3, 5])
def test_reader_and_writer_quorums_intersect(servers):
    # The writer reaches a bare write majority quickly; the reader reaches the rest.
    w = servers // 2 + 1
    fast_w = [f"s{i}" for i in range(w)]
    rules = [("c0", s, 0, START + 5, [300]) for s in [f"s{i}" for i in range(servers)] if s not in fast_w]
    rules += [("c1", s, 0, START + 5, [300]) for s in fast_w]
    s = make(servers=servers, policy=shaped(rules))
    s.lock("c0", "r", "write", hold=HOLD_MAX, at=START)
    when(s, lambda s: holding(s, "c0"), lambda s: s.lock("c1", "r", "read", hold=5))
    s.run_until(START + 1200)
    no_violations(s)


# ---------------------------------------------------------------- server restarts (dsync K4)

def test_restarted_server_does_not_regrant_a_held_lock():
    # c0 holds through s0 and s1 (its request to s2 is slow). s1 restarts with an empty
    # table. c1 cannot reach s0, so it asks s1 and s2: both have no entry.
    rules = [("c0", "s2", 0, 10_000, [500]), ("c1", "s0", 0, 10_000, [500])]
    s = make(policy=shaped(rules))
    s.lock("c0", "r", "write", hold=HOLD_MAX, at=START)

    def restart_and_compete(s):
        s.crash("s1")
        s.restart("s1", at=s.time + 1)
        s.lock("c1", "r", "write", hold=5, at=s.time + 2)
    when(s, lambda s: holding(s, "c0"), restart_and_compete)
    s.run_until(START + 1500)
    no_violations(s)


def test_whole_cluster_restart_while_held():
    s = make(servers=3)
    s.lock("c0", "r", "write", hold=HOLD_MAX, at=START)

    def restart_all(s):
        for i, srv in enumerate(s.server_ids):
            s.crash(srv)
            s.restart(srv, at=s.time + 1 + i)
        s.lock("c1", "r", "write", hold=5, at=s.time + 4)
    when(s, lambda s: holding(s, "c0"), restart_all)
    s.run_until(START + 1500)
    no_violations(s)
    assert first_acquire(s, "c1") is not None


# ---------------------------------------------------------------- delayed and duplicated messages

def test_late_grants_are_not_used():
    # Every reply to c0 sent around its first request arrives 300 ticks later, while c1
    # holds the lock and c0 is asking again.
    late = START + 300
    rules = [("*", "c0", START, START + 30, [lambda t: late - t])]
    for seed in range(3):
        s = make(seed=seed, policy=shaped(rules))
        s.lock("c0", "r", "write", hold=5, at=START)
        s.lock("c1", "r", "write", hold=HOLD_MAX, at=late - 5)
        # c0 asks again (if its first request finished) just before the late replies land.
        when(s, lambda s: s.time >= late - 4 and ("c0", "r") not in s.requests and not holding(s, "c0"),
             lambda s: s.lock("c0", "r", "write", hold=5))
        s.run_until(late + 600)
        no_violations(s)


def test_slow_grant_still_pending():
    # Replies to c0 are slow (but not lost) during its first attempt; meanwhile the
    # leases they carry may run out and c1 may take the lock.
    rules = [("*", "c0", START, START + 15, [lambda t: 90])]
    for seed in range(3):
        s = make(seed=seed, policy=shaped(rules))
        s.lock("c0", "r", "write", hold=HOLD_MAX, at=START)
        s.lock("c1", "r", "write", hold=HOLD_MAX, at=START + 10)

        def relock(s):   # c1 keeps asking whenever it is idle, until START + 400
            if s.time < START + 400:
                if ("c1", "r") not in s.requests and not holding(s, "c1"):
                    s.lock("c1", "r", "write", hold=HOLD_MAX)
                s.at(s.time + 1, relock)
        s.at(START + 11, relock)
        s.run_until(START + 1200)
        no_violations(s)


def test_duplicated_grant_counts_once():
    # c0's requests to s1 and s2 are lost; every message from s0 to c0 is duplicated.
    # c1 cannot reach s0 but gets s1 and s2.
    rules = [("c0", "s1", 0, START + 60, []), ("c0", "s2", 0, START + 60, []),
             ("s0", "c0", 0, START + 60, [1, 2]), ("c1", "s0", 0, START + 60, [])]
    s = make(policy=shaped(rules))
    s.lock("c0", "r", "write", hold=HOLD_MAX, at=START)
    s.lock("c1", "r", "write", hold=HOLD_MAX, at=START + 4)
    s.run_until(START + 800)
    no_violations(s)


def test_delayed_release_does_not_free_a_later_hold():
    # c0's release to s0 and s1 after its first hold arrives only during its second hold.
    second = START + 500
    s = make()
    s.lock("c0", "r", "write", hold=5, at=START)

    def slow_releases(sim, env):
        first_done = acquired_count(sim, "c0") == 1 and not holding(sim, "c0")
        if first_done and env.src == "c0" and env.dst in ("s0", "s1") and env.sent_at < second:
            return [second + 10 - env.sent_at]
        return healthy(sim, env)
    s.policy = slow_releases
    s.lock("c0", "r", "write", hold=HOLD_MAX, at=second)
    when(s, lambda s: s.time >= second + 11 and holding(s, "c0"),
         lambda s: s.lock("c1", "r", "write", hold=5), until=second + 300)
    s.run_until(second + 800)
    no_violations(s)


# ---------------------------------------------------------------- stale locks (dsync K1, K3, K5)

def test_crashed_holder_is_cleaned_up():
    s = make()
    s.lock("c0", "r", "write", hold=HOLD_MAX, at=START)
    crashed = when(s, lambda s: holding(s, "c0"), lambda s: (s.crash("c0"), s.lock("c1", "r", "write", hold=5)))
    s.run_until(START + 1000)
    no_violations(s)
    t = first_acquire(s, "c1")
    assert crashed and t is not None and t <= crashed[0] + LIVENESS, (crashed, t)


def test_lost_releases_are_cleaned_up():
    s = make()
    s.lock("c0", "r", "write", hold=5, at=START)
    got = when(s, lambda s: holding(s, "c0"), lambda s: None)

    def drop_c0(sim, env):   # c0's releases are lost; everything after is delivered
        if got and env.src == "c0" and got[0] <= env.sent_at <= got[0] + HOLD_MAX + 1:
            return []
        return healthy(sim, env)
    s.policy = drop_c0
    s.lock("c1", "r", "write", hold=5, at=START + 100)
    s.run_until(START + 1000)
    no_violations(s)
    t = first_acquire(s, "c1")
    assert t is not None and t <= START + 100 + LIVENESS, t


def test_lost_grant_reply_does_not_strand_the_lock():
    # s0 grants c0 but the reply is lost; c0 holds through s1 and s2 and unlocks. Then s1
    # goes down for good: c1 needs s0 and s2.
    rules = [("s0", "c0", 0, START + 30, [])]
    s = make(policy=shaped(rules))
    s.lock("c0", "r", "write", hold=5, at=START)
    done = when(s, lambda s: acquired_count(s, "c0") == 1 and not holding(s, "c0"),
                lambda s: (s.crash("s1"), s.lock("c1", "r", "write", hold=5, at=s.time + 1)))
    s.run_until(START + 1000)
    no_violations(s)
    t = first_acquire(s, "c1")
    assert done and t is not None and t <= done[0] + LIVENESS, (done, t)


def test_partially_reached_holder_is_not_purged():
    # c0 holds through s0 and s1; s2 never heard of it. A cleanup that drops an entry
    # when one peer does not know it would let c1 in through s0 and s2.
    rules = [("c0", "s2", 0, 10_000, [800])]
    s = make(policy=shaped(rules))
    s.lock("c0", "r", "write", hold=HOLD_MAX, at=START)
    when(s, lambda s: holding(s, "c0"), lambda s: s.lock("c1", "r", "write", hold=5))
    s.run_until(START + 900)
    no_violations(s)


def test_minority_down():
    s = make(servers=5, clients=2)
    s.crash("s3", at=0)
    s.crash("s4", at=0)
    for k in range(4):
        lock_if_idle(s, "c0", "write", 5, START + 200 * k)
    s.lock("c1", "r", "read", hold=5, at=START + 100)
    s.run_until(START + 1100)
    no_violations(s)
    assert acquired_count(s, "c0") == 4 and acquired_count(s, "c1") == 1


def test_restarted_servers_recover():
    s = make()
    s.lock("c0", "r", "write", hold=HOLD_MAX, at=START)
    for i, srv in enumerate(s.server_ids):
        s.crash(srv, at=START + 30 + 10 * i)
        s.restart(srv, at=START + 35 + 10 * i)
    last = START + 35 + 10 * 2
    s.lock("c1", "r", "write", hold=5, at=last)
    s.run_until(last + 1000)
    no_violations(s)
    t = first_acquire(s, "c1")
    assert t is not None and t <= last + LIVENESS, t


# ---------------------------------------------------------------- contention liveness

def test_split_vote_resolves():
    # N=4, both writers start together and each first reaches a different half.
    for seed in range(5):
        rules = [("c0", "s2", START, START + 2, [15]), ("c0", "s3", START, START + 2, [15]),
                 ("c1", "s0", START, START + 2, [15]), ("c1", "s1", START, START + 2, [15])]
        s = make(servers=4, seed=seed, policy=shaped(rules))
        s.lock("c0", "r", "write", hold=10, at=START)
        s.lock("c1", "r", "write", hold=10, at=START)
        s.run_until(START + 800)
        no_violations(s)
        assert acquired_count(s, "c0") == 1 and acquired_count(s, "c1") == 1, (seed, s.log[-30:])


def test_writer_after_lossy_period():
    # A lossy, crashy period, then a healthy network: a request on a free lock succeeds.
    for seed in range(6):
        s = make(servers=3 + seed % 3, clients=3, seed=seed, policy=lossy(loss=0.3, dup=0.1, slow=0.1, slow_max=150))
        rng = random.Random(seed)
        for k in range(12):
            lock_if_idle(s, f"c{k % 2}", rng.choice(["read", "write"]), rng.randint(1, HOLD_MAX), START + 40 * k)
        s.crash("c1", at=START + 300)
        s.crash("s1", at=START + 200)
        s.restart("s1", at=START + 260)
        heal = START + 700
        s.set_policy(healthy, at=heal)
        s.run_until(heal)
        # wait until c0 is idle, then ask with c2
        s.lock("c2", "r", "write", hold=5, at=heal + 1)
        s.run_until(heal + 1 + LIVENESS + 2 * HOLD_MAX + 200)
        no_violations(s)
        assert first_acquire(s, "c2") is not None, (seed, s.log[-30:])


# ---------------------------------------------------------------- randomized safety

@pytest.mark.parametrize("servers", [3, 4, 5])
def test_random_faults_safety(servers):
    for seed in range(20):
        s = make(servers=servers, clients=4, seed=seed * 7 + servers,
                 policy=lossy(loss=0.15, dup=0.1, slow=0.1, slow_max=250))
        rng = random.Random(seed)
        for k in range(40):
            c = f"c{rng.randrange(4)}"
            lock_if_idle(s, c, rng.choice(["read", "write", "write"]), rng.randint(1, HOLD_MAX),
                         START + rng.randrange(1500))
        for k in range(6):
            srv = f"s{rng.randrange(servers)}"
            t = START + rng.randrange(1500)
            s.crash(srv, at=t)
            s.restart(srv, at=t + rng.randint(1, 80))
        s.crash(f"c{rng.randrange(4)}", at=START + rng.randrange(1500))
        s.run_until(START + 2500)
        no_violations(s)
