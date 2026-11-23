//! The `snappea` command line.
//!
//! `snappea` runs the commands of [`crate::driver`] with its own options (test cases and
//! sources as arguments, automatic scheduling, durations with units, parallel workers,
//! time tracking) and its own output: a line per test case, the bug's report and replay
//! command, and a folder per test case ([`Style::Snappea`]). The upstream-style progress shows
//! with `--verbose`. Its exit statuses are stated once, in `exit_status!`.

use std::path::Path;

use serde_json::{json, Value};

use crate::checker::code_coverage::{uncovered_lines, CodeMap, Hits, Scope, SHOWN_UNCOVERED};
use crate::checker::strategy::StrategyKind;
use crate::driver::check::{self, CheckConfig, Gate, Style};
use crate::driver::progress::Position;
use crate::driver::scenario;
use crate::host::{full_path, Buffered, Color, Exit, Host, HostExt, ProgressView, Quiet, Stream};
use crate::project::pproj::Includes;

pub(super) const OK: i32 = 0;
pub(super) const BUG: i32 = 1;
pub(super) const ERROR: i32 = 2;
// Internal status: run() turns argument failures into the public error result.
const USAGE_ERROR: i32 = -1;

/// The exit statuses of `snappea check`, shared by the help texts that state them.
macro_rules! exit_status {
    () => {
        "\
Exit status: 0 nothing failed; 1 a bug, a required coverage gap, an undetected
mutant or, with --strict, an untested one; 2 an error, or blocking lint findings
with no bug.
"
    };
}

const HELP: &str = concat!(
    "\
🫛 snappea checks P models.

Usage: snappea [output options] <command> [options]

Commands:
  check      Find bugs in a model: run its test cases over many schedules.
             Use after every model change.
  replay     Re-run a bug's .schedule file to debug it or confirm a fix
  tests      List test cases, e.g. to pick one for `check`
  observe    Check logs of a real system against the model's spec monitors.
             Use to test that an implementation matches the model.
  coverage   Merge coverage from several runs (CI shards, processes)
  guide      How to model a design and test code against it, with runnable
             examples. Start here: `snappea guide`.
  version    Print the version

Typical loop:
  snappea check                       # check every test case
  snappea replay PCheckerOutput/tcFoo/bug.schedule -v   # debug a bug
  snappea check tcFoo                 # confirm the fix

Commands that read P sources take .pproj or .p files. Without any, they use the
*.pproj file in the current folder, or else the *.p files in it and its immediate
subfolders.

Output is text. --json, --format json or SNAPPEA_FORMAT=json print the JSON result
instead.
--json-report FILE also writes the JSON result to FILE.

Run `snappea <command> --help` for a command's options and examples.
",
    exit_status!()
);

