Run `python3 scripts/bench-search.py --out search.json` from a Rust development
shell. The suite compares random, feedback, DFS, randomized DFS, and auto with
20 fixed seeds and a 10,000 schedule budget per model/strategy/seed. It stops each
run as soon as all three independently named assertion sites have failed.
Every schedule counts, including successful schedules and exhaustive prunes;
trace rendering replays do not count. Raw JSON records first discovery of each
bug, schedules tested, completion or censoring, seed, budget, revision and dirty
status. A missing `schedules_to_all` means incomplete, never zero schedules.

The fixed synthetic suite separates visible branch progress (8 and 12 choices),
opaque integer choices (no incremental coverage signal), and competing deliveries
(4 and 5 clients, three failing orders). All bug sites are independently
reachable by construction. These are controlled search experiments, not a claim
about arbitrary production models. DFS can beat sampling on small finite trees;
coverage cannot guide an opaque predicate before it succeeds. Keep these negative
controls when adding harder and real-world workloads. Version the suite whenever
models, bug identities, bounds or stopping rules change.

Compare completion rates first. The report gives completed-only medians and a
penalized mean, charging twice the budget for an unfinished run; this avoids
making a strategy look faster just because it misses the hardest seeds. Inspect
per-seed raw results before drawing conclusions. Wall time is deliberately not
the objective here (instrumentation and state hashing have different costs).

