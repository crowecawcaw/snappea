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
