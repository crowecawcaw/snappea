//! Checking recorded logs of a running system against spec monitors.
//!
//! [`compile`] type-checks the P sources with the specs, [`prepare`] picks the specs and the
//! log parser, and [`Observer::run`] checks the logs, read to their end or live (stopping
//! at the first error), and returns a [`Report`] per spec. [`json_report`] is the
//! machine-readable result. The command lines are `snappea observe` and, with PObserve's
//! options and console output, `snappea-compat observe`.
//!
//! Parsers are Rust values implementing [`Parser`]. Two are built in ([`JsonLinesParser`]
//! for text logs, [`PTraceParser`] for P's JSON traces); programs embedding the crate pass
//! their own to [`prepare`]. The checking itself is [`crate::checker::observe`].

use std::path::{Path, PathBuf};

use crate::checker::observe::{
    split_entries, ErrorKind, Metrics, NoScheduling, ObserveError, ObserveOptions, ObserveSession, ObservedEvent,
    ParsedEntry, Parser, Report, Spec, Timestamp, UnknownEvents,
};
use crate::checker::observe_inputs::{observe_live, SequenceMerger};
use crate::checker::observe_json::{event_from_json, json_int, parse_instant};
use crate::sema::{MachineId, MachineKind, Program};
use serde_json::Value as Json;

use crate::driver::check::Style;
use crate::host::{full_path, relative_path, Host, LiveInput};
use crate::project;

/// `PObserveExitStatus`.
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_CMDLINE_ERROR: i32 = 1;
pub const EXIT_ASSERTION: i32 = 2;
pub const EXIT_INTERNAL_ERROR: i32 = 3;
pub const EXIT_PARSE_LOG_ERROR: i32 = 4;

/// What the logs are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputKind {
    /// Log entries separated by the parser's delimiter (one per line by default).
    #[default]
    Text,
    /// P's JSON traces (`.trace.json`, as `snappea check` writes them).
    Json,
}

/// How spec and parser names are looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lookup {
    /// By their names.
    #[default]
    Exact,
    /// As PObserve finds classes in a jar: a spec whose generated supplier class
    /// (`<package>.PMachines$<Spec>$Supplier`) contains the name, with PObserve's errors.
    PObserve,
}

/// What to check and how.
#[derive(Debug, Clone)]
pub struct ObserveConfig {
    /// The specs to check (or, with `all_specs`, none).
    pub specs: Vec<String>,
    /// Every spec machine but `exclude_specs`.
    pub all_specs: bool,
    pub exclude_specs: Vec<String>,
    pub lookup: Lookup,
    /// Report a key whose spec is in a hot state after its last event.
    pub check_hot: bool,
    /// Require at least one checked relevant event per spec.
    pub strict: bool,
    /// Event names no P source declares.
    pub unknown_events: UnknownEvents,
    /// Log files (absolute), read to their end.
    pub logs: Vec<PathBuf>,
    pub input_kind: InputKind,
    /// Live inputs, as given (`-` is stdin), instead of `logs`.
    pub live: Vec<String>,
    /// The JSON field ordering the lines of several live inputs.
    pub sequence: Option<String>,
    /// Keep the checked live lines, in check order, in [`Observation::recorded`].
    pub record: bool,
    /// The parser's name; without it, the single registered parser or the built-in one
    /// for `input_kind`.
    pub parser: Option<String>,
    pub parser_configuration: Option<String>,
    /// Events kept per key for error reports.
    pub replay_window: usize,
    /// The logs are already in timestamp order.
    pub assume_sorted: bool,
    /// Check only these keys (all when empty).
    pub keys: Vec<String>,
}

impl Default for ObserveConfig {
    fn default() -> Self {
        ObserveConfig {
            specs: Vec::new(),
            all_specs: false,
            exclude_specs: Vec::new(),
            lookup: Lookup::Exact,
            check_hot: false,
            strict: false,
            unknown_events: UnknownEvents::Ignore,
            logs: Vec::new(),
            input_kind: InputKind::Text,
            live: Vec::new(),
            sequence: None,
            record: false,
            parser: None,
            parser_configuration: None,
            replay_window: 100,
            assume_sorted: false,
            keys: Vec::new(),
        }
    }
}

