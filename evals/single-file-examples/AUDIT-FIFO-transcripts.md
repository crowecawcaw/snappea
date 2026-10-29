# FIFO transcript and outcome audit

Reviewer `/root/review_singlefile`; source review and its54hash anchors were saved
before receiving assignments. Unblinding was explicitly authorized by root. Read
FIFO prompts, full recorded command sequences, returned output, original grades,
and uniformly corrected model-source outcomes; examples/code writes were compared
with the separately reviewed frozen sources. Original source audit remains unchanged.
No additional solver feedback or source edits. Call numbers anchor observations.

## Assignment and main evidence

| Neutral | Run | Arm | Own saved tests | First P check before Python write? |
| --- | --- | --- | --- | --- |
| F1 | onefile-sol-flow-compat-02 | Sol6.1/medium compat/no skill | 7pass | yes:check6/7, code8 |
| F2 | onefile-luna-flow-multi-01 | Luna/high multi-file skill/native | 4pass | no:code8, check10 |
| F3 | onefile-sol-flow-single-01 | Sol6.1/medium single-file skill/native | 5pass | yes:check4 passes, code5 |
| F4 | onefile-sol-flow-compat-01 | Sol6.1/medium compat/no skill | 5pass | no:code precedes check within6 |
| F5 | onefile-sol-flow-single-02 | Sol6.1/medium single-file skill/native | 6pass | yes:check4 passes, code6 |
| F6 | onefile-luna-flow-single-01 | Luna/high single-file skill/native | 4pass | yes:check10, code28; first check was syntax error |

All six Python implementations pass all8unchanged hidden contract cases, including
forced initial load race. The table's saved-test outcomes are actual solver-run
results, not assertions inferred from DESIGN. P-first means first check attempt
before first Python implementation write, not necessarily an already complete
model: F6 still had incomplete searches when it began Python.

## What reference material actually reached the agents

- **F2 Luna multi-file:** reads skill/reference/guides, bounded-history README(call4),
  CAS README(call5); guesses nonexistent cas-retry.p, then lists files(call6).
  Reads Balance.pproj, full protocol.p and store.p(call7), but never driver.p.
  Thus independent oracle/storage source is visible; candidate/retry algorithm,
  concurrency barrier and generated/targeted test drivers are not. Reads real
  reservation/test_storage.py(call14) after a wrong test_reservation.py guess.
- **F6 Luna single-file:** directly requests Balance.p and bounded-history.p with
  implementation guide in one call3. Raw stdout29,787characters is NOT what the
  solver sees: bridge retains first9,000 and last9,000characters. Visible Balance
  prefix reaches line205, partway through Client candidate calculation; its later
  CAS response handling, Scenario phase barrier, generated/targeted drivers and
  test selections are omitted. The bounded-history tail starts near line104 and
  contains its actual drivers/tests. Agent never rereads omitted CAS sections.
  Reads real reservation test_storage.py(call5), and later storage.py(call27).
  The new layout made more algorithm source available, but a whole-file request
  did not guarantee the complete important pattern was read.
- **F3/F5 Sol single-file:** each call3 requests Balance.p; output15,301/13,555chars
  remains below18,000char bridge cap, so all291lines reach the solver, including
  independent oracle, full client algorithm, delayed-retry barrier and generated/
  fixed drivers. Both also guess nonexistent reservation/test_observation.py
  instead of actual test_storage.py; shell batches end successfully, hiding that
  failed read in aggregate exit codes. They read the implementation guide and
  introspect Python helper signatures/docstrings rather than reading the saved
  Python example. There is no proof of private attention or literal copying.
- **F1/F4 compat/no skill:** no skill mount/read. Both inspect Python helper docs/
  source via inspect/help (F1call5, F4call5), learn CLI syntax from --help and build
  substantial actual observation tests independently.

The old/new Luna FIFO cell has one trial per format. Direct-source routing is a
promising usability change, not a measured probability/effect size. Header comments
and single-file packaging changed together. The Sol comparison changes CLI and
skill together, so it cannot attribute improvements to either independently.

## Per-run feedback and resulting quality

### F1 Sol compat02

- One singleton-tuple parse error(call6), corrected with trailing comma(call7).
  Model precedes Python(code8); CLI checks100schedules, then default1(call9), then
  2,000seeded schedules(call10), no bugs. These are sampled compatibility checks,
  not exhaustive completion. No hot completion or separate reply data in P.