`.github/workflows/bench-search.yml` measures every main commit and appends its
raw result to `search.jsonl` on the existing `benchmarks` branch (latest result:
[snappea.dev/bench](https://snappea.dev/bench/#search)), using the same
retrying append helper as the throughput benchmarks. Runs have no concurrency
group so pending commits are not dropped. Manual branch runs upload artifacts
without updating main history.

Measurements compare code commit `63686f4` with the original feedback scheduler at
`9983d8d`, using Rust 1.99.1 and suite version 1. The checked-in raw results are
[baseline.json.gz](baseline.json.gz), [guided.json.gz](guided.json.gz), and
[holdout.json.gz](holdout.json.gz). Each records an exact commit and `dirty: false`.
Random and both exhaustive strategies produced identical per-seed results before
and after the change on seeds 0–19.

On seeds 0–19, standalone feedback found all three bugs in **100/100 model runs**,
compared with **80/100** for original feedback and **83/100** for random. DFS and
randomized DFS completed all 100. On the separate seeds 20–39, final feedback
completed **100/100**, random **81/100**, and both exhaustive strategies **100/100**.
The earlier automatic portfolio completed 99/100 on seeds 0–19 and 98/100 on seeds 20–39;
its initial exhaustive probe and work allocation mean it does not inherit the
standalone feedback scheduler's full improvement.

The table shows mean schedules to all bugs, charging **20,000** for an unfinished
10,000-schedule run (the report's penalized mean). This includes misses, rather
than comparing only the easiest completed seeds.

| Model | Original feedback | Final feedback | Random | DFS | Final feedback, seeds 20–39 |
|---|---:|---:|---:|---:|---:|
| branch-8 | 1,914.3 | 287.1 | 1,359.2 | 27.0 | 207.3 |
| branch-12 | 17,927.6 | 486.1 | 17,383.7 | 39.0 | 602.1 |
| opaque-8 | 1,971.5 | 349.6 | 1,406.2 | 513.0 | 351.1 |
| delivery-4 | 453.9 | 175.2 | 204.6 | 427.0 | 215.8 |
| delivery-5 | 6,113.1 | 682.9 | 4,702.7 | 3,699.0 | 735.6 |

DFS still wins decisively on the finite branch trees. Random has a lower median
on delivery-4 for seeds 0–19 (101.5 versus 154.5 schedules), even though feedback
has a better mean. On delivery-5, final feedback's median is 577.5 schedules,
versus 3,549 for random (19/20 complete) and 3,699 for DFS (20/20 complete).
These synthetic results establish improvements on specific search shapes, not a
universal advantage over random or exhaustive checking.

The experiments exposed three problems: timeline-only novelty misses data-only
branches; source `if` coverage misses progress inside conjunctions; and mutating
several input choices can repeatedly destroy rare prefixes. The final scheduler
uses statement/branch probes plus short-circuit outcomes, retains event/scenario
novelty for repeated stages, favors recent productive seeds, and mixes single
input changes with fresh suffixes. Distinct failing data prefixes get a bounded
share of mutations to find neighboring failure sites. Scheduling mutations retain
the broader operator: applying suffix resampling to scheduling choices regressed
the delivery workloads. After 128 schedules without progress, fresh sampling rises
from 25% to 75%, while corpus mutations continue. Upstream compatibility retains
its existing timeline/MinHash behavior and random stream.

Reproduce the separate-seed check with:

```sh
python3 scripts/bench-search.py --seed-start 20 --out holdout.json
```

The broad corpus benchmark runs every expanded test case from `examples/`, the
vendored tutorials, `design/time/`, the existing guided-prefix model, and every
upstream `Correct`/`DynamicError` regression directory:

```sh
python3 scripts/bench-corpus.py --out corpus.json --report corpus.md
```

It defaults to one seed, 200 schedules, 1,000 steps and a 0.1-second safety cap per
strategy/case, with two project processes. Use `--budget`, `--steps`, `--seeds`,
`--seconds-per-run`, and `--jobs` for deeper passes; `--scope examples` omits the
upstream regression directories and `--filter Inventory` selects matching inputs.
Every case gets per-strategy schedules to every distinct failure site and to all
sites in a shared catalog (the union found by all strategies/seeds). It computes
these first-discovery counts after the runs, so no strategy gets an easier target
because it missed a bug. A completed exhaustive run can certify the catalog;
otherwise **all observed issues does not mean all possible issues**. A case with
no discoveries has an unknown total unless exhaustion completed, never a bogus
zero schedules to all bugs. The report explicitly lists expected-bug cases with
no common issue observed.

Potential liveness temperature warnings are kept separately because unfair DFS
does not perform that check. Missing foreign implementations are execution
limitations, not model bugs. Static-error tests have no schedules and are listed
as excluded; compilation/resolution failures, runtime errors, step/time caps and
unrun measurements remain visible. The language application examples share the
inventory P model, so their Go/Python/Java harnesses do not create separate P
schedule spaces. Time examples use the checker's default time options; missing
profile/adapters remain explicit limitations of this quick pass.

The per-commit workflow stores the broad corpus result on the `benchmarks` branch
as `corpus/<commit>/<run-id>-<attempt>.json.gz` and uploads the complete Markdown
per-case table. Each run has its own compressed file, so retaining history cannot
push a growing JSONL file past GitHub's file-size limit. Earlier measurements remain
in the frozen `corpus.jsonl`; new runs do not append to it. Read a downloaded
snapshot with `python3 scripts/bench-corpus.py --report-from <file>.json.gz`.
Verify accounting with `python3 -m unittest discover -s scripts -p test_bench_corpus.py`.

The checked-in [compressed raw measurements](corpus-before-auto.json.gz) record a three-seed quick pass at
commit `befd956` (clean working tree). All 459 model inputs were inventoried:
289 resolved/compiled projects supplied **495 expanded test cases**; 148 static
error inputs were excluded as not schedulable, and 22 inputs had explicit
compilation/resolution limitations. There are 5,940 strategy/seed measurements.
Among 145 cases with observed common issues, feedback found the entire shared
catalog in **421/435** runs, random in **415/435**, DFS in **408/435**, and auto in
**421/435**. These counts include all successful schedules preceding discovery.
The broad improvement is modest and several cases still favor random.

Exhaustion certified 404 case catalogs. Thirteen expected DynamicError cases had
no common issue observed; the other unexhausted catalogs are provisional. The
quick 200-schedule, 1,000-step pass and 0.1-second safety cap leave misses and
potential liveness warnings visible. Its per-run catalog digest is stored in the
JSON: compare results across commits only when the catalogs and bounds match,
or recompute against a shared union catalog from the raw discoveries.

Reproduce this measured pass with:

```sh
python3 scripts/bench-corpus.py --seeds 3 --out corpus.json --report corpus.md
```

The gzip file contains ordinary JSON and can be read with `gzip -dc
benchmarks/search/corpus.json.gz` or Python's `gzip.open`. Print the per-case
report of any checked-in corpus result with
`python3 scripts/bench-corpus.py --report-from benchmarks/search/<file>.json.gz`.


Automatic search now exhausts small models first, then permanently hands off to
feedback when the frontier stops converging or a work, memory, or depth bound is
reached. There are no independent random or PCT arms. Feedback still performs
fresh sampling (25%, rising to 75% after 128 unproductive schedules), because
coverage gives no direction through opaque predicates. Timed models start with
feedback; incomplete exhaustion never certifies safety. The exhaustive phase
allows a narrow frontier to continue, which preserves deep recovery examples
that regressed when every search was cut off after the first probe window.

The new raw measurements are [auto.json.gz](auto.json.gz),
[auto-holdout.json.gz](auto-holdout.json.gz), and the
full corpus pass, [corpus.json.gz](corpus.json.gz).
Their metadata records the exact measured revision and clean working tree.

| Synthetic result | Previous auto | Exhaustive-first auto |
|---|---:|---:|
| All bugs found, seeds 0–19 | 99/100 | 100/100 |
| All bugs found, seeds 20–39 | 98/100 | 100/100 |
| Five-client delivery median, seeds 0–19 | 2,740 | 1,416.5 |
| Five-client delivery median, seeds 20–39 | 2,385 | 1,340.5 |

The delivery medians for previous auto exclude its incomplete runs; completion
rates above must be considered with them. Standalone feedback remains faster on
this workload (577.5 and 498 schedules respectively). The exhaustive-first policy
pays for the user's preference to finish small models exhaustively, rather than
choosing the fastest synthetic sampling strategy at every size.

The condition/transition-pair coverage from PR #95 was also evaluated. Atomic
condition probes now appear in reports, while existing expression-edge guidance
is retained: it covers Boolean assignments and returns as well as `if`/`while`
guards. Crediting every reported atomic outcome separately improved four-client
delivery median from 154.5 to 102, but five-client feedback fell from 20/20
completions to 19/20 and its penalized mean rose from 682.9 to 2,008 schedules.
The final policy excludes those additional atomic probe bits from independent
seed novelty, avoiding competing credit for overlapping condition signals.
Transition pairs do add independent context (A→B→C versus previously covered
individual edges), and now contribute to feedback novelty. A regression test
checks novelty for a new pair and none for a repeated pair.

These additional coverage types did not demonstrate an overall discovery speedup
in this pass. The final synthetic results match the same exhaustive-first policy
before their integration. They supply richer reports and potential path guidance,
without sacrificing the rare-expression-prefix guidance that already works.

The pre-rebase condition-probe experiment is recorded in
[condition-novelty-trial.json.gz](condition-novelty-trial.json.gz) and
[its holdout](condition-novelty-trial-holdout.json.gz), measured at `0bc64de`.
That scheduler patch is retained in the PR history before the corrective
expression-guidance change; raw data preserves the original measured revision.

The final broad pass covers 495 cases and 5,940 strategy/seed measurements.
On the same catalog digest as the previous pass, auto finds all observed issues
in **420/435** runs (previous auto 421/435), standalone feedback in **421/435**,
random in **415/435**, and DFS in **408/435**. The lost auto discovery is one seed
of the upstream OSR `DefaultImpl` case: all three new auto seeds reach the
200-schedule budget, so this is a scheduling tradeoff rather than a time-cap
artifact. This policy does not improve every model. Exhaustion certifies 404
catalogs; 13 expected DynamicError cases still have no observed common issue.

The CI inventory example exposed a limit in the initial policy above: its fixed
16,384-choice cap abandoned a finite search even while the cache kept closing
paths. The final policy renews that stall budget whenever a probe window prunes
at least as many schedules as it discovers states. Memory limits now count
retained alternatives rather than choices replayed across earlier schedules.
Atomic inventory exhausts in 20,241 schedules for seed 0; a regression checks
both randomized DFS and auto complete within 30,000 schedules with no bug.

The rerun at `bf0fae4` is recorded in [convergence.json.gz](convergence.json.gz),
[convergence-holdout.json.gz](convergence-holdout.json.gz), and
[convergence-corpus.json.gz](convergence-corpus.json.gz). The initial `auto` files above remain
available to compare the convergence fix's discovery tradeoff.

Both synthetic seed sets still complete 100/100 runs. Five-client auto medians
rise to 3,433 schedules (seeds 0–19) and 3,597 (seeds 20–39), versus the initial
cutoff policy's 1,416.5 and 1,340.5. They also exceed the previous portfolio's
completed-only medians of 2,740 and 2,385; that portfolio missed one/two runs.
This is the cost of preserving converging exhaustive completion rather than
claiming small-model completeness while abandoning the inventory example.
Standalone feedback results are unchanged. The broad corpus retains the same
catalog digest and completion counts: auto 420/435, feedback 421/435, random
415/435, DFS 408/435; 404 catalogs are exhaustively certified.

## Later experiments

- [Progress checkpoints](frontier-guidance.md) for feedback search: saved prefixes
  at increasing event counts. On by default since `5c2d0e6`.
- [Next-choice guidance](choice-guidance.md): a negative result. The code was
  removed after it; the write-up and raw data remain.

Since those experiments, `auto` also splits its exhaustive phase across workers:
each worker explores its own part of the search tree, all of them share one
explored-state cache, and the search is complete only when every part is (see
`crates/snappea/src/checker/exhaustive.rs`).
