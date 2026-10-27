# Luna/high: Snappea versus compat, no skill

**Snappea produced more useful validation artifacts; neither CLI reliably led to
robust design models. The driver report sometimes preceded useful changes, but
did not reliably produce broader scenarios.**

Eight fresh `gpt-6-luna` / high agents: two per task per CLI, no modeling skill in
either arm. Latest main `a07b926` was merged before building both binaries. Tasks,
budgets, image, observation backend and grading were matched under the
[pre-run protocol](LUNA-CLI-PROTOCOL.md). This compares the full CLI experience,
including its defaults, diagnostics, lint, coverage and mutation feedback; it does
not isolate the effect of driver reports.

- [FIFO records](luna-cli-flow.json), [VCS records](luna-cli-vcs.json).
- [Source audit](LUNA-CLI-AUDIT-source.md), [transcript/feedback audit](LUNA-CLI-AUDIT-feedback.md), [reviewed source hashes](luna-cli-source-hashes.json).

## Results

| Measurement | Snappea 01 | Snappea 02 | Compat 01 | Compat 02 |
|---|---:|---:|---:|---:|
| FIFO hidden implementation cases | 8/8 | 8/8 | 8/8 | 8/8 |
| FIFO independent monitor histories | 8/8 | 8/8 | 2/8 | 8/8 |
| FIFO saved Python tests, all pass | 5 | 3 | 4 | 2 |
| FIFO real Python histories checked against P | Yes | Yes | No | No |
| FIFO independent native model check | Pass, complete | Incomplete | No selected test | Coverage failure |
| VCS hidden implementation cases, 64 seeds | 20/20 | 19/20 | 20/20 | 18/20 |
| VCS existing wide-payload probe | Fail | Fail | Fail | Fail |
| VCS saved additional Python tests | 0 | 0 | 0 | 0 |
| VCS independent native model check | Pass, complete | Pass, complete | Coverage failure, inert default | No P files |

For VCS, the two scored compat runs have IDs `compat-02` and `compat-03`.
`compat-01` is a separately retained infrastructure failure, excluded from this
table. FIFO uses the normal `01`/`02` suffixes.

Independent model checks use the same native checker in both arms, with a
five-second search and five-second mutation budget. The table preserves those
outcomes separately from the agents' own checks. An incomplete search is not a
demonstrated Python correctness failure. No-score monitor compile/setup outcomes
are not counted as successful negative tests.

## What improved with Snappea?

- Both native FIFO runs save actual serial implementation histories and run them
  through `snappea observe` with their independent monitor. Neither compat run
  checks its captured histories against P; ordinary Python assertions still run.
  These are direct CLI observation checks, not `pysnappea` helper use.
- Native VCS runs retain explicit, executable tests. Compat VCS 01 checks an
  implicit default whose Main receives no operations: millions of explored
  schedules have one timeline and never reach the relevant handlers/assertions.
  Compat VCS 02 deletes its P files after repeated declaration/module failures.
- Native coverage feedback produces some concrete scenario repairs. Compat can
  report zero bugs for unused monitors or inert drivers; common native grading
  exposes those gaps. Native feedback can also be bypassed or answered by
  removing scope, so the CLI does not guarantee a good result.

Both native FIFO monitors pass all eight independent histories, compared with one
of two compat monitors. However, native FIFO 02 suppresses `unselected-spec` and
uses that monitor only in Python observations. Only native FIFO 01 both selects
the design monitor and passes the independent complete-search gate. Its populated
snapshot model behavior is still missing, with monitor coverage suppressions.

## Did agents act on the driver report?

- **VCS native 01:** a check shows both missing post-publication branches and
  driver-order limits. The next model edit adds approval and CI after publication,
  fixing coverage and removing those order limits. This is useful uptake of mixed
  feedback, not evidence that the new report alone caused it.
- **FIFO native 01:** after a passing report describes fixed Main operations,
  the agent activates previously inert client senders. It then reduces the driver
  after incomplete searches. This is real driver rework after report delivery;
  its motive cannot be established from command logs alone.