/// A type-checked P program with the specs.
pub struct SpecProgram {
    pub program: Program,
    /// Java package of PObserve's generated classes (`<pobserve-package>`), used in reports.
    pub java_package: String,
}

/// Reads and type-checks `files` (named relative to `root`); type errors are reported on
/// `host`, as `style`'s tool reports them. `java_package` defaults to
/// `<project_name>.pobserve`.
pub fn compile(
    host: &mut dyn Host,
    project_name: &str,
    files: &[PathBuf],
    root: &Path,
    java_package: Option<String>,
    style: Style,
) -> Result<SpecProgram, String> {
    let art = project::read(host, project_name, files, root)?;
    let sm = art.source_map();
    let r = crate::sema::compile(&sm, &crate::sema::Options::default());
    if !project::report_result(host, &art, &sm, &r, style == Style::Upstream) {
        return Err("The P specification does not type-check.".into());
    }
    // Observation enforces the spec rules of `snappea check lint`; upstream has no lint.
    if style == Style::Snappea
        && crate::driver::lint::run_observation(host, &sm).map_err(|_| "The P specification does not parse.")? > 0
    {
        return Err("The P specification has lint findings.".into());
    }
    let program = r.map(|a| a.program).map_err(|_| "The P specification does not type-check.".to_string())?;
    crate::checker::time::declares_time(&program)?;
    Ok(SpecProgram { program, java_package: java_package.unwrap_or_else(|| format!("{}.pobserve", project_name)) })
}

/// The specs and parser to check logs with.
pub struct Observer {
    program: Program,
    java_package: String,
    specs: Vec<MachineId>,
    parsers: Vec<Box<dyn Parser>>,
    parser: usize,
}

/// Picks the specs and the parser (`parsers` are added to the built-in ones) and
/// configures the parser.
pub fn prepare(
    spec_program: SpecProgram,
    cfg: &ObserveConfig,
    mut parsers: Vec<Box<dyn Parser>>,
) -> Result<Observer, String> {
    let parser = select_parser(cfg, &mut parsers)?;
    if let Some(c) = &cfg.parser_configuration {
        parsers[parser].set_configuration(c)?;
    }
    let specs = select_specs(&spec_program.program, cfg, &spec_program.java_package)?;
    Ok(Observer { program: spec_program.program, java_package: spec_program.java_package, specs, parsers, parser })
}

/// The result of [`Observer::run`].
#[derive(Debug, Clone, Default)]
pub struct Observation {
    /// One per spec, in [`Observer::specs`] order; only the first when a log could not be
    /// read.
    pub reports: Vec<Report>,
    /// The input was live.
    pub live: bool,
    /// Every input was read to its end (live input stops at the first error).
    pub input_complete: bool,
    /// With [`ObserveConfig::record`]: the checked live lines, each ending in a newline.
    pub recorded: String,
    /// A log could not be read or parsed: the file and why. [`Observation::reports`] then
    /// holds this one error, as upstream reports it.
    pub unreadable: Option<String>,
}

impl Observation {
    /// PObserve's exit status for all specs (`TrackErrors.getExitStatus`): see
    /// [`Report::exit_status`].
    pub fn exit_status(&self) -> i32 {
        Report::errors_exit_status(&self.reports.iter().flat_map(|r| r.errors.iter().cloned()).collect::<Vec<_>>())
    }

    /// Errors of all specs.
    pub fn error_count(&self) -> u64 {
        self.reports.iter().map(|r| r.error_count()).sum()
    }

    /// The specs' warnings, each once.
    pub fn warnings(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for w in self.reports.iter().flat_map(|r| r.warnings.iter()) {
            if !out.contains(&w.as_str()) {
                out.push(w);
            }
        }
        out
    }
}

