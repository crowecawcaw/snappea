# Checker discovery benchmark

The objective is to find **every declared bug site**, across different error
families. Compare all-bug completion rates first, then bug recall and p90 schedules
to all bugs. Time to the first error is not a ranking criterion.

The versioned manifest contains 24 cases with 66 known sites and two safe controls.
Families cover visible data prefixes, opaque choices, numeric boundaries,
unrelated message permutations, read/modify/write races, pending writes, multiple
priority changes, frequent failures masking rare failures, assertions, deadlocks,
unhandled events, and hot monitors at program termination. The liveness cases
terminate, so their failures are meaningful under unfair schedulers too.

Each model has an independent expected-error catalog. Unexpected or ambiguous
failures stop the benchmark. The catalog is never inferred from the union that
candidate schedulers happen to find. Safe controls reject any failure and check
whether exhaustive completion is meaningful; they use 256 schedules and do not
count as free bug completions. Change the suite version when changing program
semantics or oracles.

## Run locally

```sh
python3 scripts/bench-checker.py --split develop --seeds 10 --budget 10000 \
  --out benchmark-results/develop.json
python3 scripts/bench-checker.py --split evaluation --seed-start 100 --seeds 10 \
  --budget 10000 --out benchmark-results/evaluation.json
python3 -m unittest discover -s scripts -p test_bench_checker.py
```

The full suite (`--split all`) compares auto, feedback, random and PCT with 0, 2
and 10 change points. Ten seeds give 1,440 runs. `--strategies auto` gives 240 runs.
Development and evaluation use different sizes, permutations and actor creation
orders; the CI commands below also use disjoint seed ranges. Keep evaluation
performance separate from tuning. These synthetic cases complement the existing
search and example-corpus benchmarks; they do not model production bug prevalence.

Runs stop after all sites, completed exhaustion, or a limit. Misses are infinite
when computing nearest-rank p90 across **all seeds**, not just successful seeds.
A censored p90 is never reported as zero. PAR2 charges misses twice the schedule
limit and is secondary to completion. Time caps are recorded separately; do not
claim a full schedule-budget comparison when a time cap was reached.

Defaults are 2,000 steps, 10,000 schedules, 30 seconds per run, one checker worker
and two independent benchmark processes. Sources and executable hashes, revision,
dirty status, seeds, limits, first discovery of each site and scheduler allocations
are recorded. `--binary` measures a frozen executable without rebuilding.
Interrupted runs leave a journal rather than masquerading as complete results.

## Instructions to settle every case

`scripts/bench-instructions.py` measures what a verdict costs: the p90, across seeds,
of the CPU instructions `auto` executes until a case is settled. A bug case is settled
when every declared site has failed; a safe case when exhaustive search has finished.
Use it to judge a change to the search itself (pruning, reductions, replay), where
schedule counts mislead because schedules differ in length.

```sh
python3 scripts/bench-instructions.py --out benchmark-results/instructions.json
python3 scripts/bench-instructions.py --filter leader-election --out /tmp/one.json
python3 scripts/bench-instructions.py --native --out /tmp/settled.json   # no counts, seconds
```

Counts come from valgrind's cachegrind, one process per case and seed, so they repeat
to within a fraction of a percent and need no hardware counters. Each count leaves out
what the same process executes to parse and compile the case. An unsettled run is
infinite in the p90, as for schedules. The table also gives the least and most over
the seeds, and the last line how many cases settle.

It runs the cases above with a 25,000-schedule limit, except `safe-background`, which
has nothing to settle, and the cases of [samples.json](samples.json), test cases of
the designs in `examples/samples/`:

- **sample-bug**: an expected-bug test, settled when its declared failure is found.
- **sample-safe**: a test of a fixed design.
- **sample-stretch**: larger safe tests, on three seeds. One needs about 410,000 schedules; the other
  is not finished within 50,000 and counts as unsettled until a change lets the search
  finish.

A full run takes well over an hour on four cores at ten seeds per case; `--jobs` sets
the processes, `--seeds` and `--filter` shorten a run. It is not part of CI.

