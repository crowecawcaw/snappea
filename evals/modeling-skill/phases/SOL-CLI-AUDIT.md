# Small Sol/medium CLI comparison: source and feedback audit

Four frozen submissions, one per task per CLI, no skill. Sources were inspected
before their command histories and independent grades. This audit does not see
private model reasoning. Call numbers refer to each `tool-transcript.jsonl`.
See [protocol](SOL-CLI-PROTOCOL.md) and [reviewed hashes](sol-cli-source-hashes.json).

**Both arms produce substantive designs, monitors and tests. The difference from
Luna is much larger than the consistent difference between CLIs in this sample.**
Native VCS has broader generated lifecycle histories; compat VCS models a meaningful
manual transaction scheduler that native lint rejects for its single-machine
representation. Neither a passing native gate nor a failing policy gate is a
standalone model-quality score.

## FIFO: source quality

Both implementations put items and request-result ledgers in one Store record,
reload and recompute on failed CAS, preserve rejected/empty results, use separate
put/take namespaces, and return detached snapshots. Both models represent separate
load, proposal and versioned-CAS steps with stale-CAS retry. These preserve the
actual interference that all final Luna FIFO models abstracted away.

Both select independent `QueueContract` monitors that reconstruct FIFO and memos
from observed outcomes. Both save seven Python tests using actual public calls
and `snappea.check_events`, rather than merely comparing a handwritten event list.
Their concurrent-history searches preserve response-before-invocation edges and
compare actual arguments/results against independent sequential contracts. Both
force equal-version loads, assert real failed CAS attempts, test initialization
races, and validate capacity-2 histories with the same design monitor. Other
capacities are checked with ordinary Python assertions.

Differences are useful rather than a uniform ranking:

- Native has separate fixed serial and two-client tests. Its model emits outcomes
  at successful CAS or a store-handled cached-result event, followed by an internal
  state snapshot. Scripts cover rejected-put retry after space opens, empty-take
  retry after insertion, successful retries and shared IDs across contenders.
  Standalone snapshot calls are not a separate modeled load operation; Python
  tests do capture real snapshot results.
- Native tests delay a real successful CAS response to force completion order to
  differ from linearization order. They show the actual history passes under an
  interval-respecting order and fails if incorrectly sorted by completion time.
  Separate monitor-sensitivity controls corrupt fields in a captured real history.
- Compat has a longer 16-operation script and nondeterministically runs one client,
  two distinct clients, or two clients sharing IDs. Model snapshot and memo-hit
  observations happen at atomic load. Its forced races additionally cover full
  put versus take and empty take versus put, and it saves raw intervals and the
  chosen linearization. A synthetic negative test checks real-time constraints.
- Neither generates FIFO operation choices: the model workloads are fixed scripts;
  compat's startup choice selects a client configuration. Scheduling exploration
  is not new operation-history generation.
- Neither has a complete operation-progress/observation obligation. Native marks
  `Client` as a driver even though it contains the queue algorithm's candidate
  construction and CAS retry logic. That classification excludes this logic from
  mutation testing; its 21 reported mutants concern AtomicStore, not all queue
  algorithm code. It does not exclude Client from required coverage.

## FIFO: actual feedback uptake

### Native (`sol-cli-flow-native-01`)

- Reads CLI help and Python public docstrings/signatures in calls3/4/7. P5,
  successful selected-model check6, Python8: modeling precedes implementation.
- Its only failing command5 uses `(union {...})` test syntax;6 fixes the P module
  expression. This is P syntax recovery, not repeated CLI-option guessing.
- Calls6/9/10 show complete visible driver facts: fixed instance counts and values,
  and at most two cached-result sends per client. Outputs of4,799/9,201/9,416
  characters fit the phase bridge's18,000-character limit.
- Default check9 reports incomplete concurrent search plus mutation feedback.
  Call10 removes the last take from each concurrent script and then completes.
  It also adds a forced competing-takes Python test;11 adds response inversion.
  There is no driver expansion or generated input choice after the report.
  The command history supports a search-bound adjustment, not demonstrated uptake
  of the driver-limit hints.
- Final mutation result is9/21 detected. Two undetected mutants (removing a load
  response or successful acknowledgement) have complete searches; others have
  incomplete searches. Missing completion obligations are a real limitation;
  undetected mutants with incomplete searches are not proved equivalent.
- No Python queue algorithm change follows a model-discovered correctness bug.

### Compat (`sol-cli-flow-compat-01`)

- Reads CLI help and Python library docstrings/signatures3/4/6. Model5 checks
  before Python7. No P compile or CLI-option recovery is needed initially.
- Call8 improves the model by handling memo hits/snapshots at atomic load and adds
  explicit extra scenarios. Compat then rejects the ambiguous three-test command
  and requests a testcase name. Call9 collapses them into one test with a
  nondeterministic client-configuration choice instead of selecting/running all
  cases. This is an observable frontend ergonomics response.
- Call9 reports3,000 seeded schedules and six timelines without bugs; it does not
  establish exhaustive completion. Call10 adds forced boundary-race tests.
- No driver-limit, required-coverage or mutation reports are supplied by these
  compat checks, and no P-counterexample-driven Python algorithm repair appears.

## VCS: source quality

Both implementations use immutable persistent tries with chunked keys/values and
one versioned root CAS. They retain the root version associated with the snapshot,
reevaluate after a failed commit, and publish heads/metadata/receipt atomically.
They explicitly handle already-published bundles without reapplying heads. Chunk
sizes and fixed-fanout nodes avoid the unchunked-record defect in all four Luna
implementations. Both save five additional ordinary Python tests covering crashes,
concurrency, semantics and representation/growth. Neither reuses P monitors in
Python observations for this task.