const CHECK_HELP: &str = concat!(
    "\
Usage: snappea check [SOURCE...] [TEST...] [options]
       snappea check lint [SOURCE...] [--strict] [--list-rules]
       snappea check mutation [SOURCE...] [TEST...] [options]

Runs test cases over many schedules (event orders, nondeterministic choices) to
find assertion failures, spec violations and liveness bugs. A SOURCE is a .pproj or
.p file, a TEST a test case name or a glob (`*`, `?`). Without tests, all run.

Sources are linted first. Blocking findings fail the check, but the test cases still
run, so bugs are reported first, then the findings. Mutation testing runs too when no
test case failed; coverage is skipped. `check lint` only lints; `check mutation` only
runs mutation testing. See their --help.

Lint findings, required coverage gaps and undetected mutants fail the check.
Mutation testing is skipped when only some test cases are named: one left out may be
the one that detects a mutant. `snappea check mutation TEST` runs it on a selection.
Silence an intended finding in the source with a
`// snappea-lint: allow <rule> -- reason` comment. `--warn` and `--deny` change what
fails for one run.

Examples:
  snappea check                       # all test cases, sharing 10s
  snappea check 'tcRetry*' --timeout 1m   # more search time on a few
  snappea check --no-mutation         # faster iteration while editing
  snappea check --warn lint           # explore with lint findings as warnings
  snappea check --strict              # also fail on low-priority mutants and lints

Exploration:
      --timeout <DURATION>  Time limit per test case (accepts 90s, 5m, 1h). Without it,
                            the test cases share 10s, and each stops early once its
                            coverage stops growing. Set it before trusting a pass
                            on a large model.
  -j, --workers <N>         Worker limit (default: one per CPU core). Lower it on
                            shared machines.
      --seed <N>            Seed for reproducible choices. Not needed to reproduce
                            a bug: its .schedule replays exactly.
      --expect-bug-seeds <N,N,...>
                            Repeat declared expected-bug tests with 2–32 explicit seeds
                            (e.g. 1,2,3), and compare actual failure sites. Positive tests
                            still run once. Different sites are advisory; every seed
                            must find its declared failure. --timeout applies per seed.
      --max-bugs <N>        Distinct failures to report per test case (default: 3).
                            After the first bug the search goes on, as long as that
                            bug took (at least 1s, within the time limit), for
                            failures of other specs or assertions. 1 stops at the
                            first bug, e.g. for the fastest red/green loop.

Strategies are chosen automatically. A passing test case is `proved` when every
schedule was checked. Otherwise its search `settled` (the second half found nothing
new, so more time is not needed) or was `still finding new behavior` (run longer once,
as the hint says, or shrink the test). `-v` adds schedule and worker counts.

Time (for models with timeouts or latency budgets):
      --latency <FILE>[:<PROFILE>]
                            Latency profiles (TOML), and the profile to use
      --timed               Time-aware scheduling: never run a step that is certainly
                            later than another. Use when untimed checking reports
                            orders real timing rules out, e.g. a timeout handled
                            before a reply that arrives earlier. Keep the untimed
                            run too: it covers latencies outside the profile.

What fails the check:
      --warn <CHECK>        Report CHECK's findings as warnings and keep going: lint,
                            coverage, mutation or all. Repeatable. Use it to explore,
                            not to pass: fix or suppress findings before relying on
                            the result.
      --deny <CHECK>        Fail on CHECK's findings (the default)
      --strict              Also run the mutants that change only a value, and report
                            advisory lint rules; fail on both. Use it for a model
                            that guards a critical design, not while iterating.

Mutation testing:
      --mutation-timeout <DURATION>
                            Time limit for mutation testing (default: 30s)
      --no-mutation         Skip mutation testing

Driver limits:
      --no-driver-facts     Do not work out the limits each test's driver kept
                            (-v prints them; --json lists them)

Inputs:
      --ignore-foreign-files
                            Check the P model even though the project lists foreign
                            (.cs, .java) files, which snappea cannot run. Without it,
                            such a project fails, since the check would leave that
                            code out.

Output:
      --inspect-trace       Include source and values in replay traces; also retain a bounded
                            sample of early executions in schedules.json. Search is unchanged.
  -o, --out <DIR>           Output folder (default: PCheckerOutput)
      --list-suppressions   List suppressed mutants with reasons and stale suppressions.
  -v, --verbose             Show progress, statistics and driver limits, and log every
                            schedule (on one worker).
      --progress <WHEN>     auto, off or json. auto (default) shows a status line
                            when stderr is a terminal and a line every 30s in CI
                            (CI set); none with --verbose. json writes a JSON
                            object per update to stderr, e.g. for a harness
                            watching a long check.

Prints one line per test case with its coverage, and for a bug its report and the
`snappea replay` command that reproduces it; then the code no test case ran. By default,
missing statements, event handlers, or `if` branch outcomes in selected code fail the
check. Use a scoped `coverage` suppression with a reason for deliberate omissions.
`--warn coverage` reports them as warnings instead, for exploratory checks.

A bug's report lists the other specs hot at that point. Further distinct failures
each get a line and a replay command (see --max-bugs).

A bug's files go to <DIR>/<TEST>/, replacing the previous run's: bug.txt (the buggy
schedule's log; read this first), bug.schedule (for `snappea replay`) and
bug.trace.json (for `snappea observe --p-trace`); bug-2.txt and bug-2.schedule (and
so on) for further failures. Scenario monitors also write scenario_coverage.json.

When every test case passes, mutation testing follows: each mutant changes the model
slightly. A mutant no test case detects marks code whose effect no spec or assertion
checks, and fails the check; each is listed with what would detect it. By default only
changes a spec or another machine could see are run: a removed `send` or `announce`,
an emptied handler, a removed or forced branch. `--strict` also runs swapped operators,
removed assignments and the like. To accept a mutant, put
`// snappea-lint: allow mutation[:<kind>] -- reason` before the statement
(see docs/checking-models.md).

With -v, each passing test case also lists the limits its driver kept in every
schedule: the machines it created, the events it sent the model, in what order, and
the values it used. They show what the check never tried, e.g. `no Client sends eAcquire after
eRelease`; if the real environment can do that, extend the driver. The driver is the
test's main machine, machines declared under PTst/, and machines marked with a
`// snappea: driver` comment. Driver limits never change the exit status.

",
    exit_status!()
);

const MUTATION_HELP: &str = "\
Usage: snappea check mutation [SOURCE...] [TEST...] [options]

Runs the test cases, then runs them against mutants: copies of the model with one
small change. Lists every mutant no test case detects, with what would detect it: code
whose effect no spec or assertion checks. By default the changes are those a spec or
another machine could see (a removed `send` or `announce`, an emptied handler, a removed
or forced branch); `--strict` adds swapped operators, removed assignments and the like.

Use it to judge whether the specs are strong enough, e.g. as a CI gate. Unlike
`snappea check`, it reports no coverage or scenarios, and fails when a mutant goes
undetected (`--warn mutation` lists them without failing).

Example:
  snappea check mutation tcBank --mutation-timeout 10m

Options:
      --mutation-timeout <DURATION>
                            Time limit for mutation testing (default: 5m)
      --timeout <DURATION>  Time limit per test case for the first run (default: the
                            test cases share 10s)
  -j, --workers <N>         Worker limit (default: one per CPU core)
      --seed <N>            Seed for reproducible choices
      --expect-bug-seeds <N,N,...>  Repeat negative controls and compare failure sites
      --max-bugs <N>        Distinct failures per test case in the first run (default: 3);
                            mutants always stop at their first failure
      --latency <FILE>[:<PROFILE>], --timed
      --warn <CHECK>, --deny <CHECK>, --strict, --list-suppressions,
      --no-driver-facts, --ignore-foreign-files, --inspect-trace
                            As for `snappea check`
  -o, --out <DIR>           Output folder (default: PCheckerOutput)
  -v, --verbose             Show progress and statistics
      --progress <WHEN>     auto, off or json, as for `snappea check`

`// snappea-lint: allow mutation[:<kind>] -- reason` before a statement accepts its
mutants; `allow mutation-nested[:<kind>]` before a handler, function or machine accepts
the mutants inside it. See docs/checking-models.md.
Exit status: 0 every mutant detected; 1 a mutant undetected, a mutant not tested (the
time limit stopped it, or no passing test case ran its code) or a test case failed;
2 error, or blocking lint findings with no failed test case (mutation testing still
runs then).
Raise --mutation-timeout when mutants are not tested.
";

const REPLAY_HELP: &str = "\
Usage: snappea replay <FILE.schedule> [SOURCE...] [options]

Replays a schedule written by `snappea check` (or `snappea-compat check`): the same
event order and choices, so the same bug. Use it to debug a bug (-v logs each step)
and, after editing the model, to confirm the fix. Exit status 1 means the bug was
reproduced, 0 that it was not.

Examples:
  snappea replay PCheckerOutput/tcBank/bug.schedule -v
  snappea replay bug.schedule Bank.pproj --test tcBank

Options:
      --test <NAME>         Test case to replay, for schedules that name none
      --max-steps <N>       For a schedule written by `snappea-compat check`: the
      --fair-max-steps <N>  step options that check was given. Schedules from
      --fail-on-max-steps   `snappea check` need none.
      --ignore-foreign-files
                            Replay although the project lists foreign files, as
                            `snappea check --ignore-foreign-files` did
  -v, --verbose             Log the schedule
";

const VERSION_HELP: &str = "\
Usage: snappea version

Prints the snappea version, e.g. to report a bug or to check that a CI job installed
the release it pins.
";

const TESTS_HELP: &str = "\
Usage: snappea tests [SOURCE...]

Type-checks the sources and lists their test cases, one per line. Use it to find
names for `snappea check TEST` or to check that sources compile.
";

const COVERAGE_HELP: &str = "\
Usage: snappea coverage merge [DIR]
       snappea coverage merge-observe [REPORT.json ...] [--stdin] [--json-report OUT] [--json]

Combines coverage from separate runs, e.g. CI shards or one observe per process, to
see what the whole suite covers.

merge         Merges the scenario coverage reports (scenario_coverage.json and
              *_scenario_coverage.json) under DIR (default: the current folder),
              recursively, into one report.
merge-observe Merges `snappea observe` JSON results without the P sources. The
              results must come from the same spec sources and snappea version.

Examples:
  snappea coverage merge PCheckerOutput
  snappea coverage merge-observe reports/*.json --json-report suite.json

Run `snappea coverage merge-observe --help` for its options.
";

/// Runs `snappea` with `args` (without the program name); returns the exit status.
pub fn run(args: &[String], host: &mut dyn Host) -> i32 {
    // Recover output options even when another option is invalid. Skip values of
    // command options so a filename such as `--json` is never mistaken for a flag.
    let mut output = Args::default();
    let mut command = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--" {
            command.push(arg.clone());
            command.extend(it.cloned());
            break;
        }
        let (name, inline) = arg
            .strip_prefix("--")
            .unwrap_or(arg)
            .split_once('=')
            .map_or((arg.strip_prefix("--").unwrap_or(arg), None), |(n, v)| (n, Some(v)));
        if let Some(opt) = OUTPUT_OPTS.iter().find(|o| arg.starts_with("--") && o.long == name) {
            let mut tokens = vec![arg.clone()];
            if opt.takes_value && inline.is_none() {
                if let Some(v) = it.next() {
                    tokens.push(v.clone());
                }
            }
            // Leave malformed options for the regular parser to diagnose.
            match parse_args(&tokens, &[]) {
                Ok(parsed) => output.options.extend(parsed.options),
                Err(_) => command.extend(tokens),
            }
        } else {
            command.push(arg.clone());
            let spec = match command.first().map(String::as_str) {
                Some("check") => CHECK_OPTS,
                Some("replay") => REPLAY_OPTS,
                Some("observe") => super::observe::OPTS,
                _ => &[],
            };
            let valued = spec.iter().any(|o| {
                o.takes_value && (arg == &format!("--{}", o.long) || o.short.is_some_and(|c| arg == &format!("-{}", c)))
            });
            if valued && inline.is_none() {
                if let Some(v) = it.next() {
                    command.push(v.clone());
                }
            }
        }
    }
    // Keep output options away from a trailing command option missing its value.
    let nested = |c: &str, sub: &[&str]| {
        command.first().is_some_and(|f| f == c) && command.get(1).is_some_and(|s| sub.contains(&s.as_str()))
    };
    let at = if nested("coverage", &["merge-observe"]) || nested("check", &["lint", "mutation"]) {
        2
    } else {
        usize::from(!command.is_empty())
    };
    let tokens =
        output.options.iter().map(
            |(name, value)| {
                if *name == "json" {
                    "--json".to_string()
                } else {
                    format!("--{}={}", name, value)
                }
            },
        );
    command.splice(at..at, tokens);
    let mut capture = Quiet::recording_errors(host);
    let code = dispatch(&command, &mut capture);
    // Only usage errors bypass the command's structured output wrapper.
    let message =
        capture.captured_err.lines().next().unwrap_or("invalid command line").trim_start_matches("error: ").to_string();
    drop(capture);
    if code == USAGE_ERROR {
        let format = output_format(&output, host).unwrap_or_else(|_| {
            if output.has("json") || output.get("format") == Some("json") {
                Format::Json
            } else {
                output_format(&Args::default(), host).unwrap_or(Format::Json)
            }
        });
        let text = serde_json::to_string_pretty(&json!({"exit_code": ERROR, "error": message})).unwrap() + "\n";
        if format == Format::Json {
            host.write(Stream::Out, &text, None);
        }
        if let Some(path) = output.get("json-report") {
            if let Err(e) = host.write_file(&full_path(&host.cwd(), Path::new(path)), &text) {
                error(host, &format!("cannot write {}: {}", path, e));
            }
        }
    }
    if code == USAGE_ERROR {
        ERROR
    } else {
        code
    }
}

/// The commands [`HELP`] lists, for `snappea help <command>` and unknown-command errors.
const COMMANDS: [&str; 7] = ["check", "replay", "tests", "observe", "coverage", "guide", "version"];

fn dispatch(args: &[String], host: &mut dyn Host) -> i32 {
    let Some((cmd, rest)) = args.split_first() else {
        host.write(Stream::Out, HELP, None);
        return OK;
    };
    match cmd.as_str() {
        "check" => check(rest, host),
        "replay" => replay(rest, host),
        "tests" => tests(rest, host),
        "observe" => super::observe::observe(rest, host),
        "coverage" => coverage(rest, host),
        "guide" => super::guide::guide(rest, host),
        "version" | "--version" | "-V" => {
            let a = match parse_args(rest, &[]) {
                Ok(a) => a,
                Err(m) => return usage_error(host, &m, Some("version")),
            };
            if a.help {
                host.write(Stream::Out, &format!("{}\n{}", VERSION_HELP, OUTPUT_HELP), None);
                return OK;
            }
            with_output(host, &a, "version", |host| {
                host.out_line(&format!("snappea {}", env!("CARGO_PKG_VERSION")));
                Ok((OK, json!({"version": env!("CARGO_PKG_VERSION")})))
            })
        }
        "help" | "--help" | "-h" => match rest.first().map(String::as_str) {
            None => {
                host.write(Stream::Out, HELP, None);
                OK
            }
            Some(c) if COMMANDS.contains(&c) => dispatch(&[c.to_string(), "--help".into()], host),
            Some(c) => usage_error(host, &unknown_command(c), None),
        },
        c => usage_error(host, &unknown_command(c), None),
    }
}

/// Reports an argument error; run() writes its structured result and exits 2.
pub(super) fn usage_error(host: &mut dyn Host, msg: &str, cmd: Option<&str>) -> i32 {
    error(host, msg);
    let help = cmd.map_or("snappea --help".to_string(), |c| format!("snappea {} --help", c));
    host.write(Stream::Err, &format!("Run `{}` for usage.\n", help), None);
    USAGE_ERROR
}

pub(super) fn error(host: &mut dyn Host, msg: &str) {
    host.write(Stream::Err, "error: ", Some(Color::Red));
    host.write(Stream::Err, msg, None);
    host.write(Stream::Err, "\n", None);
}

// ---- options ----------------------------------------------------------------------------

/// An option a command accepts.
pub(super) struct Opt {
    long: &'static str,
    short: Option<char>,
    takes_value: bool,
}

pub(super) const fn flag(long: &'static str, short: Option<char>) -> Opt {
    Opt { long, short, takes_value: false }
}

pub(super) const fn value(long: &'static str, short: Option<char>) -> Opt {
    Opt { long, short, takes_value: true }
}

/// Parsed arguments: positionals, and options (long name, value) in order.
#[derive(Default)]
pub(super) struct Args {
    pub positional: Vec<String>,
    options: Vec<(&'static str, String)>,
    pub help: bool,
}

impl Args {
    pub fn has(&self, long: &str) -> bool {
        self.options.iter().any(|(l, _)| *l == long)
    }
    /// The last value given for `long`.
    pub fn get(&self, long: &str) -> Option<&str> {
        self.options.iter().rev().find(|(l, _)| *l == long).map(|(_, v)| v.as_str())
    }
    /// Every value given for `long`, in order.
    pub fn all(&self, long: &str) -> Vec<&str> {
        self.options.iter().filter(|(l, _)| *l == long).map(|(_, v)| v.as_str()).collect()
    }
}

/// Options that were removed in favor of another spelling, and that spelling.
const RENAMED: &[(&str, &str)] = &[("--time", "--timeout"), ("--allow-incomplete-coverage", "--warn coverage")];

pub(super) fn parse_args(args: &[String], spec: &[Opt]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.iter();
    let mut only_positional = false;
    while let Some(a) = it.next() {
        if only_positional || a == "-" || !a.starts_with('-') {
            out.positional.push(a.clone());
            continue;
        }
        if a == "--" {
            only_positional = true;
            continue;
        }
        if a == "-h" || a == "--help" {
            out.help = true;
            continue;
        }
        let (opt, inline) = if let Some(long) = a.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            (spec.iter().chain(OUTPUT_OPTS).find(|o| o.long == name), inline)
        } else {
            // `-j 4` or `-j4`
            let mut chars = a[1..].chars();
            let c = chars.next();
            let rest = chars.as_str();
            let opt =
                spec.iter().chain(OUTPUT_OPTS).find(|o| o.short == c).filter(|o| rest.is_empty() || o.takes_value);
            (opt, (!rest.is_empty()).then(|| rest.to_string()))
        };
        let Some(opt) = opt else {
            let name = a.split('=').next().unwrap_or(a);
            return Err(match RENAMED.iter().find(|(old, _)| *old == name) {
                Some((_, new)) => format!("unknown option '{}': it was renamed, use '{}'", name, new),
                None => {
                    let longs = spec.iter().chain(OUTPUT_OPTS).map(|o| o.long).filter(|l| !l.is_empty());
                    match crate::syntax::closest(name.trim_start_matches('-'), longs) {
                        Some(l) => format!("unknown option '{}'; did you mean '--{}'?", a, l),
                        None => format!("unknown option '{}'", a),
                    }
                }
            });
        };
        let v = match (opt.takes_value, inline) {
            (true, Some(v)) => v,
            (true, None) => it.next().cloned().ok_or_else(|| format!("'{}' needs a value", a))?,
            (false, None) => String::new(),
            (false, Some(_)) => return Err(format!("'--{}' takes no value", opt.long)),
        };
        out.options.push((opt.long, v));
    }
    Ok(out)
}

// ---- output format ----------------------------------------------------------------------

/// Options every command takes: how it prints its result.
const OUTPUT_OPTS: &[Opt] = &[value("format", None), flag("json", None), value("json-report", None)];

/// The output options, for help texts.
pub(super) const OUTPUT_HELP: &str = "\
Output format:
      --format <F>          text or json. Default: text, or SNAPPEA_FORMAT if set.
      --json                Same as --format json
      --json-report <FILE>  Also write the JSON result to FILE
";

/// How a command prints its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Format {
    Text,
    Json,
}

/// `--json` or `--format`; else `SNAPPEA_FORMAT`; else text.
pub(super) fn output_format(a: &Args, host: &dyn Host) -> Result<Format, String> {
    let parse = |v: &str, from: &str| match v {
        "text" => Ok(Format::Text),
        "json" => Ok(Format::Json),
        _ => Err(format!("{} must be text or json, not '{}'", from, v)),
    };
    if a.has("json") {
        return match a.get("format") {
            Some(f) if f != "json" => Err(format!("'--json' and '--format {}' conflict", f)),
            _ => Ok(Format::Json),
        };
    }
    if let Some(f) = a.get("format") {
        return parse(f, "'--format'");
    }
    if let Some(f) = host.env_var("SNAPPEA_FORMAT").filter(|v| !v.is_empty()) {
        return parse(&f, "SNAPPEA_FORMAT");
    }
    Ok(Format::Text)
}

/// `--progress`: how `check` shows a running check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProgressMode {
    Auto,
    Off,
    Json,
}

fn progress_mode(a: &Args) -> Result<ProgressMode, String> {
    match a.get("progress") {
        None | Some("auto") => Ok(ProgressMode::Auto),
        Some("off") => Ok(ProgressMode::Off),
        Some("json") => Ok(ProgressMode::Json),
        Some(v) => Err(format!("'--progress' takes auto, off or json, not '{}'", v)),
    }
}

/// How progress is shown: with `auto`, a status line when stderr is a terminal, a line
/// every 30 seconds in CI, and nothing otherwise or with `--verbose` (which logs every
/// schedule instead).
fn progress_view(a: &Args, host: &dyn Host) -> Option<ProgressView> {
    match progress_mode(a).ok()? {
        ProgressMode::Off => None,
        ProgressMode::Json => Some(ProgressView::Json),
        ProgressMode::Auto if a.has("verbose") => None,
        ProgressMode::Auto if host.stderr_is_terminal() => Some(ProgressView::Live { shown: false }),
        ProgressMode::Auto if is_ci(host) => Some(ProgressView::Heartbeat { printed_at: None }),
        ProgressMode::Auto => None,
    }
}

/// Whether the `CI` environment variable says this runs in CI.
fn is_ci(host: &dyn Host) -> bool {
    host.env_var("CI").is_some_and(|v| !matches!(v.as_str(), "" | "0" | "false"))
}

/// A command's exit status and result, or the status of a failure it has reported.
pub(super) type Outcome = Result<(i32, Value), Exit>;

/// Runs a command `body`, which prints text and returns its result. With JSON output the
/// text is held back and the result printed instead, with its `exit_code`; a failure it
/// has reported exits with 2, and prints `{"exit_code", "error"}` with what it reported. `--json-report FILE` gets the
/// JSON in either format.
pub(super) fn with_output(
    host: &mut dyn Host,
    a: &Args,
    cmd: &str,
    body: impl FnOnce(&mut dyn Host) -> Outcome,
) -> i32 {
    let format = match output_format(a, host) {
        Ok(f) => f,
        Err(m) => return usage_error(host, &m, Some(cmd)),
    };
    let progress = progress_view(a, host);
    let mut capture = match format {
        Format::Text => Quiet::recording_errors(host),
        Format::Json => Quiet::capturing(host),
    };
    // progress goes straight to stderr, never into the captured errors
    capture.progress = progress;
    let outcome = body(&mut capture);
    let (code, mut result) = outcome.unwrap_or_else(|_| {
        // what the command reported: its errors, else what it printed
        let err = std::mem::take(&mut capture.captured_err);
        let out = capture.captured.take().unwrap_or_else(|| std::mem::take(&mut capture.shown_out));
        let text = if err.trim().is_empty() { out } else { err };
        let text = text.trim();
        (ERROR, json!({"error": text.strip_prefix("error: ").unwrap_or(text)}))
    });
    drop(capture);
    if !result.is_object() {
        result = json!({"result": result});
    }
    result["exit_code"] = json!(code);
    let text = serde_json::to_string_pretty(&result).expect("JSON values serialize") + "\n";
    if format == Format::Json {
        host.write(Stream::Out, &text, None);
    }
    if let Some(path) = a.get("json-report") {
        if let Err(e) = host.write_file(&full_path(&host.cwd(), Path::new(path)), &text) {
            error(host, &format!("cannot write {}: {}", path, e));
            return ERROR;
        }
    }
    code
}

pub(super) fn uint(args: &Args, long: &str) -> Result<Option<usize>, String> {
    args.get(long)
        .map(|v| v.parse::<usize>().map_err(|_| format!("'--{}' needs a non-negative whole number, not '{}'", long, v)))
        .transpose()
}

/// A duration in seconds: `90`, `90s`, `5m` or `1h`.
fn parse_duration(s: &str) -> Option<u32> {
    let (num, unit) = match s.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, _)) => (&s[..i], &s[i..]),
        None => (s, "s"),
    };
    let n: u32 = num.parse().ok()?;
    let scale = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        _ => return None,
    };
    n.checked_mul(scale).filter(|&t| t > 0)
}

/// `--latency FILE[:PROFILE]`. The profile follows the last `:`, unless that `:` belongs to
/// the path (as in `C:\x.toml`).
fn split_latency(s: &str) -> (&str, Option<&str>) {
    match s.rsplit_once(':') {
        Some((file, profile))
            if !file.is_empty() && !profile.is_empty() && !profile.contains(['/', '\\']) && file.len() > 1 =>
        {
            (file, Some(profile))
        }
        _ => (s, None),
    }
}

fn is_source(arg: &str) -> bool {
    let l = arg.to_lowercase();
    l.ends_with(".pproj") || l.ends_with(".p")
}

/// `unknown command 'chek'; did you mean 'check'?`, with the commands.
fn unknown_command(c: &str) -> String {
    let mut m = format!("unknown command '{}'", c);
    if let Some(s) = crate::syntax::closest(c, COMMANDS) {
        m.push_str(&format!("; did you mean '{}'?", s));
    }
    m.push_str(&format!("{}{}", if m.ends_with('?') { " Commands: " } else { "; commands: " }, COMMANDS.join(", ")));
    m
}

/// Splits positionals into the sources (`.pproj`, `.p`) and the rest.
fn sources(cfg: &mut CheckConfig, positional: &[String]) -> Result<Vec<String>, String> {
    let mut rest = Vec::new();
    for a in positional {
        if a.to_lowercase().ends_with(".pproj") {
            if let Some(p) = &cfg.input.pproj {
                return Err(format!("two project files given: '{}' and '{}'; pass one .pproj", p, a));
            }
            cfg.input.pproj = Some(a.clone());
        } else if is_source(a) {
            cfg.input.pfiles.push(a.clone());
        } else {
            rest.push(a.clone());
        }
    }
    Ok(rest)
}

/// Whether `name` matches the glob `pattern` (`*`: any characters, `?`: one).
fn glob_match(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// The test cases matching `patterns` (all for none), in declaration order.
fn select_tests(names: &[String], patterns: &[String]) -> Result<Vec<String>, String> {
    if patterns.is_empty() {
        return Ok(names.to_vec());
    }
    for p in patterns {
        if !names.iter().any(|n| glob_match(p, n)) {
            let mut m = format!("no test case matches '{}'. Test cases: {}", p, names.join(", "));
            if let Some(s) = crate::syntax::closest(p, names.iter().map(String::as_str)) {
                m = format!("no test case matches '{}'; did you mean '{}'? Test cases: {}", p, s, names.join(", "));
            }
            if p.contains('/') || p.contains('.') {
                m.push_str(&format!(
                    ". '{}' is read as a test case name; to check files, pass .p or .pproj paths, e.g. `snappea check src/*.p`",
                    p
                ));
            }
            return Err(m);
        }
    }
    Ok(names.iter().filter(|n| patterns.iter().any(|p| glob_match(p, n))).cloned().collect())
}

// ---- commands -----------------------------------------------------------------------------

const LINT_HELP: &str = "\
Usage: snappea check lint [SOURCE...] [--strict] [--list-rules]

Type-checks and lints .p or .pproj sources without exploring schedules or writing
checker output. Fast; use it while editing. The same rules run before `snappea check`.
Exit status: 0 with no blocking findings, 2 blocking findings or source errors.
Advisory rules are reported, and fail, only with --strict.

  --strict      Also report advisory rules (`--list-rules --json` marks them) and fail on them
  --list-rules  List available rule IDs and descriptions without loading sources

Each finding says what is wrong, where, and how to fix it. Fix it first; suppress
only an intended one, on its own line before the declaration or statement the
finding points at:
  // snappea-lint: allow unused-payload-field -- the spec checks counts, not amounts
With --strict, a suppression that matches no finding is reported as `unused-suppression`.
`snappea check TEST` lints only what the selected tests need; run this command to
see every finding.

Examples:
  snappea check lint
  snappea check lint Model.pproj
  snappea check lint --list-rules
";

const LINT_OPTS: &[Opt] = &[flag("list-rules", None), flag("strict", None)];

fn lint(args: &[String], host: &mut dyn Host) -> i32 {
    let a = match parse_args(args, LINT_OPTS) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, Some("check lint")),
    };
    if a.help {
        host.write(Stream::Out, &format!("{}\n{}", LINT_HELP, OUTPUT_HELP), None);
        return OK;
    }
    if a.has("list-rules") && !a.positional.is_empty() {
        return usage_error(host, "--list-rules does not take sources", Some("check lint"));
    }
    let mut cfg = snappea_config();
    // nothing runs, so foreign code is not left out of a check
    cfg.ignore_foreign_files = true;
    match sources(&mut cfg, &a.positional) {
        Ok(rest) if rest.is_empty() => {}
        Ok(rest) => {
            let m = format!(
                "'{}' is not a .p or .pproj file; lint takes sources only, e.g. `snappea check lint src/*.p`",
                rest[0]
            );
            return usage_error(host, &m, Some("check lint"));
        }
        Err(m) => return usage_error(host, &m, Some("check lint")),
    }
    with_output(host, &a, "check lint", |host| {
        if a.has("list-rules") {
            let builtins = snappea_lint::Registry::builtins();
            let all: Vec<(&str, &str)> = builtins
                .rules()
                .map(|r| (r.id(), r.description()))
                .chain(snappea_lint::PROGRAM_RULES.iter().copied())
                .chain(snappea_lint::COMMENT_RULES.iter().copied())
                .collect();
            let rules: Vec<_> = all
                .iter()
                .map(|(id, description)| {
                    host.out_line(&format!("{}  {}", id, description));
                    json!({"id": id, "description": description, "severity": if snappea_lint::is_advisory(id) { "advisory" } else { "error" }})
                })
                .collect();
            return Ok((OK, json!({"rules": rules})));
        }
        let mut quiet = Buffered::new(host);
        let loaded = check::resolve_sources(&mut quiet, &mut cfg).and_then(|_| check::load(&mut quiet, &cfg));
        let Buffered { out, .. } = quiet;
        match loaded {
            Ok(loaded) => {
                let sm = loaded.sources.source_map();
                let strict = a.has("strict");
                let findings = crate::driver::lint::shown(
                    crate::driver::lint::findings(host, &sm, &loaded.analysis.program)?,
                    strict,
                );
                let errors = crate::driver::lint::report(host, &sm, &findings, strict);
                let json = crate::driver::lint::findings_json(&sm, &findings);
                if errors != 0 {
                    // the findings, not their text again: stderr has that
                    return Ok((ERROR, json!({"error": crate::driver::lint::SUMMARY, "findings": json})));
                }
                host.out_line("Lint passed.");
                Ok((OK, json!({"findings": json})))
            }
            Err(e) => {
                for (stream, text, color) in out {
                    host.write(stream, &text, color);
                }
                Err(e)
            }
        }
    })
}

const CHECK_OPTS: &[Opt] = &[
    flag("inspect-trace", None),
    value("timeout", None),
    value("workers", Some('j')),
    value("seed", None),
    value("expect-bug-seeds", None),
    value("max-bugs", None),
    value("latency", None),
    flag("timed", None),
    value("out", Some('o')),
    flag("verbose", Some('v')),
    flag("list-suppressions", None),
    value("mutation-timeout", None),
    flag("no-mutation", None),
    value("warn", None),
    value("deny", None),
    flag("no-driver-facts", None),
    flag("strict", None),
    flag("ignore-foreign-files", None),
    value("progress", None),
];

/// Default time limit for mutation testing, in seconds.
const MUTATION_TIMEOUT: u32 = 30;

fn check(args: &[String], host: &mut dyn Host) -> i32 {
    // a test case named `lint` or `mutation` is checked with a glob: `lint*`
    match args.first().map(String::as_str) {
        Some("lint") => return lint(&args[1..], host),
        Some("mutation") => return mutation(&args[1..], host),
        _ => {}
    }
    let parsed = match parse_args(args, CHECK_OPTS) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, Some("check")),
    };
    if parsed.help {
        host.write(Stream::Out, &format!("{}\n{}", CHECK_HELP, OUTPUT_HELP), None);
        return OK;
    }
    let mut cfg = snappea_config();
    let patterns = match check_config(&parsed, &mut cfg) {
        Ok(p) => p,
        Err(m) => return usage_error(host, &m, Some("check")),
    };
    let mutation = match mutation_timeout(&parsed) {
        Ok(t) => t,
        Err(m) => return usage_error(host, &m, Some("check")),
    };
    with_output(host, &parsed, "check", |host| {
        run_check(host, cfg, &patterns, parsed.get("latency"), Mutation::After(mutation))
    })
}

/// Default time limit of `check mutation`, in seconds.
const MUTATION_ONLY_TIMEOUT: u32 = 300;

fn mutation(args: &[String], host: &mut dyn Host) -> i32 {
    const CMD: Option<&str> = Some("check mutation");
    let parsed = match parse_args(args, CHECK_OPTS) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, CMD),
    };
    if parsed.help {
        host.write(Stream::Out, &format!("{}\n{}", MUTATION_HELP, OUTPUT_HELP), None);
        return OK;
    }
    if parsed.has("no-mutation") {
        return usage_error(host, "'--no-mutation' does not apply to `check mutation`", CMD);
    }
    let mut cfg = snappea_config();
    cfg.gates.mutation = Gate::Deny;
    let patterns = match check_config(&parsed, &mut cfg) {
        Ok(p) => p,
        Err(m) => return usage_error(host, &m, CMD),
    };
    let limit = match mutation_timeout(&parsed) {
        Ok(Some(t)) if parsed.get("mutation-timeout").is_some() => t,
        Ok(_) => MUTATION_ONLY_TIMEOUT,
        Err(m) => return usage_error(host, &m, CMD),
    };
    with_output(host, &parsed, "check mutation", |host| {
        run_check(host, cfg, &patterns, parsed.get("latency"), Mutation::Only(limit))
    })
}

/// Whether and how a check runs mutation testing (time limits in seconds).
#[derive(Debug, Clone, Copy)]
enum Mutation {
    /// After the check's report, unless `None` (`--no-mutation`).
    After(Option<u32>),
    /// `check mutation`: instead of the coverage and scenario reports.
    Only(u32),
}

/// The time limit for mutation testing in seconds, `None` to skip it.
fn mutation_timeout(a: &Args) -> Result<Option<u32>, String> {
    match (a.has("no-mutation"), a.get("mutation-timeout")) {
        (true, Some(_)) => Err("use only one of '--no-mutation' and '--mutation-timeout'".into()),
        (true, None) => Ok(None),
        (false, None) => Ok(Some(MUTATION_TIMEOUT)),
        (false, Some(t)) => match parse_duration(t) {
            Some(t) => Ok(Some(t)),
            None => Err(format!("bad duration '{}' (examples: 90, 90s, 5m, 1h)", t)),
        },
    }
}

/// Fills `cfg` from the `check` options; returns the test patterns.
fn check_config(a: &Args, cfg: &mut CheckConfig) -> Result<Vec<String>, String> {
    let patterns = sources(cfg, &a.positional)?;
    cfg.strategy = StrategyKind::Auto;
    cfg.iterations = 0;
    cfg.user_set_iterations = false;
    if let Some(t) = a.get("timeout") {
        cfg.timeout =
            Some(parse_duration(t).ok_or_else(|| format!("bad duration '{}' (examples: 90, 90s, 5m, 1h)", t))?);
        cfg.suite_budget = None;
    }
    cfg.workers = match uint(a, "workers")? {
        Some(0) => return Err("'--workers' needs at least 1 worker".into()),
        n => n,
    };
    if let Some(s) = a.get("seed") {
        cfg.seed =
            Some(s.parse().map_err(|_| format!("'--seed' needs a whole number from 0 to 4294967295, not '{}'", s))?);
    }
    cfg.max_bugs = match uint(a, "max-bugs")? {
        Some(0) => return Err("'--max-bugs' needs at least 1 (1 stops each test case at its first bug)".into()),
        Some(n) => n,
        None => MAX_BUGS,
    };
    if let Some(seeds) = a.get("expect-bug-seeds") {
        cfg.expected_bug_seeds = seeds
            .split(',')
            .map(|s| s.trim().parse::<u32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "'--expect-bug-seeds' needs comma-separated seed numbers, e.g. 1,2,3".to_string())?;
        let distinct: std::collections::BTreeSet<_> = cfg.expected_bug_seeds.iter().collect();
        if !(2..=32).contains(&cfg.expected_bug_seeds.len()) || distinct.len() != cfg.expected_bug_seeds.len() {
            return Err("'--expect-bug-seeds' needs 2 to 32 distinct seeds".into());
        }
    }
    if let Some(l) = a.get("latency") {
        cfg.latency_profile = split_latency(l).1.map(str::to_string);
    }
    cfg.timed = a.has("timed");
    cfg.inspection_trace = a.has("inspect-trace");
    if let Some(o) = a.get("out") {
        cfg.output_path = o.to_string();
    }
    cfg.verbose = a.has("verbose");
    cfg.list_suppressions = a.has("list-suppressions");
    cfg.driver_facts = !a.has("no-driver-facts");
    cfg.gates.strict = a.has("strict");
    cfg.ignore_foreign_files = a.has("ignore-foreign-files");
    cfg.progress = (progress_mode(a)? != ProgressMode::Off).then(Position::default);
    gates(a, &mut cfg.gates)?;
    check::finish_options(cfg);
    Ok(patterns)
}

/// Applies `--warn` and `--deny` (each repeatable, or a comma list) to `gates`: `all`
/// first, then single checks, so `--warn all --deny mutation` warns on all but mutation.
fn gates(a: &Args, gates: &mut check::Gates) -> Result<(), String> {
    let mut named: Vec<(&str, Gate)> = Vec::new();
    for (option, gate) in [("warn", Gate::Warn), ("deny", Gate::Deny)] {
        for name in a.all(option).into_iter().flat_map(|v| v.split(',')).map(str::trim) {
            if name != "all" && !check::Gates::CHECKS.contains(&name) {
                return Err(format!(
                    "unknown check '{}' for '--{}': use lint, coverage, mutation or all",
                    name, option
                ));
            }
            named.push((name, gate));
        }
    }
    for (name, gate) in &named {
        if named.iter().any(|(n, g)| n == name && g != gate) {
            return Err(format!("'--warn {0}' and '--deny {0}' conflict: use one", name));
        }
    }
    if a.has("no-mutation") && named.iter().any(|&(n, g)| n == "mutation" && g == Gate::Deny) {
        return Err("use only one of '--no-mutation' and '--deny mutation'".into());
    }
    named.sort_by_key(|(n, _)| *n != "all");
    for (name, gate) in named {
        gates.set(name, gate)?;
    }
    Ok(())
}

/// `replay`'s `--max-steps`, `--fair-max-steps` and `--fail-on-max-steps`, for schedules
/// that `snappea-compat check` found with those bounds: a schedule replays only with the
/// step bounds it was found with.
fn step_options(a: &Args, cfg: &mut CheckConfig) -> Result<(), String> {
    if let Some(n) = uint(a, "max-steps")? {
        cfg.max_unfair_steps = n;
        cfg.max_fair_steps = n.saturating_mul(10);
    }
    if let Some(n) = uint(a, "fair-max-steps")? {
        cfg.max_fair_steps = n;
        cfg.user_set_max_fair_steps = true;
    }
    if cfg.max_fair_steps < cfg.max_unfair_steps {
        return Err(format!(
            "'--fair-max-steps' ({}) must be at least '--max-steps' ({}); raise it, or leave it out to use 10 x '--max-steps'",
            cfg.max_fair_steps, cfg.max_unfair_steps
        ));
    }
    cfg.fail_on_max_steps = a.has("fail-on-max-steps");
    Ok(())
}

/// Distinct failures `snappea check` collects per test case without `--max-bugs`.
const MAX_BUGS: usize = 3;

/// Seconds the test cases of a `snappea check` share without `--timeout`.
const SUITE_BUDGET: f64 = 10.0;
/// Least time a test case gets from the shared budget.
const MIN_SHARE: f64 = 0.25;

/// A [`CheckConfig`] for snappea: its output folder layout, and `<IncludeProject>` paths
/// resolved against the including project file.
fn snappea_config() -> CheckConfig {
    let mut cfg = CheckConfig {
        style: Style::Snappea,
        code_coverage: true,
        driver_facts: true,
        strategy: StrategyKind::Auto,
        iterations: 0,
        timeout: Some(10),
        suite_budget: Some(SUITE_BUDGET),
        max_bugs: MAX_BUGS,
        ..CheckConfig::default()
    };
    cfg.input.includes = Includes::FromProject;
    cfg
}

fn run_check(
    host: &mut dyn Host,
    mut cfg: CheckConfig,
    patterns: &[String],
    latency: Option<&str>,
    mutation: Mutation,
) -> Outcome {
    let only = matches!(mutation, Mutation::Only(_));
    let start = host.now();
    // the upstream-style progress shows only with --verbose
    let mut con = Quiet::new(host, !cfg.verbose);
    if let Some(l) = latency {
        cfg.latency = Some(check::load_latency(&mut con, split_latency(l).0)?);
    }
    check::resolve_sources(&mut con, &mut cfg)?;
    let loaded = check::load(&mut con, &cfg)?;
    let gates = cfg.gates;

    let names: Vec<String> = loaded.analysis.program.test_cases().into_iter().map(|t| t.name).collect();
    let selected = select_tests(&names, patterns).map_err(|m| {
        error(con.inner, &m);
        Exit(ERROR)
    })?;
    if selected.is_empty() {
        error(con.inner, "the sources declare no test case; add one, e.g. `test tcMain [main=Main]: { Main };`");
        return Err(Exit(ERROR));
    }
    let all_tests = loaded.analysis.program.test_cases();
    let selected_tests: Vec<_> = all_tests.iter().filter(|t| selected.contains(&t.name)).cloned().collect();
    let intents = snappea_lint::test_intents(&loaded.sources.source_map()).unwrap_or_default();
    // Blocking lint findings fail the check, but the test cases still run so their bugs
    // are reported, and mutation testing runs when none failed, so one run lists every
    // gap; coverage is skipped. The findings print after the test cases.
    let (lint_findings, lint_failed) = match crate::driver::lint::gate_selected(
        con.inner,
        &loaded.sources,
        &loaded.analysis.program,
        gates.lint,
        gates.strict,
        &selected_tests,
        &intents,
    ) {
        Ok(n) => (n, None),
        Err(e) if e.findings.is_some() => (0, Some(e)),
        Err(e) => return Err(e.exit),
    };
    // what lint failing skips, as reported in the JSON result
    const LINT_SKIP: &str = "lint failed";
    if !cfg.expected_bug_seeds.is_empty()
        && !selected_tests.iter().any(|tc| {
            crate::driver::lint::intent(&loaded.analysis.program, tc, &intents)
                .is_some_and(|i| i.expected_bug.is_some())
        })
    {
        error(con.inner, "--expect-bug-seeds requires a selected test with a snappea: expect-bug marker");
        return Err(Exit(ERROR));
    }
    let mut planned = Vec::new();
    for name in selected {
        let tc = selected_tests.iter().find(|t| t.name == name).unwrap();
        let negative = crate::driver::lint::intent(&loaded.analysis.program, tc, &intents)
            .is_some_and(|i| i.expected_bug.is_some());
        if negative && !cfg.expected_bug_seeds.is_empty() {
            planned.extend(cfg.expected_bug_seeds.iter().map(|&seed| (name.clone(), Some(seed))));
        } else {
            planned.push((name, None));
        }
    }
    let n = planned.len();
    con.inner.out_line(&format!("Checking {}: {} test case{}", cfg.project_name, n, plural(n)));
    let mut failed = 0;
    let prog = &loaded.analysis.program;
    let probes = crate::checker::compile::compile_program_with_probes(prog).probes;
    let (mut scope, mut hits) = (Scope::default(), Hits::default());
    let mut selected_scope = Scope::default();
    let mut positive_scope = Scope::default();
    let mut cases = Vec::new();
    let mut scenarios = Vec::new();
    let mut incomplete = 0;
    // incomplete searches that did not settle either
    let mut unsettled = 0;
    let sources = loaded.sources.source_map();
    let marked = snappea_lint::driver_machines(&sources).unwrap_or_default();
    let drivers = crate::checker::driver_facts::driver_machines(prog, &sources, &marked);
    let (mut negative_driver_scope, mut negative_hits, mut positive_design) =
        (Scope::default(), Hits::default(), Scope::default());
    for tc in &selected_tests {
        if crate::driver::lint::intent(prog, tc, &intents).is_none_or(|i| i.expected_bug.is_none()) {
            positive_scope.add(&Scope::test_case(prog, tc));
            positive_design.add(&Scope::partition_drivers(prog, tc, &drivers).1);
        }
    }
    let mut passed = Vec::new();
    // The driver-limits hint is the same for every test case; say it once per run.
    let mut driver_hint_shown = false;
    let check_start = con.inner.now();
    let (base_seed, base_max_bugs) = (cfg.seed, cfg.max_bugs);
    let mut seed_runs: std::collections::BTreeMap<String, Vec<SeedFailure>> = Default::default();
    for (i, (name, seed)) in planned.into_iter().enumerate() {
        cfg.test_case_name = name;
        cfg.seed = seed.or(base_seed);
        cfg.seed_artifact = seed;
        // the shared budget left, split between the test cases left
        cfg.time_share = cfg.suite_budget.map(|b| {
            let left = b - (con.inner.now() - check_start);
            (left / (n - i) as f64).max(MIN_SHARE)
        });
        cfg.progress = cfg.progress.map(|_| Position { test: i + 1, tests: n });
        // an expected-bug test passes on its first matching bug: searching on would only
        // spend the budget the other test cases share
        let negative = selected_tests.iter().find(|t| t.name == cfg.test_case_name).is_some_and(|tc| {
            crate::driver::lint::intent(prog, tc, &intents).is_some_and(|i| i.expected_bug.is_some())
        });
        cfg.max_bugs = if negative { 1 } else { base_max_bugs };
        check::set_output_directory(&mut con, &mut cfg)?;
        let case = check::testing_process(&mut con, &loaded, &cfg)?;
        let tc = all_tests.iter().find(|t| t.name == case.test_case).unwrap();
        let intent = crate::driver::lint::intent(prog, tc, &intents);
        let expected = intent.and_then(|i| i.expected_bug.as_deref());
        let boundary = intent.is_some_and(|i| i.boundary.is_some());
        let matched = expected.is_some_and(|text| {
            case.bugs > 0 && case.bug_report.as_deref().is_some_and(|r| failure_message(r, &sources).contains(text))
        });
        if let Some(seed) = seed {
            seed_runs.entry(case.test_case.clone()).or_default().push(SeedFailure {
                seed,
                matched,
                identity: case.bug_identity.clone(),
            });
        }
        if expected.is_some() && !matched || expected.is_none() && case.bugs > 0 {
            failed += 1;
        } else if expected.is_none() && case.exhaustive_finished != Some(true) {
            incomplete += 1;
            unsettled += usize::from(verdict(&case) != Verdict::Settled);
        }
        if let Some(expected) = expected {
            con.inner.out_line(&format!(
                "{}  {}{} (expected bug: {:?})",
                if matched { "ok  " } else { "FAIL" },
                case.test_case,
                seed.map(|s| format!(" [seed {s}]")).unwrap_or_default(),
                expected
            ));
            if !matched {
                let reason = if case.bugs > 0 {
                    "the observed failure did not match"
                } else if case.exhaustive_finished == Some(true) {
                    "search complete, no bug: this test cannot reach the expected flaw"
                } else {
                    "search incomplete, no matching bug found within the run budget"
                };
                con.inner.out_line(&format!("      {reason}; check the test and expected failure text"));
            }
            if let Some(report) = &case.bug_report {
                for l in report.lines() {
                    con.inner.out_line(&format!("      {l}"));
                }
            }
            for l in hot_spec_lines(case.bug_identity.as_ref(), &case.hot_specs) {
                con.inner.out_line(&format!("      {l}"));
            }
            if let Some(schedule) = &case.schedule_file {
                con.inner.out_line(&format!("      Replay: {}", replay_command(schedule, &cfg)));
            }
            if matched && case.schedules == 1 {
                con.inner.out_line("      note: the expected bug appeared in the first schedule; check that the control exercises the intended flaw");
            }
        } else {
            print_case(con.inner, &case, &cfg, boundary);
        }
        let mut result = case_json(&case, &cfg, boundary);
        if let Some(seed) = seed {
            result["seed"] = json!(seed);
        }
        if let Some(text) = expected {
            result["result"] = json!(if matched { "expected_bug" } else { "expectation_failed" });
            let outcome = if matched {
                "matched"
            } else if case.bugs > 0 {
                "wrong_failure"
            } else if case.exhaustive_finished == Some(true) {
                "unreachable"
            } else {
                "not_found"
            };
            result["expected_bug"] = json!({"text": text, "matched": matched, "outcome": outcome});
        }
        if let Some(reason) = intent.and_then(|i| i.boundary.as_deref()) {
            result["boundary"] = json!(reason);
        }
        if let Some((s, h)) = &case.code_coverage {
            selected_scope.add(s);
            let summary = CodeMap::new(prog, &probes, s).coverage(h).summary();
            if !only && lint_failed.is_none() {
                con.inner.out_line(&format!(
                    "      Coverage: {}{}",
                    summary,
                    match (case.bugs > 0, case.collected_bugs) {
                        (false, _) => "",
                        (true, false) => " (partial: stopped at first bug)",
                        (true, true) => " (partial: stopped after the bugs)",
                    }
                ));
            }
            result["coverage"] = summary.to_json();
            result["coverage"]["partial"] = json!(case.bugs > 0);
            if expected.is_some() {
                let (mut driver_scope, design_scope) = Scope::partition_drivers(prog, tc, &drivers);
                driver_scope.exclude(&positive_design);
                let driver_coverage = CodeMap::new(prog, &probes, &driver_scope).coverage(h);
                result["expected_bug_driver_coverage"] = driver_coverage.to_json(false);
                result["expected_bug_driver_coverage"]["included"] = json!(matched);
                let design_map = CodeMap::new(prog, &probes, &design_scope);
                let design_coverage = design_map.coverage(h);
                result["expected_bug_design_coverage"] = design_coverage.summary().to_json();
                result["expected_bug_design_coverage"]["partial"] =
                    json!(case.bugs > 0 || case.exhaustive_finished != Some(true));
                if !only && lint_failed.is_none() {
                    con.inner.out_line(&format!(
                        "      Expected-bug design coverage (separate): {}",
                        design_coverage.summary()
                    ));
                }
                if matched {
                    negative_driver_scope.add(&driver_scope);
                    negative_hits.add(h);
                }
            } else {
                scope.add(s);
                hits.add(h);
            }
            if expected.is_none() && case.bugs == 0 {
                if let Some(tc) = all_tests.iter().find(|t| t.name == case.test_case) {
                    passed.push((tc.clone(), h.clone()));
                }
            }
        }
        if let Some(d) = case.driver.as_ref().filter(|_| case.bugs == 0) {
            // on request: the limits are long, and JSON always has them
            if !only && lint_failed.is_none() && cfg.verbose {
                for l in driver_lines(d, &case, !driver_hint_shown) {
                    con.inner.out_line(&l);
                }
                driver_hint_shown |= !d.facts.is_empty();
            }
            result["driver"] = json!({
                "machines": d.machines,
                "repeats": d.repeats,
                "facts": d.facts.iter().map(|f| f.to_json()).collect::<Vec<_>>(),
            });
        }
        if !case.scenario_coverage.scenarios.is_empty() {
            result["scenarios"] = serde_json::from_str(&case.scenario_coverage.to_json()).unwrap();
            if expected.is_none() {
                scenarios.push(case.scenario_coverage.clone());
            }
        }
        cases.push(result);
    }
    let seed_checks = expected_seed_checks(&seed_runs);
    for check in &seed_checks {
        let name = check["test_case"].as_str().unwrap();
        match check["status"].as_str().unwrap() {
            "different_failures" => con.inner.out_line(&format!("      warning: {name} failed at different sites across seeds; inspect each seed's failure identity and replay, then split or tighten the negative control")),
            "incomplete" => con.inner.out_line(&format!("      note: {name}: failure identity comparison is incomplete; a seed did not match or its failure has no captured identity")),
            _ => con.inner.out_line(&format!("      {name}: expected failure identity consistent across {} seeds (evidence from these runs, not a guarantee)", check["runs"].as_array().unwrap().len())),
        }
    }
    cfg.seed = base_seed;
    cfg.max_bugs = base_max_bugs;
    cfg.seed_artifact = None;
    negative_driver_scope.exclude(&positive_design);
    hits.add(&CodeMap::new(prog, &probes, &negative_driver_scope).restrict_hits(&negative_hits));
    scope.add(&negative_driver_scope);
    let map = CodeMap::new(prog, &probes, &scope);
    let suite = map.coverage(&hits);
    let coverage_suppressions = snappea_lint::coverage_suppressions(&loaded.sources.source_map()).unwrap_or_default();
    // A declared failure can stop a negative-only driver before its remaining
    // work runs. Preserve that evidence, but require coverage only for components
    // selected by a positive test (including shared drivers and their helpers).
    let required_map = CodeMap::new(prog, &probes, &positive_scope);
    let required_suite = required_map.coverage(&hits);
    let required = required_map.required_summary(&required_suite, &coverage_suppressions);
    let mut required_uncovered = required_map.required_uncovered(&required_suite, &coverage_suppressions);
    let mut negative_only_scope = negative_driver_scope.clone();
    negative_only_scope.exclude(&positive_scope);
    let negative_only = CodeMap::new(prog, &probes, &negative_only_scope).coverage(&negative_hits);
    let negative_only_gaps = negative_only.uncovered();
    let (mut uncovered, mut uncovered_suppressed) = map.split_uncovered(&suite, &coverage_suppressions);
    // a scenario no test case satisfied is a gap like code no test case ran
    let (scenario_gaps, scenario_accepted) = scenario::gaps(prog, &scenarios, &coverage_suppressions);
    for (list, add) in [
        (&mut required_uncovered, &scenario_gaps),
        (&mut uncovered, &scenario_gaps),
        (&mut uncovered_suppressed, &scenario_accepted),
    ] {
        list.extend(add.iter().cloned());
        list.sort();
    }
    let behavioral = scenario::contract(prog, &scenarios, incomplete == 0 && failed == 0, &coverage_suppressions);
    let broad_exemptions: Vec<_> = coverage_suppressions
        .iter()
        .flat_map(|(span, reason)| {
            prog.machines
                .iter()
                .filter(move |m| {
                    !m.span.is_dummy()
                        && span.file == m.span.file
                        && span.start <= m.span.start
                        && span.end >= m.span.end
                })
                .map(|m| {
                    let loc = prog.sources.location(m.span);
                    let counts = map.hidden_points(&suite, m.span);
                    json!({"machine": m.name, "file": loc.file, "line": loc.line, "reason": reason,
                    "hidden_points": counts.values().sum::<usize>(), "hidden_kinds": counts})
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let required_missing = required.statements.covered < required.statements.total
        || required.branches.covered < required.branches.total
        || required.handlers.covered < required.handlers.total
        || !scenario_gaps.is_empty();
    let required_gap_count = required.statements.total - required.statements.covered + required.branches.total
        - required.branches.covered
        + required.handlers.total
        - required.handlers.covered
        + scenario_gaps.len();
    let coverage_gated = !only && failed == 0 && lint_failed.is_none() && required_missing;
    let coverage_warned = coverage_gated && gates.coverage == Gate::Warn;
    let coverage_failed = coverage_gated && gates.coverage == Gate::Deny;
    let coverage_failure_message = format!(
        "Required coverage was not reached within the run budget ({} point{}).",
        required_gap_count,
        plural(required_gap_count)
    );
    let coverage_failure_help = format!(
        "Add scenarios that reach the listed sites, or increase the timeout if exploration ran out of time.{} For deliberate omissions, add a scoped `coverage` suppression with a reason; use --warn coverage only for exploratory checking.",
        if scenario_gaps.is_empty() {
            ""
        } else {
            " For a scenario never satisfied, add or extend a test case that drives the model through it."
        }
    );
    let excluded = selected_scope.excluded(prog);
    if !only && lint_failed.is_none() {
        if n > 1 {
            con.inner.out_line(&format!("Coverage of all test cases: {}", suite.summary()));
        }
        let heading = if n > 1 { "Not covered by any test case:" } else { "Not covered by the test case:" };
        let (conditions, pairs) = required_map.advisory_uncovered(&required_suite, &coverage_suppressions);
        let (mut positive_uncovered, _) = required_map.split_uncovered(&required_suite, &coverage_suppressions);
        positive_uncovered.extend(scenario_gaps.iter().cloned());
        positive_uncovered.sort();
        let actionable: Vec<_> = positive_uncovered
            .iter()
            .filter(|gap| conditions.binary_search(gap).is_err() && pairs.binary_search(gap).is_err())
            .cloned()
            .collect();
        for l in uncovered_lines(heading, &actionable, " (--json lists all)") {
            con.inner.out_line(&l);
        }
        if !actionable.is_empty() {
            if incomplete == 0 && failed == 0 {
                con.inner.out_line("No schedule of the selected tests reaches these gaps: their searches completed. Run other tests or add a scenario that reaches the missing behavior.");
            }
            con.inner.out_line("For an impossible spec event, use a handler with `assert false` to state that contract. For an unreachable defensive branch, remove it and document why it is unnecessary, or use `allow coverage` with a reason.");
        }
        for l in uncovered_lines(
            "Informational condition coverage (does not fail the check):",
            &conditions,
            " (--json lists all)",
        ) {
            con.inner.out_line(&l);
        }
        if !pairs.is_empty() {
            con.inner.out_line(&format!(
                "{} transition pairs not taken (informational; -v to list, --json lists all).",
                pairs.len()
            ));
            if cfg.verbose {
                for l in uncovered_lines("Informational transition pairs:", &pairs, " (--json lists all)") {
                    con.inner.out_line(&l);
                }
            }
        }
        for l in uncovered_lines(
            "Expected-bug-only driver gaps (report-only; the expected failure can stop the driver):",
            &negative_only_gaps,
            " (--json lists all)",
        ) {
            con.inner.out_line(&l);
        }
        for l in uncovered_lines("Not covered, suppressed:", &uncovered_suppressed, " (--json lists all)") {
            con.inner.out_line(&l);
        }
        for l in uncovered_lines(
            "Outside the selected tests (excluded from coverage totals):",
            &excluded,
            " (--json lists all)",
        ) {
            con.inner.out_line(&l);
        }
        if failed > 0 {
            con.inner.out_line("Coverage includes partial results from test cases stopped by their bugs.");
        }
        if coverage_failed {
            con.inner.out_line(&format!("{coverage_failure_message} {coverage_failure_help}"));
        } else if coverage_warned {
            con.inner.out_line(&format!(
                "Warning: required coverage was not reached within the run budget ({} point{}; --warn coverage).",
                required_gap_count,
                plural(required_gap_count)
            ));
        }
        let not_proved = format!("no unexpected failure found; {}", unproved(incomplete, unsettled));
        con.inner.out_line(&format!(
            "Safety: {}.",
            if failed > 0 {
                "test expectations failed"
            } else if incomplete > 0 {
                not_proved.as_str()
            } else {
                "test expectations met; bounded searches complete"
            }
        ));
        con.inner.out_line(&format!("Behavioral coverage contract: {}.", behavioral["status"].as_str().unwrap()));
        for o in behavioral["obligations"].as_array().unwrap() {
            con.inner.out_line(&format!(
                "  [{}] {} ({}), {}:{}; observer progress {}/{} states",
                o["status"].as_str().unwrap(),
                o["name"].as_str().unwrap(),
                if o["required"] == true { "required" } else { "optional" },
                o["file"].as_str().unwrap_or(""),
                o["line"],
                o["best_states_visited"],
                o["monitor_states"]
            ));
        }
        con.inner.out_line(&format!("Code coverage (raw): {}", suite.summary()));
        con.inner.out_line(&format!("Code coverage (required after exemptions): {required}"));
        for e in &broad_exemptions {
            con.inner.out_line(&format!("Broad coverage exemption: {}:{} exempts entire machine {}; {} uncovered points hidden ({:?}). Reason: {}. Prefer a narrower scope; --json lists details.",
                e["file"].as_str().unwrap_or(""), e["line"], e["machine"].as_str().unwrap(),
                e["hidden_points"], e["hidden_kinds"], e["reason"].as_str().unwrap()));
        }
        if !uncovered_suppressed.is_empty() {
            con.inner.out_line("A clean required coverage gate with exemptions does not establish complete code or behavioral coverage.");
        }
        if !scenarios.is_empty() {
            if incomplete == 0 && failed == 0 {
                con.inner.write(Stream::Out, &scenario::merge(&scenarios), None);
            }
            if cfg.verbose {
                for line in scenario::evidence_lines(&scenarios) {
                    con.inner.out_line(&line);
                }
            }
        }
    }
    let mut undetected = 0;
    let mut untested = Untested::default();
    let mut skipped = Vec::new();
    if coverage_warned {
        skipped.push("required coverage (--warn coverage reported gaps as warnings)");
    }
    if matches!(mutation, Mutation::After(None)) && failed == 0 && lint_failed.is_none() {
        skipped.push("mutation testing (--no-mutation)");
    }
    let mutation = match mutation {
        Mutation::After(None) => json!({"skipped": "--no-mutation"}),
        // a test case that failed leaves lint as the next thing to fix; mutation testing
        // still runs after a lint failure, since its findings are independent of lint's
        _ if lint_failed.is_some() && failed > 0 => json!({"skipped": LINT_SKIP}),
        _ if passed.is_empty() && failed == 0 => {
            skipped.push("mutation testing (no passing positive test case)");
            con.inner.out_line("Mutation testing skipped: no passing positive test case was selected.");
            json!({"skipped": "no passing positive test case"})
        }
        _ if failed > 0 => {
            con.inner.out_line("Mutation testing skipped: a test case failed.");
            json!({"skipped": "a test case failed"})
        }
        // a test case that was not selected may be the one that detects a mutant
        Mutation::After(Some(_)) if selected_tests.len() < all_tests.len() => {
            skipped.push("mutation testing (only some test cases were selected)");
            json!({"skipped": "only some test cases were selected"})
        }
        Mutation::After(Some(limit)) | Mutation::Only(limit) => {
            // the sources parsed when they were loaded
            let suppressed =
                snappea_lint::mutation_suppression_details(&loaded.sources.source_map()).unwrap_or_default();
            let sources = loaded.sources.source_map();
            let marked = snappea_lint::driver_machines(&sources).unwrap_or_default();
            let drivers = crate::checker::driver_facts::driver_machines(prog, &sources, &marked);
            let report = crate::driver::mutation::run(
                &mut con,
                prog,
                &cfg,
                &passed,
                &suppressed,
                &coverage_suppressions,
                &drivers,
                f64::from(limit),
                only,
                gates.strict,
            );
            for l in report.lines(SHOWN_UNCOVERED, cfg.verbose) {
                con.inner.out_line(&l);
            }
            if report.blanket_suppressions > 0 {
                con.inner.out_line("hint: blanket `allow mutation` suppressions hide every mutation kind in their scope; prefer mutation:<kind>. Use --list-suppressions to inspect their scope.");
            }
            if cfg.list_suppressions || cfg.verbose {
                for line in report.suppression_lines() {
                    con.inner.out_line(&line);
                }
            }
            // those inside an undetected coarse mutant too
            undetected = report.failing().len();
            untested = Untested {
                not_tested: report.not_tested(),
                under_searched: report.under_searched(),
                failed: report.failed_to_run(),
                unreached: report.unreached.len(),
                // `snappea check` reports code no test case ran as coverage gaps
                count_unreached: only,
            };
            report.to_json()
        }
    };
    let seconds = con.inner.now() - start;
    let (elapsed, memory) = (duration(seconds), memory(con.inner));
    let mutation_time_limit_reached = mutation.get("time_limit_reached").and_then(Value::as_bool).unwrap_or(false);
    // a gate that denies undetected mutants does not pass with mutants it did not test
    // and mutation testing that is the whole check (`check mutation`, `--strict`) does not
    // pass with mutants it did not test
    let mutation_failed =
        (undetected > 0 || (untested.any() && (only || gates.strict))) && gates.mutation == Gate::Deny;
    let mut summary = check_summary(
        failed,
        n,
        incomplete,
        coverage_failed,
        // coverage was not checked when lint failed
        if lint_failed.is_some() { 0 } else { required_gap_count },
        if lint_failed.is_some() { &[] } else { &required_uncovered },
        mutation_time_limit_reached,
        (undetected, mutation_failed),
        untested,
        &skipped,
    );
    summary["settled"] = json!(incomplete - unsettled);
    if summary["status"] == "search_incomplete" {
        summary["message"] = json!(format!("No bugs found; {}", unproved(incomplete, unsettled)));
    }
    summary["scenario_gaps"] = json!(scenario_gaps.len());
    summary["lint_failed"] = json!(lint_failed.is_some());
    if let Some(lint) = &lint_failed {
        if failed == 0 {
            summary["status"] = json!("lint_failed");
            summary["message"] = json!(lint.summary);
        } else {
            summary["message"] = json!(format!("{}; lint failed", summary["message"].as_str().unwrap_or_default()));
        }
    }
    let out = match cfg.output_path.trim_end_matches('/') {
        "" if cfg.output_path.is_empty() => ".",
        o => o,
    };
    // what was reported but did not fail the check
    let mut warnings = serde_json::Map::new();
    let differing = seed_checks.iter().filter(|c| c["status"] == "different_failures").count();
    if differing > 0 {
        warnings.insert("expected_bug_failure_identity".into(), json!(differing));
    }
    if lint_findings > 0 {
        warnings.insert("lint".into(), json!(lint_findings));
    }
    if coverage_warned {
        warnings.insert("coverage".into(), json!(required_gap_count));
    }
    if undetected > 0 && !mutation_failed {
        warnings.insert("mutation".into(), json!(undetected));
    }
    if untested.any() && !mutation_failed {
        warnings.insert("mutation_untested".into(), json!(untested.total()));
    }
    if let Some(advisories) = mutation.get("suppression_advisories").and_then(Value::as_array).filter(|a| !a.is_empty())
    {
        warnings.insert("mutation_suppressions".into(), json!(advisories.len()));
    }
    // The console names what was reported as a warning, without counts: a count invites
    // working it down, and one finding matters as much as ten.
    let with_warnings = if failed > 0 || warnings.is_empty() {
        String::new()
    } else {
        let parts: Vec<&str> = warnings
            .keys()
            .map(|k| match k.as_str() {
                "lint" => "lint findings",
                "coverage" => "coverage gaps",
                "expected_bug_failure_identity" => "expected-bug tests with differing failures",
                "mutation_suppressions" => "mutation suppression advisories",
                "mutation_untested" => "mutants not tested",
                _ => "undetected mutants",
            })
            .collect();
        format!(", with warnings: {}", parts.join(", "))
    };
    let success = if selected_tests
        .iter()
        .any(|tc| crate::driver::lint::intent(prog, tc, &intents).is_some_and(|i| i.expected_bug.is_some()))
    {
        "Test expectations met"
    } else {
        "No bugs found"
    };
    let lint_line = lint_failed.as_ref().map(|lint| {
        lint.print(con.inner, &sources);
        let mutation_ran = mutation.get("skipped").is_none();
        let skipped = match (only, mutation_ran) {
            (true, false) => "; mutation testing skipped",
            (false, false) => "; coverage and mutation testing skipped",
            (true, true) => "",
            (false, true) => "; coverage skipped",
        };
        let lint_part = if undetected > 0 {
            "lint failed, and the test cases pass with each change listed as error[undetected-mutant]".to_string()
        } else {
            "lint failed".to_string()
        };
        if failed > 0 {
            format!(
                "{} of {} test case{} failed and {} in {}{}{}. Fix the bugs first (replay each with the command above), then fix or suppress the lint findings. Output: {}/",
                failed, n, plural(n), lint_part, elapsed, memory, skipped, out
            )
        } else {
            let incomplete = if incomplete > 0 {
                format!("; {}", unproved(incomplete, unsettled))
            } else {
                String::new()
            };
            format!(
                "{success} in {}{}{}, but {}{}. Fix or suppress each finding above, then check again. Output: {}/",
                elapsed, memory, incomplete, lint_part, skipped, out
            )
        }
    });
    // nothing was skipped or left as a warning: say that the model needs no more work
    let done =
        if skipped.is_empty() && warnings.is_empty() && !untested.any() { format!("{DONE} ") } else { String::new() };
    con.inner.out_line(&if let Some(line) = lint_line {
        line
    } else if only && failed == 0 && passed.is_empty() {
        format!(
            "Test expectations met in {}{}; mutation testing skipped: no passing positive test case.",
            elapsed, memory
        )
    } else if only && failed == 0 {
        match undetected {
            0 if !untested.any() && mutation["mutants"].as_u64() == Some(0) => format!(
                "No mutant ran in {}{}{}: there was none, or every one was suppressed or equivalent.",
                elapsed, memory, with_warnings
            ),
            0 if !untested.any() => {
                format!("🫛 Mutation testing passed in {}{}{}. {}", elapsed, memory, with_warnings, DONE)
            }
            0 if mutation_failed => {
                format!("Mutation testing incomplete in {}{}: {}.", elapsed, memory, untested.describe())
            }
            0 => format!(
                "Mutation testing incomplete in {}{}{}: {}.",
                elapsed,
                memory,
                with_warnings,
                untested.describe()
            ),
            _ if mutation_failed => format!("Mutation testing failed in {}{}: {}", elapsed, memory, UNDETECTED),
            _ => format!("Mutation testing found undetected mutants in {}{}{}.", elapsed, memory, with_warnings),
        }
    } else if failed == 0 && mutation_failed && !coverage_failed && undetected > 0 {
        format!("{success} in {}{}, but mutation testing failed: {} Output: {}/", elapsed, memory, UNDETECTED, out)
    } else if failed == 0 && mutation_failed && !coverage_failed {
        format!("Mutation testing incomplete in {}{}: {}. Output: {}/", elapsed, memory, untested.describe(), out)
    } else if failed == 0 && coverage_failed {
        format!(
            "Coverage requirements were not met for {} point{}. Output: {}/",
            required_gap_count,
            plural(required_gap_count),
            out
        )
    } else if failed == 0 && incomplete > 0 {
        format!(
            "🫛 {success} in {}{}{}. {}. Output: {}/",
            elapsed,
            memory,
            with_warnings,
            capitalized(&unproved(incomplete, unsettled)),
            out
        )
    } else if failed == 0 && n == 1 {
        format!("🫛 The test case passed in {}{}{}. {}Output: {}/", elapsed, memory, with_warnings, done, out)
    } else if failed == 0 {
        format!("🫛 All {} test cases passed in {}{}{}. {}Output: {}/", n, elapsed, memory, with_warnings, done, out)
    } else {
        format!("{} of {} test case{} failed in {}{}. Output: {}/", failed, n, plural(n), elapsed, memory, out)
    });
    if let Some(note) = summary["note"].as_str() {
        con.inner.out_line(note);
    }
    if only {
        let result = json!({
            "summary": summary,
            "project": cfg.project_name,
            "test_cases": cases,
            "failed": failed,
            "seconds": round_ms(seconds),
            "output": format!("{}/", out),
            "mutation": mutation,
            "warnings": warnings,
            "expected_bug_seed_checks": seed_checks,
        });
        let code = if failed > 0 || mutation_failed { BUG } else { OK };
        return Ok(with_lint_failure(code, result, failed, lint_failed.as_deref()));
    }
    let mut coverage = suite.to_json(false);
    coverage["uncovered"] = uncovered.iter().map(|u| u.to_json()).collect();
    coverage["uncovered_suppressed"] = uncovered_suppressed.iter().map(|u| u.to_json()).collect();
    coverage["excluded_components"] = excluded.iter().map(|u| u.to_json()).collect();
    coverage["partial"] = json!(failed > 0);
    coverage["required"] = required.to_json();
    coverage["expected_bug_driver_only"] = negative_only.to_json(false);
    coverage["expected_bug_driver_only"]["required"] = json!(false);
    coverage["uncovered_required"] = required_uncovered.iter().map(|u| u.to_json()).collect();
    coverage["gate_bypassed"] = json!(!only && gates.coverage == Gate::Warn);
    coverage["failure"] = if coverage_failed {
        json!({"message": coverage_failure_message, "help": coverage_failure_help})
    } else {
        Value::Null
    };
    if lint_failed.is_some() {
        coverage = json!({"skipped": LINT_SKIP});
    }
    let result = json!({
        "summary": summary,
        "project": cfg.project_name,
        "test_cases": cases,
        "failed": failed,
        "incomplete": incomplete,
        "seconds": round_ms(seconds),
        "output": format!("{}/", out),
        "coverage": coverage,
        "coverage_failed": coverage_failed,
        "safety": {"status": if failed > 0 { "failed" } else if incomplete > 0 { "incomplete" } else { "passed" }},
        "behavioral_coverage": behavioral,
        "coverage_exemptions": broad_exemptions,
        "mutation": mutation,
        "warnings": warnings,
        "expected_bug_seed_checks": seed_checks,
        "scenarios": scenarios.iter().map(|a| serde_json::from_str::<Value>(&a.to_json()).unwrap()).collect::<Vec<_>>(),
    });
    let code = if failed > 0 || coverage_failed || mutation_failed { BUG } else { OK };
    Ok(with_lint_failure(code, result, failed, lint_failed.as_deref()))
}

/// Ends the summary of a check with nothing left to fix, so work on the model can stop.
const DONE: &str = "Nothing left to fix: more tests are optional.";
/// Ends the summary of a check that undetected mutants failed.
const UNDETECTED: &str =
    "the test cases pass with each change listed as error[undetected-mutant] above. For each, add what its help names, or suppress it with a reason.";

/// A check's exit status and result when its blocking lint findings `lint` failed it:
/// 1 when a test case failed (the bug is the first thing to fix), else 2, as when lint
/// alone runs. The result keeps the lint-failure fields `error` and `lint.findings`.
fn with_lint_failure(
    code: i32,
    mut result: Value,
    failed: usize,
    lint: Option<&crate::driver::lint::LintFailed>,
) -> (i32, Value) {
    let Some(lint) = lint else {
        return (code, result);
    };
    result["error"] = json!(lint.summary);
    result["lint"] = json!({"failed": true, "errors": lint.count, "findings": lint.findings});
    (if failed > 0 { BUG } else { ERROR }, result)
}

/// Mutants of the selected code that got no verdict (see [`crate::driver::mutation::Report`]).
#[derive(Debug, Default, Clone, Copy)]
struct Untested {
    /// The time limit came before their first turn.
    not_tested: usize,
    /// The time limit stopped them before their budget, undetected so far.
    under_searched: usize,
    /// The checker could not run them.
    failed: usize,
    /// In code no passing test case ran.
    unreached: usize,
    /// Whether `unreached` counts as untested: in `check mutation`, but not in `snappea
    /// check`, which reports that code as coverage gaps.
    count_unreached: bool,
}

impl Untested {
    fn total(&self) -> usize {
        self.not_tested + self.under_searched + self.failed + self.counted_unreached()
    }

    fn counted_unreached(&self) -> usize {
        if self.count_unreached {
            self.unreached
        } else {
            0
        }
    }

    fn any(&self) -> bool {
        self.total() > 0
    }

    /// E.g. `3 mutants not tested before the time limit, 1 in code no passing test case ran`.
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.not_tested > 0 {
            parts.push(format!(
                "{} mutant{} not tested before the time limit",
                self.not_tested,
                plural(self.not_tested)
            ));
        }
        if self.under_searched > 0 {
            parts.push(format!(
                "{} mutant{} not fully tested before the time limit",
                self.under_searched,
                plural(self.under_searched)
            ));
        }
        if self.counted_unreached() > 0 {
            parts.push(format!("{} mutant{} in code no passing test case ran", self.unreached, plural(self.unreached)));
        }
        if self.failed > 0 {
            parts.push(format!("{} mutant{} the checker could not run", self.failed, plural(self.failed)));
        }
        parts.join(", ")
    }
}

/// A concise top-level outcome. Mutation time limits change it only when `--deny mutation`
/// (or `check mutation`) requires every mutant to be tested; they never change whether the
/// test search or required-coverage gate passed.
#[allow(clippy::too_many_arguments)]
fn check_summary(
    failed: usize,
    test_cases: usize,
    incomplete: usize,
    coverage_failed: bool,
    coverage_gaps: usize,
    uncovered: &[crate::checker::code_coverage::Uncovered],
    mutation_time_limit_reached: bool,
    (mutants_undetected, mutation_failed): (usize, bool),
    untested: Untested,
    skipped: &[&str],
) -> Value {
    // What a pass does not cover, so a partial check is not read as a full one.
    let mut notes = Vec::new();
    if !skipped.is_empty() {
        notes.push(format!(
            "Not checked: {}. Run without these options before relying on the result.",
            skipped.join(", ")
        ));
    }
    let timed_out = untested.not_tested + untested.under_searched;
    if failed == 0 && timed_out > 0 {
        let mut what = Vec::new();
        if untested.not_tested > 0 {
            what.push(format!("{} mutant{} not tested", untested.not_tested, plural(untested.not_tested)));
        }
        if untested.under_searched > 0 {
            what.push(format!(
                "{} not fully tested (no test case failed before the limit stopped {})",
                untested.under_searched,
                if untested.under_searched == 1 { "it" } else { "them" }
            ));
        }
        notes.push(format!(
            "Mutation testing reached its time limit with {}. Raise the limit before adding assertions \
             for them, e.g. `snappea check mutation --mutation-timeout 5m`, or name fewer test cases.",
            what.join(" and ")
        ));
    }
    if failed == 0 && untested.counted_unreached() > 0 {
        notes.push(format!(
            "{} mutant{} in code no passing test case ran, so no test case could detect {}. \
             Add a test case or scenario that runs that code; `snappea check` lists it.",
            untested.unreached,
            plural(untested.unreached),
            if untested.unreached == 1 { "it" } else { "them" }
        ));
    }
    if failed == 0 && untested.failed > 0 {
        notes.push(format!(
            "The checker could not run {} mutant{} (a snappea bug; please report it with the model).",
            untested.failed,
            plural(untested.failed)
        ));
    }
    let (status, message) = if failed > 0 {
        ("tests_failed", format!("{} of {} test case{} failed", failed, test_cases, plural(test_cases)))
    } else if coverage_failed {
        (
            "coverage_failed",
            format!(
                "Required coverage was not reached within the run budget ({} point{})",
                coverage_gaps,
                plural(coverage_gaps)
            ),
        )
    } else if mutation_failed && mutants_undetected > 0 {
        (
            "mutation_failed",
            "The test cases pass with changes to the model that a spec or assertion should detect".to_string(),
        )
    } else if mutation_failed {
        ("mutation_incomplete", format!("Mutation testing incomplete: {} (--deny mutation)", untested.describe()))
    } else if incomplete > 0 {
        (
            "search_incomplete",
            format!("No bugs found; search incomplete for {} test case{}", incomplete, plural(incomplete)),
        )
    } else {
        ("passed", "All test cases passed".to_string())
    };
    json!({
        "status": status,
        "message": message,
        "test_cases": test_cases,
        "failed": failed,
        "incomplete": incomplete,
        "coverage_failed": coverage_failed,
        "safety": {"status": if failed > 0 { "failed" } else if incomplete > 0 { "incomplete" } else { "passed" }},
        "coverage_gaps": coverage_gaps,
        "uncovered": uncovered.iter().take(5).map(|u| json!({
            "file": u.file,
            "line": u.line,
            "context": u.context,
            "what": u.what,
        })).collect::<Vec<_>>(),
        "uncovered_more": uncovered.len().saturating_sub(5),
        "mutation_time_limit_reached": mutation_time_limit_reached,
        "mutants_undetected": mutants_undetected,
        "mutants_not_tested": untested.not_tested,
        "mutants_under_searched": untested.under_searched,
        "mutants_unreached": untested.unreached,
        "mutants_failed_to_run": untested.failed,
        "skipped": skipped,
        "note": if notes.is_empty() { Value::Null } else { json!(notes.join(" ")) },
    })
}

/// Driver facts shown for a test case; `--json` has them all.
const SHOWN_DRIVER_FACTS: usize = 5;

/// The console lines for what a test case's driver did in every schedule.
fn driver_lines(d: &check::DriverReport, case: &check::CaseOutcome, hint: bool) -> Vec<String> {
    if d.facts.is_empty() {
        return Vec::new();
    }
    let schedules = match case.exhaustive_finished {
        Some(true) => "every schedule",
        _ => "every explored schedule",
    };
    let mut out = vec![format!("      Driver limits (held in {}):", schedules)];
    for f in d.facts.iter().take(SHOWN_DRIVER_FACTS) {
        out.push(format!("        {}", f.text));
    }
    if d.facts.len() > SHOWN_DRIVER_FACTS {
        let n = d.facts.len() - SHOWN_DRIVER_FACTS;
        out.push(format!("        ... and {} more (--json lists all)", n));
    }
    if hint {
        out.push("      hint: if the real environment can do more, extend the driver to try it".into());
    }
    out
}

/// How many distinct states per choice of its deepest schedule a passing test case's
/// complete search can reach and still be reported as having little to explore.
const LITTLE_TO_EXPLORE: usize = 3;

/// Whether a passing test case explored every schedule and they reached few more states
/// than one run does ([`LITTLE_TO_EXPLORE`] per step): the model has nearly one behavior
/// under the test, so the test checks little concurrency. A note, not a failure.
fn little_to_explore(case: &check::CaseOutcome) -> bool {
    case.bugs == 0
        && case.exhaustive_finished == Some(true)
        && case.exhaustive_states.is_some_and(|(states, depth)| states <= LITTLE_TO_EXPLORE * depth.max(1))
}

/// How far a passing test case's search got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Every schedule was checked.
    Proved,
    /// Not every schedule, but the second half of the search reached no new coverage.
    Settled,
    /// New coverage was still turning up when the search stopped.
    Finding,
    /// Stopped with nothing to judge by: memory ran out, or coverage was not tracked.
    Unjudged,
}

impl Verdict {
    /// The name in JSON.
    fn name(self) -> &'static str {
        match self {
            Verdict::Proved => "proved",
            Verdict::Settled => "settled",
            Verdict::Finding => "finding_new_behavior",
            Verdict::Unjudged => "unjudged",
        }
    }
}

fn verdict(case: &check::CaseOutcome) -> Verdict {
    match case.settled {
        _ if case.exhaustive_finished == Some(true) => Verdict::Proved,
        _ if case.memory_stop.is_some() => Verdict::Unjudged,
        Some(true) => Verdict::Settled,
        Some(false) => Verdict::Finding,
        None => Verdict::Unjudged,
    }
}

/// `3 test cases not proved: 2 settled, 1 not settled`, for `incomplete`
/// searches of which `unsettled` did not settle.
fn unproved(incomplete: usize, unsettled: usize) -> String {
    let mut parts = Vec::new();
    if incomplete > unsettled {
        parts.push(format!("{} settled", incomplete - unsettled));
    }
    if unsettled > 0 {
        parts.push(format!("{} not settled", unsettled));
    }
    format!("{} test case{} not proved: {}", incomplete, plural(incomplete), parts.join(", "))
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Why a passing test case's search did not cover every schedule, for the JSON report:
/// `memory_limit` (memory use neared the limit, so the search stopped),
/// `exhaustive_search_unfinished` (the search ran out of time or states), `timed_model_sampled` (time
/// tracking keeps clocks growing, so a model that loops cannot be exhausted) or `sampled`
/// (too many schedules: the search sampled them).
fn incomplete_reason(case: &check::CaseOutcome) -> Option<&'static str> {
    if case.bugs > 0 || case.exhaustive_finished == Some(true) {
        None
    } else if case.memory_stop.is_some() {
        Some("memory_limit")
    } else if case.exhaustive_unfinished.is_some() {
        Some("exhaustive_search_unfinished")
    } else if case.time_tracked {
        Some("timed_model_sampled")
    } else {
        Some("sampled")
    }
}

/// The next step for a search that memory use stopped.
fn memory_hint(m: check::MemoryStop) -> String {
    format!(
        "search stopped at {} MB of {} MB memory; shrink bounds (fewer machines, retries or faults per test case) or split the model into smaller test cases",
        m.used_mb, m.limit_mb
    )
}

/// A `--timeout` worth trying for a search still finding new behavior after `elapsed`
/// seconds: five times as long, up to a minute. A search that has not settled in longer
/// than that is too large to: suggesting ever more time only costs it.
fn longer_timeout(elapsed: f64) -> Option<&'static str> {
    [(30.0, "30s"), (60.0, "1m")].iter().find(|(secs, _)| *secs >= 5.0 * elapsed).map(|(_, s)| *s)
}

/// Whether a search's schedules mostly reach states it already explored: at least
/// [`REVISITS`] schedules per distinct state. Independent steps run in many orders that
/// end in the same states, which a forwarder or a needless concurrent machine multiplies.
fn mostly_revisits(case: &check::CaseOutcome) -> Option<usize> {
    const REVISITS: usize = 10;
    case.exhaustive_states.map(|(states, _)| states).filter(|&states| states > 0 && case.schedules >= REVISITS * states)
}

/// Where an average schedule branches: `31 scheduling points with a choice; ready at
/// them: Network 25, Sender 20; choices made: Network 3`.
/// The machines ready at the most scheduling points with a choice, e.g. `Replica, Writer`.
fn busiest_machines(machines: &[crate::checker::branching::MachineBranching]) -> String {
    let mut ms: Vec<_> = machines.iter().filter(|m| m.ready > 0.0).collect();
    ms.sort_by(|a, b| b.ready.total_cmp(&a.ready));
    ms.iter().take(3).map(|m| m.machine.as_str()).collect::<Vec<_>>().join(", ")
}

fn branching_text(points: f64, machines: &[crate::checker::branching::MachineBranching]) -> String {
    let n = |x: f64| if x >= 9.95 { format!("{:.0}", x) } else { format!("{:.1}", x) };
    let list = |count: fn(&crate::checker::branching::MachineBranching) -> f64| {
        let mut ms: Vec<_> = machines.iter().filter(|m| count(m) > 0.0).collect();
        ms.sort_by(|a, b| count(b).total_cmp(&count(a)));
        ms.iter().take(3).map(|m| format!("{} {}", m.machine, n(count(m)))).collect::<Vec<_>>().join(", ")
    };
    let mut out = format!("{} scheduling points with a choice", n(points));
    let ready = list(|m| m.ready);
    if !ready.is_empty() {
        out.push_str(&format!("; ready at them: {}", ready));
    }
    let choices = list(|m| m.choices);
    if !choices.is_empty() {
        out.push_str(&format!("; `$`/`choose` choices: {}", choices));
    }
    out
}

struct SeedFailure {
    seed: u32,
    matched: bool,
    identity: Option<crate::checker::runtime::BugIdentity>,
}

fn bug_identity_json(identity: &crate::checker::runtime::BugIdentity) -> Value {
    json!({"kind": identity.kind, "machine": identity.machine, "state": identity.state,
        "file": identity.file, "line": identity.line, "column": identity.column})
}

fn expected_seed_checks(runs: &std::collections::BTreeMap<String, Vec<SeedFailure>>) -> Vec<Value> {
    runs.iter()
        .map(|(name, runs)| {
            let identities: std::collections::BTreeSet<_> = runs.iter().filter_map(|r| r.identity.as_ref()).collect();
            let all_matched = runs.iter().all(|r| r.matched);
            let status = if identities.len() > 1 {
                "different_failures"
            } else if !all_matched || runs.iter().any(|r| r.identity.is_none()) {
                "incomplete"
            } else {
                "consistent"
            };
            json!({"test_case": name, "status": status, "all_matched": all_matched,
            "runs": runs.iter().map(|r| json!({"seed": r.seed, "matched": r.matched,
                "identity": r.identity.as_ref().map(bug_identity_json)})).collect::<Vec<_>>()})
        })
        .collect()
}

/// Match the diagnostic message, never source excerpts or suggested fixes. Native
/// diagnostics place those after a location/help line; assertion and liveness reports
/// without a rendered snippet retain their whole message, including newlines.
fn failure_message(report: &str, sources: &crate::syntax::SourceMap) -> String {
    if let Some(assertion) = report.strip_prefix("Assertion Failed: ") {
        for source in sources.files() {
            let prefix = format!("{}:", source.name.replace('\\', "\\\\"));
            let Some(rest) = assertion.strip_prefix(&prefix) else { continue };
            let Some((line, rest)) = rest.split_once(':') else { continue };
            if line.parse::<u32>().is_err() {
                continue;
            }
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            if end == 0 {
                continue;
            }
            let message = rest[end..].strip_prefix(' ').unwrap_or(&rest[end..]);
            return format!("Assertion Failed: {message}");
        }
        return report.to_string();
    }
    report
        .lines()
        .take_while(|line| {
            let line = line.trim_start();
            !line.starts_with("-->") && !line.starts_with("= help:")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One test case's result as JSON.
fn case_json(case: &check::CaseOutcome, cfg: &CheckConfig, boundary: bool) -> Value {
    let mut v = json!({
        "name": case.test_case,
        "result": if case.bugs > 0 { "bug" } else { "ok" },
        "schedules": case.schedules,
        "workers": case.workers,
        "seconds": round_ms(case.elapsed),
        "time_limit_reached": case.timed_out,
        "coverage_stopped_growing": case.plateaued,
    });
    if case.bugs == 0 {
        v["verdict"] = json!(verdict(case).name());
    }
    if let Some(finished) = case.exhaustive_finished {
        v["search_complete"] = json!(finished);
    }
    if let Some(reason) = incomplete_reason(case) {
        v["incomplete_reason"] = json!(reason);
    }
    if let Some(m) = case.memory_stop {
        v["memory_limit_reached"] = json!({"used_mb": m.used_mb, "limit_mb": m.limit_mb});
        v["hint"] = json!(memory_hint(m));
    }
    if !case.auto_handoffs.is_empty() {
        v["auto_handoffs"] = json!(case
            .auto_handoffs
            .iter()
            .map(|h| json!({
                "worker": h.worker,
                "after_schedules": h.after_schedules,
                "choices": h.choices,
                "reason": h.reason,
            }))
            .collect::<Vec<_>>());
    }
    if let Some(frontier) = case.exhaustive_unfinished {
        v["untried_alternatives"] = json!(frontier);
    }

    if let Some((states, _)) = case.exhaustive_states.filter(|_| case.bugs == 0) {
        v["distinct_states"] = json!(states);
    }
    if let Some((points, machines)) = &case.branching {
        let r = |x: f64| (x * 10.0).round() / 10.0;
        v["branching"] = json!({
            "scheduling_points_with_choice": r(*points),
            "machines": machines.iter().map(|m| json!({
                "machine": m.machine,
                "ready_at_scheduling_points": r(m.ready),
                "choices": r(m.choices),
            })).collect::<Vec<_>>(),
        });
    }
    if !case.collection_sizes.is_empty() {
        v["collection_sizes"] = json!(case
            .collection_sizes
            .iter()
            .map(|c| json!({
                "machine": c.machine, "field": c.field, "file": c.file, "line": c.line,
                "min": c.min, "max": c.max, "observations": c.observations,
            }))
            .collect::<Vec<_>>());
    }
    if !boundary && little_to_explore(case) {
        v["little_to_explore"] = json!(true);
    }
    if case.bugs > 0 {
        v["bug"] = json!({
            "report": case.bug_report,
            "identity": case.bug_identity.as_ref().map(bug_identity_json),
            "schedule": case.schedule_file,
            "replay": case.schedule_file.as_ref().map(|s| replay_command(s, cfg)),
            "also_hot": hot_specs_json(&case.hot_specs),
        });
        if case.collected_bugs {
            v["more_bugs"] = json!(case
                .more_bugs
                .iter()
                .map(|m| json!({
                    "report": m.bug.report,
                    "identity": m.bug.identity.as_ref().map(bug_identity_json),
                    "also_hot": hot_specs_json(&m.bug.hot_specs),
                    "schedule": m.schedule_file,
                    "trace": m.trace_file,
                    "replay": replay_command(&m.schedule_file, cfg),
                }))
                .collect::<Vec<_>>());
        }
    }
    v
}

fn hot_specs_json(specs: &[crate::checker::runtime::HotSpec]) -> Value {
    json!(specs.iter().map(|h| json!({"spec": h.spec, "state": h.state, "kind": h.kind})).collect::<Vec<_>>())
}

/// The lines naming the specs other than the failing one that were hot when a bug was
/// found: those the same execution violates, then those that are only context. A
/// deadlock's report already names every hot spec.
fn hot_spec_lines(
    identity: Option<&crate::checker::runtime::BugIdentity>,
    specs: &[crate::checker::runtime::HotSpec],
) -> Vec<String> {
    if identity.is_some_and(|i| i.kind == "deadlock") {
        return Vec::new();
    }
    let list = |kind: &str| {
        specs
            .iter()
            .filter(|h| h.kind == kind)
            .map(|h| format!("{} in '{}'", h.spec, h.state))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut lines = Vec::new();
    let failing = list("liveness");
    if !failing.is_empty() {
        lines.push(format!("also hot at this point: {failing}"));
    }
    let context = list("hot");
    if !context.is_empty() {
        lines.push(format!("also hot at this point (context; these specs have not failed): {context}"));
    }
    lines
}

/// One test case's result line, and for a bug its report and how to replay it.
fn print_case(host: &mut dyn Host, case: &check::CaseOutcome, cfg: &CheckConfig, boundary: bool) {
    for handoff in case.auto_handoffs.iter().filter(|_| cfg.verbose) {
        host.out_line(&format!(
            "      auto {}: worker {} switched from exhaustive search to sampling after {} schedules ({} exhaustive choices): {}",
            case.test_case, handoff.worker, handoff.after_schedules, handoff.choices, handoff.reason
        ));
    }
    let s = case.schedules;
    // a bug ends an exhaustive search early, so its extent only matters without one
    let schedules = match case.exhaustive_finished.filter(|_| case.bugs == 0) {
        Some(true) => format!("all {} schedule{}", s, plural(s)),
        Some(false) => format!("{} schedule{}, search incomplete", s, plural(s)),
        None => format!("{} schedule{}", s, plural(s)),
    };
    let workers = if case.workers > 1 { format!(", {} workers", case.workers) } else { String::new() };
    let limit = format!(
        "{}{}",
        workers,
        if incomplete_reason(case) == Some("timed_model_sampled") {
            if case.timed_out {
                ", timed model: sampled, time limit reached"
            } else {
                ", timed model: sampled"
            }
        } else if case.memory_stop.is_some() {
            ", memory limit reached"
        } else if case.timed_out {
            ", time limit reached"
        } else if case.plateaued {
            ", stopped: coverage stopped growing"
        } else {
            ""
        }
    );
    let time = duration(case.elapsed);
    if case.bugs == 0 {
        let verdict = verdict(case);
        let what = match verdict {
            Verdict::Proved => "proved",
            Verdict::Settled => "no bug found, search settled",
            Verdict::Finding => "no bug found, still finding new behavior",
            Verdict::Unjudged if case.memory_stop.is_some() => "no bug found, memory limit reached",
            Verdict::Unjudged => "no bug found",
        };
        // how much was searched is detail: the verdict says what it amounts to
        let detail = if cfg.verbose { format!(", {} schedule{}{}", s, plural(s), workers) } else { String::new() };
        host.out_line(&format!("ok    {} ({}{}, {})", case.test_case, what, detail, time));
        if let Some(m) = case.memory_stop {
            host.out_line(&format!("      hint: {}", memory_hint(m)));
        }
        if !boundary && little_to_explore(case) {
            host.out_line(
                "      note: every schedule reaches nearly the same states, so this test checks little more than one run would",
            );
            host.out_line(
                "      hint: give the model something to get wrong: a second client, concurrent requests, a failure, or a choice",
            );
        }
        if cfg.verbose && !case.collection_sizes.is_empty() {
            let fields = case
                .collection_sizes
                .iter()
                .take(5)
                .map(|c| format!("{}.{} {}..{}", c.machine, c.field, c.min, c.max))
                .collect::<Vec<_>>()
                .join(", ");
            host.out_line(&format!("      Sampled collection sizes (elements): {fields}"));
            host.out_line("      note: ranges combine sampled states and instances; they do not prove growth or identify the cause of search size (--json lists fields and locations)");
        }
        if verdict != Verdict::Proved && incomplete_reason(case) == Some("timed_model_sampled") {
            host.out_line("      note: timed model: clocks only grow, so a loop never returns to an explored state; the search sampled schedules");
        }
        match verdict {
            Verdict::Settled => {
                let busiest = case.branching.as_ref().map(|(_, machines)| busiest_machines(machines)).unwrap_or_default();
                host.out_line(&format!(
                    "      note: not proved: too many schedules to check them all in this time{}",
                    if busiest.is_empty() { String::new() } else { format!("; most choices come from {}", busiest) }
                ));
                host.out_line("      hint: for a proof, shrink the test: fewer machines, messages, faults or choices (see `snappea guide modeling`)");
            }
            Verdict::Finding => match longer_timeout(case.elapsed) {
                Some(timeout) => host.out_line(&format!(
                    "      hint: new behavior was still turning up when the search stopped; run longer, e.g. --timeout {}",
                    timeout
                )),
                None => {
                    host.out_line(&format!(
                        "      note: new behavior was still turning up after {}: the test is too large to settle",
                        time
                    ));
                    host.out_line("      hint: shrink or split the test so each part settles: fewer machines, messages, faults or choices (see `snappea guide modeling`)");
                }
            },
            Verdict::Proved | Verdict::Unjudged => {}
        }
        if cfg.verbose {
            if let Some(states) = mostly_revisits(case).filter(|_| verdict != Verdict::Proved) {
                host.out_line(&format!(
                    "      note: {} schedules reached only {} distinct states: most run independent steps in other orders to the same states",
                    case.schedules, states
                ));
            }
            let branching = case.branching.as_ref().filter(|(points, _)| *points >= 1.0);
            if let Some((points, machines)) = branching {
                host.out_line(&format!("      Branching per schedule: {}", branching_text(*points, machines)));
            }
        }
        return;
    }
    host.write(Stream::Out, "FAIL", Some(Color::Red));
    // the search went on after the first bug: say when that bug was found
    let found = match case.first_bug_schedule.filter(|&n| case.collected_bugs && n > 0) {
        Some(n) => format!("{} schedule{}; {} searched{}", n, plural(n), s, workers),
        None => format!("{}{}", schedules, limit),
    };
    host.out_line(&format!("  {} (bug found after {}, {})", case.test_case, found, time));
    if let Some(report) = &case.bug_report {
        for l in report.lines() {
            host.out_line(&format!("      {}", l));
        }
    }
    for l in hot_spec_lines(case.bug_identity.as_ref(), &case.hot_specs) {
        host.out_line(&format!("      {}", l));
    }
    if let Some(schedule) = &case.schedule_file {
        host.out_line(&format!("      Replay: {}", replay_command(schedule, cfg)));
    }
    if case.more_bugs.is_empty() {
        return;
    }
    let n = case.more_bugs.len();
    host.out_line(&format!("      {} more distinct failure{} in this test case:", n, plural(n)));
    for m in &case.more_bugs {
        host.out_line(&format!("      - {}", m.bug.report.lines().next().unwrap_or_default()));
        host.out_line(&format!("        Replay: {}", replay_command(&m.schedule_file, cfg)));
    }
}

/// The `snappea replay` command for `schedule`, with the sources the check was given.
fn replay_command(schedule: &str, cfg: &CheckConfig) -> String {
    let mut words = vec!["snappea".to_string(), "replay".into(), schedule.to_string()];
    words.extend(cfg.input.pproj.iter().cloned());
    words.extend(cfg.input.pfiles.iter().cloned());
    if cfg.ignore_foreign_files {
        words.push("--ignore-foreign-files".into());
    }
    words.iter().map(|w| shell_quote(w)).collect::<Vec<_>>().join(" ")
}

/// Quotes `w` for a POSIX shell when it needs it.
fn shell_quote(w: &str) -> String {
    if !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=@+,".contains(c)) {
        w.to_string()
    } else {
        format!("'{}'", w.replace('\'', "'\\''"))
    }
}

pub(super) fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Seconds, to the millisecond (for JSON).
pub(super) fn round_ms(secs: f64) -> f64 {
    (secs * 1000.0).round() / 1000.0
}

/// Seconds for people: `0.18s`, `12.3s`, `2m05s`, `1h02m`.
fn duration(secs: f64) -> String {
    let secs = secs.max(0.0);
    if secs < 9.995 {
        format!("{:.2}s", secs)
    } else if secs < 59.95 {
        format!("{:.1}s", secs)
    } else if secs < 3599.5 {
        let s = secs.round() as u64;
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        let m = (secs / 60.0).round() as u64;
        format!("{}h{:02}m", m / 60, m % 60)
    }
}

/// `, peak memory 120 MB`, or nothing when the host does not know.
fn memory(host: &dyn Host) -> String {
    let mb = host.max_memory_gb() * 1024.0;
    if mb > 0.0 {
        format!(", peak memory {:.0} MB", mb.max(1.0))
    } else {
        String::new()
    }
}

const REPLAY_OPTS: &[Opt] = &[
    value("test", None),
    value("max-steps", None),
    value("fair-max-steps", None),
    flag("fail-on-max-steps", None),
    flag("ignore-foreign-files", None),
    flag("verbose", Some('v')),
];

fn replay(args: &[String], host: &mut dyn Host) -> i32 {
    let a = match parse_args(args, REPLAY_OPTS) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, Some("replay")),
    };
    if a.help {
        host.write(Stream::Out, &format!("{}\n{}", REPLAY_HELP, OUTPUT_HELP), None);
        return OK;
    }
    let mut cfg = snappea_config();
    cfg.timeout = None;
    let rest = match sources(&mut cfg, &a.positional) {
        Ok(r) => r,
        Err(m) => return usage_error(host, &m, Some("replay")),
    };
    let file = match rest.as_slice() {
        [f] if f.ends_with(".schedule") => f.clone(),
        [f] => return usage_error(host, &format!("'{}' is not a .schedule file", f), Some("replay")),
        [] => return usage_error(host, "no .schedule file given", Some("replay")),
        [_, extra, ..] => {
            return usage_error(
                host,
                &format!("unexpected argument '{}' (sources are .pproj or .p files)", extra),
                Some("replay"),
            )
        }
    };
    cfg.verbose = a.has("verbose");
    cfg.ignore_foreign_files = a.has("ignore-foreign-files");
    cfg.strategy = StrategyKind::Replay;
    if let Err(m) = step_options(&a, &mut cfg) {
        return usage_error(host, &m, Some("replay"));
    }
    let run = |host: &mut dyn Host| -> Outcome {
        let mut con = Quiet::new(host, !cfg.verbose);
        let trace = check::read_schedule(&mut con, &file)?;
        check::resolve_sources(&mut con, &mut cfg)?;
        let loaded = check::load(&mut con, &cfg)?;
        let all: Vec<String> = loaded.analysis.program.test_cases().into_iter().map(|t| t.name).collect();
        let name = match (a.get("test"), &trace.test_name) {
            (Some(t), _) => t.to_string(),
            (None, Some(t)) => t.clone(),
            (None, None) if all.len() == 1 => all[0].clone(),
            (None, None) => {
                error(
                    con.inner,
                    &format!("the schedule names no test case; pick one with --test. Test cases: {}", all.join(", ")),
                );
                return Err(Exit(ERROR));
            }
        };
        if !all.contains(&name) {
            error(con.inner, &format!("no test case '{}'. Test cases: {}", name, all.join(", ")));
            return Err(Exit(ERROR));
        }
        con.inner.out_line(&format!("Replaying {} from {}", name, file));
        let tc = check::select_test_case(&mut con, &loaded, &name)?;
        let r = check::replay(&mut con, &loaded, &tc, &mut cfg, trace)?;
        let host = con.inner;
        match &r.bug_report {
            Some(report) => {
                host.write(Stream::Out, "Reproduced the bug", Some(Color::Red));
                host.out_line(&format!(" ({}):", duration(r.elapsed)));
                for l in report.lines() {
                    host.out_line(&format!("      {}", l));
                }
                if !cfg.verbose {
                    host.out_line("Run with --verbose to see the schedule's log.");
                }
            }
            None => host.out_line(&format!("The schedule ran without finding a bug ({}).", duration(r.elapsed))),
        }
        let result = json!({
            "test_case": name,
            "schedule": file,
            "reproduced": r.bugs > 0,
            "bug": r.bug_report,
            "seconds": round_ms(r.elapsed),
        });
        Ok((if r.bugs > 0 { BUG } else { OK }, result))
    };
    with_output(host, &a, "replay", run)
}

fn tests(args: &[String], host: &mut dyn Host) -> i32 {
    let a = match parse_args(args, &[]) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, Some("tests")),
    };
    if a.help {
        host.write(Stream::Out, &format!("{}\n{}", TESTS_HELP, OUTPUT_HELP), None);
        return OK;
    }
    let mut cfg = snappea_config();
    // nothing runs, so foreign code is not left out of a check
    cfg.ignore_foreign_files = true;
    match sources(&mut cfg, &a.positional) {
        Ok(rest) if rest.is_empty() => {}
        Ok(rest) => {
            return usage_error(
                host,
                &format!("unexpected argument '{}' (sources are .pproj or .p files)", rest[0]),
                Some("tests"),
            )
        }
        Err(m) => return usage_error(host, &m, Some("tests")),
    }
    with_output(host, &a, "tests", |host| {
        // progress messages are shown only if loading fails
        let mut quiet = Buffered::new(host);
        let loaded = check::resolve_sources(&mut quiet, &mut cfg).and_then(|_| check::load(&mut quiet, &cfg));
        let Buffered { out, .. } = quiet;
        match loaded {
            Ok(loaded) => {
                let names: Vec<String> = loaded.analysis.program.test_cases().into_iter().map(|t| t.name).collect();
                for n in &names {
                    host.out_line(n);
                }
                Ok((OK, json!({"test_cases": names})))
            }
            Err(e) => {
                for (stream, text, color) in out {
                    host.write(stream, &text, color);
                }
                Err(e)
            }
        }
    })
}

fn coverage(args: &[String], host: &mut dyn Host) -> i32 {
    if args.first().is_some_and(|a| a == "merge-observe") {
        return merge_observe(&args[1..], host);
    }
    let a = match parse_args(args, &[]) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, Some("coverage")),
    };
    if a.help {
        host.write(Stream::Out, &format!("{}\n{}", COVERAGE_HELP, OUTPUT_HELP), None);
        return OK;
    }
    let dir = match a.positional.as_slice() {
        [m] if m == "merge" => host.cwd().to_string_lossy().to_string(),
        [m, d] if m == "merge" => d.clone(),
        [m, _, extra, ..] if m == "merge" => {
            return usage_error(host, &format!("unexpected argument '{}'", extra), Some("coverage"))
        }
        [] => {
            let m = "missing subcommand: use `merge [DIR]` or `merge-observe REPORT.json...`";
            return usage_error(host, m, Some("coverage"));
        }
        [other, ..] => {
            let m = format!("unknown subcommand '{}': use `merge [DIR]` or `merge-observe REPORT.json...`", other);
            return usage_error(host, &m, Some("coverage"));
        }
    };
    with_output(host, &a, "coverage", |host| {
        let full = full_path(&host.cwd(), Path::new(&dir));
        if !host.is_dir(&full) {
            error(host, &format!("directory not found: {}", dir));
            return Err(Exit(ERROR));
        }
        let scenarios = scenario::merge_dir(host, &full, true);
        Ok((OK, json!({"scenarios": scenarios})))
    })
}

const MERGE_OBSERVE_OPTS: &[Opt] = &[flag("stdin", None)];

/// Merge portable observed reports without loading or executing P sources.
fn merge_observe(args: &[String], host: &mut dyn Host) -> i32 {
    use crate::checker::observe_report::ObserveCoverageReport;
    let a = match parse_args(args, MERGE_OBSERVE_OPTS) {
        Ok(a) => a,
        Err(e) => return usage_error(host, &e, Some("coverage")),
    };
    if a.help {
        host.write(Stream::Out, &format!("Usage: snappea coverage merge-observe [REPORT.json ...] [--stdin]\n\nMerges the coverage of `snappea observe` JSON results (or of merged ones) without\nloading P sources. Use it to combine observations from several processes or test\nruns. --stdin reads a JSON array of them.\n\nExample:\n  snappea coverage merge-observe reports/*.json --json-report suite.json\n\n{}", OUTPUT_HELP), None);
        return OK;
    }
    if a.positional.is_empty() && !a.has("stdin") {
        return usage_error(host, "provide report files or --stdin", Some("coverage"));
    }
    let merge = |host: &mut dyn Host| -> Result<ObserveCoverageReport, String> {
        let mut merged = ObserveCoverageReport::default();
        for file in &a.positional {
            let text =
                host.read_to_string(&full_path(&host.cwd(), Path::new(file))).map_err(|e| format!("{file}: {e}"))?;
            merged.merge(&ObserveCoverageReport::from_json_str(&text).map_err(|e| format!("{file}: {e}"))?)?;
        }
        if a.has("stdin") {
            let mut text = String::new();
            while let Some(line) = host.read_stdin_line().map_err(|e| e.to_string())? {
                text.push_str(&line);
                text.push('\n');
            }
            let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if !value.is_array() {
                return Err("--stdin expects a JSON array of full reports".into());
            }
            merged.merge(&ObserveCoverageReport::from_json(&value)?)?;
        }
        Ok(merged)
    };
    with_output(host, &a, "coverage", |host| match merge(host) {
        Ok(merged) => {
            host.write(Stream::Out, &merged.render_text(), None);
            Ok((OK, merged.to_json()))
        }
        Err(e) => {
            error(host, &e);
            Err(Exit(ERROR))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The `--options` a text mentions.
    fn mentioned(text: &str) -> BTreeSet<String> {
        text.match_indices("--")
            .filter(|(i, _)| !text[..*i].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '-'))
            .map(|(i, _)| text[i + 2..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect())
            .filter(|name: &String| !name.is_empty())
            .collect()
    }

    /// What `snappea <args> --help` prints.
    fn help_of(args: &[&str]) -> String {
        let mut h = crate::host::MemHost::default();
        let mut args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        args.push("--help".into());
        assert_eq!(run(&args, &mut h), OK, "{}", h.stderr);
        h.stdout
    }

    /// Each command's help names exactly the options it accepts, so the two cannot drift.
    #[test]
    fn every_help_names_exactly_the_options_its_command_accepts() {
        // (command, its options, whether its help lists the output options, options its
        // help and its table may disagree on: another command's, or one it rejects)
        type Case<'a> = (&'a [&'a str], &'a [Opt], bool, &'a [&'a str]);
        let commands: &[Case] = &[
            (&["check"], CHECK_OPTS, true, &["list-rules", "p-trace"]),
            (&["check", "mutation"], CHECK_OPTS, true, &["no-mutation"]),
            (&["check", "lint"], LINT_OPTS, true, &[]),
            (&["replay"], REPLAY_OPTS, true, &[]),
            (&["tests"], &[], true, &[]),
            (&["version"], &[], true, &[]),
            (&["observe"], super::super::observe::OPTS, true, &[]),
            // the overview of both subcommands
            (&["coverage"], &[], true, &["stdin"]),
            (&["coverage", "merge-observe"], MERGE_OBSERVE_OPTS, true, &[]),
            // the guide ignores the output options, so its help leaves them out
            (&["guide"], super::super::guide::OPTS, false, &[]),
        ];
        let mut wrong = Vec::new();
        for (args, opts, output, foreign) in commands {
            let help = help_of(args);
            let mut accepted: BTreeSet<String> =
                opts.iter().map(|o| o.long.to_string()).filter(|l| !foreign.contains(&l.as_str())).collect();
            accepted.insert("help".into());
            if *output {
                accepted.extend(OUTPUT_OPTS.iter().map(|o| o.long.to_string()));
            }
            let named = mentioned(&help);
            let cmd = args.join(" ");
            for name in named.iter().filter(|n| !accepted.contains(*n) && !foreign.contains(&n.as_str())) {
                wrong.push(format!("`snappea {cmd} --help` names --{name}, which it does not accept"));
            }
            for name in accepted.difference(&named).filter(|n| *n != "help") {
                wrong.push(format!("`snappea {cmd} --help` does not name --{name}"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// The reference page states the exit statuses as the help does.
    #[test]
    fn the_reference_page_states_the_exit_statuses() {
        let page = include_str!("../../../../docs/checking-models.md").replace("\r\n", "\n").replace('\n', " ");
        let status = exit_status!().trim().replace('\n', " ");
        assert!(page.contains(&status), "docs/checking-models.md should say: {status}");
    }

    /// `snappea help <command>` works for every command the help lists, and only those.
    #[test]
    fn help_lists_and_dispatches_every_command() {
        let listed: Vec<&str> = HELP
            .split("Commands:\n")
            .nth(1)
            .unwrap()
            .lines()
            .take_while(|l| !l.is_empty())
            .filter(|l| l.starts_with("  ") && !l.starts_with("   "))
            .filter_map(|l| l.split_whitespace().next())
            .collect();
        assert_eq!(listed, COMMANDS);
        for c in COMMANDS {
            let mut h = crate::host::MemHost::default();
            assert_eq!(run(&["help".into(), c.into()], &mut h), OK, "{c}: {}", h.stderr);
            assert_eq!(h.stdout, help_of(&[c]), "{c}");
        }
    }

    #[test]
    fn a_longer_timeout_is_suggested_only_up_to_a_minute() {
        assert_eq!(longer_timeout(2.0), Some("30s"));
        assert_eq!(longer_timeout(10.0), Some("1m"));
        assert_eq!(longer_timeout(30.0), None);
    }

    #[test]
    fn unproved_names_what_settled_and_what_did_not() {
        assert_eq!(unproved(1, 0), "1 test case not proved: 1 settled");
        assert_eq!(unproved(3, 1), "3 test cases not proved: 2 settled, 1 not settled");
        assert_eq!(capitalized("no bug"), "No bug");
    }

    #[test]
    fn globs() {
        assert!(glob_match("tc*", "tcA"));
        assert!(glob_match("*B", "tcB"));
        assert!(glob_match("t?A", "tcA"));
        assert!(glob_match("tcA", "tcA"));
        assert!(!glob_match("tc", "tcA"));
        assert!(glob_match("*a*b*", "xxaYYbzz"));
        assert!(!glob_match("*a*b", "xxaYYbzz"));
    }

    #[test]
    fn short_options_take_attached_values() {
        let spec = [value("workers", Some('j')), Opt { long: "verbose", short: Some('v'), takes_value: false }];
        let args = |v: &[&str]| parse_args(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &spec);
        assert_eq!(args(&["-j4"]).unwrap().get("workers"), Some("4"));
        assert_eq!(args(&["-j", "4"]).unwrap().get("workers"), Some("4"));
        assert!(args(&["-v"]).unwrap().has("verbose"));
        assert!(args(&["-vx"]).is_err());
    }

    #[test]
    fn people_durations() {
        assert_eq!(duration(0.184), "0.18s");
        assert_eq!(duration(9.999), "10.0s");
        assert_eq!(duration(59.96), "1m00s");
        assert_eq!(duration(125.0), "2m05s");
        assert_eq!(duration(3599.6), "1h00m");
        assert_eq!(duration(3720.0), "1h02m");
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("90"), Some(90));
        assert_eq!(parse_duration("90s"), Some(90));
        assert_eq!(parse_duration("5m"), Some(300));
        assert_eq!(parse_duration("1h"), Some(3600));
        assert_eq!(parse_duration("0"), None);
        assert_eq!(parse_duration("1.5m"), None);
        assert_eq!(parse_duration("5d"), None);
    }

    #[test]
    fn automatic_defaults_and_timeout() {
        let mut cfg = snappea_config();
        assert_eq!(cfg.strategy, StrategyKind::Auto);
        assert_eq!(cfg.timeout, Some(10));
        assert_eq!(cfg.iterations, 0);
        let args = parse_args(&["--timeout".into(), "30s".into()], CHECK_OPTS).unwrap();
        check_config(&args, &mut cfg).unwrap();
        assert_eq!(cfg.timeout, Some(30));
    }

    #[test]
    fn latency_profiles() {
        assert_eq!(split_latency("l.toml"), ("l.toml", None));
        assert_eq!(split_latency("l.toml:fast"), ("l.toml", Some("fast")));
        assert_eq!(split_latency("C:\\x\\l.toml"), ("C:\\x\\l.toml", None));
        assert_eq!(split_latency("C:\\x\\l.toml:fast"), ("C:\\x\\l.toml", Some("fast")));
    }

    #[test]
    fn check_summary_separates_check_outcomes_from_mutation_limits() {
        let uncovered = crate::checker::code_coverage::Uncovered {
            file: "coverage.p".into(),
            line: 9,
            end_line: 9,
            context: "Main.S, on ePing".into(),
            what: "if (x) never true".into(),
        };
        let coverage = check_summary(0, 2, 0, true, 3, &[uncovered], true, (0, false), Untested::default(), &[]);
        assert_eq!(coverage["status"], "coverage_failed");
        assert_eq!(coverage["message"], "Required coverage was not reached within the run budget (3 points)");
        assert_eq!(coverage["uncovered"][0]["file"], "coverage.p");
        assert_eq!(coverage["mutation_time_limit_reached"], true);

        let failed = check_summary(1, 2, 0, false, 0, &[], false, (0, false), Untested::default(), &[]);
        assert_eq!(failed["status"], "tests_failed");

        let incomplete = check_summary(0, 2, 1, false, 0, &[], false, (0, false), Untested::default(), &[]);
        assert_eq!(incomplete["status"], "search_incomplete");

        let passed = check_summary(0, 2, 0, false, 0, &[], false, (0, false), Untested::default(), &[]);
        assert_eq!(passed["status"], "passed");
        assert!(passed["note"].is_null());
    }

    #[test]
    fn check_summary_notes_skipped_checks_and_undetected_mutants() {
        let partial = check_summary(
            0,
            1,
            0,
            false,
            0,
            &[],
            false,
            (0, false),
            Untested::default(),
            &["mutation testing (--no-mutation)"],
        );
        assert_eq!(partial["status"], "passed");
        assert!(partial["note"].as_str().unwrap().starts_with("Not checked: mutation testing (--no-mutation)."));

        let denied = check_summary(0, 1, 0, false, 0, &[], false, (2, true), Untested::default(), &[]);
        assert_eq!(denied["status"], "mutation_failed");
        // the message names no count: the JSON field has it
        assert_eq!(
            denied["message"],
            "The test cases pass with changes to the model that a spec or assertion should detect"
        );
        assert_eq!(denied["mutants_undetected"], 2);
        assert!(denied["note"].is_null(), "{}", denied["note"]);

        // undetected mutants reported as warnings (--warn mutation) keep their count and add no note
        let weak = check_summary(0, 1, 0, false, 0, &[], false, (4, false), Untested::default(), &[]);
        assert_eq!(weak["status"], "passed");
        assert_eq!(weak["mutants_undetected"], 4);
        assert!(weak["note"].is_null(), "{}", weak["note"]);

        let untested = Untested { not_tested: 17, ..Untested::default() };
        let unfinished = check_summary(0, 1, 0, false, 0, &[], true, (0, false), untested, &[]);
        assert_eq!(unfinished["mutants_not_tested"], 17);
        assert_eq!(unfinished["status"], "passed");
        assert!(unfinished["note"].as_str().unwrap().contains("17 mutants not tested"));
        // a gate that denies undetected mutants fails when it could not test them all
        let gated = check_summary(0, 1, 0, false, 0, &[], true, (0, true), untested, &[]);
        assert_eq!(gated["status"], "mutation_incomplete");
        assert!(gated["message"].as_str().unwrap().contains("17 mutants not tested before the time limit"));
        assert!(gated["note"].as_str().unwrap().contains("--mutation-timeout"), "{}", gated["note"]);
    }
}
