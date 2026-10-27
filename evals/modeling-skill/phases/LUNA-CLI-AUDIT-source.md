# Luna/high CLI comparison: source audit

Source-first review of frozen submissions against their public contracts. No
hidden outcomes or transcripts were used for the findings below. CLI labels are
visible in sources/design notes, so this is not a blinded review. These are
small bounded models; successful searches do not establish implementation fidelity.

## FIFO

### Native 01

- `solution.py` reloads Store copies and commits queue changes plus separate
  memos with CAS; it recomputes after a conflict. The model, however, keeps
  complete queue commands atomic. There are no explicit loads, stale candidates,
  failed CAS or recomputation in P.
- An explicit test selects independent `QueueContract`. It checks FIFO,
  capacity, memoized results and snapshots without sharing a model helper.
  Main and two clients send fixed scripts; schedules vary, operations/IDs/values
  are not generated. Accepted retries are represented, but the model does not
  retry a rejected put after room opens or an empty take after a later put.
- The model snapshot handler emits an observation **only when empty**. A request
  against a populated queue produces no result event. The monitor's nonempty
  snapshot branches carry scoped coverage suppressions justified by Python
  observations; this does not verify that all model snapshot requests complete.
- Five saved Python tests include ordinary capacity/sharing checks and a real
  serial call history passed to `snappea observe` with the same monitor and
  strict unknown-event checking. No `pysnappea` helper is used, but these are
  actual implementation observations, not handwritten model traces.
- The threaded capacity test does not force a CAS collision, record intervals,
  or check a concurrent history against P. Its design explicitly avoids treating
  concurrent append order as linearization order.

### Native 02

- Python again commits items and memos together with CAS retries; initialization
  is lazy. The model serializes whole commands in one shared Store handler;
  an incremented `version` variable is never a competing read/CAS check.
- Three clients each send fixed put/retry/take/retry scripts. There are no
  generated operation choices or model snapshots. `tests.p` is comment-only,
  assuming an implicit `DefaultImpl`; no explicit test selects the spec.
- `QueueContract` is independent and reconstructs memos, FIFO and snapshots,
  but has an `unselected-spec` suppression explicitly reserving it for observation
  tests. It does not check design executions as submitted.
- Three saved Python tests include a real serial history passed to native
  `snappea observe`; no `pysnappea` helper. Full-put retry after a take, populated
  snapshots and empty-take retry are present. A separate threaded capacity test
  has no forced race/interval capture/P linearization check.

### Compat 01

- `solution.py` uses fresh Store loads and CAS loops; FIFO state and separate
  put/take result memos share the commit. Constructor initialization also uses CAS.
- The model replaces the whole queue operation with an atomic Store handler.
  It does not represent loads, stale proposals, failed CAS or recomputation. Three
  one-shot put clients and one take client vary only their schedule.
- `tests.p` is comment-only: no explicit test selects `QueueContract`. The
  declared monitor ignores request IDs and has no `eSnapshot` handler, so it
  cannot validate retry semantics or snapshots. Its first/second queue state
  independently checks ordinary capacity/FIFO behavior only.
- Put clients for items 20 and 30 both label observations `p20`, although the
  required retry case reuses an ID with the same item. IDs are absent from the
  model's Store commands; its workload does not model the required memo semantics.
- Model observations occur when clients receive replies, after the Store has
  processed operations. Reply order need not be the operations' linearization
  order; this is an additional model/monitor ordering risk if the spec is selected.
- Saved Python tests make real calls and assert results, shared capacity,
  namespaces and snapshot copying. The threaded put test checks capacity, but
  does not force a CAS collision or preserve operation intervals. It never calls
  P or the Python observation library. Its local `events` list is asserted as an
  ordinary expected list, not checked against `QueueContract`.

### Compat 02

- `solution.py` also commits items and memo tables together with CAS retries.
  It supports arbitrary positive capacity and defensive snapshot copying.
- An explicit test selects an independent `QueueContract` monitor. The monitor
  reconstructs FIFO state, put arguments/results, take results including empty
  takes, and snapshots; it does not reuse a model helper.
