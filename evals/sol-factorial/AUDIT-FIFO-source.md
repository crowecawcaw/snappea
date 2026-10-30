# Sol factorial FIFO source audit

Reviewer `/root/review_singlefile`. Source phase reads only neutral frozen
.p/.py/.md submission sources and the unchanged public FIFO task/runtime/events.
No assignments, prompts, transcripts, grading, private fixtures or execution before
all four source reviews are preserved. DESIGN may reveal CLI names; this is
assignment-withheld source-first review, not guaranteed condition blinding.
Findings distinguish static code facts from proposed distinguishing faults and
from execution evidence not yet available. SHA anchors are in the companion JSON.

## F2

- **Selected independent contract:** `tests.p:62` selects QueueContract, whose own
  FIFO/put/take maps derive capacity acceptance, original retry results and full
  snapshot order (`monitor.p:7–39`). Absent take's irrelevant item is ignored.
- **Storage/algorithm:** actual load, candidate calculation, versioned whole-record
  CAS, loser reload/recompute; queue and either outcome memo become visible together
  (`model.p:18–41,66–89`; `solution.py:18–42`). Memo lookup is combined with the
  modeled load; this is a reasonable read-only abstraction because saved outcomes
  never change. Python uses membership for saved None and separate namespaces.
  No static violation of required Python semantics found.
- **Workload:** Fixed creates pairs of concurrent calls, waits for both actual
  completions between pairs, and deterministically fills/rejects/drains before
  retrying old outcomes against newer state (`tests.p:7–28`). Initial items vary
  in{-1,0,1}. Generated independently chooses operation kinds/items, optionally
  reuses the exact same command, then retries both only after both finish
  (`:39–51`). Stable arguments are retained; stale-snapshot and concurrent duplicate
  races are represented. Two-attempt assertion has a stated one-competing-write
  justification for these pair-bounded invocations (`model.p:68`).
- **Immediate state and response limit:** Store exposes actual current items after
  every fresh/replayed operation (`model.p:48–55`), preventing later work from
  hiding rejection/replay effects. Client checks actual fresh-CAS response equality
  (`:88`), but memo-hit `eResult` only assigns/dones (`:71`): a corrupted memo-hit
  response with an unchanged correct Store announcement has no independent
  result-value assertion. This is a proposed distinguishing fault, not executed.
- **No completion/property work count:** monitor starts cold and has no done/hot
  obligation; driver waits for eDone but does not expose an assertion requiring
  useful/nonzero work. Removing an entire invoke/observation can leave a safety-only
  model with no violated expectation. Actual fixed source contains substantial
  useful work; this is oracle sensitivity, not evidence it was skipped.
- **Actual saved implementation evidence:** real public calls/results and immediate
  snapshots are recorded; concurrent histories use invocation/response bounds and
  independent legal serialization before passing actual events to P
  (`test_observation.py:25–88,118–175`). Seven pair shapes force same-version loads,
  require a real CAS loss, then retry using reconstructed Queue objects after a
  later state advance (`:90–115,143–175`). Signed/zero values, both namespaces,
  rejected/empty/historical outcomes and snapshots are checked. Separate valid
  plus10invalid monitor histories require SpecViolation (`:196–219`). No dedicated
  false-history control for the Python serialization helper itself.
- **Scope:** capacities1/3/8 and detached snapshots are ordinary Python checks
  (`:177–194`). Reopening nondefault capacity with a default-capacity Queue adds
  an out-of-scope behavior expectation beyond the required same-capacity premise,
  potentially limiting portable-reference comparisons. DESIGN reports sampled
  bounded checking and manual mutation probes, not a general liveness proof;
  execution claims are unverified in this source phase.

## F4

- **Selected independent safety oracle:** all three tests select QueueContract
  (`tests.p:44–45,67`); independently tracks FIFO and separate result namespaces,
  derives correct acceptance/presence/head, validates full snapshots, ignores absent
  item's sentinel (`monitor.p:6–30`). No hot state, completion count, or nonzero
  useful-work obligation; eDone is simply ignored by each scenario.
- **Faithful storage and actual implementation:** separate read, candidate, full
  versioned CAS and loser reload (`model.p:13–31,55–88`). Both P and Python CAS
  even snapshots and memo-hit retries. This is a legal extra unchanged write rather
  than accidental mismatch; DESIGN explains extra contention/version increments
  (`solution.py:17–44`). Queue state and memos co-commit; None membership and Store
  ownership are correct. No static required Python semantic defect found.
