# Language helper requirements

Normative requirements for the language helpers that check application events against P
specs from tests: Python (`python/`), Go (`go/snappeatest`), Java (`java/`), Rust
(`snappea::test` in `crates/snappea`), and any future one. Every helper offers the same features, names,
defaults and failure behaviour, each written idiomatically for its language.

Reader: an agent implementing or reviewing a helper. MUST, SHOULD and MAY are as in
RFC 2119. Rationale for non-obvious choices is in [Decisions](#decisions). The current
state of each helper is in the [gap checklist](#gap-checklist).

## Naming

- Project name in prose: **Snap Pea**. Base package and identifier name: `snappea`.
- crates.io `snappea` (lib `snappea`); CLI `snappea` and `snappea-compat`.
- PyPI `pysnappea`, import `snappea` (`snappea` was taken on PyPI).
- Go module `github.com/crowecawcaw/snappea`; packages `snappeaemit`, `snappeatest`.
- Maven `io.github.crowecawcaw:snappea`; Java package `io.github.crowecawcaw.snappea`.
- Environment variable prefix `SNAPPEA_`.

## Layers

Each helper implements all five.

| Layer | Responsibility | Owned by |
| --- | --- | --- |
| L0 Core contract | Event line format, report JSON, exit codes, coverage JSON, compiling and checking | core (`snappea`) |
| L1 Emitter | `emit` from application code; no dependencies; inert outside tests; safe in production | helper |
| L2 Recording | Start, collect, check; structured result and errors | helper |
| L3 Framework integration | Automatic per-test checking in the language's main test framework | helper |
| L4 Suite | Coverage union across tests, printed at suite end (and written when a path is set) | helper, merging delegated to core |

## Uniform names

Spell every name in the language's convention (`check_hot` / `CheckHot` / `checkHot`).
Never use a synonym (`liveness`, `AllowUnobserved`, `SNAPPEA_BINARY`). A helper MAY keep an
old name as a deprecated alias for one minor release, warning when it is used.

### Environment variables

| Variable | Meaning | Precedence |
| --- | --- | --- |
| `SNAPPEA_BIN` | Checker binary | after explicit config, before bundled copy and `PATH` |
| `SNAPPEA_SPEC_FILES` | Spec files, separated like `PATH` (`os.pathsep`) | overrides config |
| `SNAPPEA_EVENTS_DIR` | Folder child processes write events to ([format](observe.md#events-folder-format)) | set by the recording |
| `SNAPPEA_ARTIFACTS` | Folder to keep artifacts of failed checks in | overrides config |
| `SNAPPEA_COVERAGE` | Suite coverage JSON path, or `off` to turn the default report off | overrides config |
| `SNAPPEA_MODE` | L3 mode: `auto`, `marked` or `off` | overrides config |
| `SNAPPEA_DOWNLOAD` | `0` turns off downloading the checker (Go only) | |

Relative paths in environment variables resolve against the process working directory.

### Concepts

| Concept | Type | Default |
| --- | --- | --- |
| `spec_files` | paths or globs; a folder means every `.p` in it | required (config or env) |
| `specs` | spec names | every spec in `spec_files` |
| `exclude` | spec names | none |
| `strict` | bool | `true` when `specs` is given; `false` when `specs` defaults to all |
| `check_hot` | bool | `false` |
| `unknown_events` | `ignore` / `warn` / `error` | `error` |
| `timeout` | duration | 30 s (helpers that start a process) |
| `artifacts` | path | a temp folder, kept only when the check fails |
| `mode` | `auto` / `marked` / `off` | `auto` where the framework allows it, else `marked` |

### API names

| Name | Layer | Meaning |
| --- | --- | --- |
| `emit`, `emit_at` | L1 | Record an event now / at a given time |
| `set_sink` | L1 | Send events outside tests to a production sink, replacing the previous one; none/nil/off turns it off |
| `file`, `stdout`, `writer`, `callback` | L1 | Production sink constructors |
| `dropped`, `flush`, `close` | L1 | Events dropped so far; wait for queued events (with a timeout); write them and turn the sink off |
| `check_spec` (or the language's scoped form) | L2 | Scoped recording that checks on exit |
| `Recording` / `Recorder`, `start`, `stop`, `check` | L2 | Manual form |
| `events_dir`, `env` | L2 | The recording's `SNAPPEA_EVENTS_DIR` folder, and an environment for children |
| `add_events_dir`, `add_log` | L2 | Extra event inputs |
| `SpecViolation`, `SetupError` | L2 | Error types |
| `expect_violation` | L2 | Negative control |

## L0 Core contract

- Helpers MUST check with the core: run `snappea observe` (CLI helpers) or call
  `snappea::checker::observe` (Rust, in-process). They MUST NOT parse P, check payloads,
  reorder events or interpret monitor semantics themselves.
- CLI helpers MUST pass `--json-report` and read results from the report, never from
  console output, and pass `--format text` when they show snappea's output to people
  (a subprocess's stdout is not a terminal, so it defaults to JSON). The report's
  `exit_code` is the exit status: 0 pass, 1 spec violation, 2 tool or input error; a
  configuration or compile error gives `{"exit_code": 2, "error": "..."}`. Classify
  errors by their `kind`.
- Spec selection: `--spec <name>` for each name when `specs` is set, else `--all-specs`
  with `--exclude-specs <name>` for each excluded spec. `strict` → `--strict`;
  `check_hot` → `--check-hot`; `unknown_events` → `--unknown-events` ([#47](https://github.com/crowecawcaw/snappea/issues/47)).
  Every report has coverage.
- Events go to the core in the [events folder format](observe.md#events-folder-format),
  with `-l <folder>`. Extra inputs (`add_events_dir`, `add_log`) are passed as further
  `-l` options (one path each), which the core merges into one input set
  ([several logs](observe.md#several-logs-and-glob-patterns)).
- Coverage: per-check `coverage` objects from the report; suite union with
  `snappea coverage merge-observe` (or `ObserveCoverageReport::merge` in Rust). Helpers MUST
  NOT merge coverage sets themselves.
- A helper SHOULD fail with a `SetupError` naming both versions when the binary lacks an
  option the helper needs, rather than drop the option.

## L1 Emitter

- `emit(event, key, payload)` and `emit_at(time, event, key, payload)`, in that order.
  Languages with keyword arguments MAY take `key` and the time by keyword (Python:
  `emit(event, payload, key=..., at=...)`).
- Payload: a named-tuple map, a single value, or nothing, in the language's natural form
  (keyword arguments, `map[string]any`, `Map`, `json!`). Enums are sent by name; sets and
  tuples as lists.
- Zero dependencies beyond the standard library (Rust: `serde` / `serde_json`, which the
  `json!` payload form needs). No test-framework dependency.
- Outside a recording, with no `SNAPPEA_EVENTS_DIR` and no production sink, `emit` does
  nothing and costs at most a lock-free check. In production it MUST NOT throw, panic or
  block.
- In tests, emitting into a recording that was already checked MUST make the check (or
  the test) fail; it is never silently dropped.
- Record the call site as `src` (`file:line`) wherever the language provides it
  cheaply (Python frames, Go `runtime.Caller`, Java `StackWalker`, Rust `#[track_caller]`).
- Order: a per-process sequence number (`seq`) assigned under a lock. Time: wall clock
  in nanoseconds, never decreasing within a process (use the larger of now and the last
  value).
- With `SNAPPEA_EVENTS_DIR` set and no in-process recording, `emit` appends to the process's
  own file in that folder ([format](observe.md#events-folder-format)).
- Production sink (see [Testing and production](production.md)):
  - `set_sink(sink)` sends events, when no recording takes them and `SNAPPEA_EVENTS_DIR` is
    unset, to `sink` as [event lines](observe.md#events-folder-format). It replaces
    the previous sink, writing what that one had queued. With `SNAPPEA_EVENTS_DIR` set it
    does nothing: a test started the process, and its folder wins.
  - Sinks: `file(path)` appends (creating the file and its folder), `stdout()`,
    `writer(w)` (flushed, never closed by the helper) and `callback(f)` (gets each line
    without the newline). Lines from one process never interleave or split; give each
    process its own file.
  - Sinks write from a background thread through a bounded queue. A full queue, a
    failed write, a callback that throws, or a payload that cannot be encoded drops the
    event and adds one to `dropped()`; `emit` never sees it.
  - `flush(timeout)` waits for the queue and returns whether it emptied. `close()`
    flushes (bounded) and turns the sink off; call it at shutdown. Helpers SHOULD also
    flush at process exit where the language has a hook.

  | Language | Set | Sinks | Dropped / flush / close |
  | --- | --- | --- | --- |
  | Python | `snappea.set_sink(target)` returns a `Sink`; `None` is off | a path, `"-"` or a text stream, a callable | `sink.dropped`, `sink.flush(timeout)`, `sink.close()` |
  | Go | `snappeaemit.SetSink(s)`; `nil` is off | `snappeaemit.File(path)` (`(*Sink, error)`), `Stdout()`, `Writer(w)`, `Callback(f)` | `snappeaemit.Dropped()`, `Flush(timeout)`, `Close()` |
  | Java | `SnappeaEmit.setSink(sink)`; `null` is off | `Sink.file(path)`, `Sink.stdout()`, `Sink.writer(out)`, `Sink.callback(f)` | `SnappeaEmit.dropped()`, `flush(timeout)`, `close()` |
  | Rust | `snappea::emit::set_sink(emitter)`; `Emitter::off()` is off | `Emitter::file(path)?`, `stdout()`, `writer(w)`, `callback(f)` | `snappea::emit::dropped()`, `flush(timeout)`, `close()` |

  Rust's sink is an `Emitter` value, which code can also take as a dependency; its
  `callback` gets each `Event` on the emitting thread, because test recordings are
  built on it.
- Unknown event names MAY be rejected at emit time in tests when the recording has
  loaded the declared names; they MUST be rejected at check time under
  `unknown_events = error`.

## L2 Recording

### Forms

- MUST offer a scoped form (context manager, try-with-resources, closure, or a fixture
  registering `t.Cleanup`) and a manual `start` / `stop` / `check` form.
- The check MUST NOT run while an exception or panic is unwinding, and MUST NOT replace
  the test's original failure. It MAY attach its result to it (suppressed exception, log).
- A recording is never shared between tests, subtests, or property-test cases.

### Errors

- `SpecViolation`: fields `spec`, `key`, `message`, `location` (P assert `file:line`),
  `events` (the key's last events, each with `event`, `payload`, `src`), `liveness`
  (true for a hot-state end under `check_hot`), `others` (further violations from the
  same check). It MUST be the framework's assertion-failure type (Python
  `AssertionError`, Java `AssertionError` / `AssertionFailedError`, Go: reported through
  `t.Error`/`t.Fatal` and returned as `*SpecViolation` for `errors.As`, Rust: a panic in
  `#[test]` and an `Err` value in the explicit form), so property-test shrinkers treat it
  as an ordinary failure.
- `SetupError`: everything else, with a `kind`:

  | `kind` | Cause |
  | --- | --- |
  | `binary_missing` | No checker binary; the message says how to install one |
  | `config` | Bad options or config file, missing spec files, P compile errors, unknown spec name |
  | `parser` | An event line or payload the core cannot read (including truncated lines) |
  | `validation` | `strict` found a selected spec with no relevant event |
  | `unknown_event` | An event name no loaded P file declares, under `unknown_events = error` |
  | `out_of_order` | Live input out of order (core report kind) |
  | `timeout` | The checker exceeded `timeout` |

  Kinds that the core reports are copied from the report's `errors[].kind`.
- Assertions on errors use these fields, never message text matching.

### Negative controls

`expect_violation(spec, match)` requires the enclosed check to raise a `SpecViolation`
for `spec` whose `message` matches `match` (a regular expression, searched). It fails
if the check passes or another spec is violated. Enclosing recordings ignore `spec` for
the rest of the test, so the deliberate violation is not reported twice.

### Failure output

Match Python's format: the spec's message; the violating key's last events (up to 20)
with `src` and a marker on the failing event (or "events ended here" for liveness); the
P assert location; the artifacts path. Example in the [Python guide](https://crowecawcaw.github.io/snappea/docs/python.html#failures).

### Event inputs

- Each recording creates its own events folder and exposes it as `events_dir()` and an
  environment `env()` (the current environment plus `SNAPPEA_EVENTS_DIR`), so starting a
  child with it is one line. Language spellings: Go `r.EventsDir()`, `r.Env()`; Java
  `recording.eventsDir()`, `recording.environment()`; Rust `recording.events_dir()`,
  `recording.env()`.
- At check time the recording reads its folder together with in-process events.
- `add_events_dir(path)` adds a folder the recording did not create (container volume,
  session-scoped service). `add_log(path, parse)` adds a log whose lines `parse` turns
  into events, or a JSON-lines log when `parse` is omitted.
- Children must be stopped or joined before the check; see the
  [format](observe.md#events-folder-format).

### Binary resolution

In order: explicit config, `SNAPPEA_BIN`, the bundled or embedded copy, `PATH`; Go then
downloads a release binary. A miss is a `SetupError` of kind `binary_missing` with
install instructions. Rust checks in-process and has no binary. Distribution per
language is under [Packaging and distribution](#packaging-and-distribution).

## L3 Framework integration

- Modes: `auto` (every test), `marked` (opted-in tests), `off`. `SNAPPEA_MODE` overrides
  config. Where the framework has no global per-test hook (Go `testing`, Rust `#[test]`),
  `auto` is unavailable: the helper offers a one-line fixture, and `off` makes it inert.
- Per-test options: skip every spec, skip some specs, `check_hot`.
- The recording starts before fixtures or setup run and is checked after the test body.
  A test that emitted nothing and is not strict passes without running the checker.
- A test that already failed is not failed again by the check.
- Recordings are never shared across property-test cases. Each helper documents the
  per-case pattern for its property framework.

| Framework | Hook | Setup inside | Teardown inside | Property testing |
| --- | --- | --- | --- | --- |
| pytest (Python) | plugin; `@pytest.mark.snappea(skip=..., check_hot=...)` | fixture setup: yes | fixture teardown: no (checked when the body ends) | Hypothesis: `with check_spec(...)` inside the `@given` body |
| `testing` (Go) | `snappeatest.Start(t, cfg)` in the test's fixture | after `Start` | `t.Cleanup` registered after `Start`: yes; before: no | `go test -fuzz`: `Start` inside the `f.Fuzz` callback with its `t` |
| JUnit 5 (Java) | `SnappeaExtension` (`@ExtendWith` or autodetection), `@Snappea(skip=..., checkHot=...)` | `@BeforeEach`: yes | `@AfterEach`: yes | jqwik: `observer.recording(...)` per `@Property` try, coverage via `@AfterContainer` |
| `#[test]` (Rust) | `model.check(\|recording\| ...)` closure | inside the closure | inside the closure | proptest: a recording per case inside the `proptest!` body |

Agents writing tests SHOULD join workers inside the recording; the table says where
that is.

## L4 Suite

- Where the framework offers a suite-end hook, the helper prints suite coverage by
  default: it keeps each check's report in memory and at suite end merges them with
  `snappea coverage merge-observe --stdin --format text`, writing no file, so a run
  reports only itself. Nothing is printed when no check ran. An error in this default
  report is a one-line warning and MUST NOT fail the run.
- One setting also writes the JSON: `SNAPPEA_COVERAGE` or the framework option (a
  path). `SNAPPEA_COVERAGE=off` turns the default report off.
- The helper owns collection and the merge call. Users write no `TestMain`, static
  mutex or JSON code where the framework offers a suite-end hook.
- With a suite-end hook (pytest session finish, JUnit `CloseableResource` /
  `LauncherSessionListener`), the helper writes the file once at suite end.
- Without one (Rust; Go without `snappeatest.Main`), there is no default report; with a
  path, the helper merges each check into the file as it completes.
- Several processes may share one `SNAPPEA_COVERAGE` path (pytest-xdist, `go test ./...`,
  Gradle forks): the helper MUST merge into an existing file under an exclusive file
  lock, never overwrite it. Deleting a stale file before a run is the user's job (CI
  starts clean).
- Output: the core `observed_monitor_suite` JSON at the path, and the `render_text`
  summary on the framework's terminal output.

## Configuration

One project file per ecosystem, same keys as the [concepts](#concepts):

| Language | File |
| --- | --- |
| Python | `[tool.snappea]` in `pyproject.toml` |
| Java | `snappea.properties`, or `snappea.*` keys in `junit-platform.properties` |
| Go | none: `snappeatest.Config` and environment variables |
| Rust | `[package.metadata.snappea]` in the crate's `Cargo.toml` |

- Paths in a config file resolve relative to that file, never the working directory.
  Go resolves `Config` paths relative to the package folder (`go test`'s working
  directory).
- Precedence: explicit API argument, then environment variable, then config file, then
  default.
- Unknown keys in a config file are a `SetupError` of kind `config`.

## Conformance corpus

`tests/conformance/<case>/` holds `spec.p`, `events.jsonl` (optional
`children/*.jsonl`) and `expect.json`. The schema and replay steps are in
[tests/conformance/README.md](../tests/conformance/README.md). In short:

```json
{
  "description": "...",
  "requires": ["events_dir"],
  "options": {"specs": [...], "exclude": [...], "strict": true, "check_hot": false, "unknown_events": "error"},
  "expect": {"outcome": "pass | violation | setup_error", "spec": "...", "key": "...",
             "message": "substring", "liveness": false, "kind": "validation",
             "coverage": {"Spec": {"statements": {"covered": 4, "total": 6}, "branches": {...}, "handlers": {...}}}}
}
```

- Each helper MUST have a test that replays every case through its own L1/L2 API
  (`emit_at` for `events.jsonl`, children files copied into the recording's events
  folder) and compares the result with `expect.json`.
- A case whose `requires` lists a feature the helper lacks is skipped with the reason
  printed. Nothing else is skipped.
- `crates/snappea/tests/cli_conformance.rs` replays the corpus through `snappea observe`, so the
  corpus stays consistent with the core.
- A helper is conformant when the corpus passes without skips and its checklist below is
  complete. A new language starts by copying the checklist.

## Docs page

Each helper has one page on the project site (`web/docs/<language>.html`, source
`web/src/site/docs/languages/`). The pages are for application developers adopting the
helper, not for helper implementors (this document). Every page has the sections below,
in this order and with these headings, so a reader can switch language and find the
same thing in the same place.

| # | Section | Contents |
| --- | --- | --- |
| 1 | Intro (no heading) | What the helper does in two or three sentences, what it does not do (it sees only the orderings the tests produce), and a link to the overview |
| 2 | Install | The one-line install, where the checker comes from ([binary resolution](#binary-resolution)), supported versions, what production code depends on |
| 3 | Quick start | The shared inventory spec, application code emitting its events, one passing test, the same test against a buggy service, and that failure's real output |
| 4 | Test setup | Configuration file, L3 framework integration and per-test options, L2 explicit checks, negative controls, property testing |
| 5 | Emitting events | L1 `emit` and `emit_at`, keys, payload forms, when to emit, the production sink |
| 6 | Other processes | `env`, `events_dir`, `add_events_dir`, `add_log`; join children before the check |
| 7 | Failures | The failure output, the `SpecViolation` fields and the `SetupError` kinds, in the language's spelling |
| 8 | Reference | The [concepts](#concepts) table in the language's spelling with where each is set, the environment variables, suite coverage, and a link to the generated API reference |

- The quick start uses the inventory spec every example shares (`examples/*/specs/inventory.p`),
  imported from the repository so the pages cannot drift from it. Failure output is
  pasted from a real run.
- The settings, environment variable and error kind tables are shared components fed
  by one table of names per language, so a default changes in one place.
- API details belong in the language's generated reference, not the page: docs.rs
  (Rust), pkg.go.dev (Go), javadoc.io (Java, from the javadoc jar on Maven Central) and
  pdoc (Python, built from the package docstrings into `docs/api/python/` by the Pages
  workflow). The page links to it and covers only what generated docs cannot: setup,
  where events go, and how to read a failure.
- Package READMEs (`python/README.md`, `go/README.md`, `java/README.md`,
  `crates/snappea/README.md`) stay short: install, one example, a link to the page, and
  notes only contributors need (building from a checkout, upgrading).
- A new helper adds its page as part of its [gap checklist](#gap-checklist).

## Decisions

### Packaging and distribution

Every language publishes **one package** with the base name `snappea`, holding the emitter,
the test helper and a way to get the checker. Production code depends on the same package
and uses only the emitter; unused test code is not loaded (Go, Java) or not compiled
(Rust features). The names per ecosystem are in [Naming](#naming).

| Language | Package | Code name | Checker |
| --- | --- | --- | --- |
| Python | PyPI `pysnappea` | `import snappea` | Native binary in platform wheels. |
| Rust | crates.io `snappea`; features `emit` (production, no checker), `test`, `cli` (the `snappea` binary) | `snappea` | In-process; no binary. |
| Go | module `github.com/crowecawcaw/snappea` | packages `snappeaemit`, `snappeatest` | `SNAPPEA_BIN`, `PATH`, then a release binary downloaded into a per-version user cache and verified against checksums in the module. The module MUST NOT contain native binaries. |
| Java | Maven Central `io.github.crowecawcaw:snappea`; JUnit 5 optional | `io.github.crowecawcaw.snappea` | Classifier jars `natives-<os>-<arch>` of the same artifact, extracted to a per-version cache on first use. |

All packages share one version and release (`release.yml`); Go tags are plain `vX.Y.Z`.
The tagged commit holds the release version everywhere; afterwards `main` keeps it in
`Cargo.toml` while Go goes back to `dev` and Java to the next `-SNAPSHOT`
([RELEASING.md](../.github/RELEASING.md)).
The checker must work offline with no per-user setup, so only Go downloads, and only
when no binary is found. A WASI build (wazero for Go, Chicory for Java) may later replace
the native binaries if it passes the conformance corpus at no more than 2× native cost.
Work is tracked in [#64](https://github.com/crowecawcaw/snappea/issues/64).

### Go suite hook

Keep the explicit fixture (`snappeatest.Start(t, cfg)`): Go has no global per-test hook,
and a one-line fixture is idiomatic. For L4, add `snappeatest.Main(m)` for use as
`func TestMain(m *testing.M) { snappeatest.Main(m) }`: `TestMain` is Go's suite hook, so
this is the framework hook, not boilerplate. Without it, the helper merges per check
under a file lock (L4), so coverage still works, only slower.

### Strict default

`strict` defaults to `true` when the user names the specs, and `false` when `specs`
defaults to every spec. Strict means "every selected spec checked an event". With named
specs that catches disabled instrumentation, which is its purpose. With every spec
selected (pytest `auto` mode) most tests exercise only some specs, so strict would fail
nearly every test.

Python: `check_spec("Name")`, `check_events(events, "Name")` and `Recorder(specs=[...])`
become strict by default. This is a behaviour change: for one minor release, a check
that would newly fail emits a `DeprecationWarning` naming `strict=False` instead of
failing; then the default flips. `[tool.snappea] strict` and `--snappea-strict` keep working.

### Rust `Model`

`Model` accepts both paths and embedded sources, and several specs:

- `Model::load()` reads `[package.metadata.snappea]` from `CARGO_MANIFEST_DIR` (paths relative
  to that `Cargo.toml`), honouring `SNAPPEA_SPEC_FILES`.
- `Model::from_files(paths)` and `Model::compile(sources)` (for `include_str!`) for
  explicit setup.
- `.specs([...])`, `.exclude([...])`, `.strict(..)`, `.check_hot(..)`,
  `.unknown_events(..)` as builders; `specs` defaults to every spec.
- `EventSink::emit(event, key, payload)`: the argument order changes to match L1.
  The crate is pre-1.0, so no alias.

### Smaller decisions

- `SNAPPEA_MODE` is added to the uniform environment variables: Go and Rust have no
  command-line switch for test runs, so an environment variable is the only uniform way
  to turn checking off.
- Events folder files are `<name>-<pid>-<random>.jsonl`: containers sharing a volume
  often share pid 1, and a process name makes reports readable
  ([#49](https://github.com/crowecawcaw/snappea/issues/49) open question).
- `unknown_events` is separate from `strict` (as #47 proposes); helpers default both on
  for named specs. A per-project allow-list of undeclared names is not added: `warn`
  covers mixed sinks.
- Session-scoped services: a helper SHOULD offer a suite-level recording (checked at
  suite end) over a folder the service writes to, replacing Python's documented
  workaround.

## Gap checklist

Unchecked items are work to do. Close each with a PR; update this list in the same PR.
The common items #45, #47, #48, #49 and #53 cover are linked.

### Python

- [x] Bundled binary (wheel)
- [x] `emit` / `at=`, `src`, `seq`, `pid`
- [x] `specs` default all, `exclude`
- [x] Structured `SpecViolation`, `expect_violation`, failure output
- [x] `auto` / `marked` / `off`; fixture setup inside, teardown outside
- [x] `SNAPPEA_EVENTS_DIR`, `add_events_dir`, `add_log`
- [x] Suite coverage with no boilerplate (`--snappea-coverage`)
- [x] `SNAPPEA_BIN`; `SNAPPEA_BINARY` as deprecated alias (#53)
- [x] `check_hot` everywhere `liveness` is used (`check_spec`, `Recorder`, mark); `liveness=` as deprecated alias. (`SpecViolation.liveness` stays.)
- [x] `strict` default per [Strict default](#strict-default), with deprecation period
- [x] `unknown_events` option, default `error` (#47)
- [x] `SetupError` with `kind` (`SnappeaError`, `SnappeaNotFound` become aliases or subclasses)
- [x] `SNAPPEA_COVERAGE`, `SNAPPEA_ARTIFACTS`, `SNAPPEA_MODE`, `timeout`
- [x] `events_dir()` / `env()` on `Recorder`
- [x] Events folder file name `<name>-<pid>-<random>.jsonl`
- [x] Coverage file merge under a file lock (pytest-xdist)
- [x] Production sink for `emit` (#48)
- [x] Conformance runner
- [x] [Docs page](#docs-page)

### Go

- [x] `Emit(event, key, payload)`, `EmitAt`
- [x] `SNAPPEA_BIN`, `strict` default true with named specs, `CheckHot`
- [x] `timeout`, artifacts kept on failure
- [x] Packaging per [Packaging and distribution](#packaging-and-distribution): module at the repo root, release binary downloaded and verified (#64)
- [x] `Specs` optional (default all), `Exclude`
- [x] Rename `AllowUnobserved` to `Strict` (`*bool` or option func); keep the old field deprecated
- [x] `*SpecViolation` / `*SetupError` for `errors.As` (#53)
- [x] `ExpectViolation(t, spec, match)` (a `Recorder` method)
- [x] Failure output in Python's format, with `src` from `runtime.Caller`
- [x] `EventsDir()`, `Env()`, `AddEventsDir`, `AddLog` (#49); move `snappeaemit` out of `python/`
- [x] `UnknownEvents`, default `error` (#47)
- [x] `snappeatest.Main(m)` prints suite coverage by default (no file); `SNAPPEA_COVERAGE` path or `off`; per-check merge without `Main`
- [x] `SNAPPEA_SPEC_FILES`, `SNAPPEA_ARTIFACTS`, `SNAPPEA_MODE`
- [x] Production emitter package (#48)
- [x] Conformance runner
- [x] [Docs page](#docs-page)

### Java

- [x] `emit(event, key, payload)`, `emitAt`
- [x] `SNAPPEA_BIN`, `strict` default true, `checkHot`, `timeout`, `artifacts`
- [x] Suite coverage via the extension, printed by default (no file); `SNAPPEA_COVERAGE` path or `off`; jqwik via `@AfterContainer`
- [x] Packaging per [Packaging and distribution](#packaging-and-distribution): one `snappea` artifact with natives classifier jars, on Maven Central (#64)
- [x] `specs` optional (default all), `exclude`
- [x] `SpecViolation extends AssertionFailedError` with structured fields; `SetupError` with `kind` (`ObservationFailure` stays as their deprecated superclass)
- [x] `expectViolation(spec, match)`
- [x] Failure output in Python's format, with `src` from `StackWalker`
- [x] `auto` (extension autodetection) / `marked` / `off`; `@Snappea(skip=..., checkHot=...)`
- [x] `eventsDir()`, `environment()`, `addEventsDir`, `addLog` (#49)
- [x] `unknownEvents`, default `error` (#47)
- [x] Config file (`snappea.properties` / `junit-platform.properties`), paths relative to it
- [x] `SNAPPEA_SPEC_FILES`, `SNAPPEA_COVERAGE`, `SNAPPEA_ARTIFACTS`, `SNAPPEA_MODE`
- [x] Production emitter package (#48)
- [x] Conformance runner
- [x] [Docs page](#docs-page)

### Rust

- [x] Packaging per [Packaging and distribution](#packaging-and-distribution): one `snappea` crate with features (#64)
- [x] In-process checking; strict default true; `check_hot`
- [x] `emit(event, key, payload)` argument order
- [x] `Model` per [Rust `Model`](#rust-model): paths, config, several specs, `exclude`
- [x] `SpecViolation` / `SetupError` with structured fields and `kind` (#53)
- [x] `expect_violation(spec, match)`
- [x] Failure output in Python's format, with `src` via `#[track_caller]`
- [x] Shared, thread-safe suite coverage; `SNAPPEA_COVERAGE` (#53)
- [x] `events_dir()`, `env()`, `add_events_dir`, `add_log` (#49), reading folders through a core helper
- [x] `unknown_events`, default `error` (#47)
- [x] Fixture pattern (`model.check(|recording| ...)`); `SNAPPEA_MODE`
- [x] Production emitter (#48): `snappea::emit` (feature `emit`)
- [x] Conformance runner
- [x] [Docs page](#docs-page)