- The model still treats each complete command as one atomic Store handler.
  Two clients send fixed scripts: A put/take/retry-put and B put. There are no
  generated operation histories, explicit stale loads/CAS failures, full-reject
  retry, repeated take, or model snapshots.
- Python tests capture actual serial return values and cover rejected-put retry
  after room opens, empty-take retry after later work, separate namespaces and
  snapshots. They compare with a manually calculated expected history, but do
  not submit that history to P. No saved concurrent observation test.

## VCS

### Native 01

- `solution.py` publishes a content-addressed immutable event through one root
  CAS, recomputes after conflicts, and folds a captured root's ancestor chain.
  Publish outcomes and effects share that visibility point. Its reducer guards
  against applying an already-published bundle again.
- The P model checks an atomic handler for one head, two approval increments,
  passing CI and duplicate publish. A single driver sends one fixed sequence.
  Embedded assertions check publication consistency and at-most-once effects.
- It models the chosen atomic API abstraction, not the implementation's
  read/prepare/root-CAS steps. Reviewer identity/distinctness, stale revisions,
  CI replacement/digest binding, multiple heads, request identities, rejection
  results, competing clients and crashes are absent. No independent spec or
  Python-to-P observation tests; no saved custom Python test file.
- Every submit/revise event contains an entire updates map. The reducer's
  semantics are plausible, but a single sufficiently large valid update map
  cannot fit its unchunked record. Full-chain replay also has linear read cost.
  These are source-supported limits, not outcomes inferred from search results.

### Native 02

- The implementation likewise prepares immutable events then CASes a shared
  root; a separate CAS allocator supplies unique record IDs. Snapshot replay
  reconstructs memos, evidence and heads.
- The selected `AtomicHeads` spec independently rejects mixed pairs from two
  competing one-shot writers and one reader. The same fixed-pair predicate is
  also asserted in Reader; no shared implementation helper supplies the oracle.
  Writer checks only response bundle identity, not success/conflict correctness.
- The model keeps the entire publish handler atomic. It omits read/prepare/CAS
  interference, approval/CI/revision lifecycle, publish request IDs, retries and
  failures. Thus it does not validate the richer implementation protocol.
- Concrete source defect: `_apply` reapplies updates for **every** event whose
  outcome is `published`. A fresh publish request against already-published A
  records `published` again; if B has since advanced A's target heads, replay of
  that new A event restores A's older heads. The missing reducer guard is
  present in native 01. The model cannot express this history. No new probe was
  run for this source finding.
- Submit/revise events again contain an unchunked updates map; full-chain replay
  grows linearly. No saved custom Python or observation tests.

### Compat 02

- The implementation uses deterministic content-addressed persistent treap
  nodes and one root CAS. It reevaluates decisions after conflicts; publish memos
  and head changes share the root. Fresh requests for already-published bundles
  do not add head updates. This avoids native 02's reducer mistake.
- The P default `Main` initializes state but never sends itself any operation.
  A separate `Test` machine raises its own events, is never created, and never
  drives Main. There is no explicit test, independent spec or useful exercised
  history in the source. Approval counting also lacks distinct reviewer identity.
- No saved custom Python/observation tests. Each treap bundle node embeds the
  entire updates map, so persistence across many records does not solve a
  single wide bundle's record-size constraint.

### Compat 03

- Frozen submission contains only `solution.py` and `DESIGN.md`; no P source,
  project, or saved custom test. The design's claim to have modeled the protocol
  cannot be validated against a retained model.
- The intended protocol is an immutable event chain plus root CAS. However,
  `_snapshot` returns state without its root version. Operations decide from
  that state; `_append_committed` then freshly reads the root/version and can
  append the stale decision to that newer root. This permits stale approvals,
  revisions or publish decisions to commit after the state they checked has
  changed. CAS protects the append but not the decision's prerequisite snapshot.
  Native 01/02 and compat 02 carry the original snapshot's root version into CAS.