- **Fixed scenario strengths/bounds:** serial history has empty replay after a later
  item, full rejection after space opens, old successful put/take, namespaces and
  snapshots (`tests.p:8–22`). Two clients race identical put/take IDs, plus separate
  producer/consumer workload with signed/zero values (`:35–39,55–62`). Inputs and
  operation shapes are fixed, not generated; delayed retries arise explicitly in
  serial case and from concurrent schedules, without a two-phase replay barrier.
- **Response/state observation limit:** Store emits public result payload at commit;
  client only receives CAS success bool (`model.p:20–29,83–88`). There is no separate
  actual public return payload in P to check for corruption. State snapshots occur
  only for explicit kind2calls, not after every rejection/replay. The serial driver
  does inspect immediately after its first full rejection, but not every outcome.
  Safety-only monitor can accept omitted entire operations; no useful-completion
  property rejects a silent/abandoned workload.
- **Actual saved Python observations:** records real call arguments/results and
  interval bounds; independent search respects return-before-invoke edges and exact
  result types before submitting actual witnessed events to P
  (`test_observation.py:28–74,130–153`). Forced same/distinct/mixed call scenarios
  gate same-version loads and assert actual CAS loss; ordinary mixed and fixed-seed
  random histories vary inputs (`:77–97,155–182`). A forced three-constructor race
  requires actual two CAS losers (`:184–196`), not just simultaneous method starts.
- **Ordinary checks and control scope:** capacities1/3/7, state sharing, snapshot-copy
  isolation and old empty/rejected outcomes (`:198–216`). Reopening with different
  default capacity adds an expectation outside the required same-capacity premise,
  and forced-constructor CAS-count expectations can limit portable-reference reuse.
  One captured valid FIFO trace is copied/corrupted to require SpecViolation
  (`:218–227`); useful negative monitor control, but no dedicated invalid-history
  control for the Python serialization search itself. Actual observations are not
  claimed to exhaust Python scheduling. DESIGN explicitly labels bounded safety,
  missing progress mutation checks and incomplete mutation analysis; execution
  results remain unverified during this source phase.

## F1

- **Selected independent contract:** three tests explicitly select QueueContract
  (`tests.p:27,44,67`), maintaining independent capacity2FIFO and separate memo
  namespaces and checking full snapshots (`monitor.p:6–30`). No empty-item sentinel
  overconstraint. Cold safety-only monitor; driver ignores Done, no completion or
  nonzero useful-work assertion.
- **Storage correspondence:** load, detached candidate, whole-record CAS, failed
  comparison and fresh read/recomputation (`model.p:14–28,64–90`). P and Python CAS
  even memo-hit put/take retries; Python snapshots linearize at load. Queue/memo
  co-commit, None membership, Store ownership and detached snapshots are sound by
  inspection (`solution.py:19–47`). No static required implementation defect found.
- **Scenario coverage:** two fixed overlapping scripts cover common full/empty,
  namespace and delayed retries; isolated identical put race; separate serial
  boundary history guarantees full rejection retry after space opens and empty
  retry after a live item (`tests.p:8–22,36–39,52–62`). Two clients maximum, fixed
  items/IDs, no generated data/operation choices. Signed/zero values are covered
  in Python rather than these P drivers.
- **Public state/response gaps:** model snapshots are sent only at each client's
  script end (`model.p:52–55`); no mandatory immediate state read after individual
  rejection/replay. Snapshot has no explicit client result payload/ack. Put/take
  clients receive CAS success bool only (`:27,88–90`), so wrong public return-data
  corruption is absent from the P model. Silent missing operations/messages may
  leave a cold monitor satisfied despite stalled useful work; DESIGN explicitly
  limits it to safety, not liveness.
- **Actual Python observations/concurrency:** saved real serial API calls and
  public snapshots exercise empty/rejected and old successful outcomes against
  later live state (`test_observation.py:127–148`). Concurrent histories record
  invocation/response counters and independently search contract-valid order with
  exact result types and real-time constraints before P replay (`:30–96`). Tests
  gate actual CAS attempts, require stale-version loss for same/distinct put IDs,
  then exercise same/distinct takes; four-way mixed histories include snapshots
  (`:99–123,150–192`). Constructor CAS race is also deliberately forced (`:217–224`).
