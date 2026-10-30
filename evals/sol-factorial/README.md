# Sol factorial completion

**Setup handoff; results are in progress.** At this handoff, seven new trials are frozen and one solver remains active; grading/audits/supplements are not all complete. No factorial outcome claim is made here.

See [HANDOFF.md](HANDOFF.md) for using the reusable task harnesses from a fresh checkout, and [protocol](PROTOCOL.md) for the exact pinned comparison. The exact cohort coordinator below requires ignored prior raw runs, skill snapshots and local binary/image artifacts; it is not a standalone fresh-clone entry point.

Eight missing-cell trials join the prior eight Sol runs;
CLI and skill vary separately, with an earlier/new execution-block caveat.

- `experiment.py start`: verify pins, provision/freeze all eight prompts and compare inputs with prior Sol runs. `runs/snapshots.json` maps `skill` to the exact immutable snapshot.
- Delegate each saved prompt to fresh Sol6.1 / medium with no inherited history; save final responses as `agent-final.txt`.
- `complete.py ID`: freeze submission, grade with unchanged harness, retain console and Docker events.
- `blind.py`: incrementally copy neutral source artifacts for assignment-withheld review. Do not disclose maps/results before reviewers finish.
- `analyze.py supplement`, `analyze.py summarize`, `uptake.py`: uniformly repair declaration assembly, run original supplemental checks, verify and report. Preserve prior cohorts unchanged.
