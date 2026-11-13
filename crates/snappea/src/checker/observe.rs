//! Runtime monitoring: checking recorded events of a running system against a spec
//! monitor, as upstream's PObserve does.
//!
//! A [`Parser`] turns log entries into timestamped, keyed [`ObservedEvent`]s. Events the
//! spec does not observe are dropped; the rest are partitioned by key, each partition is
//! sorted by timestamp and fed to a fresh instance of the spec. A failed assertion, an
//! event arriving out of order (when inputs are assumed sorted) or any other error ends the
//! check of that key and is reported with the last events seen for it (the replay window).
//!
//! Specs run with the semantics of upstream's Java backend
//! ([`Setup::java_semantics`](crate::checker::runtime::Setup::java_semantics)), so messages read
//! as PObserve's do. This module does no I/O: inputs are passed
//! as strings and time comes from a caller-supplied clock.

use std::collections::HashMap;
use std::rc::Rc;

use crate::sema::{EventId, MachineId, ModuleInfo, PType, Program};

use crate::checker::code_coverage::{CodeCoverage, CodeMap, Hits, Scope};
use crate::checker::compile::{compile_program_with_probes, Code};
use crate::checker::coverage::{Coverage, CoverageInfo, CoverageTables};
use crate::checker::javafmt::{self, JavaFormatter};
use crate::checker::runtime::{Config, Event, Failure, Runtime, Setup};
use crate::checker::strategy::{OpInfo, Strategy};
use crate::checker::time::{declares_time, Stamp, Time, TimeOptions};
use crate::checker::value::{NamedTuple, Value};

/// A point in time (`java.time.Instant`): seconds since the epoch plus nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Timestamp {
    pub secs: i64,
    pub nanos: u32,
}

impl Timestamp {
    /// `Instant.ofEpochSecond(secs, nanos)`, saturating where Java would throw.
    pub fn new(secs: i64, nanos: u32) -> Timestamp {
        Timestamp { secs: secs.saturating_add(i64::from(nanos / 1_000_000_000)), nanos: nanos % 1_000_000_000 }
    }

    /// `Instant.ofEpochMilli`.
    pub fn from_millis(ms: i64) -> Timestamp {
        Timestamp { secs: ms.div_euclid(1000), nanos: (ms.rem_euclid(1000) * 1_000_000) as u32 }
    }

    /// `Instant.toEpochMilli`, saturating where Java would throw.
    pub fn to_millis(&self) -> i64 {
        self.secs.saturating_mul(1000).saturating_add(i64::from(self.nanos / 1_000_000))
    }

    /// Epoch microseconds for the P time functions. Sub-microsecond precision is
    /// truncated toward the preceding microsecond; values outside `int` saturate.
    pub fn to_micros(&self) -> i64 {
        (i128::from(self.secs) * 1_000_000 + i128::from(self.nanos / 1000))
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }

    /// `Instant.toString()`.
    pub fn to_iso(&self) -> String {
        javafmt::instant_to_string(self.secs, self.nanos)
    }
}

/// One event recovered from the logs (`PObserveEvent`).
#[derive(Debug, Clone)]
pub struct ObservedEvent {
    /// Partition key: events with the same key are checked by one spec instance.
    pub key: String,
    pub timestamp: Timestamp,
    pub event: EventId,
    pub payload: Value,
    /// The log entry the event came from.
    pub log_line: Option<String>,
    /// Extra data shown in reports.
    pub custom_payload: Option<String>,
}

impl ObservedEvent {
    /// `PObserveEvent.toString()`.
    pub fn to_java_string(&self, spec: &Spec) -> String {
        format!(
            "PObserveEvent{{partitionKey='{}', timestamp={}, event={}, logLine='{}', customPayload={}}}",
            self.key,
            javafmt::instant_nanos9(self.timestamp.secs, self.timestamp.nanos),
            spec.java.event_to_string(self.event, &self.payload),
            self.log_line.as_deref().unwrap_or("null"),
            self.custom_payload.as_deref().unwrap_or("null"),
        )
    }
}

/// Turns log entries into events (upstream's `Parser` interface).
pub trait Parser {
    /// The name `--parser` selects the parser by.
    fn name(&self) -> &str;

    /// What separates log entries (`getLogDelimiter`): `\n` for one entry per line.
    fn log_delimiter(&self) -> String {
        "\n".into()
    }

    /// Receives `--parserConfiguration` (`setConfiguration`).
    fn set_configuration(&mut self, _configuration: &str) -> Result<(), String> {
        Ok(())
    }

    /// Parses one (trimmed) log entry. Entries that hold no event return no events; an
    /// `Err` counts as a parser error for this entry only. As upstream, the report names the
    /// entry, not the error message.
    fn parse(&self, entry: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String>;

    /// [`parse`](Parser::parse), also naming the events in the entry that no loaded P source
    /// declares, for [`ObserveOptions::unknown_events`]. Parsers that drop such events
    /// silently should override this; the default reports none.
    fn parse_entry(&self, entry: &str, spec: &Spec) -> Result<ParsedEntry, String> {
        Ok(ParsedEntry { events: self.parse(entry, spec)?, ..ParsedEntry::default() })
    }
}

/// What a parser recovered from one log entry.
#[derive(Debug, Clone, Default)]
pub struct ParsedEntry {
    pub events: Vec<ObservedEvent>,
    /// Names of events in the entry that the P sources do not declare.
    pub unknown_events: Vec<String>,
    /// The key of each name in `unknown_events`, in the same order, when the parser knows
    /// it. Lets a report say an unknown event is in a key `--key` did not select.
    pub unknown_keys: Vec<String>,
}

/// A spec monitor ready to check events, plus helpers parsers use to build them.
pub struct Spec<'p> {
    pub prog: &'p Program,
    pub decl: MachineId,
    /// Java package of the generated classes (`<pobserve-package>`), used in reports.
    pub java_package: String,
    code: Code,
    module: ModuleInfo,
    config: Config,
    time: Result<bool, String>,
    java: JavaFormatter<'p>,
    /// The spec's code coverage points, and the tables its runs record handlers in.
    map: CodeMap,
    tables: Rc<CoverageTables>,
}

