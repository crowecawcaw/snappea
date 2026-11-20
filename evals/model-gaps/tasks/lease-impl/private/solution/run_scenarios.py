#!/usr/bin/env python3
"""Run three easy scenarios against lease.py and write their event logs.

    python3 run_scenarios.py            # all
    python3 run_scenarios.py two_clients

Each scenario writes logs/<name>.jsonl: one JSON object per contract event,
{"key": <scenario>, "timestamp": <tick>, "event": <name>, "payload": ...}.
Add your own scenarios here; sim.py documents the faults you can inject.
"""
import sys

from lease import Client, LockService, Storage
from sim import Sim


def one_client():
    s = Sim(LockService, Storage, Client, name="one_client", seed=1)
    s.add_client(1, writes=2)
    return s.run(), {1: "Wrote"}


def two_clients():
    """The second client finds the lock busy and asks again."""
    s = Sim(LockService, Storage, Client, name="two_clients", seed=2)
    s.add_client(1)
    s.add_client(2, max_attempts=3, at=2)
    return s.run(), {1: "Wrote", 2: "Wrote"}


def holder_crashes():
    """Client 1 dies holding the lease; only expiry frees the lock for client 2."""
    s = Sim(LockService, Storage, Client, name="holder_crashes", seed=3)
    s.add_client(1, behavior="Crashes")
    s.add_client(2, max_attempts=6, at=5)
    return s.run(), {1: "Crashed", 2: "Wrote"}


SCENARIOS = [one_client, two_clients, holder_crashes]


def main(names):
    failed = 0
    for scenario in SCENARIOS:
        if names and scenario.__name__ not in names:
            continue
        result, expected = scenario()
        ok = result.outcomes == expected
        failed += not ok
        print(f"{'ok  ' if ok else 'FAIL'} {result.name}: outcomes {result.outcomes}"
              + ("" if ok else f", expected {expected}") + f"; log {result.log_path}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
