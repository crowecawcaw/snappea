# Small Sol/medium CLI comparison, no skill

**Both CLIs produced strong implementations and useful models. This four-run
trial does not show a clear Snappea correctness advantage or a driver-report
effect.** Sol's compat artifacts avoid the major weaknesses of the Luna cohort.

Four fresh `gpt-6.1-sol` / medium agents, one per task per CLI, no skill in either
arm. Prompts, public/private fixtures, binaries, image, observation backend and
budgets match [Luna's CLI comparison](LUNA-CLI-RESULTS.md). See the
[protocol](SOL-CLI-PROTOCOL.md), [source/transcript audit](SOL-CLI-AUDIT.md),
[FIFO records](sol-cli-flow.json) and [VCS records](sol-cli-vcs.json).

| Measurement | Snappea | Compat |
|---|---:|---:|
| FIFO hidden implementation cases | 8/8 | 8/8 |
| FIFO independent monitor histories | 8/8 | 8/8 |
| FIFO saved Python tests | 7 pass | 7 pass |
| FIFO actual histories checked with `pysnappea` | Yes | Yes |
| FIFO fixed-budget native model check | Complete, full required coverage | Incomplete |
| VCS hidden implementation cases, 64 seeds | 20/20 | 20/20 |
| VCS existing wide-payload probe | Pass | Pass |
| VCS saved Python tests, including four supplied | 9 pass | 9 pass |
| VCS fixed-budget native model check | Incomplete, full required coverage | Single-machine lint failure |

The common native recheck uses five seconds for search and five for mutation.
Different model bounds make completion an unsuitable quality ranking. Compat's
VCS model is a meaningful manual transaction interpreter, not an inert default:
its single-machine representation triggers `no-model-machine` before search.
The original source and result are preserved; no suppression was added to improve
its grade. Both FIFO portable reference swaps reject the reference, so no fault
sensitivity score is inferred from those diagnostics.

## Source quality

- **Both FIFO models** separate load, local proposal, version-checked CAS, failure
  and recomputation. Both select independent FIFO/memo/snapshot monitors. Both
  save real Python observations, interval-respecting linearization searches and
  forced CAS collisions/initialization races. Neither assumes response order is
  commit order. Both model before implementing Python.
- **FIFO differences:** native adds a delayed-response inversion test and negative
  monitor controls derived from actual observations. Compat saves raw histories
  and forces additional full-put/take and empty-take/put boundary races. Final P
  drivers use fixed operation scripts; compat chooses among client configurations.
- **Both VCS implementations** use persistent tries, chunk keys/values to honor
  record-size limits, retain the root version used for evaluation, and recompute
  after CAS failure. Each retains five additional ordinary Python tests for
  crash/retry, contention, growth, large values and contract edges. Neither saves
  Python-to-P VCS observation tests.
- **Native VCS** generates operation kind, revision, request ID and argument for
  two logical process slots. It models revision/approval/CI transitions, durable
  outcomes and competing head advances. An independent transition-check function
  checks prerequisites, stable receipts and no reapplication of old heads.
- **Compat VCS** interleaves four fixed transaction types with explicit
  snapshot/evaluate/CAS/abandon phases. It models revision/CI races and old-request
  retries, but binds each publisher to one bundle/request pair. It cannot express
  a fresh request against an already-published bundle after a competitor advances
  the heads. It still checks substantially more than the Luna compat models.

Native's broader VCS lifecycle is a useful qualitative difference, not evidence
of causation by the CLI. Its generated choices appear in the first model draft.
Both models abstract immutable staging; neither proves Python refinement or
unbounded progress. No implementation defect was demonstrated by the retained
primary or supplemental cases.

## Feedback uptake and remaining gaps

- No concrete scenario expansion is attributable to driver-limit feedback.
  Native FIFO reduces its concurrent script after incomplete search; native VCS
  retains the reported one-TransactionModel structure. No new operation generator
  is introduced after a driver report.
- Native VCS later allows competing heads to advance after primary publication,
  replaces an invalid global invariant with precise no-reapplication checks, and
  checks immutable published metadata. Prior checks already passed their coverage
  gate; no failing counterexample or driver fact triggered this improvement.
- Compat FIFO encounters a multi-test selection error, then uses one test that
  chooses its client configuration. Source review distinguishes this CLI friction
  from model-quality failures.
- Native FIFO marks a mixed-role `Client` as a driver. That machine contains the
  queue algorithm as well as its script, so its algorithm code is excluded from
  mutation testing. The reported mutant count therefore covers a narrower scope
  than the whole design. Surviving message/ack omissions also illustrate the lack
  of a completion obligation in safety-only monitors.

The practical result is that Sol handles both surfaces well, including discovering
the Python observation API without a skill. Native has a broader VCS model in
this sample, while compat contributes strong tests and models too. One run per
cell cannot establish a reliable frontend effect. The new driver report's
steering benefit remains unproven.

## Evidence

Run IDs: `sol-cli-flow-native-01`, `sol-cli-flow-compat-01`,
`sol-cli-vcs-native-01`, `sol-cli-vcs-compat-01`. All four frozen public inputs
are unchanged. No infrastructure replacement, hidden feedback, skill access or
tool/content tuning occurred. Budgets remain 900 seconds/80 calls, with 90-second
FIFO and 120-second VCS command caps. The same native Python backend supports
both arms; solvers do not invoke that backend directly.

Use the [Luna reproduction instructions](LUNA-CLI-RESULTS.md#reproduction-and-deviations)
with `--model gpt-6.1-sol --reasoning-effort medium`, fresh run IDs, and no skill.
Checker/image hashes are unchanged. Original prompts, commands, sources, grader
inputs, model outcomes, source hashes and independent rechecks are retained in
`/workspace/library-files/snappea-sol-cli-evals-evidence.tar.gz`.