impl<'p> Spec<'p> {
    /// The spec machine called `name`.
    pub fn new(prog: &'p Program, name: &str) -> Result<Spec<'p>, String> {
        let decl = prog
            .machines
            .iter()
            .position(|m| m.name == name && m.kind == crate::sema::MachineKind::Spec)
            .map(|i| MachineId(i as u32))
            .ok_or_else(|| format!("no spec machine named '{}'", name))?;
        Ok(Spec::from_decl(prog, decl))
    }

    pub fn from_decl(prog: &'p Program, decl: MachineId) -> Spec<'p> {
        let config = Config { logging: false, max_work_per_step: 0, ..Config::default() };
        let code = compile_program_with_probes(prog);
        let map = CodeMap::new(prog, &code.probes, &Scope::machine(prog, decl));
        Spec {
            prog,
            decl,
            java_package: "pobserve".into(),
            code,
            module: ModuleInfo::default(),
            config,
            time: declares_time(prog),
            java: JavaFormatter::new(prog),
            map,
            tables: Rc::new(CoverageTables::new(prog)),
        }
    }

    pub fn name(&self) -> &str {
        &self.prog.machine(self.decl).name
    }

    /// Adds what a run recorded in `info` to the spec's coverage `c`.
    fn record_coverage(&self, c: &mut CodeCoverage, info: &CoverageInfo) {
        self.map.record(c, &Hits::from_info(self.prog, &self.tables, info));
    }

    /// The events the spec observes (`getEventTypes()`), in declaration order.
    pub fn observes(&self) -> &[EventId] {
        &self.prog.machine(self.decl).observes
    }

    /// The event declared as `name`.
    pub fn event_id(&self, name: &str) -> Option<EventId> {
        self.prog.events.iter().position(|e| e.name == name).map(|i| EventId(i as u32))
    }

    /// The payload type of event `e`.
    pub fn payload_type(&self, e: EventId) -> &PType {
        &self.prog.event(e).payload
    }

    /// The value of enum element `elem` of enum `ty`.
    pub fn enum_value(&self, ty: &str, elem: &str) -> Option<Value> {
        let en = self.prog.enums.iter().find(|e| e.name == ty)?;
        en.elems.iter().find(|x| x.name == elem).map(|x| Value::Int(x.value))
    }

    /// Builds an event from its name and payload.
    pub fn event(
        &self,
        key: impl Into<String>,
        timestamp: Timestamp,
        name: &str,
        payload: Value,
        log_line: Option<&str>,
    ) -> Result<ObservedEvent, String> {
        let event = self.event_id(name).ok_or_else(|| format!("unknown event '{}'", name))?;
        Ok(ObservedEvent {
            key: key.into(),
            timestamp,
            event,
            payload,
            log_line: log_line.map(|s| s.to_string()),
            custom_payload: None,
        })
    }

    /// Builds an event whose payload is a tuple, from its field values in declaration order
    /// (field names come from the event's payload type).
    pub fn tuple_event(
        &self,
        key: impl Into<String>,
        timestamp: Timestamp,
        name: &str,
        values: Vec<Value>,
        log_line: Option<&str>,
    ) -> Result<ObservedEvent, String> {
        let id = self.event_id(name).ok_or_else(|| format!("unknown event '{}'", name))?;
        let mut ty = self.payload_type(id);
        while let PType::TypeDef(t) = ty {
            ty = &self.prog.typedefs[t.idx()].ty;
        }
        let payload = match ty {
            PType::NamedTuple(fs) if fs.len() == values.len() => {
                let names: Vec<String> = fs.iter().map(|(n, _)| n.clone()).collect();
                Value::NamedTuple(Rc::new(NamedTuple { names: names.into(), values }))
            }
            PType::Tuple(ts) if ts.len() == values.len() => Value::Tuple(Rc::new(values)),
            _ => return Err(format!("the payload of '{}' is not a tuple of {} fields", name, values.len())),
        };
        self.event(key, timestamp, name, payload, log_line)
    }

    /// `true` if events of this kind reach the spec (upstream's `filterBasedOnSpecObservation`).
    pub fn observes_event(&self, e: EventId) -> bool {
        self.observes().contains(&e)
    }
}

// ---- log entries ---------------------------------------------------------------------------

/// Splits a log file into entries as upstream's `LogBreaker` does and trims each one.
///
/// * `\n` (or the two characters `\` `n`): one entry per line.
/// * `\n<marker>\n`: entries span lines and are separated by lines equal to `<marker>`; a
///   last entry not followed by a marker line is dropped, as upstream does.
/// * anything else: the delimiter separates entries. Upstream treats it as a regular
///   expression; here it is matched literally.
pub fn split_entries(text: &str, delimiter: &str) -> Vec<String> {
    let delim = delimiter.replace("\\n", "\n");
    let lines = || {
        // BufferedReader.readLine: \n, \r and \r\n end lines
        let mut v = Vec::new();
        let mut cur = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\n' => v.push(std::mem::take(&mut cur)),
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    v.push(std::mem::take(&mut cur));
                }
                c => cur.push(c),
            }
        }
        if !cur.is_empty() {
            v.push(cur);
        }
        v
    };
    let entries: Vec<String> = if delim == "\n" {
        lines()
    } else if delim.len() >= 2
        && delim.starts_with('\n')
        && delim.ends_with('\n')
        && !delim[1..delim.len() - 1].contains('\n')
    {
        let marker = &delim[1..delim.len() - 1];
        let mut v = Vec::new();
        let mut buf = String::new();
        for line in lines() {
            if line == marker {
                v.push(std::mem::take(&mut buf));
            } else {
                buf.push_str(&line);
                buf.push('\n');
            }
        }
        v
    } else if delim.is_empty() {
        vec![text.to_string()]
    } else {
        let mut parts: Vec<String> = text.split(delim.as_str()).map(|s| s.to_string()).collect();
        if parts.first().is_some_and(|p| p.is_empty()) {
            parts.remove(0);
        }
        if parts.last().is_some_and(|p| p.is_empty()) {
            parts.pop();
        }
        parts
    };
    entries.into_iter().map(|e| java_trim(&e).to_string()).collect()
}