impl Observer {
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// The specs to check.
    pub fn specs(&self) -> Vec<Spec<'_>> {
        self.specs
            .iter()
            .map(|&id| {
                let mut spec = Spec::from_decl(&self.program, id);
                spec.java_package = self.java_package.clone();
                spec
            })
            .collect()
    }

    pub fn parser(&self) -> &dyn Parser {
        self.parsers[self.parser].as_ref()
    }

    /// Checks the logs (or live inputs) of `cfg` against every spec.
    pub fn run(&self, cfg: &ObserveConfig, host: &mut dyn Host) -> Observation {
        let specs = self.specs();
        let parser = self.parser();
        let live = !cfg.live.is_empty();
        let cwd = host.cwd();
        let display = |f: &Path| if f.starts_with(&cwd) { relative_path(&cwd, f) } else { f.display().to_string() };
        let input_names: Vec<String> = if live {
            cfg.live
                .iter()
                .map(|v| if v == "-" { "stdin".to_string() } else { display(&full_path(&cwd, Path::new(v))) })
                .collect()
        } else {
            cfg.logs.iter().map(|f| display(f)).collect()
        };
        let opts = ObserveOptions {
            replay_window: cfg.replay_window,
            assume_sorted: cfg.assume_sorted,
            keys: cfg.keys.clone(),
            check_hot: cfg.check_hot,
            strict: cfg.strict,
            unknown_events: cfg.unknown_events,
            input_names,
        };
        let mut obs = Observation { live, input_complete: true, ..Observation::default() };
        if live {
            let mut strategies: Vec<_> = specs.iter().map(|_| NoScheduling).collect();
            let mut sessions: Vec<_> = specs
                .iter()
                .zip(strategies.iter_mut())
                .map(|(spec, strategy)| ObserveSession::new(spec, opts.clone(), strategy, host.now()))
                .collect();
            let inputs: Vec<LiveInput> = cfg
                .live
                .iter()
                .map(|v| if v == "-" { LiveInput::Stdin } else { LiveInput::Path(full_path(&cwd, Path::new(v))) })
                .collect();
            let mut merger = cfg.sequence.as_deref().map(SequenceMerger::new);
            let mut recorded = String::new();
            let mut record = |line: &str| {
                if cfg.record {
                    recorded.push_str(line);
                    recorded.push('\n');
                }
            };
            let mut source = host.open_live(&inputs);
            let outcome = observe_live(
                &mut sessions,
                parser,
                source.as_mut(),
                &opts.input_names,
                merger.as_mut(),
                &mut record,
                &mut || host.now(),
            );
            drop(source);
            obs.recorded = recorded;
            obs.input_complete = outcome.complete;
            obs.reports = sessions
                .into_iter()
                .map(|s| if outcome.complete { s.finish(true, host.now()) } else { s.finish_after_failure(host.now()) })
                .collect();
            return obs;
        }
        let mut inputs = Vec::new();
        for f in &cfg.logs {
            let text = match host.read_to_string(f) {
                Ok(t) => t,
                Err(e) => {
                    let m = format!("{} ({})", f.display(), e);
                    return obs.unreadable(m.clone(), m);
                }
            };
            match cfg.input_kind {
                InputKind::Text => inputs.push(split_entries(&text, &parser.log_delimiter())),
                InputKind::Json => match ptrace_entries(&text) {
                    Ok(v) => inputs.push(v),
                    Err(e) => {
                        let why = format!("{}: {}", f.display(), e);
                        return obs.unreadable(e, why);
                    }
                },
            }
        }
        let clock_host: &dyn Host = host;
        let mut clock = || clock_host.now();
        obs.reports = specs.iter().map(|spec| spec.observe(&inputs, parser, &opts, &mut clock)).collect();
        obs
    }
}

impl Observation {
    /// A log could not be read: upstream's reader exception escapes the stream and ends the
    /// run with one error, `m`; `why` names the file.
    fn unreadable(mut self, m: String, why: String) -> Observation {
        self.unreadable = Some(why);
        self.reports = vec![Report {
            errors: vec![ObserveError {
                cause: None,
                kind: ErrorKind::Unknown,
                detail: format!("java.lang.RuntimeException: {}", m),
                message: m,
                replay: None,
            }],
            metrics: Metrics { unknown_errors: 1, ..Default::default() },
            ..Default::default()
        }];
        self
    }
}

