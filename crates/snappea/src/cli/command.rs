//! The `snappea` command line.
//!
//! `snappea` runs the commands of [`crate::driver`] with its own options (test cases and
//! sources as arguments, automatic scheduling, durations with units, parallel workers,
//! time tracking) and its own output: a line per test case, the bug's report and replay
//! command, and a folder per test case ([`Style::Snappea`]). The upstream-style progress shows
//! with `--verbose`. Exit status: 0 when no bug or required coverage gap is found, 1 when
//! either is, 2 on any other error.

use std::path::Path;

use serde_json::{json, Value};

use crate::checker::code_coverage::{uncovered_lines, CodeMap, Hits, Scope, SHOWN_UNCOVERED};
use crate::checker::strategy::StrategyKind;
use crate::driver::check::{self, CheckConfig, Style};
use crate::driver::scenario;
use crate::host::{full_path, Buffered, Color, Exit, Host, HostExt, Quiet, Stream};
use crate::project::pproj::Includes;

pub(super) const OK: i32 = 0;
pub(super) const BUG: i32 = 1;
pub(super) const ERROR: i32 = 2;
// Internal status: run() turns argument failures into the public error result.
const USAGE_ERROR: i32 = -1;

const HELP: &str = "\
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
  guide      How to model a design and test code against it. Start here:
             `snappea guide`, then `snappea guide testing` or `snappea guide syntax`.
  version    Print the version

Typical loop:
  snappea check                       # check every test case
  snappea replay PCheckerOutput/tcFoo/bug.schedule -v   # debug a bug
  snappea check tcFoo                 # confirm the fix

Commands that read P sources take .pproj or .p files. Without any, they use the
*.pproj file in the current folder, or else the *.p files in it and its immediate
subfolders.

Output is text on a terminal or in CI, JSON otherwise (agents, scripts).
--format text|json, --json or SNAPPEA_FORMAT=text|json override it.
--json-report FILE also writes the JSON result to FILE.

Run `snappea <command> --help` for a command's options and examples.
Exit status: 0 no bug or required coverage gap, 1 bug or coverage gap, 2 error.
";

const CHECK_HELP: &str = "\
Usage: snappea check [SOURCE...] [TEST...] [options]
       snappea check lint [SOURCE...] [--list-rules]
       snappea check mutation [SOURCE...] [TEST...] [options]

Runs test cases over many schedules (event orders, nondeterministic choices) to
find assertion failures, spec violations and liveness bugs. A SOURCE is a .pproj or
.p file, a TEST a test case name or a glob (`*`, `?`). Without tests, all run.

Sources are linted first; findings fail with exit status 2. `check lint` only lints;
`check mutation` only runs mutation testing. See their --help.

Examples:
  snappea check                       # all test cases, 10s each
  snappea check 'tcRetry*' --timeout 1m   # more search time on a few
  snappea check --no-mutation         # faster iteration while editing

Exploration:
      --timeout <DURATION>  Time limit per test case (default: 10s; accepts 90s, 5m, 1h).
                            Raise it before trusting a pass on a large model.
  -j, --workers <N>         Worker limit (default: one per CPU core). Lower it on
                            shared machines.
      --seed <N>            Seed for reproducible choices. Not needed to reproduce
                            a bug: its .schedule replays exactly.

Strategies are chosen automatically. A search that covers every schedule ends
early; one that times out is incomplete.

Time (for models with timeouts or latency budgets):
      --latency <FILE>[:<PROFILE>]
                            Latency profiles (TOML), and the profile to use
      --timed               Time-aware scheduling: never run a step that is certainly
                            later than another. Use when untimed checking reports
                            orders real timing rules out, e.g. a timeout handled
                            before a reply that arrives earlier.

Mutation testing:
      --mutation-timeout <DURATION>
                            Time limit for mutation testing (default: 30s)
      --no-mutation         Skip mutation testing
      --allow-incomplete-coverage
                            Do not fail when selected code coverage is missing; useful
                            for exploratory checks while a test suite is being built.

Driver limits:
      --no-driver-facts     Do not list the limits each test's driver kept