/// `String.trim()`: strips characters up to U+0020 at both ends.
pub fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

// ---- the check -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ObserveOptions {
    /// Number of most recent events kept per key for error reports (`--replaySize`).
    pub replay_window: usize,
    /// Inputs are already sorted by timestamp: merge files instead of sorting, and report
    /// events that go back in time (`--assumeInputsAreSorted`).
    pub assume_sorted: bool,
    /// Only check these keys (`--keys`, `--keyListFile`); empty = all keys.
    pub keys: Vec<String>,
    /// Report a key whose spec is left in a hot state after its last event, as `snappea check`
    /// does at the end of an execution (`--check-hot`, a snappea extension).
    pub check_hot: bool,
    /// Reject a completed observation with no successfully checked relevant events.
    /// Applied independently to every selected spec, after key filtering.
    pub strict: bool,
    /// What to do with event names no loaded P source declares (`--unknown-events`, a snappea
    /// extension). Declared events the spec does not observe are always ignored.
    pub unknown_events: UnknownEvents,
    /// Names of the inputs, in order, used to locate unknown events (file paths, `stdin`).
    /// Unnamed inputs read as `input <n>`.
    pub input_names: Vec<String>,
}

/// The policy for event names that no loaded P source declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnknownEvents {
    /// Drop them, as upstream PObserve does.
    #[default]
    Ignore,
    /// Report each distinct name once in [`Report::warnings`]; the result is unchanged.
    Warn,
    /// Report each distinct name once as an [`ErrorKind::UnknownEvent`] error.
    Error,
}

impl UnknownEvents {
    pub fn as_str(self) -> &'static str {
        match self {
            UnknownEvents::Ignore => "ignore",
            UnknownEvents::Warn => "warn",
            UnknownEvents::Error => "error",
        }
    }
}

impl std::str::FromStr for UnknownEvents {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "ignore" => Ok(UnknownEvents::Ignore),
            "warn" => Ok(UnknownEvents::Warn),
            "error" => Ok(UnknownEvents::Error),
            _ => Err(format!("unknown-events must be ignore, warn or error, not '{}'", s)),
        }
    }
}

/// An event name no loaded P source declares, seen in the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownEvent {
    pub name: String,
    /// Occurrences in the checked input.
    pub count: u64,
    /// Where it first appeared: `<input>:<line>`, or `<input> entry <n>` for parsers whose
    /// entries are not single lines.
    pub location: String,
    /// A declared event with a similar name.
    pub suggestion: Option<String>,
    /// The key it first appeared in, when keys were selected and this one was not.
    pub unselected_key: Option<String>,
}

impl UnknownEvent {
    pub fn message(&self) -> String {
        let mut m = format!(
            "Unknown event '{}': not declared in the P sources ({} occurrence{}, first at {})",
            self.name,
            self.count,
            if self.count == 1 { "" } else { "s" },
            self.location
        );
        if let Some(k) = &self.unselected_key {
            m.push_str(&format!(", in key {} (not selected)", k));
        }
        if let Some(s) = &self.suggestion {
            m.push_str(&format!("; did you mean {}?", s));
        }
        m
    }

    fn error(&self) -> ObserveError {
        let message = self.message();
        ObserveError { kind: ErrorKind::UnknownEvent, detail: message.clone(), message, replay: None, cause: None }
    }
}

/// Distinct unknown event names in the order first seen.
#[derive(Debug, Default)]
struct UnknownTracker {
    seen: Vec<UnknownEvent>,
}

impl UnknownTracker {
    /// Counts `name`; `true` the first time it is seen.
    fn note(
        &mut self,
        prog: &Program,
        name: &str,
        unselected_key: Option<&str>,
        location: impl FnOnce() -> String,
    ) -> bool {
        if let Some(u) = self.seen.iter_mut().find(|u| u.name == name) {
            u.count += 1;
            return false;
        }
        self.seen.push(UnknownEvent {
            name: name.to_string(),
            count: 1,
            location: location(),
            suggestion: suggest_event(prog, name),
            unselected_key: unselected_key.map(str::to_string),
        });
        true
    }

    /// Records the names in `report` according to `policy`.
    fn report(self, policy: UnknownEvents, report: &mut Report) {
        for u in self.seen {
            match policy {
                UnknownEvents::Ignore => continue,
                UnknownEvents::Warn => report.warnings.push(u.message()),
                UnknownEvents::Error => {
                    report.metrics.unknown_event_errors += 1;
                    report.errors.push(u.error());
                }
            }
            report.unknown_events.push(u);
        }
    }
}

/// Where entry `entry` (0-based) of input `input` is, for messages.
fn entry_location(opts: &ObserveOptions, parser: &dyn Parser, input: usize, entry: usize) -> String {
    let name = opts.input_names.get(input).cloned().unwrap_or_else(|| format!("input {}", input + 1));
    if parser.log_delimiter().replace("\\n", "\n") == "\n" {
        format!("{}:{}", name, entry + 1)
    } else {
        format!("{} entry {}", name, entry + 1)
    }
}

/// The declared event closest to `name`, if it is close enough to be a likely misspelling.
pub fn suggest_event(prog: &Program, name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let limit = (name.chars().count() / 3).clamp(1, 3);
    prog.events
        .iter()
        .map(|e| e.name.as_str())
        .filter(|n| *n != "null" && *n != "halt")
        .map(|n| (edit_distance(&lower, &n.to_lowercase()), n))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n.to_string())
}

/// Levenshtein distance, counting an adjacent transposition as one edit.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d: Vec<Vec<usize>> =
        (0..=a.len()).map(|i| (0..=b.len()).map(|j| if i == 0 { j } else { i }).collect()).collect();
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

