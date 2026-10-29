# Assignment-withheld FIFO source audit

Reviewer `/root/review_singlefile`. Read neutral F2/F4 submissions and shared public
TASK/runtime/events only. No mappings, prompts, transcripts, grades, private fixtures,
other submissions or execution consulted. Source text itself can reveal CLI names;
this is an assignment-withheld, source-first review rather than complete blinding.
Source facts below do not verify DESIGN's claims about commands/results. Hash
anchors are in the adjacent JSON file.

## F2

- **Selected independent contract:** `tests.p:1` selects QueueContract. Monitor
  maintains its own FIFO and separate put/take maps, derives expected acceptance
  and outputs, and checks public snapshots (`monitor.p:12–48`). Store announces
  state at CAS/replay boundaries before response delivery (`model.p:26–61`).
- **Algorithm fidelity:** separate load, detached candidate construction, versioned
  whole-record CAS and loser reload; queue and memo co-commit (`model.p:98–142`,
  `solution.py:22–55`). Actual implementation uses membership for saved `None`,
  separate namespaces, shared Store state, detached snapshots. No static violation
  of required Python semantics found.
- **Workload:** two independent clients, fixed values 5/9, each six operations:
  two fresh puts, take, old-put replay, old-take replay, new take
  (`model.p:92–148`). Schedules vary; data/operation scripts are not generated.
  Cross-client interference can make retries delayed/rejected/empty. No explicit
  barrier guarantees a retry follows the other client's completed update.
- **Useful completion:** initially hot monitor waits for both clients' real loop
  completion and nonzero put/take/retry counts (`monitor.p:50–57`, `model.p:148`).
  Counts are lower bounds, not a per-ID work inventory. `snapshots` is counted but
  never required; no assertion guarantees each operation's inspection.
- **P response-oracle gap:** Store announces expected replay result, but clients
  only assert `ok` on replay (`model.p:105–109,123–127`). A wrong replay reply
  payload with an unchanged correct announcement is not independently checked by
  this P model. This is a static distinguishing mutation proposal, not an executed
  fault result. Fresh replies are compared with the client's candidate result.
- **Monitor specificity:** `monitor.p:35,43` requires item=0 even when found=false.
  TASK defines no item value for absence; the saved adapter deliberately maps None
  to 0 (`test_observation.py:23`). This can reject alternative valid encodings of
  absent-item observations; it does not show a Queue implementation bug.
- **Saved actual capture:** serial public calls/results/snapshots use `emit` inside
  `check_spec(... strict=True, check_hot=True)` (`test_observation.py:14–55`).
  Additional ordinary tests cover capacities1/3, signed/zero values and Store
  sharing. No concurrent Python test/forced CAS loser. Empty take is retried only
  after both subsequently inserted items are removed (`:38–48`), so this saved
  sequence misses empty-result replay while a later live item remains. P may cover
  such cross-client schedules; no execution/completion claim made here.

## F4

- **Selected independent safety contract:** `tests.p:1` selects QueueContract.
  Monitor independently tracks FIFO and memoized decisions, ignores absent-item
  payload as appropriate, and checks snapshots (`monitor.p:6–30`).
- **Algorithm fidelity:** actual load/versioned candidate CAS and reload on loser
  (`model.p:14–40,62–97`); Python stores capacity/items/memos together with CAS
  initialization and separate namespaces (`solution.py:3–42`). No static required
  implementation violation found.
- **Workload:** deterministic sequential prelude checks old empty take after live
  item, rejected put after space opens, successful replay, and namespaces; then
  two competing scripted clients with shared and distinct IDs (`model.p:112–146`).
  No generated operation/data choices. Initialization and capacities abstracted in
  P and addressed separately in Python.
- **Progress/inspection limits:** two real eDone messages trigger final inspection
  (`model.p:148–150`), but monitor has no hot state, completion obligation, or work
  count (`monitor.p:1–33`). Dropping calls/all completions need not violate this
  safety spec. DESIGN explicitly limits its claims to safety. Only one final P
  snapshot is checked, rather than state immediately after each rejection/retry.
- **P response scope:** Store emits contract results at load/CAS; clients receive
  a memo-bearing record or CAS success bool and do not independently check actual
  public return data (`model.p:20–28,35–39,68–69`). Python tests compensate with
  actual API outputs; do not treat Store audit as a separate public-response oracle.
