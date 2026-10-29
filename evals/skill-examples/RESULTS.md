# Better examples, less setup friction, no demonstrated correctness gain

Improved the examples, had an independent subagent review them, fixed two defects
that review reproduced, then ran twelve fresh Luna/high agents. Two tasks × three
conditions × two repetitions: no skill, old skill, revised skill. Both skill arms
receive the same one-sentence instruction and mount different frozen snapshots.
No prethought sentence or additional scenario hints. No task/tool/skill tuning
occurred during the cohort.

The revised bundle is stronger as executable reference material. The trials show
fewer recorded setup-error calls and actual example reading, but **do not show a reliable
correctness gain**. Some agents preserve better model structure; others leave
coverage failures, drop checks, or omit the implementation behavior that fails.
This small pilot cannot establish either a causal effect or its absence.

- [Protocol](PROTOCOL.md), [reproduction](README.md), [verified records](results.json).
- [Independent example review and closure](EXAMPLE-REVIEW.md).
- Source-first audits: [FIFO](AUDIT-FLOW-source.md), [VCS](AUDIT-VCS-source.md).
- Then-unblinded transcripts/results: [FIFO](AUDIT-FLOW-transcripts.md), [VCS](AUDIT-VCS-transcripts.md).

## What changed in the examples

- New [CAS/retry driver](../../skills/p-modeling/examples/cas-retry/README.md):
  independent clients, generated immutable request definitions, full snapshot /
  candidate / CAS / reload steps, delayed retries after other completed work,
  independent balance/result oracle, actual returned-value checks, per-ID work
  counts and hot completion. Four targeted fault controls must fail real assertions.
- [Bounded histories](../../skills/p-modeling/examples/bounded-history/README.md):
  generated concurrent inputs, input/decision/reply correlation, actual Store
  snapshots after commands, zero-work and unfinished-work rejection. Comments
  explain when other clients may intervene and why receipt order is unsuitable.
- [Durable retry](../../skills/p-modeling/examples/durable-retry/README.md): pending
  requests no longer count as committed effects; concurrent callers are valid.
  The review found that a correct audit payload could hide a corrupt actual reply,
  and empty completion could pass. The monitor now observes actual eReply sends,
  validates returned IDs/results and rejects zero work; saved regressions cover both.
- [Publication](../../skills/p-modeling/examples/versioned-publication/README.md):
  two distinct publishers must complete with two commits; hot-state testing catches
  dropped completion. Comments explain the two-writer/one-retry bound and real
  storage atomicity required for the abstraction.
- [Reservation](../../skills/p-modeling/examples/reservation/README.md): consistent
  locked snapshots, forced shared-read thread race, real Python capture, separately
  isolated stale/capacity/duplicate rejections, immediate state checks and useful
  success. Post-lock event-order limitations are explicit.
- [Syntax reference](../../skills/p-modeling/reference/p-patterns.md): actual requested
  additions versus returned totals, a selected monitor and hot completion replace
  the weak nonnegative-ID assertion. It includes valid module/test/sequence syntax.

Every example states invariants, bounds, ordering/atomicity assumptions and omissions.
The CAS example is a model/driver reference, not a Python refinement proof; actual
Python capture is demonstrated in reservation. Recovery and concurrent crashes are
not silently claimed for models that omit them.

## Executed results

Each score cell lists two submissions. Primary cases, standalone monitor histories,
source quality and supplemental stress are different axes.

| Outcome | No skill | Old skill | Revised skill |
| --- | --- | --- | --- |
| FIFO hidden implementation cases | 8/8, 8/8 | 7/8, 8/8 | 8/8, 8/8 |
| VCS hidden implementation cases | 20/20, 20/20 | 20/20, no score: resource failure | 19/20, 19/20 |
| VCS separate wide-bundle workload passes | 1/2 | 0/2 (one timeout) | 1/2 |
| FIFO P tests select QueueContract | 0/2 | 2/2 | 2/2 |
| FIFO monitors pass all 8 independent histories | 1/2 | 1/2 | 1/2 |
| FIFO saved Python suites pass | 2/2 | 2/2 | 2/2 |

- Both revised VCS implementations fail `test_published_bundle_cannot_reapply`.
  Their journal replay applies every `published` outcome as new head effects,
  including a fresh request for an already-published old bundle. After newer work,
  that rewinds heads. Their P models do not validate this journal replay behavior.
- Old FIFO B01 treats a memoized None as a missing take result. Retrying an empty
  take after a later put removes the later item; hidden test returns `8 is not None`.
- Old VCS B02's final chunking edit leaves `stored_chunk` as None after a successful
  write, treats its own new chunk as a mismatch, skips the slot and repeats forever
  on the first uncontended submit. The last solver command times out; original
  grading OOMs before producing a report. Earlier public passes concern older code.
- All six FIFO standalone monitors have some meaningful checks, but one per arm
  fails independent histories. Revised A02/no-skill B02 overspecify the irrelevant
  item sentinel on an empty take. Revised A02/old C02 omit snapshot checking and
  miss an invalid retry effect visible only through later public state.

