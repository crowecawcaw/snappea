# Driver-improvement eval

Given a correct P model, an independent monitor and a weak passing driver, ask an
agent to improve tests. Compare native driver-limit reports on/off with the same
binary, prompt, public inputs and budgets. No skill is installed.

The wrapper fixes `--no-mutation --allow-incomplete-coverage --timeout 2s --json`
for every check in both arms. Coverage output remains available. Both arms collect driver facts with identical
native arguments; the off arm removes only each test case's `driver` JSON field
from displayed output and requested JSON sidecars. Both arms use the same
serialization path. This is a report ablation, not native versus compat.
The coverage bypass is an explicit experiment setting; agents may not introduce
new suppressions or disable additional checks. The shell bridge retains all
commands, including attempted configuration overrides.

## Run

Use a fresh collaboration agent with the configured model/thinking settings and
pass the generated `prompt.txt` verbatim. All solver file/command access must go
through `bridge.py`.

```sh
python evals/driver-improvement/runner.py start --run-id sol-priority-on-01 \
  --fixture priority --reports on --model gpt-6.1-sol --reasoning-effort medium
cat evals/driver-improvement/runs/sol-priority-on-01/prompt.txt
# After the agent returns:
python evals/driver-improvement/runner.py finish --run-id sol-priority-on-01
python evals/driver-improvement/grade.py \
  --run-root evals/driver-improvement/runs/sol-priority-on-01
```

Repeat with `--reports off`, and with the other fixtures. Budgets are 480 seconds
from first bridge call, 40 calls, and 30 seconds per command. Containers are
networkless, resource-limited and use read-only tools. Only the work directory is
writable. The native checker at `/opt/checker-native` is experiment plumbing and
must not be invoked directly by solvers.

## Score and review

Freeze submissions before hidden grading. Grade only submitted `driver.p` and
`tests.p`, restoring original events, model and monitor for every variant. Load
variants and grader code from each run's archived inputs, independently of later
live changes. Correct variants include an unchanged algorithm and an alternative
valid representation. A valid submission must accept both. Correct-variant search completion is
reported separately; scores with incomplete correct searches are explicitly
qualified, not treated as proved acceptance. Baseline variants are rerun with
the original public driver, so new detections and lost regression coverage are
measured from actual paired checks. Hidden faults receive
credit only for actual assertion failures; compile errors, lint, hot-monitor
completion failures, timeouts and missing tests earn none.

The policy report records changed immutable inputs, missing drivers, extra P
sources, added suppressions and assumptions. Native lint and an explicit required
monitor-selection check prevent bypassing the oracle. Source/transcript review
separately checks original assertion preservation, direct backend calls, vacuous
workloads, ignored instructions, and which changes followed report/coverage
feedback. Passing a bounded correct-variant check is not a proof.

Fixture private directories contain a fault inventory selected from the contract,
variant builders, stronger reference drivers and calibration results. Run
calibration before provisioning; do not tune a frozen cohort from hidden results.

```sh
PYTHONPATH=evals/driver-improvement python -m unittest test_harness -v
```

The common calibration exercises real Docker report-on/off runs, verifies equal
native arguments and equal non-driver JSON fields (apart from timing), checks
sidecar filtering, then grades every strong reference driver and original
baseline against every variant:

```sh
python evals/driver-improvement/calibrate.py
```

Per-command protected-file hashes are recorded only in the full transcript,
without warning/nudging the agent. This catches temporary immutable-input edits
across calls, even if restored before submission. Requested/effective checker
arguments are also retained in `.check-invocations.jsonl`; instruction attempts
need human review rather than keyword-based automatic verdicts.
