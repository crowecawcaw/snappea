"""Hidden scenarios for lease-impl. Each builds a Sim, runs it and returns the Result.

They use only what DESIGN.md states: lost, duplicated, delayed and reordered messages,
clients that pause or crash at their grant, and crash/restart of the two services.
Every fault ends by tick 500; a run lasts 3000 ticks.
"""
from sim import MAX_DELAY, Sim, healthy, lossy

FAULTS_END = 500


def normal(sim):
    return [sim.rng.randint(1, MAX_DELAY)]


def until(t_end, policy):
    """`policy` before tick t_end, a healthy network after."""
    def p(sim, env):
        return policy(sim, env) if sim.time < t_end else healthy(sim, env)
    return p


def dup(match, lo, hi):
    """Messages that `match` arrive twice: normally, and again lo..hi ticks later."""
    def p(sim, env):
        delays = normal(sim)
        if match(env):
            delays.append(sim.rng.randint(lo, hi))
        return delays
    return p


def drop(match):
    def p(sim, env):
        return [] if match(env) else normal(sim)
    return p


def jitter(lo, hi):
    def p(sim, env):
        return [sim.rng.randint(lo, hi)]
    return p


def first_event(sim, event, client):
    """Tick of the first `event` about `client`, or None."""
    for tick, name, payload in sim.events:
        if name == event and isinstance(payload, dict) and payload.get("id") == client:
            return tick
    return None


def mk(classes, log_dir, name, seed, policy=healthy):
    return Sim(*classes, name=name, seed=seed, policy=policy, log_dir=log_dir)


def h01_contend(c, d):
    s = mk(c, d, "h01_contend", 11)
    s.add_client(1, writes=2)
    s.add_client(2, writes=2, max_attempts=4, at=1)
    s.add_client(3, max_attempts=1, at=2)
    return s.run()


def h02_pause_past_expiry(c, d):
    """The GC pause: client 1 sleeps past its lease, client 2 takes over and writes."""
    s = mk(c, d, "h02_pause_past_expiry", 12)
    s.add_client(1, writes=2, behavior="Pauses", pause=90)
    s.add_client(2, writes=2, max_attempts=6, at=5)
    return s.run()


def h03_holder_crashes(c, d):
    s = mk(c, d, "h03_holder_crashes", 13)
    s.add_client(1, behavior="Crashes")
    s.add_client(2, max_attempts=2, at=3)
    s.add_client(3, max_attempts=6, at=6)
    return s.run()


def h04_duplicate_everything(c, d):
    s = mk(c, d, "h04_duplicate_everything", 14, until(FAULTS_END, dup(lambda e: True, 1, 25)))
    s.add_client(1, writes=2, max_attempts=3)
    s.add_client(2, writes=2, max_attempts=3, at=1)
    s.add_client(3, writes=1, max_attempts=2, at=4)
    return s.run()


def h05_lossy(c, d):
    s = mk(c, d, "h05_lossy", 15, until(FAULTS_END, lossy(loss=0.3, dup=0.3, slow=0.2)))
    s.add_client(1, writes=2, max_attempts=3)
    s.add_client(2, writes=2, max_attempts=4, at=2)
    s.add_client(3, writes=1, max_attempts=4, at=9)
    return s.run()


def h06_lossy_four(c, d):
    s = mk(c, d, "h06_lossy_four", 16, until(FAULTS_END, lossy(loss=0.25, dup=0.4, slow=0.3, slow_max=80)))
    for i in range(1, 5):
        s.add_client(i, writes=2, max_attempts=5, at=3 * i)
    return s.run()


def h07_lock_restarts_holder_dead(c, d):
    """The holder died; the lock service restarts while its lease is live."""
    s = mk(c, d, "h07_lock_restarts_holder_dead", 17)
    s.add_client(1, behavior="Crashes")
    s.add_client(2, max_attempts=8, at=30)
    s.crash("lock", at=10)
    s.restart("lock", at=18)
    return s.run()


def h08_lock_restarts_between_leases(c, d):
    s = mk(c, d, "h08_lock_restarts_between_leases", 18)
    s.add_client(1, writes=2)
    s.add_client(2, writes=2, max_attempts=3, at=60)
    s.add_client(3, max_attempts=3, at=120)
    s.crash("lock", at=40)
    s.restart("lock", at=50)
    s.crash("lock", at=100)
    s.restart("lock", at=110)
    return s.run()


def h09_storage_restarts_before_late_write(c, d):
    """Client 1 sleeps past its lease; client 2 writes; storage restarts; client 1 writes."""
    s = mk(c, d, "h09_storage_restarts_before_late_write", 19)
    s.add_client(1, writes=2, behavior="Pauses", pause=120)
    s.add_client(2, writes=2, max_attempts=6, at=5)
    s.crash("storage", at=90)
    s.restart("storage", at=100)
    return s.run()


def h10_write_reply_lost_past_expiry(c, d):
    """Storage's replies to client 1 are lost until its lease has expired and client 2
    has written; then client 1's repeated write gets through."""
    s = mk(c, d, "h10_write_reply_lost_past_expiry", 20,
           until(110, drop(lambda e: e.src == "storage" and e.dst == "c1")))
    s.add_client(1, writes=1)
    s.add_client(2, writes=1, max_attempts=8, at=4)
    return s.run()