Output:
  -o, --out <DIR>           Output folder (default: PCheckerOutput)
      --coverage-file <FILE>
                            Write every statement, branch and event handler no test
                            case ran to FILE
  -v, --verbose             Show progress and statistics, and log every schedule.
                            Uses one worker.

Prints one line per test case with its coverage, and for a bug its report and the
`snappea replay` command that reproduces it; then the code no test case ran. By default,
missing statements, event handlers, or `if` branch outcomes in selected code fail the
check. Use a scoped `coverage` suppression with a reason for deliberate omissions.
`--allow-incomplete-coverage` bypasses this gate for exploratory checks.

A bug's files go to <DIR>/<TEST>/, replacing the previous run's: bug.txt (the buggy
schedule's log; read this first), bug.schedule (for `snappea replay`) and
bug.trace.json (for `snappea observe --p-trace`). Scenario monitors also write
scenario_coverage.json.

When every test case passes, mutation testing follows: each mutant changes the model
slightly (a removed statement, a forced `if`, a swapped operator). A mutant no test
case detects marks code whose effect no spec or assertion checks. Undetected mutants
are listed but do not change the exit status. To accept one, put
`// snappea-lint: allow mutation[:<kind>] -- reason` before the statement
(see docs/checking-models.md).

Each passing test case also lists the limits its driver kept in every schedule: the
machines it created, the events it sent the model, in what order, and the values it
used. They show what the check never tried, e.g. `no Client sends eAcquire after
eRelease`; if the real environment can do that, extend the driver. The driver is the
test's main machine, machines declared under PTst/, and machines marked with a
`// snappea: driver` comment. Driver limits never change the exit status.
";

const MUTATION_HELP: &str = "\
Usage: snappea check mutation [SOURCE...] [TEST...] [options]

Runs the test cases, then runs them against mutants: copies of the model with one
small change (a removed statement, a forced `if`, a swapped operator). Lists every
mutant no test case detects: code whose effect no spec or assertion checks. Fix one
by adding an assertion or spec that the change breaks.

Use it to judge whether the specs are strong enough, e.g. as a CI gate. Unlike
`snappea check`, it reports no coverage or scenarios, and fails when a mutant goes
undetected.

Example:
  snappea check mutation tcBank --mutation-timeout 10m

Options:
      --mutation-timeout <DURATION>
                            Time limit for mutation testing (default: 5m)
      --timeout <DURATION>  Time limit per test case for the first run (default: 10s)
  -j, --workers <N>         Worker limit (default: one per CPU core)
      --seed <N>            Seed for reproducible choices
      --latency <FILE>[:<PROFILE>], --timed
                            As for `snappea check`
  -o, --out <DIR>           Output folder (default: PCheckerOutput)
  -v, --verbose             Show progress and statistics

`// snappea-lint: allow mutation[:<kind>] -- reason` before a statement, handler,
function or machine accepts its mutants. See docs/checking-models.md.
Exit status: 0 every mutant detected, 1 a mutant undetected or a test case failed,
2 error.
";

const REPLAY_HELP: &str = "\
Usage: snappea replay <FILE.schedule> [SOURCE...] [options]

Replays a schedule written by `snappea check`: the same event order and choices,
so the same bug. Use it to debug a bug (-v logs each step) and, after editing the
model, to confirm the fix. Exit status 1 means the bug was reproduced, 0 that it
was not.

Examples:
  snappea replay PCheckerOutput/tcBank/bug.schedule -v
  snappea replay bug.schedule Bank.pproj --test tcBank

Options:
      --test <NAME>         Test case to replay, for schedules that name none
      --max-steps <N>       The step options the schedule was found with. Copy
      --fair-max-steps <N>  them from the replay line `snappea check` prints;
      --fail-on-max-steps   only older schedules need them.
  -v, --verbose             Log the schedule
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

fn dispatch(args: &[String], host: &mut dyn Host) -> i32 {
    let Some((cmd, rest)) = args.split_first() else {
        host.write(Stream::Out, HELP, None);
        return OK;
    };
    match cmd.as_str() {
        "check" => check(rest, host),
        // an unadvertised alias of `check lint`
        "lint" => lint(rest, host),
        "replay" => replay(rest, host),
        "tests" => tests(rest, host),
        "observe" => super::observe::observe(rest, host),
        "coverage" => coverage(rest, host),
        "guide" => guide(rest, host),
        "version" | "--version" | "-V" => {
            let a = match parse_args(rest, &[]) {
                Ok(a) => a,
                Err(m) => return usage_error(host, &m, None),
            };
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
            Some(c @ ("check" | "replay" | "tests" | "observe" | "coverage" | "guide")) => {
                dispatch(&[c.to_string(), "--help".into()], host)
            }
            Some(c) => usage_error(host, &format!("unknown command '{}'", c), None),
        },
        c => usage_error(host, &format!("unknown command '{}'", c), None),
    }
}

const GUIDE_TOPICS: &[(&str, &str)] = &[
    ("workflow", include_str!("guide/workflow.md")),
    ("testing", include_str!("guide/testing.md")),
    ("syntax", include_str!("guide/syntax.md")),
];

const GUIDE_HELP: &str = "\
Usage: snappea guide [TOPIC]

Prints a short guide, as Markdown. Topics:
  workflow   (default) Model the design, check it, then test the code against it
  testing    Force the interleavings the model found in implementation tests, and check
             real calls against the P spec
  syntax     P syntax and a complete small model

Example:
  snappea guide testing
";

/// `snappea guide [topic]`: the modeling workflow, for people and agents new to snappea.
fn guide(args: &[String], host: &mut dyn Host) -> i32 {
    match args {
        [] => {
            host.write(Stream::Out, GUIDE_TOPICS[0].1, None);
            OK
        }
        [h] if h == "--help" || h == "-h" => {
            host.write(Stream::Out, GUIDE_HELP, None);
            OK
        }
        [topic] => match GUIDE_TOPICS.iter().find(|(name, _)| name == topic) {
            Some((_, text)) => {
                host.write(Stream::Out, text, None);
                OK
            }
            None => {
                let names: Vec<&str> = GUIDE_TOPICS.iter().map(|(n, _)| *n).collect();
                usage_error(
                    host,
                    &format!("unknown guide topic '{}'; topics: {}", topic, names.join(", ")),
                    Some("guide"),
                )
            }
        },
        _ => usage_error(host, "`snappea guide` takes at most one topic", Some("guide")),
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
            return Err(format!("unknown option '{}'", a));
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
      --format <F>          text or json. Default: text on a terminal or in CI (CI
                            set), else json. SNAPPEA_FORMAT sets the default.
      --json                Same as --format json
      --json-report <FILE>  Also write the JSON result to FILE
";

/// How a command prints its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Format {
    Text,
    Json,
}

/// `--json` or `--format`; else `SNAPPEA_FORMAT`; else text on a terminal or in CI (`CI`
/// set), JSON for programs.
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
    let ci = host.env_var("CI").is_some_and(|v| !matches!(v.as_str(), "" | "0" | "false"));
    Ok(if host.stdout_is_terminal() || ci { Format::Text } else { Format::Json })
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
    let mut capture = match format {
        Format::Text => Quiet::recording_errors(host),
        Format::Json => Quiet::capturing(host),
    };
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

/// Splits positionals into the sources (`.pproj`, `.p`) and the rest.
fn sources(cfg: &mut CheckConfig, positional: &[String]) -> Result<Vec<String>, String> {
    let mut rest = Vec::new();
    for a in positional {
        if a.to_lowercase().ends_with(".pproj") {
            if let Some(p) = &cfg.input.pproj {
                return Err(format!("two project files given: '{}' and '{}'", p, a));
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
            return Err(format!("no test case matches '{}'. Test cases: {}", p, names.join(", ")));
        }
    }
    Ok(names.iter().filter(|n| patterns.iter().any(|p| glob_match(p, n))).cloned().collect())
}

// ---- commands -----------------------------------------------------------------------------

const LINT_HELP: &str = "\
Usage: snappea check lint [SOURCE...] [--list-rules]

Type-checks and lints .p or .pproj sources without exploring schedules or writing
checker output. Fast; use it while editing. The same rules run before `snappea check`.
Exit status: 0 clean, 2 unsuppressed findings or source errors.

  --list-rules  List available rule IDs and descriptions without loading sources

Suppress an intentional finding on the following declaration or statement:
  // snappea-lint: allow unused-payload-field -- intentionally abstract this field
See docs/linting.md for rule details and suppression scope.
";

fn lint(args: &[String], host: &mut dyn Host) -> i32 {
    let a = match parse_args(args, &[flag("list-rules", None)]) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, Some("check lint")),
    };
    if a.help {
        host.write(Stream::Out, LINT_HELP, None);
        return OK;
    }
    if a.has("list-rules") && !a.positional.is_empty() {
        return usage_error(host, "--list-rules does not take sources", Some("check lint"));
    }
    let mut cfg = snappea_config();
    match sources(&mut cfg, &a.positional) {
        Ok(rest) if rest.is_empty() => {}
        Ok(_) => return usage_error(host, "lint accepts sources (.p or .pproj), not test names", Some("check lint")),
        Err(m) => return usage_error(host, &m, Some("check lint")),
    }
    with_output(host, &a, "lint", |host| {
        if a.has("list-rules") {
            let rules: Vec<_> = snappea_lint::Registry::builtins()
                .rules()
                .map(|r| {
                    host.out_line(&format!("{}  {}", r.id(), r.description()));
                    json!({"id": r.id(), "description": r.description()})
                })
                .collect();
            return Ok((OK, json!({"rules": rules})));
        }
        let mut quiet = Buffered::new(host);
        let loaded = check::resolve_sources(&mut quiet, &mut cfg).and_then(|_| check::load(&mut quiet, &cfg));
        let Buffered { out, .. } = quiet;
        match loaded {
            Ok(loaded) => {
                if crate::driver::lint::run(host, &loaded.sources)? != 0 {
                    return Err(Exit(ERROR));
                }
                host.out_line("Lint passed.");
                Ok((OK, json!({"findings": []})))
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
    value("timeout", None),
    value("time", None), // Existing spelling remains an unadvertised alias.
    value("workers", Some('j')),
    value("seed", None),
    value("latency", None),
    flag("timed", None),
    value("out", Some('o')),
    flag("verbose", Some('v')),
    value("mutation-timeout", None),
    flag("no-mutation", None),
    flag("allow-incomplete-coverage", None),
    flag("no-driver-facts", None),
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
        run_check(
            host,
            cfg,
            &patterns,
            parsed.get("latency"),
            Mutation::After(mutation),
            parsed.has("allow-incomplete-coverage"),
        )
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
        run_check(host, cfg, &patterns, parsed.get("latency"), Mutation::Only(limit), true)
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
    if a.get("timeout").is_some() && a.get("time").is_some() {
        return Err("use only '--timeout' ('--time' is an alias)".into());
    }
    if let Some(t) = a.get("timeout").or_else(|| a.get("time")) {
        cfg.timeout =
            Some(parse_duration(t).ok_or_else(|| format!("bad duration '{}' (examples: 90, 90s, 5m, 1h)", t))?);
    }
    cfg.workers = match uint(a, "workers")? {
        Some(0) => return Err("'--workers' needs at least 1 worker".into()),
        n => n,
    };
    if let Some(s) = a.get("seed") {
        cfg.seed =
            Some(s.parse().map_err(|_| format!("'--seed' needs a whole number from 0 to 4294967295, not '{}'", s))?);
    }
    if let Some(l) = a.get("latency") {
        cfg.latency_profile = split_latency(l).1.map(str::to_string);
    }
    cfg.timed = a.has("timed");
    if let Some(o) = a.get("out") {
        cfg.output_path = o.to_string();
    }
    cfg.verbose = a.has("verbose");
    cfg.driver_facts = !a.has("no-driver-facts");
    check::finish_options(cfg);
    Ok(patterns)
}

/// `--max-steps`, `--fair-max-steps` and `--fail-on-max-steps`, which `check` and `replay`
/// share (a schedule replays only with the step bounds it was found with).
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
        return Err("'--fair-max-steps' must be at least '--max-steps'".into());
    }
    cfg.fail_on_max_steps = a.has("fail-on-max-steps");
    Ok(())
}

/// The step options of `cfg` that differ from the defaults, as arguments.
fn step_args(cfg: &CheckConfig) -> Vec<String> {
    let default = CheckConfig::default();
    let mut out = Vec::new();
    if cfg.max_unfair_steps != default.max_unfair_steps {
        out.extend(["--max-steps".to_string(), cfg.max_unfair_steps.to_string()]);
    }
    if cfg.user_set_max_fair_steps {
        out.extend(["--fair-max-steps".to_string(), cfg.max_fair_steps.to_string()]);
    }
    if cfg.fail_on_max_steps {
        out.push("--fail-on-max-steps".into());
    }
    out
}

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
    allow_incomplete_coverage: bool,
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
    if crate::driver::lint::run(con.inner, &loaded.sources)? != 0 {
        return Err(Exit(ERROR));
    }
    let names: Vec<String> = loaded.analysis.program.test_cases().into_iter().map(|t| t.name).collect();
    let selected = select_tests(&names, patterns).map_err(|m| {
        error(con.inner, &m);
        Exit(ERROR)
    })?;
    if selected.is_empty() {
        error(con.inner, "the sources declare no test case");
        return Err(Exit(ERROR));
    }
    let n = selected.len();
    con.inner.out_line(&format!("Checking {}: {} test case{}", cfg.project_name, n, plural(n)));
    let mut failed = 0;
    let prog = &loaded.analysis.program;
    let probes = crate::checker::compile::compile_program_with_probes(prog).probes;
    let (mut scope, mut hits) = (Scope::default(), Hits::default());
    let mut cases = Vec::new();
    let mut scenarios = Vec::new();
    let mut incomplete = 0;
    let all_tests = prog.test_cases();
    let mut passed = Vec::new();
    for name in selected {
        cfg.test_case_name = name;
        check::set_output_directory(&mut con, &mut cfg)?;
        let case = check::testing_process(&mut con, &loaded, &cfg)?;
        if case.bugs > 0 {
            failed += 1;
        } else if case.exhaustive_finished != Some(true) {
            incomplete += 1;
        }
        print_case(con.inner, &case, &cfg);
        let mut result = case_json(&case, &cfg);
        if let Some((s, h)) = &case.code_coverage {
            let summary = CodeMap::new(prog, &probes, s).coverage(h).summary();
            if !only {
                con.inner.out_line(&format!(
                    "      Coverage: {}{}",
                    summary,
                    if case.bugs > 0 { " (partial: stopped at first bug)" } else { "" }
                ));
            }
            result["coverage"] = summary.to_json();
            result["coverage"]["partial"] = json!(case.bugs > 0);
            scope.add(s);
            hits.add(h);
            if case.bugs == 0 {
                if let Some(tc) = all_tests.iter().find(|t| t.name == case.test_case) {
                    passed.push((tc.clone(), h.clone()));
                }
            }
        }
        if let Some(d) = case.driver.as_ref().filter(|_| case.bugs == 0) {
            if !only {
                for l in driver_lines(d, &case) {
                    con.inner.out_line(&l);
                }
            }
            result["driver"] = json!({
                "machines": d.machines,
                "repeats": d.repeats,
                "facts": d.facts.iter().map(|f| f.to_json()).collect::<Vec<_>>(),
            });
        }
        if !case.scenario_coverage.scenarios.is_empty() {
            result["scenarios"] = serde_json::from_str(&case.scenario_coverage.to_json()).unwrap();
            scenarios.push(case.scenario_coverage.clone());
        }
        cases.push(result);
    }
    let map = CodeMap::new(prog, &probes, &scope);
    let suite = map.coverage(&hits);
    let coverage_suppressions = snappea_lint::coverage_suppressions(&loaded.sources.source_map()).unwrap_or_default();
    let required = map.required_summary(&suite, &coverage_suppressions);
    let required_uncovered = map.required_uncovered(&suite, &coverage_suppressions);
    let required_missing = required.statements.covered < required.statements.total
        || required.branches.covered < required.branches.total
        || required.handlers.covered < required.handlers.total;
    let required_gap_count = required.statements.total - required.statements.covered + required.branches.total
        - required.branches.covered
        + required.handlers.total
        - required.handlers.covered;
    let coverage_failed = !only && failed == 0 && !allow_incomplete_coverage && required_missing;
    let coverage_failure_message = format!("Required coverage is missing ({} points).", required_gap_count);
    let coverage_failure_help = "Add scenarios that reach the listed sites, or increase the timeout if exploration ran out of time. For deliberate omissions, add a scoped `coverage` suppression with a reason; use --allow-incomplete-coverage only for exploratory checking.";
    let excluded = scope.excluded(prog);
    if !only {
        if n > 1 {
            con.inner.out_line(&format!("Coverage of all test cases: {}", suite.summary()));
        }
        let heading = if n > 1 { "Not covered by any test case:" } else { "Not covered by the test case:" };
        for l in uncovered_lines(heading, &suite.uncovered(), " (--json lists all)") {
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
            con.inner.out_line("Coverage includes partial results from test cases stopped at their first bug.");
        }
        if coverage_failed {
            con.inner.out_line(&format!("{coverage_failure_message} {coverage_failure_help}"));
        } else if failed == 0 && allow_incomplete_coverage && !required_uncovered.is_empty() {
            con.inner.out_line(&format!(
                "Coverage gate bypassed: required coverage is missing ({} points).",
                required_gap_count
            ));
        }
        if !scenarios.is_empty() {
            con.inner.write(Stream::Out, &scenario::merge(&scenarios), None);
        }
    }
    let mut undetected = 0;
    let mut skipped = Vec::new();
    if !only && allow_incomplete_coverage && required_missing {
        skipped.push("required coverage (--allow-incomplete-coverage)");
    }
    if matches!(mutation, Mutation::After(None)) && failed == 0 {
        skipped.push("mutation testing (--no-mutation)");
    }
    let mutation = match mutation {
        Mutation::After(None) => json!({"skipped": "--no-mutation"}),
        _ if failed > 0 => {
            con.inner.out_line("Mutation testing skipped: a test case failed.");
            json!({"skipped": "a test case failed"})
        }
        Mutation::After(Some(limit)) | Mutation::Only(limit) => {
            // the sources parsed when they were loaded
            let suppressed = snappea_lint::mutation_suppressions(&loaded.sources.source_map()).unwrap_or_default();
            let sources = loaded.sources.source_map();
            let marked = snappea_lint::driver_machines(&sources).unwrap_or_default();
            let drivers = crate::checker::driver_facts::driver_machines(prog, &sources, &marked);
            let report =
                crate::driver::mutation::run(con.inner, prog, &cfg, &passed, &suppressed, &drivers, f64::from(limit));
            for l in report.lines(SHOWN_UNCOVERED) {
                con.inner.out_line(&l);
            }
            undetected = report.undetected().len();
            report.to_json()
        }
    };
    let seconds = con.inner.now() - start;
    let (elapsed, memory) = (duration(seconds), memory(con.inner));
    let mutation_time_limit_reached = mutation.get("time_limit_reached").and_then(Value::as_bool).unwrap_or(false);
    let summary = check_summary(
        failed,
        n,
        incomplete,
        coverage_failed,
        required_gap_count,
        &required_uncovered,
        mutation_time_limit_reached,
        undetected,
        &skipped,
    );
    let out = match cfg.output_path.trim_end_matches('/') {
        "" if cfg.output_path.is_empty() => ".",
        o => o,
    };
    con.inner.out_line(&if only && failed == 0 {
        match undetected {
            0 => format!("🫛 Every mutant was detected in {}{}.", elapsed, memory),
            n => format!("{} mutant{} not detected in {}{}.", n, plural(n), elapsed, memory),
        }
    } else if failed == 0 && (coverage_failed || incomplete > 0) {
        format!("{} in {}{}. Output: {}/", summary["message"].as_str().unwrap(), elapsed, memory, out)
    } else if failed == 0 && n == 1 {
        format!("🫛 The test case passed in {}{}. Output: {}/", elapsed, memory, out)
    } else if failed == 0 {
        format!("🫛 All {} test cases passed in {}{}. Output: {}/", n, elapsed, memory, out)
    } else {
        format!("{} of {} test case{} failed in {}{}. Output: {}/", failed, n, plural(n), elapsed, memory, out)
    });
    if let Some(note) = summary["note"].as_str() {
        con.inner.out_line(note);
    }
    if only {
        let result = json!({
            "project": cfg.project_name,
            "test_cases": cases,
            "failed": failed,
            "seconds": round_ms(seconds),
            "output": format!("{}/", out),
            "mutation": mutation,
        });
        return Ok((if failed > 0 || undetected > 0 { BUG } else { OK }, result));
    }
    let mut coverage = suite.to_json(false);
    coverage["excluded_components"] = excluded.iter().map(|u| u.to_json()).collect();
    coverage["partial"] = json!(failed > 0);
    coverage["required"] = required.to_json();
    coverage["uncovered_required"] = required_uncovered.iter().map(|u| u.to_json()).collect();
    coverage["gate_bypassed"] = json!(!only && allow_incomplete_coverage);
    coverage["failure"] = if coverage_failed {
        json!({"message": coverage_failure_message, "help": coverage_failure_help})
    } else {
        Value::Null
    };
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
        "mutation": mutation,
        "scenarios": scenarios.iter().map(|a| serde_json::from_str::<Value>(&a.to_json()).unwrap()).collect::<Vec<_>>(),
    });
    Ok((if failed > 0 || coverage_failed { BUG } else { OK }, result))
}

