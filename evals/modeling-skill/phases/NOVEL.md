# Transfer checks: schema migration and streaming windows

Two new scenarios, separate from the installed skill's examples. Skill content stayed unchanged throughout these runs (`7b68681ecb4626249add3e8ed15f424dee9dd083d509fb9e0e595a2340af0c0d`). No solver saw reference implementations, hidden histories, faults, or other submissions.

## Tasks

| Scenario | Model-only task | Adding-tests task | Distinct design risks |
|---|---|---|---|
| Online schema migration | [Contract/events](migration/model/public/TASK.md) | [Fixed Python service and monitor](migration/testing/public/TASK.md) | Prepared backfill versus live mutation; tombstones; transformed representations; readiness and cutover |
| Streaming window aggregation | [Contract/events](streaming/model/public/TASK.md) | [Fixed Python service and monitor](streaming/testing/public/TASK.md) | Out-of-order event time; late first arrival/corrections; finalization; per-window revisions; complete ordered output batches |

- Fresh Luna/high agents, 600 seconds/50 calls, 90-second command cap.
- Model phase: two skill agents and one no-skill control per initial task. Streaming then had a corrected-task pair; see below.
- Test phase: two skill agents and one control per scenario.
- Same task/checker/image/budget within each comparison; skill condition adds only the skill sentence and read-only files.
- Record native search completion, independent monitor probes, test fault sensitivity, and source audit separately. Passing native checks alone is not a design score.
- [Results](NOVEL_RESULTS.md); [raw summary](novel-pilot.json). Original and corrected grading remain separately labeled.

## Reproduce a new trial

The [shared protocol](README.md) applies. Phase names:

```sh
python evals/modeling-skill/phases/runner.py start --phase migration/model --run-id migration-model-new
python evals/modeling-skill/phases/runner.py start --phase migration/testing --run-id migration-tests-new
python evals/modeling-skill/phases/runner.py start --phase streaming/model --run-id streaming-model-new
python evals/modeling-skill/phases/runner.py start --phase streaming/testing --run-id streaming-tests-new
```

- Delegate each saved `runs/<id>/prompt.txt` to a fresh Luna/high agent with `fork_turns=none`.
- Use `--condition baseline` for control; the checker remains installed.
- Finish with `runner.py finish --run-id <id>`, then `grade.py --run-id <id>`.
- New trials archive grader fixtures **and harness source**, public inputs, skill hash, prompt, commands, submission hashes, and pinned image/checker.
- Current model grading first checks the monitor alone, then loads the submitted P project if shared types/helpers are required. Observation loads declarations; it does not execute model machines.

## Calibration

Run the private controls with:

```sh
python evals/modeling-skill/phases/calibrate_novel.py migration
python evals/modeling-skill/phases/calibrate_novel.py streaming
```

Reports go under ignored `runs/calibration-<scenario>-current/`. Model calibration validates reference monitors; the partial migration reference driver is reported separately from oracle success.

Before solver trials:

- Migration monitor: 7 valid histories accepted, 8 invalid rejected. The reference test suite accepts two valid implementations and rejects 9 faults. The illustrative reference P model does not meet the complete-coverage gate; it is not presented as a complete reference design.
- Streaming monitor: 6 valid histories accepted, 9 invalid rejected. Its fixed trace driver passes native checking; that calibrates monitor execution, not algorithm fidelity. The reference test suite accepts the correct implementation and rejects 8 faults, including accepted bucket boundaries, negative sums, late-first output, finalization thresholds, revisions, and output order.
- Reference suites are portable: tested after copying them beside public files in fresh offline containers. Original local-only path assumptions were fixed during calibration, before test-agent trials.
- The streaming testing monitor scans window starts through the watermark, so large watermarks increase checking cost. The model task explicitly bounds event timestamps to `[0,70)`; the testing service/monitor also handles later timestamps.
- Migration's iteration-free readiness oracle is exact under its contract: only initial rows `a` and `b` can be missing from the shadow; every later live mutation dual-writes arbitrary keys. This is an invariant, not a restriction to two service keys.

## Recorded corrections and tooling

- Streaming's first model task omitted the checker command. Its control looked for upstream P tools and incorrectly concluded no checker existed. Retain that cohort, then rerun a fresh skill/control pair with `snappea check ...` explicitly in the shared task. Do not pool the task revisions.
- Original model grading assumed all monitor declarations lived in `monitor.p`. A migration monitor legitimately shared its `Row` type from `model.p`. Corrected loading accepts this without changing the submission or hidden cases. Its original compile-failure grade remains intact; [recheck_novel_monitors.py](recheck_novel_monitors.py) records a labeled posthoc correction and preserves original native-check evidence.
- Calibration found a [foreach iterator-scope checker bug](tooling/foreach-field/README.md). A machine-field iterator can be miscompiled as a frame-local slot; a handler-local iterator works. General map iteration is not broken. No production checker or runtime behavior was changed.
