# Feedback revision results

**Test quality improved modestly; generated model histories did not improve.**
Nine fresh Luna/high trials used the revised skill and updated checker. All six
test suites passed the correct implementation(s), saved real observation tests,
and completed without setup errors or timeouts. Neither skill model adopted
bounded input-history generation or manually calculated monitor probes, despite
reading the new example's P source. These small adaptive pilots do not establish
a general skill advantage.

- [Protocol and changes](FEEDBACK.md)
- [Machine-readable results](feedback-pilot.json)
- [Test source/transcript audit](FEEDBACK-AUDIT-tests.md)
- [Model source/transcript audit](FEEDBACK-AUDIT-models.md)

## Adding tests

| Task | Skill 01 | Skill 02 | No-skill control | Correct variants accepted |
|---|---:|---:|---:|---|
| Streaming | **7/8 faults** | **7/8 faults** | 6/8 faults | All: 1/1 |
| Priority queue holdout | **8/8 faults** | 7/8 faults | 7/8 faults | All: 2/2 |

The earlier streaming skill suites caught 6/8 and 5/8; its earlier control caught
5/8. That comparison is descriptive: both checker feedback and skill content
changed. The new matched control also improved. Do not pool revisions or claim
the extra detections were caused by one particular edit or by P alone; ordinary
Python assertions contribute to these whole-suite scores.

Concrete changes and remaining gaps:

- Both new streaming skill suites include negative and zero values. The control
  still uses positive values only. All record actual calls/results in order and
  close each returned output list with `eEndCall`.
- Streaming skill 01 still misses nonzero exact bucket boundaries. It tests
  timestamp 0, but that does not discriminate the off-by-one partition fault at
  timestamps 10, 20, etc. Skill 02 tests those boundaries but misses a window's
  first arrival after its end. The control misses first-late arrival and signed
  aggregation.
- Queue skill 01 catches every seeded fault, including changing an item's
  priority/order after a rejected duplicate, and re-admission after cancellation.
- Queue skill 02 names a fresh-admission test but cancels an item already behind
  its peer. Both retained and fresh order return the same item first, so the
  wrong-order fault survives.
- The queue control names a duplicate-no-effect test but gives an already-first
  item an even higher priority. The faulty update does not change its asserted
  pop order. A duplicate on a later item that would move it ahead is a stronger
  probe.
- The holdout was authored without consulting the skill; it has no matching
  skill example. Its public task explicitly requires strict checking in both
  conditions. The permitted correct alternative changes only a private counter
  offset; it tests representation tolerance, not all valid implementations.

These tests are useful, readable suites. The remaining failures are specific
observational gaps, rather than fabricated event histories or an inability to
use the Python helpers. A scenario's name is not evidence that it distinguishes
the intended defect.

## Making models

| Run | Native command | Valid histories accepted | Invalid histories rejected |
|---|---|---:|---:|
| Skill 01 | Pass; complete search, required coverage met | **6/6** | 8/9 |
| Skill 02 | Coverage failure: 6 required points missing; search complete | 5/6 | **9/9** |
| Control | Pass; complete search, **monitor not selected** | 5/6 | 8/9 |

All three implement stateful aggregation rather than replaying precomputed
outputs. The two skill submissions attach their monitors correctly. The control
never resolves test-selection syntax: `tests.p` lists machines without
`assert WindowContract`, and `monitor.p` suppresses the unselected-spec lint.
Its successful native command does not validate the service against that monitor;
the monitor-history column is a separate independent observation check.

- Skill 01 has the strongest fixed workload: it includes late-first arrival,
  negative/zero values, correction and finalization. It silently clamps backward
  watermark observations rather than rejecting that invalid history. This was
  present in the monitor's initial construction, not an assertion later removed
  to pass coverage.
- Skill 02's service and monitor both omit immediate output for late-first
  arrival into an absent window. They agree with each other but reject a valid
  external history. Source separation does not establish oracle independence.
- Both skill agents read the compact reference and bounded-history P example.
  Neither uses `$` or `choose` in the submitted model; each caller executes one
  predetermined call order. Neither transcript records manual valid/invalid
  monitor probes. Completed searches cover four schedules each, not alternative
  workloads. The control likewise uses fixed calls (73 internal schedules).
- The clearer failure cause was used accurately by skill 02: it reported the
  six missing coverage points separately from search completion, then documented
  its exploratory coverage bypass. No mutation-cap explanation was substituted
  for the coverage failure. This is one positive workflow observation, not an
  isolated causal test of output formatting.
- Syntax friction remains: reserved identifiers, tuple commas, and test selection
  still needed repairs. Executable reference validation prevents stale examples,
  but does not guarantee agents apply every rule they read.

## Interpretation

Keep the general valid-value guidance, executable compact reference, and clearer
failure summary. The saved tests show useful uptake, with residual gaps that the
seeded faults expose. The generator example alone did not induce generated
workloads, and manual oracle probes were also skipped. These capabilities remain
unreliable; an end-to-end confirmation would be premature.

The next focused experiment should test whether a more direct, shorter skill
instruction to generate bounded operation/argument choices changes the actual
submitted drivers. Judge adoption from source, not schedule counts. Keep oracle
calibration separate: validate some contract-calculated histories independently
of the service/monitor pair. No further skill tuning occurred during this cohort.

## Reproduction and validation

- Skill tree: `75cb6bc852fb31b9d88dc026d12300f26dd307b5d32f62dae09fa31b64beaaae`.
- Checker: `2ea61378c21ff899f3977eaefc505753b566f2fa3a114d5288c4a8a60d864194`, built separately under `target/feedback`.
- Offline image: `sha256:00dbf3678a1e6fc660c1cc87fe05f9a8d2cdaeda2daa73fedd9941bb55ce5431`.
- Existing bridge limits: 600 seconds / 50 commands / 90 seconds per command;
  one skill sentence; frozen public/private/harness inputs and submissions.
- Streaming calibration: reference monitor accepts 6/6 valid, rejects 9/9
  invalid; reference tests accept 1 correct implementation and reject 8 faults.
- Priority calibration: reference tests accept 2 correct implementations and
  reject 8 faults. Standalone calibration also checks hand-authored positive and
  negative monitor histories.
- Skill validation passes: executable P reference, all correct/broken examples,
  manual oracle controls, real Python observation controls, and links.
- Workspace tests, workspace build, Clippy with warnings denied, formatting, and
  the emit-only build pass. Production runtime and compatibility paths are
  unchanged; the new summary is specific to the native Snappea CLI.

Raw evidence bundle: `snappea-feedback-evals-evidence.tar.gz`, with a SHA-256
sidecar and per-file manifest under `/workspace/library-files/`. It contains all
nine frozen trials, the skill snapshot, command transcripts, graders, outcomes,
calibration reports, source audits, and workspace-test/Clippy logs. Workdir
copies, caches, and generated checker output are excluded. Every archived file
was verified against its manifest.