/// A concise top-level outcome. Mutation time limits are reported independently because
/// they do not change whether the test search or required-coverage gate passed.
#[allow(clippy::too_many_arguments)]
fn check_summary(
    failed: usize,
    test_cases: usize,
    incomplete: usize,
    coverage_failed: bool,
    coverage_gaps: usize,
    uncovered: &[crate::checker::code_coverage::Uncovered],
    mutation_time_limit_reached: bool,
    mutants_undetected: usize,
    skipped: &[&str],
) -> Value {
    // What a pass does not cover, so a partial check is not read as a full one.
    let mut notes = Vec::new();
    if !skipped.is_empty() {
        notes.push(format!("Skipped: {}. Run without these options before relying on the result.", skipped.join(", ")));
    }
    if failed == 0 && mutants_undetected > 0 {
        notes.push(format!(
            "{} mutant{} not detected: the model ran the changed code but no assertion failed. \
             Add assertions or scenarios that tell them apart, or suppress equivalent ones with a reason.",
            mutants_undetected,
            plural(mutants_undetected)
        ));
    }
    let (status, message) = if failed > 0 {
        ("tests_failed", format!("{} of {} test case{} failed", failed, test_cases, plural(test_cases)))
    } else if coverage_failed {
        ("coverage_failed", format!("Required coverage is missing ({} points)", coverage_gaps))
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
        "skipped": skipped,
        "note": if notes.is_empty() { Value::Null } else { json!(notes.join(" ")) },
    })
}

