---
diataxis: how-to
personas: ["application-developer",  "coding-agent"]
questions:
  - "How do I check services in containers live with snappea observe?"
  - "How does a supervisor stop the system when observation fails?"
---

# Two inventory workers in Docker, one live observer

Run two long-lived inventory workers in separate containers. Both modify the
same SQLite inventory database and emit their committed application events. A
host-side `snappea observe` reads each container's events, the supervisor checks
its exit status, and removes both containers when observation fails. This is a
small example of checking services while an external test driver runs them.

Requires Docker Engine running with access to bind mounts, Python 3.9+, and a
Rust toolchain to build the checker. The workers use only Python's standard
library; the image build pulls `python:3.13-slim` and installs no packages.
After the image is built, both containers run with networking disabled.

From the repository root:

```sh
cargo build -p snappea --bin snappea --locked
export SNAPPEA_BIN="$PWD/target/debug/snappea"
docker build -f examples/multiprocess/docker/Dockerfile \
  -t snappea-inventory-example:local examples/multiprocess
python examples/multiprocess/docker/run.py
```

The default healthy scenario exits 0. Every invocation creates a fresh database
and artifacts directory, printed at the end. To choose a location, pass
`--artifacts /path/to/a/new/directory`; that final directory must not exist.
Use `--snappea /path/to/snappea` or `SNAPPEA_BIN` to select the binary; the fallback is `snappea`
on `PATH`. `--image` selects a different prebuilt worker image.

Run the actual overselling defect:

```sh
python examples/multiprocess/docker/run.py --scenario oversell
# Expected shell exit status: 1. Both containers have been removed.
```

`--scenario oversell` enables the worker's clearly marked `--allow-oversell`
business-rule fault: it skips the stock-availability guard while retaining the
same database updates, instrumentation, and spec. Stock starts at 3; worker A
reserves 2 and worker B incorrectly reserves another 2, committing stock -1.
These are real database operations, with no fake violation events. P reports
`reservation exceeds available stock` at worker B's reservation.

To run the integration test, including asserting the expected failure:

```sh
python -m unittest discover -s examples/multiprocess/docker -p test_example.py -v
```

The test passes only if the healthy command exits 0 and the overselling command
exits 1 with the exact structured spec violation, both workers emitted events,
both containers were still running at detection, and both were removed afterward.
An unexpected spec failure or setup error fails the test. The
[CI workflow](../../../.github/workflows/multiprocess-docker.yml) builds the
image, runs these real containers, and uploads all reports even if a test fails.

## Setup and lifetime

Read [run.py](run.py), the shared [harness](../common/harness.py), then the
[worker](../common/worker.py) and [application](../common/inventory.py).

The supervisor initializes a fresh database before starting observation. It
starts the observer **before any application setup that emits events**, then
starts both containers with a shared directory mounted at `/data`. Each worker
sends an explicit readiness acknowledgment; the supervisor then issues business
commands and waits for their acknowledgments. Queue readers and deadlines bound
all waits. There are no startup sleeps or wall-clock ordering assumptions.

The healthy sequence is: A stocks 3, A reserves 2, B's request for 2 is rejected,
A releases, B reserves 2, and B releases. Rejected operations change no state
and emit no event. Each worker therefore emits successful operations. Workers
stay alive waiting for their next command between operations.

Each container writes its events to its stderr, which `docker run` copies into
a pipe the observer reads. Docker forwards only a container's stdin, stdout and
stderr, and a named pipe in a bind mount does not cross into the VM that Docker
Desktop and similar tools run containers in, so stderr is the portable channel.
Anything else a container writes there, such as a crash traceback, reaches the
observer and fails the run with a parser error showing that line.

For the healthy run, the supervisor stops and reaps the workers. Their exits end
the pipes, and the observer completes the history at EOF and writes the final
report. For the faulty run, it leaves both workers waiting for commands.
A live observer stops at the first violation, so it returns on the violating event **before EOF**. The supervisor confirms both actual
containers are running, then forcibly removes them and reaps their launchers.
Every exit path uses the same cleanup and verifies containers are absent; killing
only a `docker run` client is insufficient to remove a container.

The example invokes the equivalent of:

```sh
snappea observe examples/multiprocess/specs/inventory.p \
  --spec InventoryCorrect --live /dev/fd/5 --live /dev/fd/7 --sequence seq \
  --strict --format text --record events.jsonl --json-report report.json
```

`--strict` fails a completed stream in which no event was relevant to the spec.

The native `snappea` process and JSON report both use exit code 1 for a spec
violation. The report additionally includes
`input_complete: false` and `stop_reason: "error"`. A healthy EOF reports 0,
`input_complete: true`, and `stop_reason: "eof"`. A real supervisor must propagate
failure to its test runner, as `run.py` does. Catching a violation in the test
suite is only appropriate for this explicitly expected-defect case.

## Event order and integration boundary

Each operation runs under SQLite `BEGIN IMMEDIATE`. Business state and its
outbox row commit atomically. Workers publish the row only after commit. The
outbox's monotonically increasing sequence is assigned inside the transaction;
with `--sequence seq` the observer checks events in that order, whichever
container's pipe delivers them first, and fails on a duplicate or a gap. The sequence also serves as a logical
event timestamp. Per-process clocks and pipe-arrival order cannot establish a
shared history. In this example the driver deliberately coordinates operations
to exercise a reproducible cross-worker history; it is not a concurrent schedule
explorer.

Both workers share the `coffee` partition key, so P checks one inventory history
across process boundaries. Using a different key per process would hide the
oversell. The same database supports independent product keys and reservations
scoped by product and order. The spec reconstructs inventory from committed
stock/reserve/release events without accessing the database; the supervisor also
checks the persisted stock as an ordinary application assertion.

For your project, replace the worker commands with real requests and inject the
event sink at the transaction boundary. Start observing before seeding relevant
state; keep the observer alive until producers stop; fail the enclosing test if
the observer reports an issue. Replace this single-database sequence with an
ordering guarantee appropriate to your system. This small publisher emits only
newly committed rows: it does not implement crash recovery, outbox retries, or
deduplication. A missing sequence, worker crash, malformed event, setup failure,
or expired deadline fails the run and triggers cleanup.

## Reports and observed P coverage

`run.py` prints the observer's human-readable report and coverage to stdout,
then a concise JSON result. The artifacts directory retains:

| File | Contents |
| --- | --- |
| `events.jsonl` | Committed events in the order the observer checked them (`--record`) |
| `worker-a.stdout`, `worker-b.stdout` | Each worker's replies (`ready`, `ack`) |
| `observer.stdout`, `observer.stderr` | Full checker output |
| `report.json` | Structured violations and observed monitor coverage |
| `result.json` | Supervisor result, exit statuses, emitters, and cleanup evidence |
| `inventory.sqlite` | Application state and transactional outbox |

The healthy run reports 12/12 statements, 0/0 branches, and 3/3 event handlers.
Handlers starting with `assert false` reject invalid histories and are excluded
because they must never run. The faulty run reports a checked prefix with 8/12
statements and 2/3 handlers. `report.json` contains coverage points and the points
that ran under `specs[0].coverage`, allowing CI
to inspect what ran and what did not. This measures the P monitor exercised by
the actual history; it does not measure application code coverage or all possible
P paths, schedules, or distributed failures.