- **Controls/limits:** capacities1/3/7, reconstruction, snapshot isolation and old
  empty/rejected results (`:194–215`); two corrupted real public results require
  SpecViolation (`:226–238`). No dedicated invalid-history control for Python's
  serialization helper. Extra constructor-capacity-disagreement rejection and
  constructor CAS-count assertions can limit reference-suite portability beyond
  required same-capacity semantics. DESIGN admits incomplete broad model search
  and bounded/incomplete mutation evidence; source review does not verify those
  command/result claims.

## F3

- **Selected independent contract plus reply/progress specs:** `tests.p:51` selects
  QueueContract, ReplyContract and Completion. QueueContract independently derives
  capacity/FIFO/memo results and exact snapshots, ignoring absent-item sentinel
  (`monitor.p:6–38`). ReplyContract compares actual client-returned results against
  already observed stable outcomes by namespace/ID (`model.p:154–176`); combined
  with QueueContract this checks actual public data without imposing receipt order.
- **Storage fidelity:** separate atomic load, candidate construction, full-record
  versioned CAS, loser reload/recompute; memo hit returns from loaded record
  (`model.p:21–45,73–110`; `solution.py:18–42`). Queue and outcome memo co-commit;
  rejected/None results preserved with membership. Actual fresh CAS payload also
  has explicit equality assertion. No static required Python semantic bug found.
- **Generated workload and actual delayed retries:** independently chooses put/take,
  signed/zero values and initial lengths0/1/2; targeted identical-put, identical-take
  and cross-namespace ID cases (`tests.p:5–33`). Seed operations finish first, then
  two competing calls; barrier starts stable-argument retries only after both
  first calls return (`model.p:123–145`). Two-attempt test bound is justified by one
  other fresh concurrent writer. This is finite structured generation, not arbitrary
  histories or unbounded progress.
- **Immediate public state, real reply check:** Store announces live snapshots at
  every memo-hit load and every successful CAS (`model.p:23–30,39–44`), before later
  work can hide rejected/replayed effects. eReturned reflects actual Client result
  after invocation (`:110`), so bad response data is checked separately from Store
  announcements. Fixed result tables remain stable across delayed deliveries.
- **Completion strength/limit:** initially hot Completion requires eDone, after real
  eFirstDone/eClientDone barriers (`model.p:139–151`), but has no operation/reply/
  inspection counts or nonzero useful-work requirement. Replacing invoke's body
  with no work could still produce both phases' done signals and cool Completion,
  while the safety/reply monitors see nothing. This proposed omission fault is not
  an executed result. Hot completion is stronger than a cold safety-only monitor,
  but must not be described as explicit useful-work/count validation.
- **Actual saved tests:** real serial calls reconstructed each time with immediate
  snapshots; eight generated serial histories and targeted delayed old outcomes
  (`test_observation.py:28–39,111–136`). Seven concurrent shapes force same-version
  loads, require actual CAS loss, retry both only after both originals, capture
  actual invocation/response/results, and search a contract-valid real-time order
  consistent with final public state before P replay (`:41–108,138–178`).
- **Controls/limits:** ordinary capacities1/3/5/17, detached snapshots,150memo-pairs,
  reconstruction and old results (`:180–207`). Separate valid/six invalid monitor
  probes require SpecViolation; a deliberately impossible take-before-future-put
  history tests Python serialization's real-time constraint (`:209–232`). Extra
  constructor-capacity disagreement rejection may limit portable-reference reuse.
  DESIGN reports sampled/model and manual implementation-mutation results, not
  exhaustive mutation proof; these execution claims are not yet verified here.

## Source feature inventory

| Feature | F1 | F2 | F3 | F4 |
| --- | --- | --- | --- | --- |
| Independent QueueContract selected | yes | yes | yes | yes |
| Load/candidate/full CAS/reload | yes | yes | yes | yes |
| Generated operation/data choices in P | no | yes | yes | no |
| Hot completion selected | no | no | yes | no |
| Explicit nonzero useful-work/reply counts | no | no | no | no |
| Actual P public reply payload checked on replay | no payload | no memo-hit check | yes | no payload |
| Immediate modeled public state after every operation | no | yes | yes | no |
| Real Python/P saved capture | yes | yes | yes | yes |
| Forced real CAS loss plus interval-based linearization | yes | yes | yes | yes |
| Saved negative monitor control | yes | yes | yes | yes |
| Dedicated Python serialization negative control | no | no | yes | no |

These are static artifact-presence distinctions, not verified execution outcomes or
condition rankings. All four Python implementations look contract-correct by static
inspection. Finite coverage and actual command outcomes require the separate later
transcript/result phase. No solver feedback or source edits were performed.