/// The machine-readable result (`--json-report`): PObserve's exit status (`exit_code`), and
/// per spec the errors with the key and the log entries of the events leading up to each one.
/// Live input adds whether it was read to its end (`input_complete`, `stop_reason`). Each
/// spec has its code coverage, which `snappea coverage merge-observe` merges.
pub fn json_result(specs: &[Spec], obs: &Observation) -> Json {
    let (reports, exit_code) = (&obs.reports, obs.exit_status());
    let specs: Vec<Json> = specs
        .iter()
        .zip(reports)
        .map(|(spec, r)| {
            let errors: Vec<Json> = r
                .errors
                .iter()
                .map(|e| {
                    let kind = e.kind.name();
                    let (key, events) = match &e.replay {
                        Some(rp) => (
                            Json::from(rp.key.clone()),
                            rp.events
                                .iter()
                                .map(|ev| {
                                    serde_json::json!({
                                        "key": ev.key,
                                        "timestamp": ev.timestamp.to_iso(),
                                        "event": spec.prog.event(ev.event).name,
                                        "log_line": ev.log_line,
                                    })
                                })
                                .collect(),
                        ),
                        None => (Json::Null, Vec::new()),
                    };
                    serde_json::json!({
                        "kind": kind,
                        "key": key,
                        "message": e.message,
                        "cause": e.cause,
                        "events": events,
                    })
                })
                .collect();
            let mut result = serde_json::json!({
                "spec": spec.name(),
                "events_read": r.metrics.events_read,
                "partition_keys": r.metrics.partition_keys,
                "verified_keys": r.metrics.verified_keys,
                "errors": errors,
            });
            result["coverage"] = r.coverage.to_json(true);
            if !r.unknown_events.is_empty() {
                result["unknown_events"] = r
                    .unknown_events
                    .iter()
                    .map(|u| {
                        serde_json::json!({
                            "event": u.name,
                            "count": u.count,
                            "location": u.location,
                            "suggestion": u.suggestion,
                        })
                    })
                    .collect();
            }
            if !r.warnings.is_empty() {
                result["warnings"] = Json::from(r.warnings.clone());
            }
            result
        })
        .collect();
    let mut json = serde_json::json!({ "exit_code": exit_code, "specs": specs });
    if obs.live {
        json["input_complete"] = Json::Bool(obs.input_complete);
        json["stop_reason"] = Json::from(if obs.input_complete { "eof" } else { "error" });
    }
    json
}

/// [`json_result`] as text.
pub fn json_report(specs: &[Spec], obs: &Observation) -> String {
    let mut text = serde_json::to_string_pretty(&json_result(specs, obs)).unwrap_or_default();
    text.push('\n');
    text
}

/// The specs to check: each named one, or with `all_specs` every spec machine but the
/// excluded ones.
fn select_specs(prog: &Program, cfg: &ObserveConfig, package: &str) -> Result<Vec<MachineId>, String> {
    if !cfg.all_specs {
        return cfg.specs.iter().map(|n| select_spec(prog, n, package, cfg.lookup)).collect();
    }
    let is_spec = |m: &&crate::sema::MachineDecl| m.kind == MachineKind::Spec;
    for x in &cfg.exclude_specs {
        if !prog.machines.iter().filter(is_spec).any(|m| &m.name == x) {
            return Err(format!("--exclude-specs: there is no spec named {}.", x));
        }
    }
    let specs: Vec<MachineId> = prog
        .machines
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind == MachineKind::Spec && !cfg.exclude_specs.contains(&m.name))
        .map(|(i, _)| MachineId(i as u32))
        .collect();
    if specs.is_empty() {
        return Err("--all-specs: the P sources declare no specs to check.".into());
    }
    Ok(specs)
}

