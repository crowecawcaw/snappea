# Checking logs with `snappea observe`

`snappea observe` checks events recorded by a running system against the spec monitors of
a P project. It replaces [PObserve](https://github.com/p-org/P/tree/master/Src/PObserve)
without jars: the P sources are type-checked in memory.

```
Usage: snappea observe [SOURCE...] [LOG...] --spec <NAME> [options]
```

A SOURCE is a `.pproj` or `.p` file. Without one, the `*.pproj` in the current folder
is used, or else the `*.p` files in it and its immediate subfolders, as for `snappea check`.
Every other argument is a LOG: a file, a folder of logs or a quoted glob pattern.

```sh
snappea observe --spec BankBalanceCorrect logs/                     # JSON lines
snappea observe --spec BankBalanceCorrect bug.trace.json --p-trace  # a P JSON trace
snappea observe PSpec/Bank.p app.log --spec BankBalanceCorrect --key 7 --key 9
snappea observe --spec BankBalanceCorrect a.log b.log "archive/**/*.log"
```

It prints one line per spec, and each error under it:

```
$ snappea observe inventory.p logs/ --spec InventoryCorrect
FAIL  InventoryCorrect (1 error; 2 events, 1 key)
      key c: Assertion failure: inventory.p:27:7 reservation exceeds available stock
      Coverage: 8/12 statements, 0/0 branches, 2/3 event handlers
Not covered by the events:
  inventory.p:30  InventoryCorrect.Ready, on eReleased: never handled
```

Each spec's coverage says which of its event handlers, statements and `if` outcomes
the events ran ([below](#coverage)).

Exit status: 0 no violation, 1 a violation (a spec assertion, or a hot state with
`--check-hot`), 2 any other error (command line, P type errors, log parser errors,
lint findings in the specs, unknown event names, `--strict`, I/O). Command-line errors go to stderr as
`error: <message>`.

Before reading logs, `snappea observe` lints the specs for handlers that can never
run: a handler for an event the spec's `observes` list leaves out
([`unobserved-handler`](linting.md#unobserved-handlers)). A finding fails with exit
status 2 unless a `// snappea-lint: allow unobserved-handler -- reason` comment
suppresses it. Model-test rules such as `unselected-spec` do not apply here.

On a terminal or in CI (`CI` set) the output is text; otherwise (agents, scripts,
supervisors) it is the JSON result below. `--format text|json` or `--json` chooses,
as does `SNAPPEA_FORMAT=text|json`, and `--json-report FILE` also writes the JSON to
`FILE`, for a run whose text people read.

`snappea-compat observe` takes PObserve's options instead (`-pf`, `-pp`, `-l A B ...`,
`-ik`, `-pc`, `--keys`, `--replaySize`, `-o`, ...), writes PObserve's console output
and report folder (`PObserve-<date>/` with `replayEvents_*.log` and
`PObserveMetrics.txt`) and uses PObserve's exit codes (0 ok, 1 command line, 2 spec
violation, 3 internal error, 4 parser errors only). It also takes the checks below
that PObserve doesn't have (`--all-specs`, `--check-hot`, `--strict`,
`--unknown-events`, `--json-report`, live input).

## Options

| Option | Effect |
| --- | --- |
| `--spec NAME` | The spec to check. Repeat it or separate names with commas to check several in one run. |
| `--all-specs` | Check every spec in the P sources (instead of `--spec`). |
| `--exclude-specs NAME` | A spec `--all-specs` leaves out; repeat or use commas. |
| `-l PATH` | A LOG, as an option (one value per `-l`). All logs, folders and patterns form one input set ([below](#several-logs-and-glob-patterns)). |
| `--live PATH` | Check a live stream (a pipe, named pipe or file; `-` is stdin) until it ends, stopping at the first error. Repeatable with `--sequence`. |
| `--stdin` | The same as `--live -`. |
| `--sequence FIELD` | Check live lines in the order of a global sequence number in each line. |
| `--record FILE` | Write the checked live lines to `FILE`, in check order. |
| `--p-trace` | The logs are P JSON traces, such as the `bug.trace.json` of `snappea check`. |
| `--parser NAME` | The log parser: `JsonLines` (default), `PTrace`, or a [custom one](#custom-parsers). |
| `--parser-config JSON` | Parser settings, e.g. `JsonLines` field paths ([below](#log-formats)). |
| `--key KEY` | Check only this partition key; repeat for several. |
| `--key-file FILE` | Check only the keys in `FILE`, one per line. |
| `--replay-window N` | Events kept per key to report with an error (default 100). |
| `--sorted` | The logs are already in timestamp order: merge them instead of sorting. |
| `--check-hot` | Also report a key whose spec is in a hot state after its last event, with the message `snappea check` uses for liveness bugs at the end of an execution. |
| `--strict` | Require at least one successfully checked relevant event per selected spec, after key filtering. |
| `--unknown-events error\|warn\|ignore` | What to do with event names no loaded P source declares (default `error`; `snappea-compat` keeps PObserve's `ignore`). See [Unknown event names](#unknown-event-names). |
| `--format text\|json`, `--json` | Print text or the JSON result (default: text on a terminal or in CI, else JSON). |
| `--json-report FILE` | Also write the JSON result to `FILE`. |
| `-v`, `--verbose` | Show progress, logged messages and each error's events. |

`snappea observe` writes no report folder; the JSON result is its machine-readable
output. Its `exit_code` is the exit status:

```json
{
  "exit_code": 1,
  "specs": [
    {
      "spec": "MutualExclusion",
      "events_read": 3,
      "partition_keys": 1,
      "verified_keys": 0,
      "errors": [
        {
          "kind": "spec_violation",
          "key": "L1",
          "message": "Assertion failure: specs/lock.p:19:7 c2 entered while c1 is inside",
          "cause": null,
          "events": [
            {"key": "L1", "timestamp": "2026-10-18T00:00:00.000000001Z", "event": "eEnter", "log_line": "{...}"},
            {"key": "L1", "timestamp": "2026-10-18T00:00:00.000000002Z", "event": "eEnter", "log_line": "{...}"}
          ]
        }
      ],
      "coverage": {
        "statements": {"covered": 3, "total": 4},
        "branches": {"covered": 0, "total": 0},
        "handlers": {"covered": 1, "total": 2},
        "uncovered": [
          {"file": "specs/lock.p", "line": 24, "end_line": 24, "context": "MutualExclusion.Locked, on eExit", "what": "never handled"}
        ],
        "source_fingerprint": "sha256:...",
        "points": ["..."],
        "hit": ["..."]
      }
    }
  ]
}
```

A run that fails before checking (bad sources, unreadable logs) prints
`{"exit_code": 2, "error": "..."}`. `snappea-compat observe --json-report` writes the
same result with PObserve's exit code (0 success, 2 violation, 3 validation, ordering,
internal or unknown-event error, 4 parser errors only).

`kind` is `spec_violation`, `out_of_order`, `parser`, `validation` (`--strict`),
`unknown_event` (`--unknown-events error`) or `unknown`. `events` are the
key's last events (`--replay-window`), the failing one last; parser errors have none,
their `message` is the log entry and `cause` says why it could not be read.

The [Python guide](https://crowecawcaw.github.io/snappea/docs/python.html) builds on these to check specs from pytest.

## Log formats

Parsers turn log entries into events. Two are built in:

* `JsonLines` (default): one JSON object per line,
  `{"key": "7", "timestamp": 1700000000000, "event": "eWithDraw", "payload": {...}}`.
  The timestamp is epoch milliseconds or ISO-8601. Payloads are matched against the
  event's payload type. `--parser-config '{"key": "ctx.account", "event": "type"}'`
  remaps the fields (dotted paths).
* `PTrace` (`--p-trace`): the `bug.trace.json` written by `snappea check`, checking
  its sends and announcements.

## Several logs and glob patterns

Each LOG argument (or `-l` value) is a file, a directory (its files, not its
subdirectories) or a glob pattern; `a.log more/` reads the same files as
`-l a.log -l more/`. All files form one input set,
combined as the files of one directory are: each key's events are sorted by
timestamp (equal timestamps keep file order), or merged in order with
`--sorted`. A file named more than once is read once.

snappea expands patterns itself, so quote them to keep the shell from doing it. This
also works in cmd and PowerShell, which don't expand patterns, and avoids
command-line length limits with large log sets:

```sh
snappea observe --spec BankBalanceCorrect "logs/**/*.log"
```

`*`, `?` and `[abc]` / `[a-z]` / `[!abc]` match within one path component, and `**`
matches any number of directories, including none. `*`, `?` and `**` skip names
starting with `.`. A matched directory reads its files, as a directory argument does. Matches
are sorted. A pattern that matches nothing is an error, like a missing path. A value
that exists as written is taken as given, so arguments the shell already expanded
read the same as a quoted pattern.

Rust callers use the same code: `snappea::checker::observe_inputs::read_logs(&StdFiles,
cwd, &paths, "\n")` returns the inputs `Spec::observe` takes, and `log_paths` returns
only the file list.

## Custom parsers

Other log formats need a parser written in Rust: implement
`snappea::checker::observe::Parser`, then check with `snappea::driver::observe`:
`compile` type-checks the P sources, `prepare(program, &config, parsers)` picks the
specs and the parser (your parsers are added to the built-in ones and selected by
`ObserveConfig::parser`), `Observer::run(&config, host)` checks the logs, and
`json_report` renders the [JSON report](#options). To run PObserve's command line with
your parsers, call `snappea::compat::observe::run_with(args, host, parsers)`;
`crates/snappea/examples/observe_regression.rs` does this with ports of upstream's
regression parsers.

## Time and latency budgets

Specs can declare the same global foreign time functions used by `snappea check`:

```p
fun Now(): int;
fun NowBest(): int;
fun Mark(): int;
fun Elapsed(m: int): int;
fun ElapsedBest(m: int): int;
```

Time tracking is enabled automatically by these declarations; signatures are checked
as in `snappea check`. During each observed event, `Now()` and `NowBest()` return its
log timestamp in **epoch microseconds**. ISO-8601 timestamps retain microsecond
precision (nanoseconds are rounded down); numeric timestamps remain epoch
milliseconds in the log format and are converted to microseconds. Values outside
the P `int` range saturate. Ordering still uses the full timestamp precision.

`Mark()` saves the current clock and returns a handle. `Elapsed(m)` and
`ElapsedBest(m)` return the exact difference from that mark in microseconds, so an
existing budget assertion such as `assert Elapsed(m) <= 2000` checks the recorded
duration. Each partition key has a fresh monitor and its own handles, starting at
zero. The spec's start entry action reads time zero; mark request starts in their
event handlers to measure request duration. State transitions and raised events
within a handler keep that observed event's clock.

`Elapse` and `ElapseCost` cannot be called from a monitor, as in `snappea check`.
Observation uses recorded timestamps and takes no latency profile or `--timed`
option. Logs without timestamps keep the parser's existing default of epoch zero;
provide timestamps to check meaningful budgets. `PTrace` currently has no time
entries, so its observations also use zero.

## Coverage

Observations also measure atomic condition outcomes in `if`/`while` decisions and
consecutive state-transition pairs within a single monitor instance. Merging reports
unions witnessed pairs; it never joins transitions from different keys or runs.
See [coverage types and limits](https://snappea.dev/docs/coverage.html).

Every observation reports, per spec, which of its `do` and `goto` event handlers,
statements and `if` outcomes (true and false) the events ran, as `snappea check` does
for a model. Code that starts with `assert false` must never run, so it is not
counted. The text output lists what never ran: for each piece of code, the outermost
part that did not run (a handler never triggered rather than each of its statements),
with its file and line; `--json` lists all of it under `coverage.uncovered`.

Code a spec never ran is a part of the property no event exercised. The application
may not emit an event (or emit it with a misspelled name: see
[unknown event names](#unknown-event-names)), the tests may not reach that situation,
or the spec may have code that cannot run.

`coverage.points` and `coverage.hit` hold what could run and what did, so results
merge without the P sources ([below](#combining-coverage-across-tests-and-processes)).
Rust callers read `Report.coverage`, a
`snappea::checker::code_coverage::CodeCoverage` with `summary()`, `uncovered()`,
`merge(&other)` and `to_json(points)`.

## Live checks for external tests

Use `--live` (or `--stdin`, the same as `--live -`) to check newline-delimited events
while a test or service is still running. Live input always stops at the first error:

```sh
producer | snappea observe inventory.p --spec Inventory \
  --stdin --check-hot --json-report observe.json
```

`--live PATH` reads a stream until it ends: stdin (`-`), a pipe such as
`/dev/fd/5`, a named pipe made with `mkfifo`, or a file (read to its current end,
so not one that a producer is still appending to). The producer must flush each
complete line. Unlike batch log observation, live observation does not sort:
timestamps must be nondecreasing **within each partition key** (equal timestamps
are allowed). Separate keys may interleave freely.

### Several producers

Give each producer its own stream and order them with `--sequence`:

```sh
snappea observe inventory.p --spec Inventory \
  --live a.events --live b.events --sequence seq \
  --strict --record events.jsonl --json-report observe.json
```

`--sequence FIELD` names a JSON field (a dotted path such as `meta.seq`) holding a
global sequence number, such as a database outbox assigns. Numbers start at 1 and
leave no gaps across all streams and keys. Event *n* is checked as soon as events 1
to *n*-1 have been, whichever stream they came from, so an idle producer that is
still running never delays checking the others. Every non-blank line needs a
number, including events the spec doesn't observe. The run fails, with JSON kind
`unknown` and exit code 2, on a line without a whole number, a duplicate
number, more than 10,000 lines waiting behind a missing number, or a missing
number when every stream has ended. Several `--live` inputs require `--sequence`;
ordering several streams by timestamp alone is not supported yet.

`--record FILE` writes the lines in the order they were checked, one per line,
when the observer finishes. It keeps a reproducible copy of a live run that
`snappea observe` can check again later as an ordinary log.

### Stopping

The observer finishes once **every** stream has ended (EOF), or at its first
parser, ordering, sequence, monitor initialization or spec error, even while
streams remain open. Every selected spec sees the current line before the observer
stops. The observer reads at most one line ahead of the one it is checking on each
stream. Replay storage retains only the last `--replay-window` events per key,
alongside each key's monitor state. The observer retains monitor state for every
encountered key until the run ends.

At EOF, `--check-hot` checks monitors for unfinished hot states. Hot states do not
fail a live, incomplete stream. An early failure reports only the consumed prefix;
JSON includes `input_complete: false` and `stop_reason: "error"`. At EOF these are
`true` and `"eof"`, including when the EOF `--check-hot` check fails. Coverage, when enabled,
describes the checked prefix.

Live input needs a newline-delimited parser (not `--p-trace`) and cannot be combined
with log arguments. When it stops early, `snappea observe` prints `Live input stopped at
the first error; the results cover the lines checked.` It exits with 1 for a spec
violation and 2 for parser, ordering, sequence or transport errors. In `snappea-compat
observe`, live input also needs `--fail-fast`.

For external tests, run the observer under a supervisor that starts the observer
before the producers, watches its exit status, terminates the producers on failure,
and otherwise lets their exits end the streams. The observer does not terminate the
producers itself. A shell pipeline alone cannot reliably supervise long-running
children. [examples/multiprocess](../examples/multiprocess/native/README.md) is a
complete supervisor for two worker processes, natively and in Docker.

Embedding applications can use `ObserveSession` to push one entry at a time and
inspect `failed()`, then `finish(eof, now)` to collect a report. Give each selected
spec its own session and feed all sessions before reading another line.
`snappea::checker::observe_inputs` has the pieces `--live` uses: `SequenceMerger` orders
lines by sequence number, and `observe_live` feeds a `LiveSource` of several streams
to the sessions.

## Combining coverage across tests and processes

Save each observation's JSON result, then merge them; nothing is re-executed and the
P sources are not needed:

```sh
snappea coverage merge-observe reports/*.json --json-report suite-coverage.json
```

```
Coverage (some observations failed):
  InventoryCorrect: 12/12 statements, 0/0 branches, 3/3 event handlers
```

It prints each spec's coverage and what no observation ran, or the merged JSON.
`snappea coverage merge` is the separate command for scenario coverage from
`snappea check`. Test helpers pass a JSON array of results once at suite end:

```sh
snappea coverage merge-observe --stdin --json-report suite-coverage.json < observations.json
```

- An empty array gives empty coverage. Merged results can be merged again.
- The merged JSON has `scope: "observed_monitor_suite"`, `schema_version: 1`, a
  `specs` array (each spec's merged coverage) and an `observations` array of distinct
  results without their coverage (exit code, errors, input completion when known).
  Failed and partial observations still contribute coverage.
- Results merge only for the same spec sources (a SHA-256 fingerprint of their
  contents, so different checkouts merge) and the same snappea version; otherwise the
  merge fails without changing the output.
- Rust: `snappea::checker::observe_report::ObserveCoverageReport` with `from_observation`
  (an in-process `Report`), `from_json` / `from_json_str`, `merge`, `render_text` and
  `to_json`.

## Strict observation

Use `--strict` in integration tests to catch accidentally disabled instrumentation,
empty recordings, or filters that discard all relevant events. Every selected spec
must successfully check at least one event. A spec with an existing parser, ordering,
or safety error keeps that error instead of receiving a secondary validation error.
A missing explicitly requested spec is already an error without strict mode.

Strict failures have JSON error kind `validation` and `snappea` exit code 2.
Live streams validate nonempty input only at EOF. An actual violation still stops
live input immediately, with prefix coverage and `input_complete: false`.
Strict mode does not imply `--check-hot`, check every key named by a filter, or
detect arbitrary missing events: use model assertions or explicit completion events
for stronger application-specific guarantees.

Strict mode does not catch a misspelled or undeclared event name: such events are
dropped before checking. Combine it with `--unknown-events error` for that.

Rust callers set `ObserveOptions.strict = true`. A strict `ObserveSession.finish(false,
now)` rejects an otherwise clean incomplete prefix. Supervisors finishing several
sessions after a known error may use `finish_after_failure(now)` to preserve the
original error without secondary validation failures; they must retain that original
error in the overall test result.

## Unknown event names

An event whose name no loaded P source declares is often a misspelled `emit`
(`eReserverd` for `eReserved`), which would hide the very bug the spec should catch.
`--unknown-events` decides what happens to them:

* `error` (default): report each distinct name once as an error of kind
  `unknown_event`, with its count, where it first appeared and a likely intended
  name. The exit status is the tool/input error (`snappea` 2, PObserve and
  `snappea-compat` 3), even alongside a spec violation, since the input cannot be trusted.
  Known events are still checked. With `--stdin` the run stops at the first unknown
  name.
* `warn`: print one line per distinct name to stderr, with the same details; the
  result and exit status are unchanged. The JSON report lists the names under
  `unknown_events` and the lines under `warnings`.
* `ignore`: drop them. Use it for logs that also hold events the spec does not
  model. `snappea-compat observe` defaults to `ignore`, as PObserve does.

```text
warning: Unknown event 'eReserverd': not declared in the P sources (1 occurrence, first at typo.jsonl:2); did you mean eReserved?
```

Events that a loaded P source declares but the selected spec does not observe are
always ignored: one recording can feed several specs, each watching a subset. To
accept names that only other tooling uses, declare them in a P file
(`event eAuditTrail;`) or use `warn`. `--strict` does not imply `--unknown-events`.

Locations are `<file>:<line>` (or `<file> entry <n>` for parsers whose entries span
lines), and `stdin:<line>` or `<live path>:<line>` for live input, counting each
stream's own lines. The built-in parsers report undeclared
names; a custom Rust `Parser` does so by overriding `parse_entry`. Rust callers set
`ObserveOptions.unknown_events` (with `input_names` for locations), for both
`Spec::observe` and `ObserveSession`.

## Events folder format

Test helpers and emitters exchange events through a folder: each process writes its own
file there, and `snappea observe <folder>` (or a helper) checks every file together. The
[language helpers](language-support.md) use it for events from child processes, and it
is the shape to use for any producer that writes events for later checking.

**The folder.** Named by the `SNAPPEA_EVENTS_DIR` environment variable. A recording creates
one folder and sets the variable for the processes it starts. Each writing process
appends to one file of its own, `<name>-<pid>-<random>.jsonl`, where `<name>` is the
program or service name when known (it shows up in failure reports) and `<random>` keeps
names unique across containers that share a volume and a pid. A writer never rewrites or
truncates a file, and never writes another process's file. Readers read every regular
file in the folder, in name order; they do not depend on the name.

**The line format.** One JSON object per line, ending in `\n`, as the `JsonLines`
parser above reads it:

```json
{"key": "L1", "timestamp": "2026-10-18T00:00:00.123456789Z", "event": "eEnter", "payload": {"client": "c1"}, "src": "app/locks.py:42", "seq": 7, "pid": 4120}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `event` | yes | The P event name. |
| `key` | yes | Partition key; each key gets its own spec instances. `""` is one shared stream. |
| `timestamp` | yes | ISO-8601 UTC with nanoseconds (preferred), or an integer of epoch **milliseconds**. |
| `payload` | no | Matched against the event's payload type: an object for a named tuple, a single value otherwise, or `null` / absent for none. |
| `src` | no | `file:line` of the emit call, shown in failure reports. |
| `seq` | no | Emission order within the writing process. |
| `pid` | no | The writing process. |
| `at` | no | `true` when the caller set the time (a logical clock), not the wall clock. |

Readers ignore other fields. A writer emits each line with a single append (`O_APPEND`
or equivalent) and flushes it, so concurrent writers in one process never interleave a
line and a crash loses at most the line being written.

**Ordering.** Batch observation sorts each key's events by timestamp. Equal timestamps
keep input order: files in name order, lines in file order. Helpers put the test
process's own events first. A wall-clock writer never lets its timestamps decrease
(keep the larger of now and the previous value). Processes on one machine share a
clock; across machines clock skew can reorder a key's events, so give each key one
writer, or pass a logical counter as the timestamp.

**When a file is complete.** Nothing in the folder marks completion. The test stops or
joins every process that writes to the folder before checking it; events written after
the check are lost. A truncated last line (not valid JSON), from a crashed or
still-running writer, is a `parser` error (exit 2), never dropped silently.

Undeclared event names in these files follow the same policy as any other
[input](#unknown-event-names). Several folders are checked together by naming each
([above](#several-logs-and-glob-patterns)); folders still being written are
checked with `--live`.

The emitters write this format in production too, to a file, stdout or a callback:
see [Testing and production](production.md).
