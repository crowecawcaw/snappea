# Novel scenario results

**The new tasks do not establish a general skill advantage.** Migration test suites are strong with or without the skill; streaming suites still miss value/bucket boundaries. Model quality varies substantially in both conditions. These are small development pilots, not effect-size estimates.

Fourteen fresh Luna/high trials: eight model-only and six adding-tests runs. The skill remained frozen at `7b68681ecb4626249add3e8ed15f424dee9dd083d509fb9e0e595a2340af0c0d`; no scenario-specific skill edits or solver coaching occurred. See [protocol and calibration](NOVEL.md), [machine-readable outcomes](novel-pilot.json), and source audits below.

## Adding tests

All six agents saved tests that capture actual public calls/results and check them against the supplied P monitor. All pass on the correct service; both migration conditions also accept the permitted same-revision replay variant.

| Scenario | Skill run 01 | Skill run 02 | No-skill control |
|---|---:|---:|---:|
| Migration: valid implementations accepted | 2/2 | 2/2 | 2/2 |
| Migration: seeded faults rejected | **9/9** | **9/9** | **9/9** |
| Streaming: correct implementation accepted | 1/1 | 1/1 | 1/1 |
| Streaming: seeded faults rejected | **6/8** | **5/8** | **5/8** |

- Migration histories cover stale prepared snapshots, live writes/deletes, tombstones, superseded/unknown tokens, cutover, and post-cutover behavior. Skill-01 also creates a new key. The control reaches the same fault-detection ceiling.
- All streaming suites omit negative values and an **accepted** event exactly on a window boundary. They therefore miss sign-loss and wrong-bucket faults. The broader skill suite additionally tests a window's first event arriving after its end but before its lateness cutoff; the other two miss that distinct case.
- Guard isolation and stale-operation guidance did not automatically produce broad valid-value coverage. A timestamp near a rejection threshold does not test an accepted timestamp's bucket assignment.
- Skill suites consistently set explicit strict observation/error handling. The migration control uses `strict=False`; the streaming control relies on named-spec defaults, whose missing-event grace period can warn rather than fail. Actual captured histories are present in these runs, so this is a future-regression guard difference, not fabricated test evidence.
- These scores measure whole saved suites, including direct Python assertions; they do not show that P alone caught the defects.

Source: [test audit](NOVEL-AUDIT-tests.md).

## Streaming models

| Shared task revision / run | Native check at fixed recheck budget | Valid histories accepted | Invalid histories rejected |
|---|---|---:|---:|
| Initial task, skill-01 | Pass; search complete | 5/6 | 9/9 |
| Initial task, skill-02 | Pass; search complete | **6/6** | **9/9** |
| Initial task, control | Does not compile; no checker invoked by agent | 0/6 | 0/9 |
| Task with explicit checker command, skill-01 | Coverage failure; base search complete | **6/6** | **9/9** |
| Same corrected task, control | Pass; search complete | 5/6 | 9/9 |

- Both conditions produce stateful aggregation algorithms and separate event-driven monitors, rather than merely precomputed announcements.
- The initial skill-01 model and monitor omit the immediate first output when a window's first event arrives after the window end but before finalization. The valid-history probe catches this shared mistake. The corrected-task control makes the same omission.
- The corrected-task skill monitor handles all fifteen probes, but its selected history leaves substantial code uncovered. The agent misattributed exit 1 to the mutation schedule cap; the actual report says `coverage_failed: true` and 217 required points missing. Mutation-budget exhaustion by itself is not the failure.
- All streaming models use a predetermined public operation sequence. Their internal message schedules do not explore alternative event/watermark arrival orders. They are useful bounded executable checks, with less model-checking benefit than a generator of different histories would provide.
- The initial control looked for upstream P tools and concluded none existed, although `snappea` was installed. The task had omitted the checker command. A fresh matched pair received that command in the shared task; preserve the initial cohort but do not treat its discovery failure as a clean skill-effect comparison or pool task revisions.

Sources: [initial audit](NOVEL-AUDIT-stream-model-v1.md), [corrected-task audit](NOVEL-AUDIT-stream-model-v2.md).

## Migration models

| Condition/run | Native check at fixed recheck budget | Valid histories accepted | Invalid histories rejected | Source assessment |
|---|---|---:|---:|---|
| Skill-01 | Incomplete search/coverage | 1/7 | 8/8 | Real live/backfill actors, but model and monitor share an encoding error; incomplete deliverables |
| Skill-02 | Coverage failure; base search complete | 6/7* | 8/8* | Correct bounded merge/representation, independent monitor; explicit a/b/c domain |
| Control | Search/coverage incomplete at 5 seconds | **7/7** | **8/8** | Generic maps and meaningful competing actors; longer concurrent search remains incomplete |

- Skill-01 copies raw logical snapshot rows into the shadow table, then expects those same raw values in its monitor. `inspect_new` requires encoded values. Native assertions can miss a defect when model and oracle repeat it; the independent valid histories reject that monitor. Rejecting every invalid history does not make its oracle strong.
- Skill-01 also chose `--timeout 90s` under a 90-second shell-command cap. The command was aborted, and the agent stopped without `DESIGN.md`. Record this execution-budget failure separately from the representation defect.
- Skill-02 correctly models revision-guarded backfill, live changes, and tombstones. It declares an a/b/c input bound; its monitor rejects the full-API valid history using another string key. Its valid-history score is 6/6 within that declared key subset, 6/7 against the full service contract. The bound is a scope/reusability limit, not evidence that its modeled a/b/c histories are wrong.
- The control accepts all seven valid and rejects all eight invalid histories, with generic key maps. Its final 10-second run reaches the concurrent search limit while an ordered case completes; the independent 5-second recheck also falls short of coverage. No unbounded correctness claim follows.
- `apply` is atomic at the public-call level in this task; the intended interference is between snapshot preparation and application. This is a different boundary from the prior read/CAS queue task.
- The original model task could state read decoding more explicitly. That ambiguity does not explain Skill-01's unambiguous `inspect_new` encoding defect; its public testing task explicitly requires logical reads to agree across cutover. The checked-in model task now clarifies decoding for future runs; these scores use the original frozen task, with no solver rerun on that clarification.

\* Original isolated-monitor grading could not resolve the `Row` type declared in `model.p`. Corrected source loading produced these scores without changing the submission or hidden histories. Original compile-failure grades, original native-check evidence, archived harness, and labeled `supplemental-monitor-sources` reports are retained. Reusing a shared type is legitimate modularity, not oracle dependence.

Source: [migration model audit](NOVEL-AUDIT-migration-model.md).

## Tooling finding

Calibration exposed a [reproducible foreach iterator-scope bug](tooling/foreach-field/README.md). An iterator declared as a machine field can be written as a frame-local slot, causing a crash or incorrect storage access. Moving only the iterator declaration into the handler makes the same map iteration pass. General map iteration is not broken. No production checker changes were made; the pinned binary/image stayed unchanged.

## What this tells us

- The skill can support useful models and real observation tests in unfamiliar domains, but does not reliably outperform controls.
- The earlier lease success did not transfer into complete streaming tests: the retained advice emphasizes acceptance guards and stale state more than valid value classes and output partition boundaries.
- Oracle independence remains a practical weakness: separate source files do not prevent model/monitor co-adaptation.
- More prose is not the demonstrated answer. Keep these cold results as evidence before revising the skill again; distinguish future tuning from this transfer check.
- The isolated stages still have material gaps, so these tasks have not yet earned an end-to-end confirmation run.
