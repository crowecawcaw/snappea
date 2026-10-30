# Sol missing-cell VCS source-first audit

Neutral V1/V4 frozen submissions only so far: read all P, Python implementation,
saved Python tests and DESIGN source. No assignment map, prompts, transcripts,
report JSON, generated output, grades, hidden fixtures or executions consulted.
Some DESIGN commands reveal CLI names; external assignments/results are withheld,
not guaranteed condition blinding. Findings are static, execution claims unverified.

## V1

- Selected independent Contract derives expected results from command/history, checks both actual heads/publication flags and client-return result (Bundle.p:16-38). Starts hot; Done requires four observations/two request IDs. No explicit returned-event count, though drivers await actual replies before signalling completion. Fixed ReadyMain forces a useful publication and conflicting competitor (:102-116); generated Main varies revisions, approvals, CI and parents (:88-101).
- Separate load, client candidate, version-conditional CAS, loser reload (:40-85), stable ID replay after both initial calls complete. Store announces actual resulting public state at transition; client announces actual reply. Publication ID and bundle identity conflated within two-ID abstraction; published-with-fresh-ID branch unreachable because memo check dominates. Both modeled heads share one parent, so isolated second-parent guard fault is not distinguished. Rejection becoming eligible after competing head advance is source-feasible in generated choices, but no dedicated forced ordering. No explicit crash/evidence-operation/AVL/chunk model; DESIGN discloses bounds.
- Python one joint root containing heads/blob and two immutable AVL index roots, snapshot/CAS recomputation, exact collision-checked content addressing, chunked keys/values (solution.py:14-110,120-214). All pub outcomes durable; already-published fresh ID only adds memo (:197-198,212), no head reapplication. Original submit digest retained and mutable outputs detached through JSON decoding. No obvious required API defect statically; P does not validate AVL/chunk logic.
- Seven saved tests cover isolated authorization/duplicate evidence/wrong CI/revision/lifecycle,16 competing services, dynamically counted crash-at-every-publish-write (requires actual crash),320 advancing bundles/call budget,64branches/Unicode/long IDs/aliases, two monitor-negative probes, and real accepted/rejected public histories replayed using check_spec/emit with strict/hot completion (test_solution.py:19-162). Observations are sequential real results/state; concurrency tested separately, no append-order-as-linearization claim. Some input guard classes/checks remain outside P. DESIGN names compatibility CLI and a deliberate version-guard mutant, to verify later in transcript phase.

## V4

- Forty nondeterministic steps over two pending candidate slots separate snapshot acquisition/calculation from version-conditional publication, with aborts and reads (Commit.p:46-114). This single-machine table STILL preserves interference: two old candidates can coexist and stale publication loses. No P message-based clients are needed to model that explicit step structure. Four drivers initialize reachable fresh/ready/published/conflict states (:155-162).
- Meaningful separately written CheckCommit checks old snapshot, retained memos, publication/evidence/parents, revision clearing and ordered result guards (:122-152). No selected independent spec/public history interface or actual client reply transport; algorithm and assertions live in Coordinator. The closing generation increment/assert (:116-118) is arithmetic consistency of counters, not execution of a fresh candidate/commit or useful progress. All40steps can abort/read without publication. DESIGN correctly disclaims finite loop as liveness proof; the inline progress comment overstates what those two lines establish.
- Only one bundle plus abstract competing head-change operation, two memo IDs, no update-map/digest/unauthorized-reviewer algorithm. CheckCommit validates result guard precedence but no external actual-state/reply monitor for Python. Published state and memos retained across pending-slot deletion; abstraction does not explicitly reconstruct client volatile state on restart.
- Python immutable, collision-checked, chunked keys/values and AVL index; one root CAS, fresh snapshot on loser, durable memo-first outcomes, original submit digest, separate namespaces (solution.py:20-115,129-235). Already-published fresh ID stores memo without touching heads. No obvious required API defect statically; balanced-tree/blob details outside P.
- Five saved ordinary Python tests cover same-ID concurrency/evidence, dynamically counted publication crash cuts,350bundles+64branch call bound, huge escaped payload/aliases, and CI/guard/replay precedence (test_extra.py:8-107). No actual Python-to-P capture or independent monitor probes. Fixture private Store access only in test crash setup, not application code. DESIGN names native CLI, reports incomplete searches and no mutation; execution claims remain unverified.

## Initial source distinction

V1 has selected public history/result oracle, hot/count completion and saved real
implementation capture; V4 uses transition assertions and ordinary Python tests,
retains explicit read/commit interference, but lacks that observation/completion
interface. Both Python designs appear to use correct joint visibility and bounded
record/index representations. No condition attribution or correctness conclusion
from source alone. Remaining V2/V3 will be added before map/results access.