The OOM trial remains included. Docker events record `oom`, then exit137; there is
no invented 0/20 and no replacement solver. The original grader/settings remain
unchanged. To avoid repeating that resource exhaustion in supplemental stress,
all six wide probes use the same 10-second execution deadline around unchanged
assertions. Timeouts are execution failures, not property detections.

## Did the examples get used?

All eight skill agents read the skill and syntax reference. Actual example reads
are documented in the audits, rather than inferred from listings or links:

- Both old VCS agents read Revalidated.p. Revised VCS B01 reads the CAS protocol
  and Store; C02 reads all CAS P sources, including the driver.
- Revised FIFO agents read the CAS README, but neither reads its executable P
  files or runs its probes. One old FIFO agent reads the complete durable example;
  the other reads reservation's Python capture tests.
- Old FIFO B01 uses real check_spec/emit capture; C02 uses check_events. Revised
  A02 uses real monitor-backed observations; C01 only records private tuples for
  Python assertions and never invokes P on them. No VCS run captures implementation
  behavior into P, despite actual model/example reading. Both no-skill FIFO agents
  also execute actual captured CLI observation: B02 saves replay in its test;
  A01 runs replay after regenerating implementation captures but omits it from
  the saved test. Its earlier handcrafted probe is a separate oracle test.

Lower counts on a setup-friction proxy are visible in the command traces:

| Shell calls mentioning Snappea and ending exit2 | No skill | Old skill | Revised skill |
| --- | --- | --- | --- |
| FIFO, two runs | 55 | 13 | 7 |
| VCS, two runs | 21 | 6 | 2 |

This is an imperfect descriptive proxy: it includes syntax/type/lint/option errors,
counts shell batches rather than compiler invocations, and misses errors masked by
later successful commands. Audits document the concrete repairs. It is not a
correctness score or an estimated effect size.

## What did not transfer reliably

- No-skill FIFO agents suppress the unselected-monitor lint; all skill agents keep
  selection. Selection still does not ensure useful checking: revised C01 emits no
  monitored public events and returns with coverage_failed. Old B01 also leaves
  required coverage unfinished rather than producing a passing model.
- Old FIFO B01 also models split versioned load/candidate/CAS retry, but only for
  fresh puts, without modeled memos/takes. Revised FIFO A02 models full-record
  load/CAS retry and delayed histories, but
  removes stale snapshot observations and the snapshot observer after failures.
  Old C02 also drops snapshots after coverage/syntax difficulties and abstracts
  away failed CAS. No FIFO submission retains a hot completion obligation, even
  though examples demonstrate it.
- Revised VCS C02 adds a duplicate client to cover memo replay, but removes completion
  machinery after an event-order failure. B01 removes a modeled at-most-once
  assertion after replaying a duplicate-observation/routing counterexample, then accepts explicit incomplete
  coverage. These narrow the checked properties; neither validates journal replay.
- All six FIFO agents first write Python before their first P check. VCS old agents check P
  first in both runs; revised in one; no-skill in neither. Order alone does not
  establish model fidelity: the revised P-first VCS implementation still rewinds heads.
- Several saved test suites add constructor-capacity requirements that the public
  contract/private reference does not require. Their reference-incompatible portable
  probes are uninterpretable, not zero fault detections. The three compatible FIFO
  suites (two no-skill and old C02) each reject 2/3 seeded faults.

The examples now demonstrate stronger mechanisms, but agents still decide which
ones to preserve. The remaining gap is carrying independent properties, completion
and actual implementation transitions into the submission. More example prose
alone is not supported as the next remedy by these runs.

## Verification and evidence

The complete example validator passes: local links; runnable syntax positive and
missing-effect control; all positive suites; intended broken models; independent
valid/invalid state/reply/completion histories; CAS fault controls; publication and
actual-reply regression tests; four real Python reservation tests. Review findings
were fixed and independently rechecked before snapshots were frozen.

Twelve prompts, paired inputs/graders, binary/image, budgets and both immutable
snapshots are verified. No prethought addition, hidden feedback or mid-cohort tuning.
The uniform FIFO model-source supplement changes no source assembly in this cohort.
Its repeated five-second check completes no-skill B02 where the original search was
incomplete: this is finite-wall-budget variation, not a source correction. Raw
original and supplemental results remain distinct; the helper's unittest status
label is not a model verdict. Models use JSON/test selection/search status/exit code.

Old skill tree: `75cb6bc852fb31b9d88dc026d12300f26dd307b5d32f62dae09fa31b64beaaae`.
Revised: `827079afa5ecc92094b1aef194cbcf1392cc8cfe358a8cb27b9af60e1a7d2fec`.
Native binary remains the earlier pinned driver-report checker, not a rebuild.
Raw trials, snapshots, original/supplemental results, Docker failure events,
commands, final responses, references and reviewer hashes are archived at:

`/workspace/library-files/snappea-skill-examples-evidence.tar.gz`
