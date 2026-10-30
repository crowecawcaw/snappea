//! `snappea observe`: checks logs of a running system against spec monitors.

use std::path::{Path, PathBuf};

use super::command::{
    error, flag, parse_args, plural, uint, usage_error, value, with_output, Args, Opt, Outcome, ERROR, OK, OUTPUT_HELP,
};
use crate::checker::code_coverage::uncovered_lines;
use crate::checker::observe::{ErrorKind, ObserveError, Report, Spec, UnknownEvents};
use crate::checker::observe_inputs::add_log_paths;
use crate::driver::check::Style;
use crate::driver::observe::{self as driver, InputKind, ObserveConfig};
use crate::host::{write_output, Color, Exit, Host, HostExt, HostFiles, Quiet, Stream};
use crate::project::{self, pproj::Includes, Input};

pub(super) const HELP: &str = "\
Usage: snappea observe [SOURCE...] [LOG...] --spec <NAME> [options]

Checks events logged by a real system against the model's spec monitors. Use it to
test that an implementation behaves as the model says: in integration tests, on
production logs, or live. A SOURCE is a .pproj or .p file; without any, the *.pproj
file in the current folder is used, or else the *.p files in it and its immediate
subfolders. Every other argument is a LOG: a file, a folder of logs, or a glob
(quoted; ** for subfolders). Logs are JSON lines by default.

Examples:
  snappea observe --spec BankBalanceCorrect logs/
  snappea observe --all-specs --strict --check-hot \"test-logs/**/*.jsonl\"
  my-service | snappea observe --spec BankBalanceCorrect --stdin

In Python tests, the pysnappea package (imported as `snappea`) checks events
without log files: `from snappea import check_events, check_spec`.

Specs:
      --spec <NAME>           Spec monitor to check; repeat or separate with commas
      --all-specs             Check every spec monitor. Use in test suites.
      --exclude-specs <NAME>  A spec --all-specs leaves out; repeat or use commas

Input:
  -l, --logs <PATH>           A LOG, as an option
      --live <PATH>           Read a live stream of lines until it ends (- is stdin),
                              stopping at the first error. Repeat for several; they
                              need --sequence.
      --stdin                 Same as --live -
      --sequence <FIELD>      Check the lines of live streams in the order of this JSON
                              field: a global sequence number from 1, with no gaps.
                              Use when several processes stream events.
      --record <FILE>         Write the checked live lines, in check order, to FILE,
                              to re-check or debug them later
      --p-trace               The logs are P JSON traces (bug.trace.json of snappea check)
      --parser <NAME>         Log parser: JsonLines (default), PTrace (--p-trace)
      --parser-config <JSON>  Parser settings; JsonLines takes the dotted paths of
                              its fields, e.g. {\"event\": \"msg.type\", \"key\": \"msg.id\"}
      --key <KEY>             Check only this key; repeat for several. Use to narrow
                              a large log to a failing key.
      --key-file <FILE>       Check only the keys in FILE, one per line
      --replay-window <N>     Events kept per key to report with an error (default: 100)
      --sorted                The logs are already in timestamp order: merge instead
                              of sorting (faster on large logs)

Checks:
      --check-hot             Report a key whose spec is in a hot state after its last
                              event: work that never finished (liveness)
      --strict                Fail a spec that checked no relevant event. Use in
                              tests to catch missing instrumentation.
      --unknown-events <P>    Events no P source declares: error (default), warn or
                              ignore. Use ignore for logs that mix in other events.

Output:
  -v, --verbose               Show progress, logged messages and each error's events

Prints one line per spec with its coverage, and its errors; then the handlers,
statements and branches of the specs no event ran.

Exit status: 0 no violation found, 1 violation found, 2 any other error.
";

pub(super) const OPTS: &[Opt] = &[
    value("spec", None),
    flag("all-specs", None),
    value("exclude-specs", None),
    value("logs", Some('l')),
    value("live", None),
    flag("stdin", None),
    value("sequence", None),
    value("record", None),
    flag("p-trace", None),
    value("parser", None),
    value("parser-config", None),
    value("key", None),
    value("key-file", None),
    value("replay-window", None),
    flag("sorted", None),
    flag("check-hot", None),
    flag("strict", None),
    value("unknown-events", None),
    flag("verbose", Some('v')),
];

/// Errors shown per spec; the rest are counted (all are in the JSON report).
const SHOWN_ERRORS: usize = 10;

/// The command line, validated.
struct Command {
    input: Input,
    cfg: ObserveConfig,
    record: Option<String>,
    verbose: bool,
}

pub(super) fn observe(args: &[String], host: &mut dyn Host) -> i32 {
    let a = match parse_args(args, OPTS) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, Some("observe")),
    };
    if a.help {
        host.write(Stream::Out, &format!("{}\n{}", HELP, OUTPUT_HELP), None);
        return OK;
    }
    let cmd = match command(&a, host) {
        Ok(c) => c,
        Err(m) => return usage_error(host, &m, Some("observe")),
    };
    with_output(host, &a, "observe", |host| run(host, cmd))
}