## V3

- Selected independent Contract checks outcomes, actual heads, complete bundle/evidence map, memo map and actual client replies; hot completion requires four observations and two replies per ID (Bundle.p:18-68). Separate load/candidate/CAS/reload clients and stable delayed invocation barrier (:71-180). Replay CASes unchanged logical state, disclosed harmless validation overapproximation rather than Python memo-hit fast path.
- Ten bounded modes include competing publishers, revision/CI/approval interference, stale/unauthorized/wrong-CI abstractions, readiness advancement, conflict, and initially already-published old bundle with heads advanced to2 (:158-171). This latter reachable-state abstraction DOES test fresh publish ID against an older published bundle without reapplying heads. Two heads always use one expected base; no isolated second-parent check. Reviewer identity is an abstract approval count, acknowledged. First-approval-from-zero branch is source-unreachable under all fixed initial modes (approvals2 except1 in mode3); native coverage may flag it, but no execution presumed.
- Separate PublicHeads spec and fixed modeled public scenario (:182-233) permit real Python replay. It checks a narrower two-ready-bundle publish/replay interface, not all full Contract evidence state. It is selected in tcPublic and checks four actual observations before cold completion. Actual model clients wait for replies; full Contract checks those payloads while public oracle checks result fields.
- Python one immutable exact-key-bucket radix-trie root, collision-checked immutable objects,8000char fragment/binary trees, joint root CAS and recomputation; all API guard order/memo/original submit semantics visible (solution.py:30-122,132-250). Already-published fresh ID only memoizes result; no head reapplication. No obvious required Python semantic violation statically; tree/fragment details outside P.
- Six saved tests include isolated evidence/revision/guard and alias behavior, explicit old-published fresh request after newer publication,12competing schedules, dynamically counted crash cuts,300bundles/64branch large escaped payload, real PublicHeads capture with strict/hot completion (test_extra.py:20-132). No invented internal commits or concurrent response-order claim. DESIGN states compat CLI and one deliberate version-guard mutant; verify outputs later. No negative monitor probe saved here.

## V2

- Three message-based clients, separate snapshot/candidate/version-CAS/reload, three generated operations each, unique request IDs per round and explicit same-command publish replay after ambiguous success (Model.p:148-240). One possible preparation abandonment per operation, then retry; publish replay result checked against original result. No hot/work-count completion: Main halts after three Done messages but a stalled client does not fail a cold safety property.
- Evaluate builds candidates; separately written Sequential derives full expected state/result from an independently retained reference image (Model.p:10-96,119-143). This is a meaningful independent transition oracle, unlike calling the same function for both algorithm and expected value. No selected public event spec or real public reply transport/result path (Committed is CAS bool); no Python-to-P monitor replay interface. Client is marked driver, excluding orchestration from mutation candidates while global Evaluate remains algorithm code. Labels alone should not imply that this transition oracle is weaker than every selected spec; assess the specific checked boundaries.
- Two bundles/heads, approvals with reviewer IDs, CI validity/failure, revisions, parents, durable memos. Both heads share the same parent. Generated later revisions/other publications can advance state and expose old-published fresh-request semantics, but no guaranteed targeted history. Submission/chunk/AVL/allocator outside P, disclosed. Bounded search/execution claims await transcripts.
- Python one root, immutable AVL tree, exact-key tie handling after hash ordering, chunked keys/values, unique allocator namespace per attempt and full root-CAS guard recomputation (solution.py:16-146,154-271). All outcome branches/memo-first/original submit replay visible; published fresh ID does not rewrite heads. Returns copied results, no obvious required Python semantic bug statically. P does not prove representation mechanics.
- Four saved ordinary Python tests:300bundles +64branches/very large escaped payload/call bound,5competing publishers, dynamically counted publication crash cuts requiring actual Crash, revision/evidence/guard/duplicate outcome semantics (test_extra.py:8-95). No actual Python-to-P capture or monitor probes. DESIGN self-reveals native CLI. Model and implementation follow broadly corresponding optimistic root boundaries, but have no observed implementation link.

## Source phase complete

All four neutral VCS submissions inspected/hash-anchored before external mapping,
transcripts, grades or executions. V1/V3 contain selected public-history monitors,
actual reply checks, hot/count completion, delayed replay and saved real capture.
V2/V4 have meaningful conditional-publication/transition safety checks and good
ordinary Python tests but no public-history replay or useful completion checks;
V2 has a genuine separate sequential oracle. V4's counter assertion is not a
liveness test. All four Python representations appear structurally consistent
with joint visibility and bounded record/index requirements. Static review is
not correctness proof or treatment attribution. DESIGN may reveal CLI names.