Read the results case by case. The script prints no mean or sum over cases: across
models of different sizes those mostly say how the largest moved. `--compare
earlier.json` adds each case's change against an earlier result.

Results of the changes in #205 to #211 (Rust 1.97.0, valgrind 3.22). The sample tests
without `tcBug` in their name were measured on one seed here, before they ran on ten,
so their figure is one run, not a p90:

| Case | Seeds | Before (`fe2fef4`) | After (#211) | Change |
|---|---:|---:|---:|---:|
| aba-cas/tcFixedAbaInterleaving | 1 | 1,420,691,313 | 276,393,766 | x0.19 |
| boundaries-1024 | 10 | 12,891,283 | 13,422,334 | x1.04 |
| boundaries-256 | 10 | 3,194,774 | 3,312,158 | x1.04 |
| branches-10 | 10 | 1,227,522 | 1,236,708 | x1.01 |
| branches-14 | 10 | 1,674,516 | 1,682,197 | x1.00 |
| cache-invalidation/tcBugNaiveCache | 10 | 51,532,065 | 24,008,706 | x0.47 |
| circuit-breaker/tcDuplicateCooldown | 1 | 2,911,217,452 | 527,639,124 | x0.18 |
| config-rollout/tcBugLastArrival | 10 | 14,091,258 | 9,400,621 | x0.67 |
| consumer-group-rebalance/tcBugNoGenerationCheck | 10 | 25,988,488 | 16,369,593 | x0.63 |
| consumer-group-rebalance/tcJoinRebalance | 1 | 5,180,270,384 | 1,467,950,544 | x0.28 |
| dual-write-crash/tcFixedRelayCrash | 1 | 2,925,969,386 | 694,001,677 | x0.24 |
| elevator/tcTwoPassengers | 1 | 2,788,833,122 | 983,652,635 | x0.35 |
| idempotent-payments/tcIndependentKeys | 1 | 1,465,360,801 | 399,382,021 | x0.27 |
| in-flight-duplicate/tcFixedMixedKeys | 1 | 3,311,821,704 | 574,453,670 | x0.17 |
| job-queue/tcMoreWorkersThanJobs | 1 | unsettled | 56,000,756,461 | settles now |
| job-queue/tcWorkerCrash | 1 | 4,879,122,168 | 929,874,520 | x0.19 |
| leader-election/tcBugVoteNotPersisted | 10 | 24,788,217 | 16,029,024 | x0.65 |
| leader-election/tcSplitVote | 1 | 5,163,356,529 | 1,878,200,415 | x0.36 |
| leader-election/tcTwoCandidates | 1 | 3,251,332,863 | 1,253,022,107 | x0.39 |
| lease-lock-fencing/tcTwoClientsContend | 1 | 5,604,938,795 | 1,073,404,203 | x0.19 |
| lease-without-fencing/tcBugStaleLeaderWrite | 10 | 10,380,593 | 6,672,081 | x0.64 |
| lost-update-2 | 10 | 2,998,824 | 3,267,892 | x1.09 |
| lost-update-3 | 10 | 3,173,096 | 3,304,450 | x1.04 |
| lost-update/tcFixedThreeDeposits | 1 | 2,050,165,398 | 412,991,463 | x0.20 |
| minority-quorum/tcFixedPartition | 1 | 1,252,613,401 | 405,918,579 | x0.32 |
| offline-sync/tcSlowReply | 1 | 4,480,916,477 | 591,935,680 | x0.13 |
| online-migration/tcBugBlindBackfill | 10 | 33,626,423 | 21,572,889 | x0.64 |
| opaque-2048 | 10 | 24,654,566 | 25,687,402 | x1.04 |
| opaque-512 | 10 | 6,577,358 | 6,851,106 | x1.04 |
| orders-5 | 10 | 243,693,976 | 154,135,908 | x0.63 |
| orders-6 | 10 | 1,296,043,884 | 967,734,527 | x0.75 |
| priority-switches-4 | 10 | 4,052,089,068 | 2,268,666,135 | x0.56 |
| priority-switches-5 | 10 | 6,732,427,857 | 4,455,944,314 | x0.66 |
| quorum-kv/tcBugReadOne | 10 | 74,209,985 | 61,639,290 | x0.83 |
| quorum-kv/tcOneReplicaDown | 1 | 1,663,266,418 | 237,714,858 | x0.14 |
| quorum-kv/tcTwoWriters | 1 | unsettled | unsettled |  |
| runtime-12 | 10 | 1,286,805 | 1,306,643 | x1.02 |
| runtime-8 | 10 | 941,721 | 957,019 | x1.02 |
| safe-finite | 10 | 1,165,173 | 1,196,633 | x1.03 |
| seat-hold/tcCancelRacesRehold | 1 | 5,189,241,228 | 1,106,271,543 | x0.21 |
| skewed-12 | 10 | 5,417,632 | 5,515,094 | x1.02 |
| skewed-8 | 10 | 4,220,791 | 4,305,148 | x1.02 |
| stale-replica-read/tcBugMonotonicReads | 10 | 62,289,042 | 27,675,454 | x0.44 |
| stale-replica-read/tcFixedMonotonicReads | 1 | 3,211,101,079 | 672,082,041 | x0.21 |
| stall-128-noise | 10 | 347,810,933 | 337,915,019 | x0.97 |
| stall-32 | 10 | 89,099,710 | 73,746,748 | x0.83 |
| stall-64-noise | 10 | 789,811,237 | 788,371,094 | x1.00 |
| stall-96 | 10 | 63,369,651 | 65,981,804 | x1.04 |
| terminal-hot-256 | 10 | 4,559,925 | 4,675,659 | x1.03 |
| terminal-hot-64 | 10 | 1,464,442 | 1,490,029 | x1.02 |
| token-refresh/tcBothTabsExpire | 1 | 452,762,476 | 128,784,398 | x0.28 |
| transactional-outbox/tcBugDualWrite | 10 | 3,299,858 | 1,150,362 | x0.35 |
| worker-drain/tcBugTerminateEarly | 10 | 19,750,505 | 29,200,970 | x1.48 |

## CI

The `auto finds all benchmark bugs` job runs both splits, ten seeds each, with a
25,000-schedule limit. It fails if any seed misses any declared site, hits the time
cap, reports an unexpected failure, or misreports safe-control exhaustion:

```sh
cargo build --release --locked -p snappea --example checker_bench
python3 scripts/bench-checker.py --binary target/release/examples/checker_bench \
  --strategies auto --split develop --seeds 10 --seed-start 0 \
  --budget 25000 --check --out benchmark-results/develop.json
python3 scripts/bench-checker.py --binary target/release/examples/checker_bench \
  --strategies auto --split evaluation --seeds 10 --seed-start 100 \
  --budget 25000 --check --out benchmark-results/evaluation.json
```

CI uploads its measurements as workflow artifacts. Generated results, build
outputs, experiment logs and temporary reports are not committed.

## Compare revisions

```sh
python3 scripts/report-checker.py before.json after.json
```

The report refuses to compare different catalogs, sources, seed ranges or bounds.
Inspect each case rather than just an aggregate mean: better completion can still
slow a tail or lose an individual seed. Plain JSON and deterministic `.json.gz`
results are both supported.

## When each bug is found

`scripts/bench-discovery.py` draws a histogram of when `auto` first finds each known
bug, one point per bug site and seed, by schedules and by seconds. The bulk is the
usual cost. The tail and the `not found` bar are the uncertainty left after a short
run. Use it to judge a search change: the whole shape should move left.

```sh
python3 scripts/bench-discovery.py --out benchmark-results/discovery.json
python3 scripts/bench-discovery.py --out /tmp/after.json --compare benchmark-results/discovery.json
python3 scripts/bench-discovery.py --out /tmp/after.json --redraw   # draw again, no run
```

It writes the points as JSON and the picture beside it as `.svg`, and prints the
p50, p90 and p99 and the slowest sites. `--compare` draws an earlier result as an
outline. Schedules repeat exactly for a seed. Seconds are what a user waits; they
vary with the machine and with `--jobs`, so compare seconds only between runs on the
same machine. Runs use one checker worker, 20 seeds and the bug cases above. A miss
is infinite in a quantile.