- **Strong saved implementation checks:** real serial capture tests rejection replay,
  empty-result replay while later work remains, historical take replay, signed/zero
  values, reconstruction and detached snapshots (`test_observation.py:100–122`).
  Concurrent tests force two snapshots of one version using a barrier, require an
  actual CAS failure, record invocation/response boundaries, and search for a legal
  serialization respecting response-before-invocation edges before `check_events`
  (`:30–71,74–96,124–157`). This avoids assuming append/completion order is the
  linearization order. Both shared and distinct IDs are covered.
- **Oracle controls/bounds:** saved Python checks capacities1/3/7 and constructor
  overlap (`:159–179`), one impossible and one overlapping valid history for the
  serialization oracle (`:181–188`), six explicit invalid monitor traces
  (`:190–204`). These synthetic controls are clearly separate from actual capture.
  Small fixed histories and no P completion proof remain explicit limitations.

## F3

- **Selected independent result and state contract:** all three tests select
  QueueContract and separate Completion (`tests.p:57–79`). QueueContract derives
  FIFO/memos independently, checks actual QueueClient-returned payloads by stable
  ID and checks public snapshots (`monitor.p:10–54`, `model.p:110–111`). Absent
  take payload is ignored when found=false.
- **Faithful load/candidate/CAS:** whole-record versioned storage, detached client
  calculation, conflict/reload/recompute, including rejected/empty outcome memos
  (`model.p:24–47,81–106`; `solution.py:18–41`). Memo-hit P replay has a separate
  read-only Store transition; DESIGN explicitly explains immutable memo reasoning
  and distinguishes this from the implementation's original memo-hit load.
  No static required Python semantic violation found.
- **Targeted and generated workload:** serial13-call scenario checks empty replay
  while a later item remains, rejection after space opens, historical successful
  puts/takes and separate namespaces (`tests.p:8–20`). Competing clients add one
  generated item in{-1,0,1}, shared put IDs, fixed delayed replays and conflict
  schedules (`:38–45`). Duplicate scenario races identical put/take IDs (`:65–70`).
  Operation shapes are fixed; only one item choice is generated, not arbitrary
  histories. No barrier guarantees delayed retries follow the other's completion;
  serial cases deterministically establish the relevant state advances.
- **Useful completion:** initially hot Completion requires a nonzero planned
  workload, exact operation/reply counts, one immediate snapshot per operation
  plus a final snapshot, then cools (`model.p:116–135`). Drivers wait for real client
  and inspection replies before eFinished (`tests.p:21–26,46–53,68–75`). Counts are
  aggregate rather than per-ID inventories, but cover omitted whole operations,
  missing results and missing inspections. The bounded retry-round assertion is
  explicit (`model.p:83`), not an unbounded production progress guarantee.
- **Immediate resulting-state checks:** Store observes its current record after
  every committed/replayed operation, including rejection (`model.p:54–62`). This
  avoids relying only on a correct result announcement or a later final state.
- **Saved actual implementation evidence:** serial tests reconstruct Queue objects,
  record actual API results, and compare each immediate snapshot to calculated
  contents (`test_observation.py:108–129`). Concurrent tests force two same-version
  loads, require at least three real CAS conflicts, record invocation/response
  intervals, find a contract-valid ordering respecting real-time precedence, then
  pass actual results and final snapshot to QueueContract (`:33–100,131–165`).
  They cover fresh distinct puts, same-ID rejected puts, same-ID takes and retries.
  Thus append order is not presumed to be commit order.
- **Independent controls and limits:** a hand-calculated valid history plus ten
  explicit invalid histories require SpecViolation (`:199–224`), distinct from
  implementation capture. Ordinary tests cover capacities1/3/17, repeated cycles,
  old IDs across growth, snapshot isolation and overlapping constructors
  (`:167–197`). Concurrent initialization is attempted with a thread pool but
  unlike operation contention has no barrier guaranteeing a collision. The
  Python serialization helper has no separate saved negative-control unit test;
  monitor controls test a different oracle. Neither point is an observed failure.
  DESIGN distinguishes finite search/mutation evidence from equivalence/progress
  proof; those reported execution outcomes remain unverified in this source audit.

## F5

- **Selected independent contract:** test selects QueueContract and Completion
  (`tests.p:66`). Oracle derives capacity/FIFO and original put/take results from
  history, checks immediate public state, and ignores absent-item sentinel
  (`monitor.p:7–34`). Client separately compares actual responses with its saved
  candidate or memo result (`model.p:74–96`); unlike an audit-only response check,
  corrupting a reply alone would violate an explicit client equality.