- Saved actual Python tests pass6then7methods(calls8/9): interval-constrained
  independent serialization, forced CAS races for shared and distinct put IDs,
  real public snapshots. No saved negative control for either oracle.
- Original/native supplemental5s recheck finds no bug/coverage gap but search
  remains incomplete. That is a bounded search result, not implementation failure.

### F2 Luna multi-file

- Python first(call8); P repairs entry tuple, locals-before-statements, singleton
  tuple(calls10/11/19). Three checker-ending exit2calls. Coverage initially reports
  missing rejection/empty and snapshot sites(calls12/20). Agent adds actual Store
  snapshots(call18) and a second fresh put/take per client(call28), reaching all
  required sites without suppressions, at cost of longer incomplete exploration.
- Saved actual serial capture initially omits events.p, then lacks eDone; agent
  fixes both(calls21–23). All4tests pass again after fresh-result checks(call30).
  No Python concurrent CAS race; empty take's saved retry is after later queue
  already drains. Completion remains hot/count-based but reply payload on replay
  remains unchecked; absent take item is overconstrained to zero.
- Final own60s check still search_incomplete(call32); DESIGN accurately says so.
  Uniform5s corrected model check is also incomplete with full required coverage.
  It does not drop monitor or use coverage bypass.

### F3 Sol single-file01

- Full reference visible(call3), initial P model compiles/checks complete with
  coverage(call4), then Python(call5). No compiler/setup exit2call.
- Actual5test methods pass(call6). First mutation output detects52/64 and lists
  omitted invoke, replay observation, snapshot and actual-reply observation as
  survivors. Agent adds independent planned operation/reply/snapshot totals to
  hot Completion(call7), increasing detection to59/64 under its run settings.
  Adds competing same-ID duplicate driver and restricts oracle probes to
  SpecViolation rather than arbitrary Exception(call8). This is concrete feedback
  uptake, not an inferred reasoning claim or normalized cross-model score.
- Own30s model search completes all3tests(call8), with remaining5mutants explicitly
  marked search_incomplete under1,000schedule per-mutant cap. DESIGN calls out
  bounds; some equivalence explanations are plausible but not exhaustive proofs.
  Uniform5s recheck has one incomplete competing search: does not contradict the
  longer own check and must not be reported as a proved model defect.

### F4 Sol compat01

- Python appears before first P attempt within call6. Fixes singleton tuples and
  invalid test composition(calls6–8). Memo-return abstraction is moved to load
  at call9, matching actual implementation, but dead memo-hit branches remain in
  client fallback; native coverage later flags those six sites.
- Actual observation failure(call10) reveals the recorded snapshot payload aliases
  the caller's mutable list: later append retroactively changes the event, even
  though Queue storage is correct. Agent fixes adapter with list(result)(call11),
  then all5tests pass. Useful tooling/documentation lesson: freeze mutable payload
  values at observation time. It also changes negative controls to SpecViolation.
- Compatibility rejects an unselected two-test invocation(call12). Agent combines
  serial setup and concurrent scenario into one test(call13), retaining their
  semantics, and1,000sampled schedules pass. It inspects compatibility event
  coverage(call11), but has no native required-branch or mutation feedback.
- Native uniform recheck reports six genuinely unreachable memo-hit fallback sites
  and incomplete search; Python/monitor evidence remains valid. No P progress
  obligation; strongest dedicated Python serialization-oracle controls of cohort.

### F5 Sol single-file02

- Full reference visible(call3), P complete/covered before Python(call4 then6).
  No compiler/setup exit2call. First mutation output58/68(call5) leaves omitted
  invoke/replay-observation/snapshot survivors; agent adds hot exact-count
  Completion(call6), achieving64/68. Later expands initial-state and operation
  choices, preserving complete search4,203schedules(call8).
- Saved actual tests add six forced-CAS pair shapes, delayed retries after both
  real contenders, independent real-time serialization, four contenders, growing
  histories and forced constructor CAS race. All6methods pass(call9/10).
  Corrects probes to SpecViolation and adds useful valid probe(call8).