def h11_duplicate_replies(c, d):
    """Every reply from a service arrives twice, the copy 2..16 ticks later."""
    s = mk(c, d, "h11_duplicate_replies", 21,
           until(FAULTS_END, dup(lambda e: e.src in ("lock", "storage"), 2, 16)))
    s.add_client(1, writes=3)
    s.add_client(2, writes=2, max_attempts=3, at=1)
    s.add_client(3, writes=1, max_attempts=2, at=2)
    return s.run()


def h12_successor_pauses(c, d):
    """Client 1 is quick; client 2 gets the lock soon after and sleeps while holding it."""
    s = mk(c, d, "h12_successor_pauses", 22)
    s.add_client(1)
    s.add_client(2, max_attempts=2, behavior="Pauses", pause=15, at=12)
    s.add_client(3, max_attempts=8, at=20)
    return s.run()


def h13_release_lost(c, d):
    """Client 1's messages to the lock service are lost for 12 ticks after its write."""
    def lost(sim, env):
        if env.src == "c1" and env.dst == "lock":
            t = first_event(sim, "WriteAccepted", 1)
            if t is not None and sim.time < t + 12:
                return []
        return normal(sim)
    s = mk(c, d, "h13_release_lost", 23, lost)
    s.add_client(1)
    s.add_client(2, max_attempts=6, at=3)
    return s.run()


def h14_late_duplicate_requests(c, d):
    """Every request to the lock service arrives again 15..45 ticks later."""
    s = mk(c, d, "h14_late_duplicate_requests", 24,
           until(FAULTS_END, dup(lambda e: e.dst == "lock", 15, 45)))
    s.add_client(1, writes=1)
    s.add_client(2, writes=1, max_attempts=4, at=2)
    s.add_client(3, writes=1, max_attempts=4, at=30)
    return s.run()


def h15_restarts_and_loss(c, d):
    s = mk(c, d, "h15_restarts_and_loss", 25, until(FAULTS_END, lossy(loss=0.2, dup=0.3, slow=0.2)))
    for i in range(1, 4):
        s.add_client(i, writes=2, max_attempts=6, at=4 * i)
    for node, down, up_ in (("lock", 12, 20), ("storage", 16, 26), ("lock", 55, 70),
                            ("storage", 80, 84), ("lock", 130, 135)):
        s.crash(node, at=down)
        s.restart(node, at=up_)
    return s.run()


def h16_lock_down_at_expiry(c, d):
    """The holder died and the lock service is down when its lease runs out."""
    s = mk(c, d, "h16_lock_down_at_expiry", 26)
    s.add_client(1, behavior="Crashes")
    s.add_client(2, max_attempts=3, at=70)
    s.crash("lock", at=15)
    s.restart("lock", at=60)
    return s.run()


def h17_reordered(c, d):
    """Every message takes 1..25 ticks, so they overtake each other."""
    s = mk(c, d, "h17_reordered", 27, until(FAULTS_END, jitter(1, 25)))
    s.add_client(1, writes=3, max_attempts=3)
    s.add_client(2, writes=3, max_attempts=5, at=2)
    s.add_client(3, writes=2, max_attempts=5, at=5)
    return s.run()


def h18_storage_restarts_duplicate_writes(c, d):
    """Every write arrives again 20..40 ticks later; storage restarts in between."""
    s = mk(c, d, "h18_storage_restarts_duplicate_writes", 28,
           until(FAULTS_END, dup(lambda e: e.dst == "storage", 20, 40)))
    s.add_client(1, writes=2)
    s.add_client(2, writes=2, max_attempts=5, at=3)
    s.crash("storage", at=14)
    s.restart("storage", at=18)
    s.crash("storage", at=45)
    s.restart("storage", at=50)
    return s.run()


def h19_late_copy_of_busy_reply(c, d):
    """Every reply from the lock service arrives again 11..13 ticks later, about when
    the next attempt is in flight."""
    s = mk(c, d, "h19_late_copy_of_busy_reply", 29,
           until(FAULTS_END, dup(lambda e: e.src == "lock", 11, 13)))
    s.add_client(1, behavior="Pauses", pause=20)
    s.add_client(2, max_attempts=2, at=4)
    s.add_client(3, max_attempts=2, at=5)
    s.add_client(4, max_attempts=2, at=6)
    return s.run()


SCENARIOS = [h01_contend, h02_pause_past_expiry, h03_holder_crashes, h04_duplicate_everything,
             h05_lossy, h06_lossy_four, h07_lock_restarts_holder_dead,
             h08_lock_restarts_between_leases, h09_storage_restarts_before_late_write,
             h10_write_reply_lost_past_expiry, h11_duplicate_replies, h12_successor_pauses,
             h13_release_lost, h14_late_duplicate_requests, h15_restarts_and_loss,
             h16_lock_down_at_expiry, h17_reordered, h18_storage_restarts_duplicate_writes,
             h19_late_copy_of_busy_reply]

if __name__ == "__main__":
    # run one scenario: hidden.py <name> <log_dir> <result_file>
    import json
    import sys
    name, log_dir, out = sys.argv[1:4]
    try:
        from lease import Client, LockService, Storage
        fn = {f.__name__: f for f in SCENARIOS}[name]
        r = fn((LockService, Storage, Client), log_dir)
        res = {"ok": True, "log": r.log_path, "outcomes": r.outcomes, "unfinished": r.unfinished}
    except BaseException as e:  # a broken submission is a failed scenario, not a crash
        res = {"ok": False, "error": f"{type(e).__name__}: {e}"[:400]}
    with open(out, "w") as f:
        json.dump(res, f)
