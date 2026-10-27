# Compatibility with upstream P

This reference is about `snappea-compat`. `snappea` checks the same way, finds the same bugs
and writes the same trace formats, but does not imitate upstream where it doesn't have
to:

* Options: its own, shorter ones (`snappea <command> --help`).
* Errors: parse, type and lint errors show the source around them and a suggested fix;
  other errors add a `hint:` line. The messages themselves are upstream's.
* Console: one line per test case, the bug's report and its `snappea replay` command, and a
  summary with time and peak memory. Upstream's progress only with `-v`.
* Output folder: `PCheckerOutput/<test>/` with fixed names (`bug.txt`, `bug.schedule`,
  `bug.trace.json`, `scenario_coverage.json`), replaced by each run. No `BugFinding/`
  folder, history folders, `.sci`, coverage or summary file: coverage and the summary go
  to the console (or the JSON result).
* `<IncludeProject>` paths in a `.pproj` are resolved against the including project
  file's folder first, then the current folder (upstream: only the current folder).
* `observe`: sources and logs are positional arguments, live input stops at the first
  error without `--fail-fast`, and there is no report folder (use `--json-report`). Exit
  codes are 0, 1 and 2, as for `check`; see [snappea observe](observe.md).

**Exact** (drop-in):

* `check`, `merge-scenario-coverage` and `observe` options, help, console output and
  exit codes.
* Parse and type error messages.
* Bug reports, assertion and liveness messages.
* Trace files (`.txt`, `.trace.json`, `.schedule`), coverage reports, output folder
  layout and history rotation.
* `.pproj` files.
* PObserve output files (replay logs, `ParserError_N.txt`, `UnknownError_N.txt`,
  `PObserveMetrics.txt`).

**Functional** (same result, details differ):

* Findings may be reported in a different order.
* `check` reads P sources, not a compiled `.dll`: a `.pproj` or `.p` file as its path,
  `--pproj`, `--pfiles` and `--projname` (upstream: options of `p compile`), or else the
  `.pproj` or `.p` files in the current folder.
* Source discovery is logged differently, since there is no compiled output to find.
* Files are enumerated in sorted order; elapsed time and memory figures differ.
* An action that runs forever without reaching a scheduling point is reported as a bug
  (upstream hangs).
* Nesting deeper than 128 levels is a parse error, `nesting is too deep` (upstream has
  no limit). Blocks, statements, expressions, types and each operand of an operator
  chain such as `a + b + c` count a level; the upstream tests nest at most 16 deep.
* `merge-scenario-coverage --help` prints its usage (upstream looks for a directory
  named `--help`).
* `observe` reports a null dereference with its own message.
* `observe` extensions: several names after `--spec`, `--all-specs`,
  `--exclude-specs`, `--json-report`, `--strict`, `--unknown-events`,
  `--check-hot`, several `-l` values and glob patterns, and live input (`--stdin`,
  `--live`, `--sequence`, `--record`). Runs that use none of them are unchanged.
* Foreign functions and log parsers are written in Rust, not C# or Java.
  `snappea-compat` warns about the foreign files a project lists and ignores them;
  `snappea check` rejects such a project unless given `--ignore-foreign-files`.

**Not supported yet**:

* PVerifier mode.
* `observe` event filters (`--filters`), socket mode and `SERIALIZEDEVENTS` input.
* `--memout` is accepted and ignored.
* `--graph` and `--graph-bug` are accepted but, as upstream, write nothing.

**Out of scope**:

* `p compile` and its output (`PGenerated/`, `.dll`, jars). There is no `compile`
  command; `OutputDir` and `Target` in `.pproj` files are ignored.
* .NET and Java stack frames in exception reports, traces and `UnknownError_N.txt`.
* Foreign code in C# or Java, and PObserve jars (`--jars`).

## How compatibility is checked