- An explicit `applied` flag avoids native 02's fresh-request reapply bug in
  isolation, but it too is decided from a potentially stale snapshot.
- Each event embeds a full updates map; wide-record and full-chain replay limits
  remain. No new probe was run for these source findings.

## Supplemental confirmation after source review

- Native 02's existing primary case `test_published_bundle_cannot_reapply`
  confirms the identified reducer bug: after B advances `schema` to `s3`, a
  fresh publish request for old A restores it to `s1`. This is a demonstrated
  correctness failure, not merely a source risk or a newly designed probe.
- Compat 03's existing concurrent evidence/revision/publish and overlapping
  publish cases both find non-linearizable histories. They substantiate the
  stale-decision concern: one ends with an old-revision approval retained after
  revision, another lets conflicting bundles both report successful publication.
- The existing supplemental wide-bundle probe fails all four VCS implementations
  with `ValueError: record exceeds 16384 bytes` for the same 64-branch,
  27,565-byte update map. This confirms the single-payload record-size limit;
  it remains separate from primary case scores.
- Independent native checks pass the two native VCS models at their selected
  bounds, despite their omissions and native 02's implementation bug. Compat 02
  fails required coverage for 37 points, including every Main operation handler;
  compat 03 has no source to check. These checks support vacuity detection, not
  a claim that the passing models faithfully model the entire contract.

## Structural comparison

| Run | Design exercised/selected oracle in source | Actual Python history checked by P | Main omission |
|---|---|---|---|
| FIFO native 01 | Explicit test + independent FIFO/memo monitor | Yes, serial CLI observation | CAS protocol abstracted; no generated operations; populated model snapshots silently omitted |
| FIFO native 02 | No explicit test; spec deliberately observation-only | Yes, serial CLI observation | Design spec unselected; CAS protocol abstracted |
| FIFO compat 01 | No explicit test; incomplete unselected monitor | No | IDs/memos absent from model and monitor; atomic-command abstraction |
| FIFO compat 02 | Explicit test + independent FIFO/memo monitor | No; actual captures only compared in Python | Fixed scripts; atomic-command abstraction |
| VCS native 01 | Fixed script + embedded publication assertions | No | Lifecycle, identities, multiple heads and CAS protocol omitted |
| VCS native 02 | Competing writers/reader + selected independent pair invariant | No | No lifecycle/retry identities; implementation reapply defect outside model |
| VCS compat 02 | Default Main is inert; Test never drives it | No | No useful modeled execution |
| VCS compat 03 | No retained P source | No | Stale decision commits against a freshly read root version |

Native produced the only saved Python-to-P checks here, while compat 02's FIFO
monitor is useful and selected. Neither CLI elicited implementation-level CAS
models or generated operation histories. The bounded sources omit important
requirements in both arms. More selected checks or more schedules do not establish
that the models cover the contracts; transcript feedback uptake is reviewed
separately in `LUNA-CLI-AUDIT-feedback.md`.

Programmatic checks have concrete, limited reach here:

- Missing P artifacts are a deliverable check; unselected specs are already a
  lint; inert Main handlers are already visible to required coverage.
- Driver reports can expose fixed counts and sequences, but cannot infer every
  omitted contract requirement or prove that an atomic-handler abstraction is
  inappropriate. All eight sources still need that human source comparison.
- Observations after real calls are verifiable in saved test code, but a file
  named `test_observation.py` or a local list named `events` is insufficient.
- Missing completion observations need a modeled obligation/operation count;
  a safety-only monitor cannot reject an event it never sees. Native 01's
  populated snapshot omission illustrates that limit.

## Provenance

Runs are under `phases/runs/luna-cli-flow-*` and
`certified-bundles/runs/luna-cli-vcs-*`; the preserved submission manifests give
full file hashes. `luna-cli-source-hashes.json` records reviewed source hashes.
All eight scored submissions were frozen before review. VCS compat 01 is excluded
as a bridge infrastructure failure. Source findings were written before consulting
grading; the existing-case confirmation above is separately labeled. No new
auditor-authored probe was run.