- **Algorithm fidelity:** separate atomic load and whole-record conditional CAS,
  discarded loser candidate/reload, co-committed state+outcome memo
  (`model.p:18–35,66–98`; `solution.py:18–42`). Memo-hit replay is a separate
  read-only Store step, with stable memo results; DESIGN documents that abstraction.
  Python CAS-initializes Store, uses authoritative shared capacity, and returns
  detached snapshots. No static required implementation violation found.
- **Generated distinctions and deterministic follow-up:** optional initial full
  state; each of two independent calls chooses put/take; driver can reuse one
  identical ID/argument definition (`tests.p:30–43`). After both real completions,
  serial history fills/rejects/drains, retries an old rejection after room exists,
  retries empty take after a later put, old successful puts/takes and the initial
  contenders (`:44–57`). Values cover negative/zero/positive. History remains
  finite and later operation shape fixed; generation is targeted, not arbitrary.
- **Useful completion and state:** initially hot Completion requires expected
  operation and snapshot totals16/18, with driver eFinished only after all real
  eDone responses (`tests.p:2–15,58–64`). Every Store operation/replay announces
  actual current state immediately (`model.p:38–45`). Counts are aggregate, with
  expected count supplied at completion; no distinct-ID inventory or separately
  counted returned payloads. Actual driver control flow still waits for them.
- **Saved actual capture and independent concurrency witness:** Recording wraps
  actual calls and saves invocation/response boundaries; serial test reads public
  snapshots immediately after every result (`test_observation.py:31–100,135–158`).
  Bounded independent FIFO search respects real-time precedence, includes snapshots
  and memos, and only then passes actual events to P. Concurrent helpers force two
  same-version loads and assert CAS loss; six scenarios cover distinct/same-ID puts,
  put/take, distinct/same-ID takes, full-put/take. Delayed retries follow both
  original contenders and reconstruct Queue instances (`:103–131,160–192`).
- **Additional evidence design:** four competing puts repeat12times and retry both
  accepted/rejected IDs after draining (`:194–214`); capacities1/3/7 and200cycles,
  detached snapshots, old outcomes, existing Store capacity, and forced constructor
  CAS race (`:216–250`). Separate valid/five invalid monitor traces require
  SpecViolation (`:252–266`). The Python serialization helper has no dedicated
  saved false-history control; monitor probes test the P oracle rather than this
  Python helper. This is a testing limitation, not an observed false acceptance.
  No crashes/eviction/unbounded progress modeled, explicitly stated in DESIGN;
  its execution/mutation-result claims are not verified by this source-only audit.

## F6

- **Selected independent oracle and completion:** both tests select QueueContract
  (`tests.p:1–2`). It derives independent FIFO/memos, ignores empty-item sentinel,
  checks snapshots, starts hot, and at eDone requires scenario put/take counts and
  at least one inspection (`monitor.p:18–71`). Coordinator waits for the real
  client(s) to finish before eDone (`model.p:166–170`). Counts are aggregate and
  supplied at completion, not per-ID inventories; several outcome-class counters
  are recorded but never asserted (`monitor.p:12–15,30–31,53–54`).
- **Algorithm fidelity:** load/full candidate/versioned CAS, loser reload,
  co-committed queue and memo maps (`model.p:24–55,94–151`; `solution.py:50–95`).
  Correct Python membership preserves saved None, distinct namespaces, shared
  capacity and detached snapshots; no static required semantic violation found.
- **P actual replay response gap:** Store announces saved result, whereas clients'
  final assertions accept any result if the ID existed in the loaded memo
  (`model.p:114,150–151`). Thus a corrupt replay response can coexist with a correct
  announcement and pass those assertions. This is a static proposed distinguishing
  fault, not an executed counterexample. Fresh responses have meaningful equality.
- **Workload/inspection:** fixed serialized10-call history covers rejection retry
  after space opens, historical accepted put/take, separate namespaces and immediate
  empty retry (`model.p:184–196`). Separate concurrent test is just one fresh put
  versus one fresh take (`:209–215`), no concurrent duplicate or rejected replay.
  No generated inputs. Inspections occur only after put ID r or a client's last
  operation (`:76–79`), not after every rejection/replay. Python separately covers
  empty replay against a later live item.