The [upstream parity page](https://snappea.dev/docs/parity.html) derives its corpus counts,
normalization rules and known differences from the committed fixtures at site build time.

Upstream's outputs are recorded once and committed, so CI compares against them without
running upstream P. Upstream is built from source at the commit in
`tests/upstream/goldens/UPSTREAM` (newer than the latest NuGet release).

| What | Measure | Runs |
| --- | --- | --- |
| `check` exit code | exact | every run |
| `check` console (stdout and stderr, separately) | exact after normalization | every run |
| Parse and type errors, compiler warnings | exact | 423 regression tests (upstream: `p compile`) |
| Readable traces (`.txt`) | exact after normalization | every bug found |
| XML traces (`--xml-trace`) | exact after normalization | command line |
| JSON traces (`.trace.json`) | exact text after normalization | every bug found |
| Schedules (`.schedule`) | exact | every bug found |
| Coverage reports (`.coverage.txt`) | exact | every run |
| Summaries (`_pchecker_summary.txt`) | exact except time and memory | every run |
| Scenario coverage (`.json`, console, `merge-scenario-coverage`) | exact | scenario coverage runs |
| Output folder layout and history rotation | exact list of files written | every run |
| `.sci` coverage files | written, contents not compared | every run |
| Schedules explored by other strategies (PCT, fair PCT, POS, probabilistic, feedback) and step limits | exact, through the traces and statistics they print | tutorial |
| Help, usage errors, unknown options and test cases, project names, replay, `--list-tests` | exact | command line |
| `observe`: exit code, console, replay logs, error files, metrics | exact after normalization | 96 vendored PObserve runs; 94 compare output ([below](#observe)) |

Normalization (`tests/upstream/goldens/normalize.tsv`, applied to both tools):

* Dropped: lines about finding the project and the compiled `.dll` and the
  `Parsing ...`/`Type checking ...` progress lines (snappea-compat reads sources and
  prints `.. Checking <project>` instead).
* Masked: elapsed time and memory, the working directory, .NET task ids
  (`created by task '2'`), .NET stack frames, and a byte order mark.
* Compiler warnings: upstream prints them in `p compile`, snappea-compat in `check`; the
  recorder puts the compile step's warnings first in the check steps' output.

Files over 128 KB are stored as the SHA-256 of their normalized contents, so they are still
compared exactly.

Not compared: `p compile` output and generated code, timings, `--version`, runs with more
than one worker (`-j`), PEx mode (upstream runs it on a Java build of the model), and
anything under "Out of scope". Not recorded: the PVerifier tutorials
(`Tutorial/Advanced`), the tutorial with C# foreign code (`PriorityQueue`) and
`Liveness/DynamicError/InfiniteLoopInAtomicBlock`, on which upstream hangs.

Known differences are listed, with reasons, in
`tests/upstream/goldens/known-differences.tsv` (today: six help and usage texts, which
mention `--pproj`/`observe` instead of `compile`). The test fails when a listed case
starts to match, so the list only shrinks.

The corpus (`tests/upstream/goldens/cases/<id>/`):

* `regression/…` (423 cases): every upstream regression test. `StaticError` tests:
  compile. Others: `check -s 10 --max-steps 1000 --seed 42`, plus `--list-tests` for
  parametric ones.
* `tutorial/…` (8 cases): each tutorial project: `--list-tests`, then every test case with
  `-s 100 --seed 42`.
* `cli/…` and `options/…` (34 cases): help and usage errors, project names, verbose, `-o`,
  `--xml-trace`, replay, history rotation, scenario coverage merging, and each strategy and
  step-limit option on two test cases of the client-server tutorial.

Where it runs:

* `cargo test` and CI (`ci.yml`, job "upstream compat"):
  `crates/snappea/tests/compat_check_upstream.rs` and `compat_observe_upstream.rs` run
  snappea-compat in memory on every case. `UPSTREAM_CASES=<id prefix>` runs a subset.
* `.github/workflows/upstream-goldens.yml` (manual, and weekly against upstream's master)
  builds upstream, re-records and fails if the result differs from the committed goldens:
  at the pinned commit this shows the goldens are upstream's output, at master what
  changed upstream.

To record again, or after moving the pin:

```sh
git clone https://github.com/p-org/P && git -C P checkout "$(cat tests/upstream/goldens/UPSTREAM)"
(cd P/Bld && ./build.sh --skip-submodules)
UPSTREAM_P="dotnet $PWD/P/Bld/Drops/Release/Binaries/net8.0/p.dll" scripts/record-upstream-goldens.py
```

### observe

`tests/upstream/PObserve` holds upstream PObserve's outputs on its regression logs
(`scripts/vendor-pobserve-goldens.py`). Normalized: colors, the output folder, the
configuration lines naming jars and project files, timings, Java stack frames, and the
order of error blocks (upstream checks keys in parallel). CI covers 96 vendored runs:
94 compare exit codes, console and files; the two in `known-differences.tsv` compare only
exit codes because the null-dereference message names a variable of upstream's generated
Java code. The vendor script selects logs smaller than 8 KB, each run with and without
`--assumeInputsAreSorted`.

The separate manual comparison (`scripts/compare-pobserve.py`) covers the full upstream
suite: 192 runs, all exit codes matching, 186 outputs identical after normalization,
and six differing only in that null-dereference message. Those full-suite figures are
not the vendored CI coverage.