/// Driver facts shown for a test case; `--json` has them all.
const SHOWN_DRIVER_FACTS: usize = 5;

/// The console lines for what a test case's driver did in every schedule.
fn driver_lines(d: &check::DriverReport, case: &check::CaseOutcome) -> Vec<String> {
    if d.facts.is_empty() {
        return Vec::new();
    }
    let s = case.schedules;
    let schedules = match case.exhaustive_finished {
        Some(true) => format!("all {} schedule{}", s, plural(s)),
        _ => format!("all {} explored schedule{}", s, plural(s)),
    };
    let mut out = vec![format!("      Driver limits (held in {}):", schedules)];
    for f in d.facts.iter().take(SHOWN_DRIVER_FACTS) {
        out.push(format!("        {}", f.text));
    }
    if d.facts.len() > SHOWN_DRIVER_FACTS {
        let n = d.facts.len() - SHOWN_DRIVER_FACTS;
        out.push(format!("        ... and {} more (--json lists all)", n));
    }
    out.push("      hint: if the real environment can do more, extend the driver to try it".into());
    out
}

/// One test case's result as JSON.
fn case_json(case: &check::CaseOutcome, cfg: &CheckConfig) -> Value {
    let mut v = json!({
        "name": case.test_case,
        "result": if case.bugs > 0 { "bug" } else { "ok" },
        "schedules": case.schedules,
        "workers": case.workers,
        "seconds": round_ms(case.elapsed),
        "time_limit_reached": case.timed_out,
    });
    if let Some(finished) = case.exhaustive_finished {
        v["search_complete"] = json!(finished);
    }
    if case.bugs > 0 {
        v["bug"] = json!({
            "report": case.bug_report,
            "schedule": case.schedule_file,
            "replay": case.schedule_file.as_ref().map(|s| replay_command(s, cfg)),
        });
    }
    v
}