/// Names given to a repeatable option, also separated by commas.
fn names(a: &Args, long: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in a.all(long).iter().flat_map(|v| v.split(',')).map(str::trim).filter(|n| !n.is_empty()) {
        if !out.iter().any(|o| o == n) {
            out.push(n.to_string());
        }
    }
    out
}

fn command(a: &Args, host: &mut dyn Host) -> Result<Command, String> {
    let cwd = host.cwd();
    let mut input = Input { includes: Includes::FromProject, ..Input::default() };
    let mut logs: Vec<PathBuf> = Vec::new();
    let log_args = a.positional.iter().map(String::as_str).filter(|p| !is_source_arg(p));
    for p in a.positional.iter().filter(|p| is_source_arg(p)) {
        if p.to_lowercase().ends_with(".pproj") {
            if let Some(q) = &input.pproj {
                return Err(format!("two project files given: '{}' and '{}'", q, p));
            }
            input.pproj = Some(p.clone());
        } else {
            input.pfiles.push(p.clone());
        }
    }
    for l in log_args.chain(a.all("logs")) {
        add_log_paths(&HostFiles(host), &cwd, l, &mut logs)?;
    }
    let mut live: Vec<String> = Vec::new();
    for l in a.all("live").into_iter().chain(a.has("stdin").then_some("-")) {
        if l == "-" && live.iter().any(|x| x == "-") {
            return Err("stdin can be read only once (--stdin is --live -)".into());
        }
        live.push(l.to_string());
    }
    let mut cfg = ObserveConfig {
        specs: names(a, "spec"),
        all_specs: a.has("all-specs"),
        exclude_specs: names(a, "exclude-specs"),
        check_hot: a.has("check-hot"),
        strict: a.has("strict"),
        unknown_events: match a.get("unknown-events") {
            Some(v) => v.parse::<UnknownEvents>()?,
            // a misspelled event name would otherwise go unchecked
            None => UnknownEvents::Error,
        },
        input_kind: if a.has("p-trace") { InputKind::Json } else { InputKind::Text },
        sequence: a.get("sequence").map(str::to_string),
        record: a.has("record"),
        parser: a.get("parser").map(str::to_string),
        parser_configuration: a.get("parser-config").map(str::to_string),
        assume_sorted: a.has("sorted"),
        keys: names_raw(a, "key"),
        ..ObserveConfig::default()
    };
    if let Some(n) = uint(a, "replay-window")? {
        if n == 0 {
            return Err("'--replay-window' needs at least 1 event".into());
        }
        cfg.replay_window = n;
    }
    if let Some(f) = a.get("key-file") {
        let text = host
            .read_to_string(&crate::host::full_path(&cwd, Path::new(f)))
            .map_err(|e| format!("cannot read key file '{}': {}", f, e))?;
        for k in text.lines().map(str::trim).filter(|k| !k.is_empty()) {
            if !cfg.keys.iter().any(|x| x == k) {
                cfg.keys.push(k.to_string());
            }
        }
    }
    if cfg.specs.is_empty() && !cfg.all_specs {
        return Err("name the specs to check with --spec, or use --all-specs".into());
    }
    if !cfg.specs.is_empty() && cfg.all_specs {
        return Err("'--spec' and '--all-specs' cannot be combined".into());
    }
    if !cfg.exclude_specs.is_empty() && !cfg.all_specs {
        return Err("'--exclude-specs' needs '--all-specs'".into());
    }
    if !live.is_empty() && !logs.is_empty() {
        return Err("live input (--live, --stdin) cannot be combined with log files".into());
    }
    if !live.is_empty() && cfg.input_kind == InputKind::Json {
        return Err("'--p-trace' cannot be combined with live input (--live, --stdin)".into());
    }
    if live.len() > 1 && cfg.sequence.is_none() {
        return Err("several live inputs need --sequence to order their lines".into());
    }
    if live.is_empty() && (cfg.sequence.is_some() || cfg.record) {
        return Err("'--sequence' and '--record' apply to live input (--live, --stdin)".into());
    }
    if live.is_empty() && logs.is_empty() {
        return Err("no logs given: name log files or folders, or use --live".into());
    }
    cfg.logs = logs;
    cfg.live = live;
    Ok(Command { input, cfg, record: a.get("record").map(str::to_string), verbose: a.has("verbose") })
}

/// Values of a repeatable option, each once.
fn names_raw(a: &Args, long: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in a.all(long) {
        if !out.iter().any(|o| o == v) {
            out.push(v.to_string());
        }
    }
    out
}

