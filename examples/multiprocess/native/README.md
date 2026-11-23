# Observe two local inventory processes

This integration test starts two independent Python application processes with
`subprocess.Popen`, sharing a SQLite inventory database, and a third process
running `snappea observe`. Both applications emit events from committed inventory
transactions, each into a pipe of its own that the observer reads. The parent
drives the workers with commands and owns every process's lifetime. Python's standard library is the only application
dependency; no Docker daemon or Python `snappea` package is required.

The application, worker protocol, P specification, and observer supervisor are
shared with the [Docker example](../docker/README.md). The local adapter is
[`SubprocessBackend`](run.py); switching launch method does not change the
domain rules or expected failure.

## Run from a checkout

From the repository root:

```sh
cargo build -p snappea --bin snappea --locked
export SNAPPEA_BIN="$PWD/target/debug/snappea"
python -m unittest discover -s examples/multiprocess/native -v
```

The two integration tests launch actual workers and validate these histories:

* **Healthy:** worker A stocks three units and reserves two. Worker B's attempt
  to reserve two is rejected. After A releases its reservation, B reserves and
  releases successfully. Both workers stop, which closes their event pipes, and
  the observer exits successfully at EOF.
* **Oversell:** the service's stock guard is disabled. After A reserves two,
  B incorrectly reserves another two. The P monitor fails with
  `reservation exceeds available stock`. The supervisor verifies the observer
  exited with both workers still alive and their event pipes still open, then
  terminates and reaps both workers. A parser error, missing event, timeout, or different
  assertion cannot satisfy this test.

Run either as a standalone command as well:

```sh
python examples/multiprocess/native/run.py --scenario healthy
python examples/multiprocess/native/run.py --scenario oversell
```

The healthy command exits **0**. The oversell command exits **1**, preserving
the observer's failure for a test runner in another process. The unittest suite
expects that specific failure, so the complete suite exits 0. Both commands
print the observer's human-readable diagnostics and coverage, followed by a JSON
summary and artifact location. Use `--snappea /absolute/path/to/snappea` to select the
binary explicitly or `--artifacts /path/to/new-directory` to choose output.
Each default run uses a fresh directory under `native/artifacts`; existing
output directories are never overwritten.

## Setup to reuse in a project

[`common/harness.py`](../common/harness.py) owns the testing boundary. It starts
the observer before the applications, waits for ready messages, and drives
operations using command acknowledgements. Queue reads, worker waits, and
shutdown have timeouts; readiness and observer failure require no sleeps.
Cleanup runs in `finally` even when startup or checking fails. The adapter
terminates each still-running worker, escalates to kill after five seconds, and
waits to reap it. These workers do not launch child processes.

Each worker gets two channels. Its stdin and stdout carry commands and replies
(`ready`, `ack`). Committed events go to `--events`, the write end of a pipe the
harness creates; the observer reads every pipe:

```sh
snappea observe specs/inventory.p --spec InventoryCorrect \
  --live /dev/fd/5 --live /dev/fd/7 --sequence seq \
  --strict --format text --record events.jsonl --json-report report.json
```

With `--live`, the observer stops at the first violation. `--strict` fails a completed stream in which no event was relevant to the spec.

To integrate your own service, replace the launch command in `SubprocessBackend`
and the commands/readiness adapter in the supervisor. Give each process its own
event stream (a pipe, a named pipe from `mkfifo`, or stdin for a single one).
Keep the observer's exit watcher and process ownership in the outer test
fixture: stop the producers on an observer failure, and let their exits end the
streams otherwise. The observer checks events; the supervisor cancels the
producers.

The service accepts an event sink, keeping transport out of its business logic.
[`common/inventory.py`](../common/inventory.py) writes each event to an outbox
in the same SQLite transaction as the inventory change and publishes only after
commit. The outbox sequence number (`seq`) captures commit order across workers.
Two pipes can deliver committed events in a different order, so `--sequence seq`
has the observer check event *n* as soon as events 1 to *n*-1 have been checked,
whichever pipe they came from. A duplicate number, or a gap still open when every
pipe has ended, fails the run. Rejected operations emit no state-change event.
The monitor checks the shared product key, so it can detect a violation caused
by actions from different processes.

The sequence also supplies a logical timestamp for this safety property. If your
monitor checks elapsed time, record real timestamps and define an ordering policy
appropriate to your application. This small publisher does not retry deliveries;
a production outbox dispatcher also needs delivery tracking and a deduplication
policy.

## Reports and CI

Each scenario preserves:

| Artifact | Purpose |
| --- | --- |
| `observer.stdout` / `observer.stderr` | Human-readable checker diagnostics and monitor coverage |
| `report.json` | Structured checker result, errors, completion status, and coverage |
| `result.json` | Validated scenario summary, final stock, both emitters, cleanup, and whether workers were alive at failure |
| `events.jsonl` | The events in the order the observer checked them (`--record`); `src` names the worker |
| `inventory.sqlite` | Committed service state and outbox |
| `worker-*.stdout`, `worker-*.stderr` | Worker replies and diagnostics |

`report.json` includes each spec's statement, branch, and event-handler coverage,
plus portable coverage points and the points that ran. Healthy runs cover 12/12
statements and 3/3 handlers; there are no conditional branches. Handlers starting
with `assert false` are excluded because they must never run. Faulty runs cover
8/12 statements and 2/3 handlers in the consumed prefix, with
`input_complete: false` and `stop_reason: "error"`. This measures executed P
monitor code rather than exhaustive P model paths. See [the coverage format](../../../docs/observe.md#coverage).

The [native workflow](../../../.github/workflows/native-multiprocess.yml) builds
this checkout's checker, executes both scenarios, and uploads reports even on a
test failure. `MULTIPROCESS_ARTIFACTS` can set the test suite's artifact parent.
