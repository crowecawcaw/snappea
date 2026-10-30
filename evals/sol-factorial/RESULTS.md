# Sol: CLI × skill comparison

Skill runs more often put a P check before implementation, and their VCS agents
added real implementation-to-P validation. Native
feedback helped some agents strengthen completion checks, but neither native CLI
alone nor the skill alone consistently produced useful-work checks. Implementation
scores remain tied. One native lint crosscheck exposed a suspected false positive
in a richer reply monitor.

This adds eight missing-cell trials to the prior eight Sol trials: **16 fresh
`gpt-6.1-sol` / medium agents**, two FIFO and two VCS in each CLI/skill cell.
Prior native+skill and compat/no-skill cells ran earlier; the new off-diagonal
cells ran later. This completes the layout, not a contemporaneously randomized
factorial experiment. Tiny cell counts and the execution block limit attribution.

[Protocol](PROTOCOL.md), [new records](results.json), [all 16 records](combined-results.json),
[reference visibility](uptake.json), [prior results](../single-file-examples/RESULTS.md),
[FIFO source](AUDIT-FIFO-source.md) / [transcripts](AUDIT-FIFO-transcripts.md),
[VCS source](AUDIT-VCS-source.md) / [transcripts](AUDIT-VCS-transcripts.md).

## Matched setup

Same skill snapshot, native/compat binaries, Python backend/library, tasks, hidden
cases, read-only VCS references, image and budgets as the prior eight Sol runs.
Hashes and normalized prompts/inputs were verified across all 16. All eight new
prompts frozen before delegation; no parent history, network, hints, replacements
or rolling content changes. Source review preceded external assignments/results;
source itself sometimes reveals the CLI. Original grades preserved, same planned
source-assembly/model/observation/wide supplements applied. No solver infrastructure
failures and no model/tool/skill edits by evaluators.

## Correctness scores

**Every FIFO implementation passes 8/8 primary cases; every VCS implementation
passes 20/20. All eight Sol VCS wide-bundle probes pass.** All new saved FIFO suites
pass; each new FIFO monitor accepts all four valid histories and rejects all four
invalid histories. These are repeated
fixed suites with known ceilings, not proof of complete system correctness.

All new FIFO suites reject the portable reference on extra validation/constructor
expectations, so portable fault scores are not interpretable. The prior Sol suites
were also reference-incompatible. Do not report zero kills or compare these with
per-model mutation percentages.

## Where the artifacts differ

Each cell has four artifacts, two per task. “Model first” means first P check
attempt before the first Python implementation write, not a completed proof.
Hot completion alone can still accept an empty invocation; useful-work counts
add explicit obligations for operations/observations. Keep them separate.

| CLI / skill | Model first | FIFO hot completion | FIFO useful-work counts | VCS public monitor + real Python/P capture | VCS hot/useful-work checks |
| --- | --- | --- | --- | --- | --- |
| Compat / no skill (earlier) | 2/4 | 0/2 | 0/2 | 0/2 | 0/2 |
| Compat / skill (new) | 4/4 | 1/2 | 0/2 | 2/2 | 2/2 |
| Native / no skill (new) | 2/4 | 0/2 | 0/2 | 0/2 | 0/2 |
| Native / skill (earlier) | 4/4 | 2/2 | 2/2 | 2/2 | 2/2 |

- **Skill and design phase:** all eight skill agents check P before Python, versus
  four of eight without skill. First attempts can fail syntax or leave bounded
  search incomplete; do not infer the whole model was verified before coding.
- **VCS:** all four skill agents build independently derived public state/result
  monitors, actual reply checks, delayed competing replay, hot/work completion and
  saved real Python histories. No-skill agents still model actual optimistic
  read/candidate/CAS interference and write strong ordinary crash/concurrency/
  growth tests. One new native/no-skill model has a genuinely independent full
  Sequential transition oracle, added after mutation feedback. Lack of a selected
  public spec is not proof of a weak oracle; the concrete missing capability is
  the public-history interface and implementation replay link.