impl Default for ObserveOptions {
    fn default() -> Self {
        ObserveOptions {
            replay_window: 100,
            assume_sorted: false,
            keys: Vec::new(),
            check_hot: false,
            strict: false,
            unknown_events: UnknownEvents::Ignore,
            input_names: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Strict observation validation failed (no checked events or incomplete input).
    Validation,
    /// A spec assertion failed (`PAssertionFailureException`).
    SpecViolation,
    /// An event older than its predecessor (`PObserveEventOutOfOrderException`).
    OutOfOrder,
    /// The parser failed on a log entry (`PObserveLogParsingException`).
    Parser,
    /// Anything else, e.g. an event the spec's current state does not handle.
    Unknown,
    /// An event name no loaded P source declares, with `--unknown-events error`.
    UnknownEvent,
}

impl ErrorKind {
    /// The kind's name in JSON results.
    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::SpecViolation => "spec_violation",
            ErrorKind::OutOfOrder => "out_of_order",
            ErrorKind::Parser => "parser",
            ErrorKind::Unknown => "unknown",
            ErrorKind::Validation => "validation",
            ErrorKind::UnknownEvent => "unknown_event",
        }
    }
}

/// The events of one key leading up to an error (`PObserveReplayEvents`).
#[derive(Debug, Clone)]
pub struct Replay {
    pub key: String,
    /// Milliseconds of the last event added.
    pub error_timestamp: i64,
    pub events: Vec<ObservedEvent>,
}