/// The spec machine `name`. With [`Lookup::PObserve`], as
/// `CreateInstanceFromJar.getInstance(SPECIFICATION_SUPPLIER, name)`: the spec machine whose
/// generated supplier class name (`<package>.PMachines$<Spec>$Supplier`) contains `name`.
fn select_spec(prog: &Program, name: &str, package: &str, lookup: Lookup) -> Result<MachineId, String> {
    let specs = || prog.machines.iter().enumerate().filter(|(_, m)| m.kind == MachineKind::Spec);
    if lookup == Lookup::Exact {
        return match specs().find(|(_, m)| m.name == name) {
            Some((i, _)) => Ok(MachineId(i as u32)),
            None => {
                let names: Vec<&str> = specs().map(|(_, m)| m.name.as_str()).collect();
                Err(if names.is_empty() {
                    format!("no spec named {}; the P sources declare no specs", name)
                } else {
                    format!("no spec named {}. Specs: {}", name, names.join(", "))
                })
            }
        };
    }
    let found: Vec<usize> = specs()
        .filter(|(_, m)| format!("{}.PMachines${}$Supplier", package, m.name).contains(name))
        .map(|(i, _)| i)
        .collect();
    match found.as_slice() {
        [] => Err("SPECIFICATION_SUPPLIER not found in the specified jar.".into()),
        [i] => Ok(MachineId(*i as u32)),
        many => Err(format!(
            "Found multiple SPECIFICATION_SUPPLIERs in the jar({})",
            many.iter().map(|_| "Supplier").collect::<Vec<_>>().join(", ")
        )),
    }
}

/// The parser named `cfg.parser` among the registered parsers, then the built-in ones
/// (PObserve: `CreateInstanceFromJar.getInstance(PARSER, name)`). Without a name, a single
/// registered parser is used, else the built-in parser for the input kind.
fn select_parser(cfg: &ObserveConfig, parsers: &mut Vec<Box<dyn Parser>>) -> Result<usize, String> {
    let registered = parsers.len();
    parsers.push(Box::new(JsonLinesParser::default()));
    parsers.push(Box::new(PTraceParser));
    let pobserve = cfg.lookup == Lookup::PObserve;
    let names = |ps: &[Box<dyn Parser>]| ps.iter().map(|p| p.name().to_string()).collect::<Vec<_>>().join(", ");
    match &cfg.parser {
        Some(n) => parsers.iter().position(|p| p.name() == n).ok_or_else(|| {
            if pobserve {
                "PARSER not found in the specified jar.".into()
            } else {
                format!("no parser named {}. Parsers: {}", n, names(parsers))
            }
        }),
        None => match registered {
            0 => Ok(match cfg.input_kind {
                InputKind::Text => registered,
                InputKind::Json => registered + 1,
            }),
            1 => Ok(0),
            _ if pobserve => Err(format!("Found multiple PARSERs in the jar({})", names(&parsers[..registered]))),
            _ => Err(format!(
                "several parsers are registered ({}); pick one with --parser",
                names(&parsers[..registered])
            )),
        },
    }
}

/// [`event_from_json`] as a [`ParsedEntry`] that names `name` when the program does not
/// declare it.
fn parsed_from_json(
    spec: &Spec,
    key: String,
    timestamp: Timestamp,
    name: &str,
    payload: Option<&Json>,
    log_line: Option<&str>,
) -> Result<ParsedEntry, String> {
    Ok(match event_from_json(spec, key, timestamp, name, payload, log_line)? {
        Some(e) => ParsedEntry { events: vec![e], unknown_events: Vec::new() },
        None => ParsedEntry { events: Vec::new(), unknown_events: vec![name.to_string()] },
    })
}

// ---- built-in parsers ----------------------------------------------------------------------

/// One JSON object per log line:
/// `{"key": "acct-1", "timestamp": 1706894854309, "event": "eWithDrawReq", "payload": {...}}`.
///
/// `timestamp` is epoch milliseconds or an ISO-8601 string; `key` and `timestamp` are
/// optional (missing ones read as `""` and `0`). `--parserConfiguration` remaps the fields
/// with a JSON object of dotted paths, e.g. `{"event": "msg.type", "key": "msg.account"}`.
/// Blank lines and events the program does not declare are skipped.
#[derive(Debug, Clone)]
pub struct JsonLinesParser {
    pub key: String,
    pub timestamp: String,
    pub event: String,
    pub payload: String,
}

impl Default for JsonLinesParser {
    fn default() -> Self {
        JsonLinesParser {
            key: "key".into(),
            timestamp: "timestamp".into(),
            event: "event".into(),
            payload: "payload".into(),
        }
    }
}