- **FIFO:** every cell already has independent QueueContract, actual Python/P
  histories, forced real CAS loss and interval-aware concurrency validation, as
  requested by TASK. Skill artifacts additionally check immediate modeled public
  state after operations (4/4 versus 0/4 no-skill) and vary bounded inputs/operations
  (4/4 versus 1/4). Actual P reply payloads including replay are checked in 3/4
  skill models versus 0/4 no-skill models. One compat+skill model retains a replay
  response gap; both new native/no-skill models omit separate public reply data.
  These distinctions do not establish better ordinary Python tests or correctness.
- **Completion:** native+skill FIFO agents reacted to omission-mutation survivors
  by adding operation/snapshot counts; one also explicitly counts client replies.
  New native/no-skill agents see surviving missing-message/work mutations and
  document safety limits without adding useful-work checks. New compat+skill
  agents make semantic fault controls, but no operation counts; one hot monitor
  only waits for eDone. That can cool even when an invocation does no useful work.
  Guidance and feedback appear complementary here, not universally sufficient.

VCS bounds still matter: several models share one parent across both heads or
conflate request and bundle identity. New compat+skill trial 2 explicitly includes
an already-published old bundle after heads advance, testing a fresh request that
must not reapply it. Broader evidence/crash/storage mechanics remain outside the
small P abstractions; ordinary implementation tests cover separate obligations.

## Reference use and compatibility adaptation

All four new compat+skill agents request actual Balance.p at call 3. FIFO trial 2
and VCS trial 2 see the entire file; the other two batch enough output to clip a
middle section. A requested whole file is not necessarily delivered whole. Manual
source/transcript audits reconstruct the bridge's 18k FIFO limit (first/last 9k)
and 16k VCS limit (first/last 8k); [uptake records](uptake.json) preserve matched visible line ranges.

The skill retained native command examples unchanged. Compat agents use the chosen
CLI, adapting file-list flags where needed. VCS trial 2 first tries unsupported
native-style `--no-mutation` and `15s` options at call 4, then switches to supported
compatibility flags at call 5. Both FIFO agents use `--pfiles` and initially encounter
multiple-test selection errors, then preserve scenarios under one nondeterministic Main so the
bare compatibility check works. They increase schedule counts above the default;
these remain sampled checks, not exhaustive evidence. No extra adaptation coaching
or native CLI mount was provided. Real Python observations use the same native
backend through library helpers in both CLI arms.

Manual sensitivity checks are useful but heterogeneous: compat+skill FIFO trial 1
requires bug status for three temporary P-source faults; trial 2 runs five Python
mutants and retains only unsuccessful suite status, not detailed failure kinds.
These are agent-authored checks, not five independently certified assertion kills
or a normalized automatic-mutation benchmark.

## Native lint crosscheck found a tooling gap

Compat+skill FIFO trial 2 adds a selected ReplyContract: request IDs become keys in
saved result maps, then actual client replies index those maps and are asserted
equal. The uniform native crosscheck reports those request_id fields unused,
plus an item field intentionally checked by the separate QueueContract. Map-key
findings are a **suspected dataflow false positive**, not an isolated confirmed
reproduction in this experiment. Intentional monitor decomposition may warrant a
scoped suppression; request-ID correlation should not be erased from assessment.

The native supplemental check stops at lint with exit 2, before behavioral search.
It is not an assertion counterexample or evidence that the reply monitor is inert.
Compatibility model checks and standalone QueueContract histories remain separate
positive evidence. The agent never saw native lint. Exact diagnostics are retained;
no suppressions/tool changes were added by evaluators. Its output contains JSON
followed by rendered diagnostics, so the new summary decoder preserves both rather
than treating ExtraData as a broken grader. Original output is untouched.

## Evidence and limits

Original and supplemental grades remain separate. New model/observation source
assembly did not change any of the four new FIFO event files; the same validated
repair was still applied uniformly. In the native 5-second supplemental model
checks: new native trial 1 completes all three tests; native trial 2 has two focused
complete tests and one incomplete broad case; compat+skill trial 1 is incomplete
with full required coverage; trial 2 is lint-gated as described. Incompletion is
not a behavioral counterexample.

All cells have useful tests; no common portable mutation score can rank them.
Per-model mutation populations evolve, surviving searches are often incomplete,
and manual controls differ. Report specific strengthening/weakening and actual
observations, not schedule/coverage/mutation totals as a design-quality score.
The source distinctions favor skill-assisted model use in these artifacts, while
implementation correctness remains at a ceiling and causal claims remain limited.