/// One test case's result line, and for a bug its report and how to replay it.
fn print_case(host: &mut dyn Host, case: &check::CaseOutcome, cfg: &CheckConfig) {
    let s = case.schedules;
    // a bug ends an exhaustive search early, so its extent only matters without one
    let schedules = match case.exhaustive_finished.filter(|_| case.bugs == 0) {
        Some(true) => format!("all {} schedule{}", s, plural(s)),
        Some(false) => format!("{} schedule{}, search incomplete", s, plural(s)),
        None => format!("{} schedule{}", s, plural(s)),
    };
    let workers = if case.workers > 1 { format!(", {} workers", case.workers) } else { String::new() };
    let limit = format!("{}{}", workers, if case.timed_out { ", time limit reached" } else { "" });
    let time = duration(case.elapsed);
    if case.bugs == 0 {
        host.out_line(&format!("ok    {} ({}{}, {})", case.test_case, schedules, limit, time));
        return;
    }
    host.write(Stream::Out, "FAIL", Some(Color::Red));
    host.out_line(&format!("  {} (bug found after {}{}, {})", case.test_case, schedules, limit, time));
    if let Some(report) = &case.bug_report {
        for l in report.lines() {
            host.out_line(&format!("      {}", l));
        }
    }
    if let Some(schedule) = &case.schedule_file {
        host.out_line(&format!("      Replay: {}", replay_command(schedule, cfg)));
    }
}