- Final edit accepts already-established Store capacity on reopening(call10),
  beyond required same-capacity case; no core model change. Own/default and
  uniform5s checks complete/full coverage. Remaining4mutants have incomplete
  per-mutant search; count64/68 is not a proof of oracle completeness.

### F6 Luna single-file

- P first; seven checker-ending exit2calls10–16 repair local placement, reserved
  identifier data, singleton tuples, duplicate types and tuple event handler type.
  More compile friction than F2 despite more source visibility; no claim format
  reliably reduces syntax errors. Many incomplete searches follow(calls17–24).
- It eventually compares actual storage state at its modeled snapshot read, but
  call30 reduces inspections from every operation to rejected-ID r and final,
  weakens snapshot count requirement, and shortens second client's script. Search
  then completes. It later moves rejected retry after a take(call37), but
  exploration becomes incomplete again. It separates serial retry and one-shot
  concurrency(call39), adds a third take client to cover missing CAS-loser branch
  (call40), then reduces to just put/take after default-budget incomplete search
  (call42). Final complete model has two narrow scenarios, no generated inputs.
- Initial per-scenario accepted/rejected/found-work assertions are replaced by
  parameterized put/take counts and at least one snapshot(call39), making them
  reusable but leaving those outcome-class counters unused. It retains the hot
  completion monitor and actual algorithm split; no suppression/coverage bypass.
- Real application emits are copied from understood reservation pattern, with
  serial check_spec capture. Initial spec-file and wrong expected shared-put
  errors repaired(call32); all4tests pass repeatedly, including after latest
  source edits(call44). Concurrent Python barrier is before method invocation,
  not an enforced same-version race; no interval-based observed P concurrency.
- Final own checks complete/full required coverage(calls43–46); final mutation
  report88/104,16survivors all per-mutant search_incomplete. This is not comparable
  directly with Sol's smaller mutation populations or evidence of full robustness.

## Grade interpretation

Original FIFO grading restores original events.p wholesale. F2/F6 valid extra model
and completion declarations are lost, causing eDone/type setup errors in model,
saved observations and standalone monitor histories. These raw failures are grader
assembly faults, not silent/inert monitors. Preserve originals; uniform supplemental
source assembly keeps the exact public schema plus legal nonconflicting type/event
prefixes/suffixes. Source repair is not solver feedback. Corrected observation
outcomes are appended below once all six finish. Portable fault results are only
interpretable for suites that first accept the valid reference; arbitrary extra
constructor/argument-validation expectations often make other suites incompatible.

## Uniform corrected outcomes (all six complete)

| Neutral | Saved Python tests | Independent monitor histories | Native5s model recheck | Portable faults |
| --- | --- | --- | --- | --- |
| F1 | 7methods pass | 4/4valid + 4/4invalid | search_incomplete | not interpretable |
| F2 | 4methods pass | 3/4valid + 4/4invalid | search_incomplete | 2/3 |
| F3 | 5methods pass | 4/4valid + 4/4invalid | search_incomplete | not interpretable |
| F4 | 5methods pass | 4/4valid + 4/4invalid | coverage_failed | not interpretable |
| F5 | 6methods pass | 4/4valid + 4/4invalid | passed | not interpretable |
| F6 | 4methods pass | 4/4valid + 4/4invalid | passed | not interpretable |

All saved suites pass against their own frozen implementation after legal source
assembly is preserved. F2's valid-history failure is real monitor overconstraint
(found=false but nonzero irrelevant item); its raw zero-history result was not.
Only F2 is portable-reference compatible and rejects2/3seeded faults; the surviving
empty-take memo fault matches its saved scenario's delayed-retry gap. Other suites'
reference incompatibility is not a zero mutation score. F6's suite depends on real
application emission that the swapped-in reference lacks; other extra API validation
and constructor-race expectations also differ from reference. Do not weaken the
agents' useful own tests to force a portable score.

The uniform5s check uses native tooling for every frozen model, including compat
submissions, and is a supplemental artifact-quality comparison, not the feedback
compat agents saw. F3 had complete own30s searches despite the short uniform
incomplete result. F4's dead fallback branches are a genuine structural model issue,
not a queue implementation bug. The latest skill package yields encouraging Sol
model-quality distinctions and concrete mutation-feedback responses; primary Python
correctness and observed concurrent test sophistication show no consistent gain.