- **Actual capture and limits:** emit is in real Queue methods after known results
  (`solution.py:63,72,85–94,105`); serial tests capture actual calls under strict
  check_spec, check immediate rejection snapshots, old successful outcomes,
  namespaces and signed/zero values (`test_observation.py:17–62`). They explicitly
  disable check_hot, so this capture checks safety rather than completion.
  Ordinary tests cover capacity3 and two competing puts (`:64–94`). The concurrency
  barrier is before Queue.put, not between load and CAS, and no CAS conflict is
  required; actual stale-snapshot interference is not guaranteed. Concurrent calls
  are intentionally not interpreted as an ordered P history. There are no saved
  independent monitor-negative controls or observed-concurrency linearization
  tests. Snapshot equality uses sets only for the two unordered concurrent puts,
  where either insertion order is valid.
- **Public declaration additions:** `events.p` retains original ePut/eTake/eSnapshot
  semantics but prepends PutMemo/TakeMemo types and adds internal model events.
  All are required when assembling this submission; dropping these legal extras
  would cause source-assembly errors rather than expose a model defect.

## F1

- **Selected independent safety oracle:** `tests.p:1` selects QueueContract;
  monitor independently derives capacity/FIFO, memoized results, and snapshots,
  ignoring irrelevant absent-item value (`monitor.p:6–35`). No hot/completion
  property or nonzero useful-work count; clients halt after six chosen operations
  (`model.p:44–52`). All12choices can be snapshots, so generation does not itself
  guarantee a mutating or retry workload.
- **Algorithm and abstraction:** separate Read and conditional whole-record Swap,
  loser reload/recompute, queue+memo co-commit (`model.p:13–28,58–88`). Model uses
  CAS even for snapshots and memo hits, while Python returns those from load;
  DESIGN explicitly identifies the stronger implementation abstraction. Fresh
  mutation boundaries match actual load/candidate/CAS, but memo-hit/read-only
  version advances and conflicts differ. No static required Python violation found
  in the actual implementation (`solution.py:24–55`).
- **Generated shapes:** two independent clients choose put/take/snapshot, reuse
  IDs a/b/c consistently mapped to10/20/30 (`model.p:50–52,93–100`). Retries and
  delayed interference are possible, not forced by targeted scenarios or a phase
  barrier. P omits signed/zero input classes; Python tests include them.
- **Observation/response limits:** Store announces result at accepted Swap; client
  receives only Swap's success bool (`model.p:20–27,85–88`). This models no separate
  actual public response payload to compare against the independent oracle.
  Snapshots are randomly chosen calls, not required immediately after rejected or
  replayed operations. The finite script halts normally, but no property rejects
  lost/abandoned useful work.
- **Saved actual tests:** serial wrapper records real call outputs and snapshots,
  including rejected put after room opens and empty take replay with newer item
  (`test_observation.py:109–139`). Concurrent histories record actual invocation/
  return bounds, independently find legal FIFO order respecting nonoverlap edges,
  and pass only that witnessed ordering to P (`:25–77,141–169`). Same-ID and
  distinct-ID put tests force actual competing CAS attempts using a test-only
  compare_swap barrier and require a real conflict (`:80–101,171–181`). Further
  mixed three-client histories, repeated takes, capacities1/3/7, detached snapshots
  and40concurrent unique writes/removals provide broader ordinary checks
  (`:183–235`). Thus concurrent append order is not presumed linearization order.
- **Control limits:** no dedicated saved invalid-history test for the Python
  serialization oracle and no synthetic negative-control test for QueueContract.
  These tests establish positive captured histories only; correctness of both
  independent validators remains an untested component. P is bounded/safety-only;
  DESIGN's command/result claims were not verified by this source-only review.

## Source feature inventory (no result/assignment inference)

| Artifact feature | F1 | F2 | F3 | F4 | F5 | F6 |
| --- | --- | --- | --- | --- | --- | --- |
| Independent QueueContract selected | yes | yes | yes | yes | yes | yes |
| Fresh load/candidate/CAS/reload modeled | yes | yes | yes | yes | yes | yes |
| Generated operation/data choices | yes | no | one item | no | yes | no |
| Hot/count completion selected | no | yes | yes | no | yes | yes |
| Actual P reply payload checked, including replay | no payload | no replay check | yes | no payload | client equality | no replay check |
| Immediate modeled state check after every operation | no | yes | yes | no | yes | no |
| Actual Python→P saved observations | yes | yes | yes | yes | yes | yes |
| Forced real CAS loss + interval-aware concurrent P history | yes | no | yes | yes | yes | no |
| Saved negative P monitor probes | no | no | yes | yes | yes | no |
| Saved negative control for Python serialization oracle | no | n/a | no | yes | no | n/a |

These are artifact-presence distinctions, not proof of execution or correctness.
“Immediate state” describes observation at the modeled storage boundary; Python
capture ordering and scope are described per submission above.