#[derive(Debug, Clone)]
pub struct ObserveError {
    pub kind: ErrorKind,
    /// `getMessage()` of the upstream exception (`null` when it has none).
    pub message: String,
    /// `toString()` of the upstream exception: the first line of its stack trace.
    pub detail: String,
    pub replay: Option<Replay>,
    /// Why the parser rejected the entry, for parser errors (upstream reports only the entry).
    pub cause: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EventMetrics {
    pub name: String,
    pub verified: u64,
    pub total_ms: u64,
    pub max_ms: u64,
}

impl EventMetrics {
    pub fn avg_ms(&self) -> f32 {
        if self.verified > 0 {
            self.total_ms as f32 / self.verified as f32
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub partition_keys: u64,
    pub verified_keys: u64,
    pub parser_errors: u64,
    pub spec_errors: u64,
    pub out_of_order_errors: u64,
    pub unknown_errors: u64,
    pub validation_errors: u64,
    pub unknown_event_errors: u64,
    pub events_read: u64,
    /// Per observed event type, in the order upstream prints them.
    pub events: Vec<EventMetrics>,
    /// Wall time of the check in milliseconds.
    pub elapsed_ms: i64,
}

impl Metrics {
    pub fn verified_events(&self) -> u64 {
        self.events.iter().map(|e| e.verified).sum()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Which handlers, statements and branches of the spec the events ran.
    pub coverage: CodeCoverage,
    /// What the keys checked so far ran, before it is added to `coverage` at the end.
    pub(crate) collected: Option<CoverageInfo>,
    /// In the order upstream records them: parser errors, then errors per key.
    pub errors: Vec<ObserveError>,
    pub metrics: Metrics,
    /// Error-level messages logged while running (parser exceptions, unhandled events).
    pub log: Vec<String>,
    /// Undeclared event names, unless [`UnknownEvents::Ignore`].
    pub unknown_events: Vec<UnknownEvent>,
    /// Messages that do not change the result, e.g. with [`UnknownEvents::Warn`].
    pub warnings: Vec<String>,
}

impl Report {
    /// PObserve-compatible status: 0 success, 2 safety/liveness violation,
    /// 3 validation/ordering/internal/unknown-event error, or 4 parser error. A secondary
    /// strict validation failure does not mask a violation from another spec; an unknown
    /// event name (with [`UnknownEvents::Error`]) does, since the input is not trustworthy.
    pub fn exit_status(&self) -> i32 {
        Self::errors_exit_status(&self.errors)
    }

    /// `snappea observe`'s exit status for [`Self::exit_status`]: 0 no error, 1 a spec
    /// violation, 2 any other error.
    pub fn snappea_exit_code(pobserve_status: i32) -> i32 {
        match pobserve_status {
            0 => 0,
            2 => 1,
            _ => 2,
        }
    }

    /// Status for a combined set of spec errors, using [`Self::exit_status`] priority.
    pub fn errors_exit_status(errors: &[ObserveError]) -> i32 {
        if errors.is_empty() {
            0
        } else if errors
            .iter()
            .any(|e| matches!(e.kind, ErrorKind::Unknown | ErrorKind::OutOfOrder | ErrorKind::UnknownEvent))
        {
            3
        } else if errors.iter().any(|e| e.kind == ErrorKind::SpecViolation) {
            2
        } else if errors.iter().any(|e| e.kind == ErrorKind::Validation) {
            3
        } else {
            4
        }
    }

    fn require_events(&mut self, spec: &str) {
        if self.errors.is_empty() && self.metrics.verified_events() == 0 {
            self.metrics.validation_errors += 1;
            let message = format!("Strict observation: spec '{}' checked no relevant events after key filtering; check instrumentation, spec selection, and key filters.", spec);
            self.errors.push(ObserveError {
                kind: ErrorKind::Validation,
                detail: message.clone(),
                message,
                replay: None,
                cause: None,
            });
        }
    }

    /// Errors counted in the final summary (all but parser errors).
    pub fn error_count(&self) -> u64 {
        self.metrics.spec_errors
            + self.metrics.out_of_order_errors
            + self.metrics.unknown_errors
            + self.metrics.validation_errors
            + self.metrics.unknown_event_errors
    }
}

/// Monitors never reach a scheduling point; this strategy is never consulted.
pub struct NoScheduling;

impl Strategy for NoScheduling {
    fn next_operation(&mut self, current: u64, _ops: &[OpInfo]) -> Option<u64> {
        Some(current)
    }
    fn next_bool(&mut self, _max: i32) -> bool {
        false
    }
    fn next_int(&mut self, _max: i32) -> i32 {
        0
    }
    fn prepare_next_iteration(&mut self) -> bool {
        false
    }
    fn scheduled_steps(&self) -> usize {
        0
    }
    fn has_reached_max_steps(&self) -> bool {
        false
    }
    fn is_fair(&self) -> bool {
        false
    }
    fn description(&self) -> String {
        String::new()
    }
}

/// Outcome of running the spec over one key's events.
enum KeyOutcome {
    Ok,
    Error(ObserveError),
    /// The spec could not even be started: the whole run stops.
    Fatal(ObserveError),
}

impl<'p> Spec<'p> {
    fn failure_error(&self, failure: Option<Failure>, replay: Option<Replay>, log: &mut Vec<String>) -> ObserveError {
        match failure {
            Some(Failure::Assertion(m)) => ObserveError {
                cause: None,
                kind: ErrorKind::SpecViolation,
                detail: format!("pobserve.runtime.exceptions.PAssertionFailureException: {}", m),
                message: m,
                replay,
            },
            Some(Failure::Unhandled { state, event }) => {
                log.push(format!("prt.State[{}] missing event handler for {}", state, event));
                ObserveError {
                    cause: None,
                    kind: ErrorKind::Unknown,
                    message: "null".into(),
                    detail: format!(
                        "prt.State prt.State[{}] has no handler for class {}.PEvents${}",
                        state, self.java_package, event
                    ),
                    replay,
                }
            }
            Some(Failure::Exception(ty, m)) => ObserveError {
                cause: None,
                kind: ErrorKind::Unknown,
                detail: format!("{}: {}", ty, m),
                message: m,
                replay,
            },
            Some(Failure::Other(m)) => ObserveError {
                cause: None,
                kind: ErrorKind::Unknown,
                detail: format!("java.lang.RuntimeException: {}", m),
                message: m,
                replay,
            },
            None => ObserveError {
                cause: None,
                kind: ErrorKind::Unknown,
                message: "null".into(),
                detail: "java.lang.RuntimeException".into(),
                replay,
            },
        }
    }

    /// `CheckPartitionedEventStream.check`.
    fn check_key(
        &self,
        key: &str,
        events: Vec<ObservedEvent>,
        opts: &ObserveOptions,
        metrics: &mut HashMap<EventId, EventMetrics>,
        clock: &mut dyn FnMut() -> f64,
        report: &mut Report,
    ) -> KeyOutcome {
        let log = &mut report.log;
        let time = self.time == Ok(true);
        let mut strategy = NoScheduling;
        let mut rt = Runtime::new(
            self.prog,
            &self.code,
            &self.module,
            &self.config,
            &mut strategy,
            Setup {
                java_semantics: Some(&self.java),
                time: time.then(TimeOptions::default),
                coverage: Some(Coverage::new(self.tables.clone(), report.collected.take().unwrap_or_default())),
                ..Setup::default()
            },
        );
        let outcome = (|| {
            let mon = match rt.start_monitor(self.decl) {
                Ok(m) => m,
                Err(_) => return KeyOutcome::Fatal(self.failure_error(rt.failure.take(), None, log)),
            };
            let window = opts.replay_window.max(1);
            let mut replay: std::collections::VecDeque<ObservedEvent> = std::collections::VecDeque::new();
            let make_replay = |q: &std::collections::VecDeque<ObservedEvent>| Replay {
                key: key.to_string(),
                error_timestamp: q.back().map(|e| e.timestamp.to_millis()).unwrap_or(0),
                events: q.iter().cloned().collect(),
            };
            let mut prev = Timestamp::from_millis(-1);
            for ev in events {
                if replay.len() == window {
                    replay.pop_front();
                }
                replay.push_back(ev.clone());
                if ev.timestamp < prev {
                    let message = format!(
                    "Encountered an out of order event.\nEncountered an out of order event::\nTimestamp of last processed event: {}\nTimestamp of current event: {}\nError event details: {}",
                    prev.to_iso(),
                    ev.timestamp.to_iso(),
                    ev.to_java_string(self)
                );
                    return KeyOutcome::Error(ObserveError {
                        cause: None,
                        kind: ErrorKind::OutOfOrder,
                        detail: format!("pobserve.commons.exceptions.PObserveEventOutOfOrderException: {}", message),
                        message,
                        replay: Some(make_replay(&replay)),
                    });
                }
                prev = ev.timestamp;
                let start = (clock() * 1000.0).floor() as i64;
                let stamp = time.then(|| Rc::new(Stamp { at: Time::point(ev.timestamp.to_micros()), sender: 0 }));
                let e = Event { id: ev.event, payload: ev.payload.clone(), stamp, vt: None };
                rt.step_work = 0;
                if rt.monitor_event(mon, e, None).is_err() {
                    return KeyOutcome::Error(self.failure_error(rt.failure.take(), Some(make_replay(&replay)), log));
                }
                let ms = ((clock() * 1000.0).floor() as i64 - start).max(0) as u64;
                if let Some(m) = metrics.get_mut(&ev.event) {
                    m.verified += 1;
                    m.total_ms += ms;
                    m.max_ms = m.max_ms.max(ms);
                }
            }
            if opts.check_hot {
                if let Some(state) = rt.monitor_hot_state(mon) {
                    let message = format!(
                        "{} detected liveness bug in hot state '{}' at the end of program execution.",
                        rt.monitor_full_name(mon),
                        state
                    );
                    return KeyOutcome::Error(ObserveError {
                        cause: None,
                        kind: ErrorKind::SpecViolation,
                        detail: format!("pobserve.runtime.exceptions.PAssertionFailureException: {}", message),
                        message,
                        replay: Some(make_replay(&replay)),
                    });
                }
            }
            KeyOutcome::Ok
        })();
        report.collected = rt.coverage.take().map(|c| c.into_info());
        outcome
    }

    /// Parses `inputs` (the trimmed entries of each log file) with `parser` and checks the
    /// events against the spec (`PObserveExecutor.run`). `clock` returns seconds.
    pub fn observe(
        &self,
        inputs: &[Vec<String>],
        parser: &dyn Parser,
        opts: &ObserveOptions,
        clock: &mut dyn FnMut() -> f64,
    ) -> Report {
        let mut report = Report { coverage: self.map.empty(), ..Report::default() };
        let start = clock();
        if let Err(message) = &self.time {
            report.metrics.unknown_errors = 1;
            report.errors.push(self.failure_error(Some(Failure::Other(message.clone())), None, &mut report.log));
            return report;
        }
        let keys: std::collections::HashSet<&str> = opts.keys.iter().map(|s| s.as_str()).collect();

        // parse, keeping the events the spec observes
        let mut per_file: Vec<Vec<ObservedEvent>> = Vec::new();
        let mut unknown = UnknownTracker::default();
        for (input, entries) in inputs.iter().enumerate() {
            let mut evs = Vec::new();
            for (index, entry) in entries.iter().enumerate() {
                let parsed = if opts.unknown_events == UnknownEvents::Ignore {
                    parser.parse(entry, self).map(|events| ParsedEntry { events, ..ParsedEntry::default() })
                } else {
                    parser.parse_entry(entry, self)
                };
                match parsed {
                    Ok(ParsedEntry { events: v, unknown_events, unknown_keys }) => {
                        for (i, name) in unknown_events.iter().enumerate() {
                            let key = unknown_keys.get(i).map(String::as_str);
                            let unselected = key.filter(|k| !keys.is_empty() && !keys.contains(k));
                            unknown.note(self.prog, name, unselected, || entry_location(opts, parser, input, index));
                        }
                        for e in v {
                            if self.observes_event(e.event) && (keys.is_empty() || keys.contains(e.key.as_str())) {
                                report.metrics.events_read += 1;
                                evs.push(e);
                            }
                        }
                    }
                    Err(cause) => {
                        report.metrics.parser_errors += 1;
                        report.log.push("Parser Exception::".into());
                        report.log.push(format!("Exception occurred while parsing log line: {}", entry));
                        report.errors.push(ObserveError {
                            kind: ErrorKind::Parser,
                            message: entry.clone(),
                            detail: format!("pobserve.commons.exceptions.PObserveLogParsingException: {}", entry),
                            replay: None,
                            cause: Some(cause),
                        });
                    }
                }
            }
            per_file.push(evs);
        }
        unknown.report(opts.unknown_events, &mut report);
        let all: Vec<ObservedEvent> =
            if opts.assume_sorted { merge_sorted(per_file) } else { per_file.into_iter().flatten().collect() };

        // partition by key
        let mut order: Vec<String> = Vec::new();
        let mut parts: HashMap<String, Vec<ObservedEvent>> = HashMap::new();
        for e in all {
            if !parts.contains_key(&e.key) {
                order.push(e.key.clone());
            }
            parts.entry(e.key.clone()).or_default().push(e);
        }
        report.metrics.partition_keys = order.len() as u64;
        let key_values: Vec<Value> = order.iter().map(|k| Value::str(k)).collect();
        let key_order: Vec<String> = javafmt::hashmap_order(key_values.iter().map(|k| (k, ())))
            .into_iter()
            .map(|(k, _)| match k {
                Value::Str(s) => s.to_string(),
                _ => unreachable!(),
            })
            .collect();

        let mut metrics: HashMap<EventId, EventMetrics> = self
            .observes()
            .iter()
            .map(|&e| (e, EventMetrics { name: self.prog.event(e).name.clone(), ..Default::default() }))
            .collect();
        for key in key_order {
            let mut events = parts.remove(&key).unwrap_or_default();
            if !opts.assume_sorted {
                events.sort_by_key(|e| e.timestamp);
            }
            match self.check_key(&key, events, opts, &mut metrics, clock, &mut report) {
                KeyOutcome::Ok => report.metrics.verified_keys += 1,
                KeyOutcome::Error(err) => {
                    match err.kind {
                        ErrorKind::SpecViolation => report.metrics.spec_errors += 1,
                        ErrorKind::OutOfOrder => report.metrics.out_of_order_errors += 1,
                        _ => report.metrics.unknown_errors += 1,
                    }
                    report.errors.push(err);
                }
                KeyOutcome::Fatal(err) => {
                    report.metrics.unknown_errors += 1;
                    report.errors.push(err);
                    break;
                }
            }
        }

        // event metrics in the order of upstream's HashMap<String, EventMetrics>
        let names: Vec<Value> = self.observes().iter().map(|&e| Value::str(&self.prog.event(e).name)).collect();
        let mut seen = std::collections::HashSet::new();
        for (_, e) in javafmt::hashmap_order(names.iter().zip(self.observes().iter())) {
            if seen.insert(*e) {
                if let Some(m) = metrics.remove(e) {
                    report.metrics.events.push(m);
                }
            }
        }
        if opts.strict {
            report.require_events(self.name());
        }
        if let Some(collected) = report.collected.take() {
            self.record_coverage(&mut report.coverage, &collected);
        }
        report.metrics.elapsed_ms = ((clock() - start) * 1000.0).round() as i64;
        report
    }
}

/// Incremental, ordered observation of a live stream. Each key has its own monitor;
/// replay storage is bounded by `replay_window` per key. Call `finish` only at EOF.
/// A failed session is terminal: no subsequent entry is processed.
pub struct ObserveSession<'s> {
    spec: &'s Spec<'s>,
    rt: Runtime<'s>,
    opts: ObserveOptions,
    keys: HashMap<String, (usize, Option<Timestamp>, std::collections::VecDeque<ObservedEvent>)>,
    report: Report,
    started: f64,
    /// Entries pushed so far, to locate unknown events.
    entries: usize,
    unknown: UnknownTracker,
}

impl<'s> ObserveSession<'s> {
    /// `strategy` is never consulted by spec monitors; use [`NoScheduling`].
    pub fn new(spec: &'s Spec<'s>, opts: ObserveOptions, strategy: &'s mut dyn Strategy, now: f64) -> Self {
        let rt = Runtime::new(
            spec.prog,
            &spec.code,
            &spec.module,
            &spec.config,
            strategy,
            Setup {
                java_semantics: Some(&spec.java),
                time: (spec.time == Ok(true)).then(TimeOptions::default),
                coverage: Some(Coverage::new(spec.tables.clone(), CoverageInfo::default())),
                ..Setup::default()
            },
        );
        let mut report = Report { coverage: spec.map.empty(), ..Report::default() };
        report.metrics.events = spec
            .observes()
            .iter()
            .map(|e| EventMetrics { name: spec.prog.event(*e).name.clone(), ..Default::default() })
            .collect();
        if let Err(message) = &spec.time {
            report.metrics.unknown_errors = 1;
            report.errors.push(spec.failure_error(Some(Failure::Other(message.clone())), None, &mut report.log));
        }
        Self {
            spec,
            rt,
            opts,
            keys: HashMap::new(),
            report,
            started: now,
            entries: 0,
            unknown: UnknownTracker::default(),
        }
    }

    pub fn failed(&self) -> bool {
        !self.report.errors.is_empty()
    }

    fn fail(&mut self, error: ObserveError) {
        match error.kind {
            ErrorKind::Parser => self.report.metrics.parser_errors += 1,
            ErrorKind::SpecViolation => self.report.metrics.spec_errors += 1,
            ErrorKind::OutOfOrder => self.report.metrics.out_of_order_errors += 1,
            ErrorKind::Unknown => self.report.metrics.unknown_errors += 1,
            ErrorKind::Validation => self.report.metrics.validation_errors += 1,
            ErrorKind::UnknownEvent => self.report.metrics.unknown_event_errors += 1,
        }
        self.report.errors.push(error);
    }

    /// Records a transport error and stops the stream.
    pub fn input_error(&mut self, message: String) {
        if !self.failed() {
            self.fail(self.spec.failure_error(Some(Failure::Other(message)), None, &mut Vec::new()));
        }
    }

    /// Checks one complete log entry. `false` means stop reading immediately.
    /// Events must be nondecreasing in timestamp within each key; no sorting occurs.
    pub fn push(&mut self, entry: &str, parser: &dyn Parser, clock: &mut dyn FnMut() -> f64) -> bool {
        let line = self.entries;
        self.push_at(0, line, entry, parser, clock)
    }

    /// [`push`](Self::push) for entry `line` (0-based) of input `input`
    /// ([`ObserveOptions::input_names`]), for messages that locate it.
    pub fn push_at(
        &mut self,
        input: usize,
        line: usize,
        entry: &str,
        parser: &dyn Parser,
        clock: &mut dyn FnMut() -> f64,
    ) -> bool {
        if self.failed() {
            return false;
        }
        self.entries += 1;
        let parsed = if self.opts.unknown_events == UnknownEvents::Ignore {
            parser.parse(java_trim(entry), self.spec).map(|events| ParsedEntry { events, ..ParsedEntry::default() })
        } else {
            parser.parse_entry(java_trim(entry), self.spec)
        };
        let events = match parsed {
            Ok(ParsedEntry { events, unknown_events, unknown_keys }) => {
                for (i, name) in unknown_events.iter().enumerate() {
                    let (opts, prog) = (&self.opts, self.spec.prog);
                    let key = unknown_keys.get(i).map(String::as_str);
                    let unselected = key.filter(|k| !opts.keys.is_empty() && !opts.keys.iter().any(|s| s == k));
                    let first = self.unknown.note(prog, name, unselected, || entry_location(opts, parser, input, line));
                    if first && self.opts.unknown_events == UnknownEvents::Error {
                        // stop at the first unknown name: report it with what is known so far
                        let error = self.unknown.seen.last().expect("just noted").error();
                        self.report.unknown_events.push(self.unknown.seen.pop().expect("just noted"));
                        self.fail(error);
                        return false;
                    }
                }
                events
            }
            Err(cause) => {
                self.fail(ObserveError {
                    kind: ErrorKind::Parser,
                    message: entry.to_string(),
                    detail: format!("pobserve.commons.exceptions.PObserveLogParsingException: {}", entry),
                    replay: None,
                    cause: Some(cause),
                });
                return false;
            }
        };
        for ev in events {
            if !self.spec.observes_event(ev.event) || (!self.opts.keys.is_empty() && !self.opts.keys.contains(&ev.key))
            {
                continue;
            }
            self.report.metrics.events_read += 1;
            if !self.keys.contains_key(&ev.key) {
                let mon = match self.rt.start_monitor(self.spec.decl) {
                    Ok(mon) => mon,
                    Err(_) => {
                        let error = self.spec.failure_error(self.rt.failure.take(), None, &mut self.report.log);
                        self.fail(error);
                        return false;
                    }
                };
                self.keys.insert(ev.key.clone(), (mon, None, std::collections::VecDeque::new()));
                self.report.metrics.partition_keys += 1;
            }
            let (mon, prev, replay) = self.keys.get_mut(&ev.key).unwrap();
            if replay.len() == self.opts.replay_window.max(1) {
                replay.pop_front();
            }
            replay.push_back(ev.clone());
            let replay = Replay {
                key: ev.key.clone(),
                error_timestamp: ev.timestamp.to_millis(),
                events: replay.iter().cloned().collect(),
            };
            if prev.is_some_and(|t| ev.timestamp < t) {
                self.fail(ObserveError { kind: ErrorKind::OutOfOrder, message: format!("Event timestamp {} precedes the previous timestamp for key '{}'; live input must be ordered per key", ev.timestamp.to_iso(), ev.key), detail: "pobserve.commons.exceptions.PObserveEventOutOfOrderException".into(), replay: Some(replay), cause: None });
                return false;
            }
            *prev = Some(ev.timestamp);
            let start = clock();
            let stamp = (self.spec.time == Ok(true))
                .then(|| Rc::new(Stamp { at: Time::point(ev.timestamp.to_micros()), sender: 0 }));
            self.rt.step_work = 0;
            if self.rt.monitor_event(*mon, Event { id: ev.event, payload: ev.payload, stamp, vt: None }, None).is_err()
            {
                let error = self.spec.failure_error(self.rt.failure.take(), Some(replay), &mut self.report.log);
                self.fail(error);
                return false;
            }
            let ms = ((clock() - start) * 1000.0).max(0.0) as u64;
            if let Some(m) =
                self.report.metrics.events.iter_mut().find(|m| m.name == self.spec.prog.event(ev.event).name)
            {
                m.verified += 1;
                m.total_ms += ms;
                m.max_ms = m.max_ms.max(ms);
            }
        }
        true
    }

    /// Finishes a checked prefix. Hot-state liveness checks run only for actual EOF,
    /// never when a sibling spec failed or the input transport failed.
    pub fn finish(self, eof: bool, now: f64) -> Report {
        self.finish_inner(eof, false, now)
    }

    /// Finish a prefix because another session already reported an error. The caller
    /// must preserve that error in the overall result; do not add secondary strict
    /// validation errors or run end-of-input liveness checks.
    pub fn finish_after_failure(self, now: f64) -> Report {
        self.finish_inner(false, true, now)
    }

    fn finish_inner(mut self, eof: bool, sibling_failed: bool, now: f64) -> Report {
        std::mem::take(&mut self.unknown).report(self.opts.unknown_events, &mut self.report);
        if !self.failed() && eof {
            for (key, (mon, _, events)) in &self.keys {
                if self.opts.check_hot {
                    if let Some(state) = self.rt.monitor_hot_state(*mon) {
                        let message = format!(
                            "{} detected liveness bug in hot state '{}' at the end of program execution.",
                            self.rt.monitor_full_name(*mon),
                            state
                        );
                        self.report.metrics.spec_errors += 1;
                        self.report.errors.push(ObserveError {
                            kind: ErrorKind::SpecViolation,
                            detail: format!("pobserve.runtime.exceptions.PAssertionFailureException: {}", message),
                            message,
                            replay: Some(Replay {
                                key: key.clone(),
                                error_timestamp: events.back().map(|e| e.timestamp.to_millis()).unwrap_or(0),
                                events: events.iter().cloned().collect(),
                            }),
                            cause: None,
                        });
                        break;
                    }
                }
                self.report.metrics.verified_keys += 1;
            }
        }
        if self.opts.strict && !self.failed() {
            if eof {
                self.report.require_events(self.spec.name());
            } else if !sibling_failed {
                self.fail(ObserveError {
                    kind: ErrorKind::Validation,
                    message: "Strict observation: input ended before EOF; only a prefix was checked.".into(),
                    detail: "Incomplete observation input".into(),
                    replay: None,
                    cause: None,
                });
            }
        }
        if let Some(collected) = self.rt.coverage.take() {
            self.spec.record_coverage(&mut self.report.coverage, &collected.into_info());
        }
        self.report.metrics.elapsed_ms = ((now - self.started) * 1000.0).round() as i64;
        self.report
    }
}

/// Merges per-file event streams already sorted by time (`MergeIntoSortedStream`).
fn merge_sorted(files: Vec<Vec<ObservedEvent>>) -> Vec<ObservedEvent> {
    let mut iters: Vec<std::iter::Peekable<std::vec::IntoIter<ObservedEvent>>> =
        files.into_iter().map(|f| f.into_iter().peekable()).collect();
    let mut out = Vec::new();
    loop {
        let mut best: Option<(usize, Timestamp)> = None;
        for (i, it) in iters.iter_mut().enumerate() {
            if let Some(e) = it.peek() {
                if best.map(|(_, t)| e.timestamp < t).unwrap_or(true) {
                    best = Some((i, e.timestamp));
                }
            }
        }
        match best {
            Some((i, _)) => out.push(iters[i].next().unwrap()),
            None => return out,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_split_like_log_breaker() {
        assert_eq!(split_entries("a\n b \r\nc", "\n"), vec!["a", "b", "c"]);
        assert_eq!(split_entries("a\nb\n--\nc\n--\nd\n", "\n--\n"), vec!["a\nb", "c"]);
        assert_eq!(split_entries("a;;b;", ";;"), vec!["a", "b;"]);
    }

    #[test]
    fn microseconds_preserve_precision_and_saturate_at_int_bounds() {
        assert_eq!(Timestamp::from_millis(1706894854333).to_micros(), 1706894854333000);
        assert_eq!(Timestamp::new(1, 123_456_789).to_micros(), 1_123_456);
        assert_eq!(Timestamp::new(-1, 999_999_999).to_micros(), -1);
        assert_eq!(Timestamp::new(i64::MAX, 0).to_micros(), i64::MAX);
        assert_eq!(Timestamp::new(i64::MIN, 0).to_micros(), i64::MIN);
        let secs = i64::MIN.div_euclid(1_000_000);
        let nanos = (i64::MIN.rem_euclid(1_000_000) * 1000) as u32;
        assert_eq!(Timestamp::new(secs, nanos).to_micros(), i64::MIN);
        assert_eq!(Timestamp::new(secs, nanos + 1000).to_micros(), i64::MIN + 1);
    }

    #[test]
    fn timestamps() {
        let t = Timestamp::from_millis(-1);
        assert_eq!((t.secs, t.nanos), (-1, 999_000_000));
        assert_eq!(t.to_millis(), -1);
        assert_eq!(Timestamp::from_millis(1706894854333).to_millis(), 1706894854333);
        assert_eq!(Timestamp::new(i64::MAX, 1_500_000_000), Timestamp { secs: i64::MAX, nanos: 500_000_000 });
        assert_eq!(Timestamp::new(i64::MAX, 0).to_millis(), i64::MAX);
        assert_eq!(Timestamp::new(i64::MIN, 0).to_millis(), i64::MIN);
    }
}
