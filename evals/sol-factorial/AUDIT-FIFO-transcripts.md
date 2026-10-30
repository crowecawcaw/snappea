# Sol factorial FIFO transcript and outcome audit

Reviewer `/root/review_singlefile`. All four neutral source reviews and39source hash
anchors were preserved before assignments were disclosed; source notes remain
unchanged. Root authorized FIFO-only transcript unblinding, then full disclosure
once the VCS source phase finished. Read four prompt/metadata files, complete
recorded command sequences, relevant returned source/check/test outputs, original
and uniformly supplemental grades. Source writes were assessed against the prior
frozen-source review; raw full outputs can exceed what the solver actually saw.
No solver feedback, new checker runs, or source/skill edits by this reviewer.

## Mapping and outcomes

| Neutral | Run ID | First P check vs Python | Saved suite |
| --- | --- | --- | --- |
| F1 | factorial-sol-flow-native-no-skill-02 | check5 before code6; initial search incomplete | 6methods pass |
| F2 | factorial-sol-flow-compat-skill-01 | check4, merged/default pass5, 5,000schedule check before code7 | 4methods pass |
| F3 | factorial-sol-flow-compat-skill-02 | parse4, selection error5, merged/default pass6 before code7 | 6methods pass |
| F4 | factorial-sol-flow-native-no-skill-01 | parse5, valid/incomplete check6 before code7 | 7methods pass |

All four are fresh gpt-6.1-sol/medium,900seconds/80calls/90seconds per command,
matching existing tools/backend/skill/task conditions. First attempt before Python
does not imply an exhaustive or even syntactically valid model at that time. Both
new native/no-skill agents had incomplete valid searches when starting Python.
Both compat/skill agents deliberately increase schedules beyond default1.

All four implementations pass8/8hidden contract cases including forced load race;
all saved suites pass their own implementation; all monitors pass4valid+4invalid
independent histories. All portable-reference comparisons are invalid because
candidate suites test extra capacity/argument-validation or constructor-CAS behavior
not promised by the common reference. Do not report zero fault detections or rank
portable mutation scores across these arms.

Uniform native5s supplements: F1 and F2 have full required coverage but incomplete
search; F4 completes all three selected cases with full coverage; F3 is blocked by
native lint before any behavioral search. Its valid standalone QueueContract and
own compatibility model/test results remain separate evidence.

## Reference delivery and CLI adaptation

- **F2 compat+skill01:** reads SKILL, complete syntax reference and implementation
  guide(calls1/2), requests Balance.p plus Reservation.p plus help/helper source in
  call3. Raw22,945chars exceed bridge18,000cap; only first9,000/last9,000visible.
  Balance reaches partway through line205, but Scenario barrier, generated/fixed
  cases and its later client handling are omitted. No reread repairs this gap.
  Actual oracle/Store and part of Client source do reach it. Does not read the
  separate real Python test file; helper introspection teaches capture API.
- **F3 compat+skill02:** call3 returns13,621chars, so entire291-line Balance model
  including barrier/generated/targeted cases is visible. Reads actual reservation
  test_storage.py in call7 before writing implementation. No filename guess errors.
  The resulting P Scenario and two-phase delayed retry match this relevant pattern,
  while output delivery alone cannot establish private attention or literal copying.
- Both compat/skill agents adapt native examples to the instructed compat CLI
  correctly. Initial multiple top-level tests trigger compatibility's requirement
  for explicit selection (F2call4; F3call5). Rather than omit cases, they combine
  the scenarios under one nondeterministic Main(F2call5; F3call6) so the bare check
  command still explores every case. Both use --pfiles and10,000sampled schedules;
  default one-schedule checks are not presented as exhaustive evidence.
- Both native/no-skill agents inspect CLI help and Python helper docs/signatures
  (F1calls3/4; F4calls3/4), with no skill reference access. Both build substantial
  observation tests independently, including forced real CAS loss and interval-
  constrained concurrent history validation.

## Feedback and revisions

### F1 native/no-skill02

- First model check5 already reaches all required coverage but remains incomplete.
  Python/test code6 has a wrong expect_violation invocation lacking required spec;
  switches to explicitly asserted SpecViolation(call7), then all6tests pass.
- Removes inaccurate Client driver annotation to keep algorithm mutation-visible,
  adds isolated same-ID race(call7), later adds deterministic serial boundary
  scenario(call9) without removing broad competitor scenario. Own final check has
  two focused complete cases and one incomplete broad case; all required coverage.
- Final native feedback36/69bounded detections hits30second mutation cap and shows
  omitted messages/snapshots/whole-client steps surviving. Agent documents these
  as safety/progress limitations rather than adding hot/count/reply obligations.
  This is incomplete feedback uptake, not a claim that listed survivors are proven
  equivalent or that implementation is wrong. No coverage suppression/bypass.

### F4 native/no-skill01

- Initial invalid test composition syntax is fixed(calls5/6), after which required
  coverage is full but broad concurrent search incomplete. First Python follows.
- Shortens duplicated concurrent script and adds a separate mixed producer/consumer
  scenario(call8), retaining important serial replay boundaries. Final three cases
  complete; generated input/operation choices are not added.
- Strong saved tests initially pass6methods(call7). Added three-constructor race
  leaves its test barrier active for later serial main-thread calls, producing
  BrokenBarrierError(calls8/9). Fix removes barrier after constructor race(call10),
  then all7methods pass. This fixes test machinery, not Queue semantics.
- Removes Client driver annotation(call10) before final mutation report40/71,
 30second cap reached. Reports surviving missing-message/progress mutations and
  does not add work/count/hot completion; model remains explicitly safety-only.
  No coverage bypass/suppression.

### F2 compat+skill01