- **VCS native 02:** coverage and constructor-value facts expose unexercised
  retry/success paths. The agent removes the corresponding model branch and
  assertion instead of expanding scenarios.
- **FIFO native 02:** expands clients after an assertion failure, before its first
  driver report. No later expansion can be credited to that report.

Final drivers still use fixed operation scripts and bounded fixed values. None of
the eight generates operation choices. Reports make limitations visible, but the
agents frequently stop once checks pass or weaken the modeled scope.

## Remaining design failures

- All FIFO models process whole queue commands atomically. None represents the
  implementation's separate load, failed CAS and recomputation steps. All lack a
  complete operation-progress/observation obligation; silent missing events can
  escape a safety-only monitor.
- Native VCS 02 replays an already-published bundle's updates on a fresh publish
  request, rolling heads back after another bundle advances them. Existing hidden
  `test_published_bundle_cannot_reapply` catches it. The passing P model omits that
  lifecycle.
- Compat VCS 02 decides from a snapshot, then obtains a newer root version for
  commit without revalidating. Two existing concurrent-history tests fail.
  No model survives in its frozen submission.
- Every VCS implementation keeps an entire updates map in one event/tree record.
  All fail the existing 64-branch, 27,565-byte wide-payload probe with the store's
  16,384-byte record limit. This is a separate existing supplemental probe, not
  included in the primary 20-case score.

The two compatible portable FIFO test swaps each reject two of three seeded
implementation faults and miss the ignored-CAS-failure variant. The other two
swaps reject the reference implementation and are therefore not scored for fault
sensitivity. Threaded capacity tests without a forced CAS race do not establish
that retry/recompute errors are tested.

This small cohort supports a specific conclusion: Snappea helps agents notice
and validate more of what they wrote, especially connecting Python histories to
P. It does not yet reliably steer them to model the actual algorithm, generate
broader histories, or preserve all contract requirements. The primary scores do
not establish a reliable implementation-correctness benefit.

## Reproduction and deviations

Use the runners' `--condition baseline --tool snappea|snappea-compat --model
gpt-6-luna --reasoning-effort high`. Phase runner: `--phase flow --binary
target/driver-eval/release/snappea`. VCS runner: `--arm p`, with
`SNAPPEA_EVAL_BINARY` pointing to that native binary and `SNAPPEA_EVAL_SOURCE`
pointing to the checkout. Use the pinned image below for both VCS image variables.
Delegate each generated prompt verbatim to a fresh agent; freeze before grading.

- Native SHA-256: `b7ea15b06ee99225277adaeeee4dda33b2d2e6a6484b22ec3c13d9973d051fb2`.
- Compat SHA-256: `f969f66ae7dc03a39823113fff33c0a94e4da61918253be173070d73024f8092`.
- Image: `sha256:00dbf3678a1e6fc660c1cc87fe05f9a8d2cdaeda2daa73fedd9941bb55ce5431`.
- Budgets: 900 seconds/80 calls; FIFO command cap 90 seconds, VCS 120 seconds.
- Only the selected CLI is on PATH. The same native backend at
  `/opt/python-snappea` supports the Python library; prompts forbid invoking it
  directly. Audits find no direct backend calls. No skill is mounted.
- FIFO TASK changes only its example command to the selected CLI's accepted
  syntax. Original and adapted public hashes are preserved. VCS public/private
  fixtures match exactly across all scored runs.
- VCS compat 01 read the executable as text. Invalid UTF-8 caused the bridge to
  freeze after three calls. It is retained as `infrastructure_error`; a fresh
  agent replaces it. The bridge now decodes with replacement characters, with a
  regression test. The live bridge fix applies to subsequent VCS calls; archived
  original harness files are preserved per run. No checker, task, skill or grader
  tuning was applied in response to scored outcomes; solvers received no hidden
  feedback.

Validation: all 33 CLI tests, formatting, the phase protocol tests, snapshot tests,
tool-variant tests and FIFO reference/fault/monitor calibration pass. No new
auditor-authored behavioral probes were added. Raw trials, exact binaries,
prompts, commands, original inputs, grades and reviews are preserved in
`/workspace/library-files/snappea-luna-cli-evals-evidence.tar.gz`.
