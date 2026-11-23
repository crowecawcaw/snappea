//! The `snappea` command line.
//!
//! `snappea` runs the commands of [`crate::driver`] with its own options (test cases and
//! sources as arguments, automatic scheduling, durations with units, parallel workers,
//! time tracking) and its own output: a line per test case, the bug's report and replay
//! command, and a folder per test case ([`Style::Snappea`]). The upstream-style progress shows
//! with `--verbose`. The exit statuses of `snappea` and `snappea check` are stated once,
//! in `exit_status!`.

use std::path::Path;

use serde_json::{json, Value};

use crate::checker::code_coverage::{uncovered_lines, CodeMap, Hits, Scope, SHOWN_UNCOVERED};
use crate::checker::runtime::StepBounds;
use crate::checker::strategy::StrategyKind;
use crate::driver::check::{self, CheckConfig, Gate, SuiteConfig};
use crate::driver::progress::Position;
use crate::driver::scenario;
use crate::host::{full_path, Buffered, Color, Exit, Host, HostExt, ProgressView, Quiet, Stream};

mod args;
mod check_cmd;
mod report;

pub(super) use args::*;
pub(super) use check_cmd::*;
pub(super) use report::*;

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
mutant or, with --strict or --deny mutation, an untested one; 2 an error, or
blocking lint findings with no bug.
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
                            coverage, mutation or all (repeat it, or list several
                            with commas). Use it to explore, not to pass: fix or
                            suppress findings before relying on the result.
      --deny <CHECK>        Fail on CHECK's findings, as by default. --deny mutation
                            (or all) also fails on mutants left untested, e.g. at
                            the time limit: use it for a final check. When --warn
                            and --deny name the same check (or all), the last one
                            wins.
      --strict              Also run the mutants that change only a value, and report
                            advisory lint rules; fail on both. Use it for a model
                            that guards a critical design, not while iterating.

Mutation testing:
      --mutation-timeout <DURATION>
                            Time limit for mutation testing (default: 30s)
      --no-mutation         Skip mutation testing

Driver limits:
      --no-driver-limits    Do not work out the limits each test's driver kept
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
      --no-driver-limits, --ignore-foreign-files, --inspect-trace
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
    let mut cfg = CheckConfig::snappea();
    let rest = match sources(&mut cfg.input, &a.positional) {
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
    let mut cfg = CheckConfig::snappea();
    // nothing runs, so foreign code is not left out of a check
    cfg.ignore_foreign_files = true;
    match sources(&mut cfg.input, &a.positional) {
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

const MERGE_OBSERVE_HELP: &str = "\
Usage: snappea coverage merge-observe [REPORT.json ...] [--stdin]

Merges the coverage of `snappea observe` JSON results (or of merged ones) without
loading P sources. Use it to combine observations from several processes or test
runs. --stdin reads a JSON array of them.

Example:
  snappea coverage merge-observe reports/*.json --json-report suite.json
";

/// Merge portable observed reports without loading or executing P sources.
fn merge_observe(args: &[String], host: &mut dyn Host) -> i32 {
    const CMD: Option<&str> = Some("coverage merge-observe");
    use crate::checker::observe_report::ObserveCoverageReport;
    let a = match parse_args(args, MERGE_OBSERVE_OPTS) {
        Ok(a) => a,
        Err(e) => return usage_error(host, &e, CMD),
    };
    if a.help {
        host.write(Stream::Out, &format!("{}\n{}", MERGE_OBSERVE_HELP, OUTPUT_HELP), None);
        return OK;
    }
    if a.positional.is_empty() && !a.has("stdin") {
        return usage_error(host, "provide report files or --stdin", CMD);
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
        let (mut cfg, mut suite) = (CheckConfig::snappea(), SuiteConfig::default());
        assert_eq!(cfg.strategy, StrategyKind::Auto);
        // the preset alone stops (library callers); the suite budget overrides it
        assert_eq!(cfg.timeout.map(f64::from), suite.budget);
        assert_eq!(suite.budget, Some(check::SUITE_BUDGET));
        assert_eq!(cfg.iterations, 0);
        let args = parse_args(&["--timeout".into(), "30s".into()], CHECK_OPTS).unwrap();
        check_config(&args, &mut cfg, &mut suite).unwrap();
        assert_eq!((cfg.timeout, suite.budget), (Some(30), None));
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
