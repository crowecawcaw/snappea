---
diataxis: how-to
personas: [contributor]
questions:
  - How do I run the design-and-build eval for one agent and one arm?
  - How are flaws in a solver's protocol detected?
  - How do I check that the hidden tests still catch known flaws?
---

# Design-and-build eval

An agent gets a moderately detailed description of a distributed component and builds
it against a fixed driver interface. Hidden adversarial scenarios then look for the
subtle flaws the real system shipped with. The question: does a checker (snappea, or
upstream-style `p`) help the agent find those edge cases during design?

## Task: `dlock`

`tasks/dlock/public/TASK.md` describes a MinIO dsync-style read/write lock service:
broadcast to N lock servers, quorum of grants, smaller read quorum, stale locks that
must clear. The environment has crashes, restarts with empty tables, and a network
that loses, duplicates, reorders and delays. `public/sim.py` is the driver: a
deterministic simulator the solver writes `LockServer`/`LockClient` against.

`private/` holds what the solver never sees:

- `test_hidden.py`: 28 adversarial scenarios. They control only the network (by
  source, destination and time), crashes and restarts, and react to what the
  solution does ("once c0 holds the lock, restart s1"), so they work for any protocol
  and any timeout or lease length.
- `reference.py`: a lease-based design that passes every scenario.
- `mutants.py`: seeded flaws as edits to the reference, plus whole baselines:
  `dsync_original.py` (the 2020 design: stale locks never clear, restarts forget
  grants) and `dsync_fixed.py` (upstream's later fixes; restart amnesia remains).

Each hidden test maps to a flaw class in `harness.py` (`FLAW_CLASSES`), such as
`restart-amnesia`, `stale-lock-liveness`, `late-grant`, `quorum-intersection`.

## Arms

| Arm | On PATH | Prompt says |
|---|---|---|
| `snappea` | `snappea` | run `snappea guide` |
| `compat` | `snappea-compat` as `p` | `p compile`, `p check` |
| `none` | nothing | nothing |

## Run one agent

Build the binaries and the image once (`cargo build --release -p snappea --bins`,
`../minimal-prompt/image/build.sh`). Then:

```sh
python harness.py start --arm snappea --run-id dlock-snappea-01   # prints the prompt
# give the prompt to the agent; it works only through `harness.py bridge`
python harness.py finish dlock-snappea-01 --reply "<agent's final message>"
python harness.py grade dlock-snappea-01
python harness.py summary
```

`grade` writes `runs/<id>/grade.json`: hidden tests passed, failing tests, flaw
classes, the agent's own tests, and process stats (commands, checker runs, P lines).

## Check the hidden tests

Run this after changing `sim.py`, the hidden tests or the mutants:

```sh
python harness.py calibrate
```

It passes when the reference passes every hidden test and every mutant and baseline
fails at least one.

Known blind spot: dsync's K1 (maintenance purges a live lock on one peer's word)
needs a purge threshold shorter than a hold. The task bounds holds at `HOLD_MAX`, so
a sensible threshold hides it; `test_partially_reached_holder_is_not_purged` catches
only short thresholds.