- Initial P sources compile; multiple-case selection requires merged Main(calls4/5).
  Checks Python observation API with one explicitly synthetic empty snapshot(call6),
  then runs5,000sampled schedules and writes actual implementation/capture tests
  (call7). Synthetic API smoke test is not mistaken for implementation evidence.
- Reads event coverage and summary(call8); tests actual negative monitor error to
  find SpecViolation type(call9). Strengthens generated P same-ID race, Python
  same-ID rejected/empty cases, invalid snapshot ordering and stale take probes;
  narrows exception assertions from arbitrary Exception to SpecViolation(call10).
  All4saved methods pass including seven forced real CAS-race shapes.
- Manually creates three temporary P-source faults missing version guard, bypassing
  memo-hit replay, and rejecting all puts(call11). Each must produce checker bug
  status within300schedule bound; compile/selection failure is not accepted.
  Captured log reports bugs within4/1/1schedules; individual assertion diagnostics
  are not retained for every fault because common checker artifact paths overwrite.
  These are useful self-authored sensitivity checks, not a calibrated shared score.
- Retains independent contract and immediate current-state checks but no hot/count
  completion, and memo-hit actual response remains unchecked independently of the
  Store announcement. Own10,000schedule check has no bug; uniform native5s search
  remains incomplete with full required coverage.

### F3 compat+skill02

- Repairs reserved identifier data(parse4/rename5) and merges four top-level tests
  into nondeterministic Main(selection5/repair6), preserving generated/duplicate/
  namespace cases. Full example source visible before these model writes.
- Reads actual Python tests(call7), writes/runs6saved methods(call8): targeted and
  eight generated serial histories, seven forced CAS races with delayed retries,
  final snapshot and real-time linearization witness, separate monitor controls
  and a dedicated impossible-history control for the Python oracle. All pass.
- Adds actual-result extraction on memo-hit loads, client eReturned announcements
  and separately selected ReplyContract(call9); checks10,000schedules and event
  coverage. Hot Completion waits for actual two-phase done barriers, but no
  operation/reply/snapshot counts or explicit nonzero work requirement are added.
  Empty invoke could still let barriers complete: do not call this strong useful-
  work/count validation merely because it has a hot monitor.
- Manually mutates implementation in process and runs its saved targeted serial+
  forced-race methods(call10): ignore put memos, forget rejection, forget None,
  LIFO, ignore CAS failure. All five suite runs are unsuccessful. Original Queue
  is not edited; next command imports unchanged source. Detailed failure outputs
  were discarded, so this audit cannot certify each failure's assertion/exception
  kind. Report as agent-created checks, not five independently verified fault kills.
- Final repeated6tests and10,000schedule compatibility checks pass(call11). No
  suppression/bypass; claims are sampled event coverage, not branch/exhaustive proof.

## Native crosscheck lint gate is not a behavioral counterexample

F3's new ReplyContract stores request IDs as map keys and later indexes those maps
by the returned command ID (`model.p:159–173`). Native unused-payload-field reports
those ePut/eTake.request_id fields as unused despite their correlation role, plus
ePut.item. The latter is intentionally checked by separate selected QueueContract;
ReplyContract need not independently duplicate every input check. The two map-key
findings are a suspected dataflow false positive, not proven by an isolated repro
in this task. A scoped suppression can express intentional decomposition; agent
never saw native lint because its assigned CLI was compatibility mode.

Preserve exit2 and exact diagnostics. No model behavior was checked in this native
supplement, so there is no model assertion counterexample or coverage result to
substitute. Output contains leading JSON plus rendered diagnostic tail; consumers
must decode leading JSON rather than mark it malformed infrastructure. Existing
original/supplemental output remains unchanged. Candidate fixes/tool changes are
outside this experiment.

## FIFO interpretation against prior cells

Combine descriptively with the prior Sol FIFO cells under the same pins, not with
Luna or different models. These additional cells ran in a later wave, so even a
completed2x2layout is not a new concurrently randomized factorial experiment.
Two repetitions per task/cell cannot estimate a stable causal effect.

- All8Sol FIFO submissions implement correct queue semantics on the8hidden cases,
  select independent FIFO monitors, represent real fresh read/candidate/CAS/reload,
  and save actual Python/P histories with forced CAS races and interval-aware
  linearization. These are task-level successes shared by every cell.
- Both skill cells(4/4FIFO models) inspect immediate public state after each modeled
  operation; both no-skill cells(0/4) do so. Skill models also generate operation/data
  distinctions4/4 versus1/4no-skill (the earlier compat/no-skill generated driver).
  These are consistent artifact features, not proof of improved implementation.
- Actual P reply payloads including replay are checked in3/4skill versus0/4no-skill;
  compat+skill01 still has a memo-hit reply gap. The new compat+skill02 model's lint
  gate must not erase this positive behavioral structure from qualitative review.
- Separate **hot completion:** prior native+skill2/2, new compat+skill1/2, no-skill
  cells0/4. Separate **explicit useful operation/inspection counts:** only prior
  native+skill2/2 (one also counts actual replies); new compat+skill0/2. Do not pool weak eDone-only
  completion with counters that detect omitted entire operations/observations.
- Native alone exposes missing-work/message mutation survivors but both new agents
  leave them as documented safety limitations. Skill with compat prompts both
  agents to invent manual mutation checks, but those target familiar semantic
  faults, not omitted useful work. Prior native+skill agents concretely reacted to
  omission mutants by adding counts. This suggests complementary guidance/feedback
  here, not a universal superiority claim or common normalized mutation score.
- New cells all attempt P before Python; across all8FIFO runs skill4/4 versus
  no-skill3/4. First attempt alone does not establish completed models. Strong
  ordinary concurrent tests persist across all cells; no consistent advantage in
  implementation correctness or general Python test sophistication is established.