/// The `snappea replay` command for `schedule`, with the sources and step options the check
/// was given.
fn replay_command(schedule: &str, cfg: &CheckConfig) -> String {
    let mut words = vec!["snappea".to_string(), "replay".into(), schedule.to_string()];
    words.extend(cfg.input.pproj.iter().cloned());
    words.extend(cfg.input.pfiles.iter().cloned());
    words.extend(step_args(cfg));
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
        [] => return usage_error(host, "missing subcommand: merge", Some("coverage")),
        [other, ..] => return usage_error(host, &format!("unknown subcommand '{}'", other), Some("coverage")),
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

/// Merge portable observed reports without loading or executing P sources.
fn merge_observe(args: &[String], host: &mut dyn Host) -> i32 {
    use crate::checker::observe_report::ObserveCoverageReport;
    let a = match parse_args(args, &[flag("stdin", None)]) {
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
        let coverage = check_summary(0, 2, 0, true, 3, &[uncovered], true, 0, &[]);
        assert_eq!(coverage["status"], "coverage_failed");
        assert_eq!(coverage["message"], "Required coverage is missing (3 points)");
        assert_eq!(coverage["uncovered"][0]["file"], "coverage.p");
        assert_eq!(coverage["mutation_time_limit_reached"], true);

        let failed = check_summary(1, 2, 0, false, 0, &[], false, 0, &[]);
        assert_eq!(failed["status"], "tests_failed");

        let incomplete = check_summary(0, 2, 1, false, 0, &[], false, 0, &[]);
        assert_eq!(incomplete["status"], "search_incomplete");

        let passed = check_summary(0, 2, 0, false, 0, &[], false, 0, &[]);
        assert_eq!(passed["status"], "passed");
        assert!(passed["note"].is_null());
    }

    #[test]
    fn check_summary_notes_skipped_checks_and_undetected_mutants() {
        let partial = check_summary(0, 1, 0, false, 0, &[], false, 0, &["mutation testing (--no-mutation)"]);
        assert_eq!(partial["status"], "passed");
        assert!(partial["note"].as_str().unwrap().starts_with("Skipped: mutation testing (--no-mutation)."));

        let weak = check_summary(0, 1, 0, false, 0, &[], false, 4, &[]);
        assert_eq!(weak["mutants_undetected"], 4);
        assert!(weak["note"].as_str().unwrap().starts_with("4 mutants not detected"));
    }
}