fn is_source_arg(arg: &str) -> bool {
    let l = arg.to_lowercase();
    l.ends_with(".pproj") || l.ends_with(".p")
}

fn fail(host: &mut dyn Host, msg: &str) -> Exit {
    error(host, msg);
    Exit(ERROR)
}

fn run(host: &mut dyn Host, cmd: Command) -> Outcome {
    let Command { input, cfg, record, verbose } = cmd;
    let cwd = host.cwd();
    let mut con = Quiet::new(host, !verbose);
    let resolved = project::resolve(&mut con, &input)?;
    let spec_program = driver::compile(&mut con, &resolved.project_name, &resolved.p_files, &cwd, None, Style::Snappea)
        .map_err(|m| fail(con.inner, &m))?;
    let observer = driver::prepare(spec_program, &cfg, Vec::new()).map_err(|m| fail(con.inner, &m))?;
    if !cfg.live.is_empty() && observer.parser().log_delimiter().replace("\\n", "\n") != "\n" {
        return Err(fail(con.inner, "live input needs a parser of newline-delimited entries"));
    }
    let host = con.inner;
    let specs = observer.specs();
    let obs = observer.run(&cfg, host);
    if let Some(path) = &record {
        write_output(host, path, &obs.recorded).map_err(|_| Exit(ERROR))?;
    }
    if let Some(why) = &obs.unreadable {
        return Err(fail(host, &format!("cannot read log {}", why)));
    }
    let mut failed = 0;
    let mut uncovered = Vec::new();
    for (spec, report) in specs.iter().zip(&obs.reports) {
        if !report.errors.is_empty() {
            failed += 1;
        }
        print_spec(host, spec, report, verbose);
        uncovered.extend(report.coverage.uncovered());
    }
    uncovered.sort();
    uncovered.dedup();
    for l in uncovered_lines("Not covered by the events:", &uncovered, " (--json lists all)") {
        host.out_line(&l);
    }
    if obs.live && !obs.input_complete {
        host.out_line("Live input stopped at the first error; the results cover the lines checked.");
    }
    let n = specs.len();
    if n > 1 {
        host.out_line(&if failed == 0 {
            format!("All {} specs passed.", n)
        } else {
            format!("{} of {} specs failed.", failed, n)
        });
    }
    for warning in obs.warnings() {
        host.write(Stream::Err, &format!("warning: {}\n", warning), Some(Color::Yellow));
    }
    let code = Report::snappea_exit_code(obs.exit_status());
    Ok((code, driver::json_result(&specs, &obs)))
}

/// A spec's result line, and its errors.
fn print_spec(host: &mut dyn Host, spec: &Spec, r: &Report, verbose: bool) {
    let m = &r.metrics;
    let stats = format!(
        "{} event{}, {} key{}",
        m.events_read,
        plural(m.events_read as usize),
        m.partition_keys,
        plural(m.partition_keys as usize)
    );
    let n = r.errors.len();
    if n == 0 {
        host.out_line(&format!("ok    {} ({})", spec.name(), stats));
    } else {
        host.write(Stream::Out, "FAIL", Some(Color::Red));
        host.out_line(&format!("  {} ({} error{}; {})", spec.name(), n, plural(n), stats));
    }
    if verbose {
        for l in &r.log {
            host.out_line(&format!("      {}", l));
        }
    }
    for e in r.errors.iter().take(SHOWN_ERRORS) {
        for l in describe(e).lines() {
            host.out_line(&format!("      {}", l));
        }
        if let (true, Some(replay)) = (verbose, &e.replay) {
            for ev in &replay.events {
                let line = ev.log_line.clone().unwrap_or_else(|| ev.to_java_string(spec));
                host.out_line(&format!("        | {}", line));
            }
        }
    }
    if n > SHOWN_ERRORS {
        host.out_line(&format!("      ... and {} more (--json lists all)", n - SHOWN_ERRORS));
    }
    host.out_line(&format!("      Coverage: {}", r.coverage.summary()));
}

/// An error in one line or a few: what went wrong, and for which key.
fn describe(e: &ObserveError) -> String {
    let key = e.replay.as_ref().map(|r| r.key.as_str()).filter(|k| !k.is_empty());
    let what = match e.kind {
        ErrorKind::SpecViolation => e.message.clone(),
        ErrorKind::OutOfOrder => format!("out of order: {}", e.message),
        ErrorKind::Parser => match &e.cause {
            Some(c) => format!("cannot parse log entry ({}): {}", c, e.message),
            None => format!("cannot parse log entry: {}", e.message),
        },
        ErrorKind::Unknown => format!("error: {}", e.message),
        ErrorKind::Validation | ErrorKind::UnknownEvent => e.message.clone(),
    };
    match key {
        Some(k) => format!("key {}: {}", k, what),
        None => what,
    }
}