### Native (`sol-cli-vcs-native-01`)

- `TransactionModel` implements a bounded explicit scheduler for two process slots.
  It generates operation kind, revision, request ID and argument choices, with
  separate snapshot/base-version capture, speculative Apply and commit/crash.
  This is actual bounded operation/input generation, beyond fixed actor scripts.
- Separate `Check` assertions reconstruct contract obligations from old/new state:
  receipt stability/replay, successful-publication prerequisites, revision evidence
  invalidation, CI replacement and immutable published metadata. They do not call
  Apply to derive their expected answers.
- An abstract competing bundle can advance heads after the primary publication;
  fresh and replayed primary publish requests must preserve those newer heads.
  This represents the lifecycle missing from the Luna rollback model. The model
  still abstracts immutable staging and the trie/chunk layout, and is a bounded
  safety model rather than a progress or Python-refinement proof.
- Python tests include write-by-write crash/restart, concurrent publishers,
 300-bundle growth, wide escaped payloads, aliases and forced hash collisions.
  The latter also checks a surrogate-containing branch name. No obvious source
  defect was found in this review; passing tests do not prove absence of defects.

### Compat (`sol-cli-vcs-compat-01`)

- `Main` is a meaningful manual state-machine interpreter: four actor indices each
  have snapshot, candidate, version and phase. `choose(4)` interleaves read/evaluate/
  commit phases for two conflicting publications, a revision and failing CI;
  `$` models abandonment before commit. It is not an inert default Main.
- Assertions check whole head pairs, mutually exclusive conflicting publications,
  atomic publication/memo state, unchanged prior memos, no head rewrite after a
  publication, and revision invalidation.100 bounded phase steps vary histories
  of fixed transaction purposes; request/argument identities are fixed.
- Model gaps: approvals start certified rather than evolving through distinct
  reviewers; each bundle has one coupled memo/publication identity. It cannot
  express a fresh request for the same already-published bundle or a later bundle
  advancing heads before that fresh request. Both publishers expect initial heads.
- The Python implementation independently handles those cases. Tests include every
  publication-write crash offset,220 prior bundles/memos, a64-branch cost check,
  very large escaped keys/values, isolation and semantic boundaries.

## VCS: actual feedback uptake

### Native

- Python4 precedes P5 despite the design-first prompt.5 uses reserved `state` as a
  tuple field;6 renames it.6 then fails `no-model-machine`.7 puts the same explicit
  scheduler in TransactionModel and adds a wrapper Main; the scheduling semantics
  are unchanged. Driver facts now only say Main creates one TransactionModel.
  They do not describe the scheduler's inner generated choices.
- Initial exploratory7 bypasses required coverage. Later9/11 use the normal gate,
  full required coverage, `--no-mutation`, and explicitly incomplete searches.
  Those visible driver facts do not lead to more processes or changed drivers.
-9 strengthens immutable-object hash-collision handling;10 adds a collision test.
  This is source/test development, not a repair caused by a P counterexample.
-11 removes `!root.published || root.h0 == root.proposed`, permits a competing
  bundle to advance heads after publication, and adds precise checks preserving
  heads on later primary publishes. The removed assertion is invalid in that
  broader environment. This is a justified oracle/scope refinement, not weakening
  a property to hide a checker failure. Previous model checks had found no bug;
  the transcript does not establish feedback as its cause.

### Compat

- Help3/4, P5, Python6: model check precedes implementation. P5 runs1,000 schedules
  with no bug. There are no subsequent P model changes or diagnostic-driven repairs.
  Five extra tests are added7 and design documentation8.
- No direct backend invocation, skill lookup or skill content appears in any of
  the four command histories. Python library calls legitimately use the shared
  backend internally. Neither VCS run reads the mounted ordinary P examples.

## Independent results, consulted after source review

- Both FIFO implementations pass8/8 hidden cases; both monitors pass4valid and
 4invalid histories; both saved seven-test suites pass.
- Native FIFO independently completes both selected searches with full required
  coverage. Compat's longer model is incomplete at five seconds and has12 required
  gaps. Seven gaps are redundant code made unreachable when load handles memo hits
  and snapshots; the remainder are startup choices not reached during incomplete
  exploration. Do not label those choices permanently absent. Both portable
  reference swaps are incompatible, so implementation-fault sensitivity is unscored.
- Both VCS implementations pass20/20 hidden cases, the existing wide-payload probe,
  and nine saved/supplied Python tests. No new behavioral probe was authored.
- Native VCS independently has full required coverage, no found bug and incomplete
  search. Compat VCS is rejected before execution by `no-model-machine`: its sole
  Main contains the entire manual scheduler. That is a policy/representation
  failure, not evidence of an inert model or algorithmic counterexample. The
  semantically similar wrapper transformation used by native demonstrates why the
  raw gate is insufficient for judging model quality.

## Interpretation and provenance

The sample does not demonstrate a consistent design-quality advantage for one CLI.
Both FIFO artifacts are strong. Native VCS covers more lifecycle/input choices;
compat also reaches a sound-looking persistent-root design with meaningful P
checking. No actual Python algorithm repair is attributable to driver-limit output.
Fuller source review remains necessary because missing lifecycles, progress
obligations and mixed driver/algorithm roles are not summarized by a pass count.

All four were frozen before inspection. Every submitted file was checked against
its original SHA-256 manifest; `sol-cli-source-hashes.json` records source hashes
and exact transcript/manifest hashes. The audit changes only this report and that
hash record. It does not modify frozen artifacts, tools, skills or grading.