fn lookup<'j>(j: &'j Json, path: &str) -> Option<&'j Json> {
    if path.is_empty() {
        return Some(j);
    }
    path.split('.').try_fold(j, |cur, part| cur.get(part))
}

fn key_string(j: Option<&Json>) -> String {
    match j {
        None | Some(Json::Null) => String::new(),
        Some(Json::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

fn timestamp_of(j: Option<&Json>) -> Result<Timestamp, String> {
    match j {
        None | Some(Json::Null) => Ok(Timestamp::default()),
        Some(Json::String(s)) => match s.parse::<i64>() {
            Ok(ms) => Ok(Timestamp::from_millis(ms)),
            Err(_) => parse_instant(s).ok_or_else(|| format!("invalid timestamp '{}'", s)),
        },
        Some(n) => json_int(n).map(Timestamp::from_millis).ok_or_else(|| format!("invalid timestamp {}", n)),
    }
}

impl Parser for JsonLinesParser {
    fn name(&self) -> &str {
        "JsonLines"
    }

    fn set_configuration(&mut self, configuration: &str) -> Result<(), String> {
        let j: Json = serde_json::from_str(configuration)
            .map_err(|e| format!("The specified parserConfiguration ({}) is invalid. ({})", configuration, e))?;
        let Some(o) = j.as_object() else {
            return Err(format!(
                "The specified parserConfiguration ({}) is invalid. (expected a JSON object)",
                configuration
            ));
        };
        for (k, v) in o {
            let v = v.as_str().ok_or_else(|| format!("parserConfiguration: '{}' must be a string", k))?.to_string();
            match k.as_str() {
                "key" => self.key = v,
                "timestamp" => self.timestamp = v,
                "event" => self.event = v,
                "payload" => self.payload = v,
                other => return Err(format!("parserConfiguration: unknown field '{}'", other)),
            }
        }
        Ok(())
    }

    fn parse(&self, entry: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
        self.parse_entry(entry, spec).map(|p| p.events)
    }

    fn parse_entry(&self, entry: &str, spec: &Spec) -> Result<ParsedEntry, String> {
        if entry.is_empty() {
            return Ok(ParsedEntry::default());
        }
        let j: Json = serde_json::from_str(entry).map_err(|e| e.to_string())?;
        let name = lookup(&j, &self.event).and_then(|v| v.as_str()).ok_or("missing event name")?;
        let ts = timestamp_of(lookup(&j, &self.timestamp))?;
        let key = key_string(lookup(&j, &self.key));
        parsed_from_json(spec, key, ts, name, lookup(&j, &self.payload), Some(entry))
    }
}

/// The `SendEvent` and `Announce` entries of P's JSON traces (`p check` bug traces), for
/// `--inputkind JSON`. All events share one key and keep their trace order.
#[derive(Debug, Clone, Copy, Default)]
pub struct PTraceParser;

impl Parser for PTraceParser {
    fn name(&self) -> &str {
        "PTrace"
    }

    fn parse(&self, entry: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
        self.parse_entry(entry, spec).map(|p| p.events)
    }

    fn parse_entry(&self, entry: &str, spec: &Spec) -> Result<ParsedEntry, String> {
        let j: Json = serde_json::from_str(entry).map_err(|e| e.to_string())?;
        let details = j.get("details").ok_or("missing details")?;
        let name = details.get("event").and_then(|v| v.as_str()).ok_or("missing event name")?;
        let name = name.rsplit('.').next().unwrap_or(name);
        let log = details.get("log").and_then(|v| v.as_str());
        parsed_from_json(spec, String::new(), Timestamp::default(), name, details.get("payload"), log)
    }
}

/// `PTraceJSONFileReader`: the `SendEvent`/`Announce` entries of a JSON trace, each as JSON
/// text.
fn ptrace_entries(text: &str) -> Result<Vec<String>, String> {
    let j: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let arr = j.as_array().ok_or("expected a JSON array")?;
    let mut out = Vec::new();
    for e in arr {
        let ty = e.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if ty.eq_ignore_ascii_case("sendevent") || ty.eq_ignore_ascii_case("announce") {
            out.push(e.to_string());
        }
    }
    Ok(out)
}
