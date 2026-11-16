//! The JSON trace (`PCheckerLogJsonFormatter` + `JsonWriter`): one entry per runtime log
//! event with upstream's field names, payload encoding and vector clocks, serialized the way
//! `System.Text.Json` writes it (camelCase, nulls omitted, indented, default escaping).

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::sema::Program;

use crate::checker::value::{event_cs_name, Value};

/// A JSON value with ordered object keys.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn str(s: impl Into<String>) -> Json {
        Json::Str(s.into())
    }

    /// Indented serialization (two spaces per level, `"key": value`).
    pub fn to_pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out
    }

    fn write(&self, out: &mut String, indent: usize) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Int(i) => write!(out, "{}", i).unwrap(),
            Json::Float(f) => {
                if f.is_finite() {
                    let s = format!("{}", f);
                    out.push_str(&s);
                } else {
                    out.push_str("null");
                }
            }
            Json::Str(s) => write_string(out, s),
            Json::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, indent + 1);
                    it.write(out, indent + 1);
                }
                newline(out, indent);
                out.push(']');
            }
            Json::Obj(fields) => {
                if fields.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, indent + 1);
                    write_string(out, k);
                    out.push_str(": ");
                    v.write(out, indent + 1);
                }
                newline(out, indent);
                out.push('}');
            }
        }
    }

    /// `RecursivelyReplaceNullWithString`.
    fn replace_nulls(self) -> Json {
        match self {
            Json::Null => Json::str("null"),
            Json::Obj(f) => Json::Obj(f.into_iter().map(|(k, v)| (k, v.replace_nulls())).collect()),
            Json::Arr(a) => Json::Arr(a.into_iter().map(Json::replace_nulls).collect()),
            other => other,
        }
    }
}

fn newline(out: &mut String, indent: usize) {
    out.push('\n');
    for _ in 0..indent {
        out.push_str("  ");
    }
}

/// `JavaScriptEncoder.Default` escaping: control characters, HTML-sensitive characters and
/// everything outside ASCII as `\uXXXX`.
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\\' => out.push_str("\\\\"),
            '"' | '\'' | '<' | '>' | '&' | '+' | '`' => write!(out, "\\u{:04X}", c as u32).unwrap(),
            c if (c as u32) < 0x20 || (c as u32) >= 0x7f => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    write!(out, "\\u{:04X}", u).unwrap();
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `LogDetails`, in declaration (= serialization) order.
#[derive(Debug, Clone, Default)]
pub struct Details {
    /// Optional browser replay data. Omitted from compatible traces.
    pub inspection: Option<Json>,
    pub log: Option<String>,
    pub id: Option<String>,
    pub error: Option<String>,
    pub event: Option<String>,
    pub creator_name: Option<String>,
    pub creator_type: Option<String>,
    pub state: Option<String>,
    pub start_state: Option<String>,
    pub end_state: Option<String>,
    pub payload: Option<Json>,
    pub action: Option<String>,
    pub exception: Option<String>,
    pub halt_inbox_size: Option<i64>,
    pub was_blocked: Option<bool>,
    pub sender: Option<String>,
    pub target: Option<String>,
    pub is_target_halted: Option<bool>,
    pub is_entry: Option<bool>,
    pub is_in_hot_state: Option<bool>,
    pub event_type: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub monitor: Option<String>,
    pub strategy: Option<String>,
    pub strategy_description: Option<String>,
    /// Identity of the payload for matching sends with dequeues (not serialized).
    pub payload_key: String,
}

impl Details {
    /// `replace_nulls` is the bug-trace post-processing that turns nulls into `"null"`
    /// strings and always writes a payload; the verbose log skips it.
    fn to_json(&self, replace_nulls: bool, clock: Option<Json>) -> Json {
        let mut f: Vec<(String, Json)> = Vec::new();
        let mut s = |k: &str, v: &Option<String>| {
            if let Some(v) = v {
                f.push((k.into(), Json::str(v.clone())));
            }
        };
        s("log", &self.log);
        s("id", &self.id);
        s("error", &self.error);
        s("event", &self.event);
        s("creatorName", &self.creator_name);
        s("creatorType", &self.creator_type);
        s("state", &self.state);
        s("startState", &self.start_state);
        s("endState", &self.end_state);
        if replace_nulls {
            // payload is always present after null replacement
            f.push(("payload".into(), self.payload.clone().unwrap_or(Json::Null).replace_nulls()));
        } else if let Some(p) = &self.payload {
            f.push(("payload".into(), p.clone()));
        }
        let mut s = |k: &str, v: &Option<String>| {
            if let Some(v) = v {
                f.push((k.into(), Json::str(v.clone())));
            }
        };
        s("action", &self.action);
        s("exception", &self.exception);
        if let Some(n) = self.halt_inbox_size {
            f.push(("haltInboxSize".into(), Json::Int(n)));
        }
        if let Some(b) = self.was_blocked {
            f.push(("wasBlocked".into(), Json::Bool(b)));
        }
        if let Some(v) = &self.sender {
            f.push(("sender".into(), Json::str(v.clone())));
        }
        if let Some(v) = &self.target {
            f.push(("target".into(), Json::str(v.clone())));
        }
        for (k, v) in [
            ("isTargetHalted", self.is_target_halted),
            ("isEntry", self.is_entry),
            ("isInHotState", self.is_in_hot_state),
        ] {
            if let Some(b) = v {
                f.push((k.into(), Json::Bool(b)));
            }
        }
        if let Some(v) = &self.event_type {
            f.push(("eventType".into(), Json::str(v.clone())));
        }
        if let Some(v) = &self.event_types {
            f.push(("eventTypes".into(), Json::Arr(v.iter().map(|s| Json::str(s.clone())).collect())));
        }
        for (k, v) in [
            ("monitor", &self.monitor),
            ("strategy", &self.strategy),
            ("strategyDescription", &self.strategy_description),
        ] {
            if let Some(v) = v {
                f.push((k.into(), Json::str(v.clone())));
            }
        }
        if let Some(c) = clock {
            f.push(("clock".into(), c));
        }
        if let Some(i) = &self.inspection {
            f.push(("inspection".into(), i.clone()));
        }
        Json::Obj(f)
    }
}

/// A vector clock: (machine index, count), in the order upstream's dictionary keeps them.
/// A machine's own count comes first.
type Clock = Vec<(u32, i64)>;

/// The most vector-clock entries one trace keeps, summed over its log entries; each is
/// about 30 bytes of `.trace.json`. A clock lists every machine that ever touched its
/// machine, so a long schedule that keeps creating machines (say, a timer per retry)
/// would otherwise need memory and output quadratic in its length. Entries past the
/// budget carry no clock.
pub const CLOCK_BUDGET: usize = 2_000_000;

/// `VectorClockGenerator`, on interned machine names.
#[derive(Debug, Clone, Default)]
struct VectorClocks {
    names: Vec<String>,
    ids: HashMap<String, u32>,
    /// Each machine's clock, by machine index.
    clocks: Vec<Clock>,
    unhandled_sends: HashMap<String, Clock>,
    /// send id -> (sent count, received count)
    send_counts: HashMap<String, (i64, i64)>,
    /// Scratch for `merge`: a key's position in the local clock (`u32::MAX`: absent).
    pos: Vec<u32>,
    /// Past the clock budget: still match sends with receives (an unmatched receive drops
    /// its entry, as upstream's handler throws) but keep no clocks.
    matching_only: bool,
}

impl VectorClocks {
    fn id(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.ids.get(name) {
            return id;
        }
        let id = self.names.len() as u32;
        self.names.push(name.to_string());
        self.ids.insert(name.to_string(), id);
        self.clocks.push(if self.matching_only { Vec::new() } else { vec![(id, 0)] });
        self.pos.push(u32::MAX);
        id
    }

    /// Merges `sender` into `machine`'s clock: each key both have takes the larger count,
    /// then the sender's other keys follow in its order. Linear in the clocks' sizes.
    fn merge(&mut self, machine: u32, sender: &[(u32, i64)]) {
        let local = &mut self.clocks[machine as usize];
        for (i, (k, _)) in local.iter().enumerate() {
            self.pos[*k as usize] = i as u32;
        }
        for &(k, sv) in sender {
            if k == machine {
                continue;
            }
            match self.pos[k as usize] {
                u32::MAX => {
                    self.pos[k as usize] = local.len() as u32;
                    local.push((k, sv));
                }
                p => {
                    let slot = &mut local[p as usize].1;
                    *slot = sv.max(*slot);
                }
            }
        }
        for (k, _) in local.iter() {
            self.pos[*k as usize] = u32::MAX;
        }
    }

    /// Stops tracking clocks (see `matching_only`) and frees them.
    fn drop_clocks(&mut self) {
        self.matching_only = true;
        self.clocks.iter_mut().for_each(|c| *c = Vec::new());
        self.unhandled_sends.values_mut().for_each(|c| *c = Vec::new());
    }

    /// `HandleLogEntry`: the entry's clock (empty once clocks are dropped), or `None` when
    /// upstream's handler throws (the entry is dropped).
    fn handle(&mut self, ty: &str, d: &Details) -> Option<Clock> {
        let name = d.id.as_deref().or(d.monitor.as_deref()).or(d.sender.as_deref())?;
        let machine = self.id(name);
        if let Some(own) = self.clocks[machine as usize].first_mut() {
            own.1 += 1;
        }
        match ty {
            "SendEvent" => {
                let general = format!(
                    "_{}:_{}:_{}",
                    d.target.as_deref().unwrap_or(""),
                    d.event.as_deref().unwrap_or(""),
                    d.payload_key
                );
                let counts = self.send_counts.entry(general.clone()).or_insert((-1, 0));
                counts.0 += 1;
                let id = format!("{}:_{}", general, counts.0);
                if self.unhandled_sends.contains_key(&id) {
                    return None;
                }
                let clock = self.clocks[machine as usize].clone();
                self.unhandled_sends.insert(id, clock);
            }
            "MonitorProcessEvent" => {
                if let Some(s) = &d.sender {
                    let sender = *self.ids.get(s.as_str())?;
                    let sc = self.clocks[sender as usize].clone();
                    self.merge(machine, &sc);
                }
            }
            "ReceiveEvent" | "DequeueEvent" => {
                let general = format!("_{}:_{}:_{}", name, d.event.as_deref().unwrap_or(""), d.payload_key);
                let &(sent, received) = self.send_counts.get(&general)?;
                if received == sent {
                    self.send_counts.remove(&general);
                } else {
                    self.send_counts.get_mut(&general).unwrap().1 += 1;
                }
                let id = format!("{}:_{}", general, received);
                let sc = self.unhandled_sends.remove(&id)?;
                self.merge(machine, &sc);
            }
            _ => {}
        }
        Some(self.clocks[machine as usize].clone())
    }
}

/// One entry of a [`JsonLog`].
#[derive(Debug, Clone)]
struct Entry {
    ty: String,
    details: Details,
    /// Its vector clock; `None` for an entry without one.
    clock: Option<Box<[(u32, i64)]>>,
}

/// The JSON log of one schedule.
#[derive(Debug, Clone, Default)]
pub struct JsonLog {
    inspection_count: usize,
    entries: Vec<Entry>,
    vc: VectorClocks,
    /// Vector-clock entries kept so far (see [`CLOCK_BUDGET`]).
    clock_entries: usize,
}

impl JsonLog {
    pub fn new() -> JsonLog {
        JsonLog::default()
    }

    fn note(&mut self, ty: &str, text: String) {
        let details = Details { log: Some(text), ..Details::default() };
        self.entries.push(Entry { ty: ty.into(), details, clock: None });
    }

    /// `JsonWriter.AddToLogs`.
    pub fn add(&mut self, ty: &str, d: Details, update_vc: bool) {
        // Bound browser-only snapshots without dropping any ordinary trace events.
        if ty == "Inspection" {
            self.inspection_count += 1;
            if self.inspection_count > 10_000 {
                if self.inspection_count == 10_001 {
                    self.note("InspectionLimit", "Variable capture limit reached (10,000 snapshots). Later values remain last known; messages and failures are still recorded.".into());
                }
                return;
            }
        }
        let mut clock = None;
        if update_vc {
            let Some(c) = self.vc.handle(ty, &d) else { return };
            if !self.vc.matching_only {
                if self.clock_entries + c.len() > CLOCK_BUDGET {
                    self.vc.drop_clocks();
                    self.note(
                        "ClockLimit",
                        format!(
                            "Vector clock limit reached ({} clock entries over {} machines). Later entries have no clock; messages and failures are still recorded.",
                            self.clock_entries,
                            self.vc.names.len()
                        ),
                    );
                } else {
                    self.clock_entries += c.len();
                    clock = Some(c.into_boxed_slice());
                }
            }
        }
        self.entries.push(Entry { ty: ty.to_string(), details: d, clock });
    }

    fn entry_json(&self, e: &Entry, replace_nulls: bool) -> Json {
        let names = &self.vc.names;
        let clock = e
            .clock
            .as_ref()
            .map(|c| Json::Obj(c.iter().map(|&(k, v)| (names[k as usize].clone(), Json::Int(v))).collect()));
        Json::Obj(vec![
            ("type".into(), Json::str(e.ty.clone())),
            ("details".into(), e.details.to_json(replace_nulls, clock)),
        ])
    }

    /// The log as a JSON array of `{type, details}` entries (see `Details::to_json`).
    pub fn to_json(&self, replace_nulls: bool) -> Json {
        Json::Arr(self.entries.iter().map(|e| self.entry_json(e, replace_nulls)).collect())
    }

    /// The `.trace.json` file contents: `to_json(true).to_pretty()`, built one entry at a
    /// time so the whole trace is never held twice.
    pub fn to_json_text(&self) -> String {
        if self.entries.is_empty() {
            return "[]".into();
        }
        let mut out = String::from("[");
        for (i, e) in self.entries.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            newline(&mut out, 1);
            self.entry_json(e, true).write(&mut out, 1);
        }
        newline(&mut out, 0);
        out.push(']');
        out
    }
}

/// `IPValue.ToDict()`.
pub fn to_dict(v: &Value, prog: &Program) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(i) => Json::Int(*i),
        Value::Float(f) => Json::Float(*f),
        Value::Str(s) => Json::str(s.to_string()),
        Value::Machine(m) => Json::str(m.name.to_string()),
        Value::Event(e) => Json::str(event_cs_name(prog, *e).to_string()),
        // PTuple.ToDict keeps the raw field values, which System.Text.Json serializes by
        // their public properties
        Value::Tuple(t) => Json::Obj(t.iter().enumerate().map(|(i, v)| (i.to_string(), raw(v, prog))).collect()),
        Value::NamedTuple(t) => {
            Json::Obj(t.names.iter().zip(t.values.iter()).map(|(n, v)| (n.clone(), to_dict(v, prog))).collect())
        }
        Value::Seq(s) => Json::Arr(s.iter().map(|v| to_dict(v, prog)).collect()),
        Value::Set(s) => Json::Arr(s.iter().map(|v| to_dict(v, prog)).collect()),
        Value::Map(m) => Json::Obj(m.iter().map(|(k, v)| (k.to_escaped(prog), to_dict(v, prog))).collect()),
        // IPValue.ToDict's default: the escaped string
        Value::Foreign(f) => Json::str(f.display()),
    }
}

/// How `System.Text.Json` serializes a runtime `IPValue` object directly.
fn raw(v: &Value, prog: &Program) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Machine(m) => Json::Obj(vec![
            ("id".into(), Json::Obj(vec![("isNameUsedForHashing".into(), Json::Bool(false))])),
            (
                "permissions".into(),
                Json::Arr(m.permissions.iter().map(|e| Json::str(event_cs_name(prog, *e).to_string())).collect()),
            ),
        ]),
        Value::Seq(s) => Json::Arr(s.iter().map(|v| raw(v, prog)).collect()),
        Value::Set(s) => Json::Arr(s.iter().map(|v| raw(v, prog)).collect()),
        _ => Json::Obj(Vec::new()),
    }
}

/// Payload identity used for vector-clock matching.
pub fn payload_key(v: Option<&Value>, prog: &Program) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(v) => v.to_escaped(prog),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(from: &str, to: &str, key: usize) -> Details {
        Details {
            id: Some(from.into()),
            sender: Some(from.into()),
            target: Some(to.into()),
            event: Some("e".into()),
            payload_key: key.to_string(),
            ..Details::default()
        }
    }

    fn dequeue(at: &str, key: usize) -> Details {
        Details { id: Some(at.into()), event: Some("e".into()), payload_key: key.to_string(), ..Details::default() }
    }

    /// A hub that hears from a new timer machine `n` times and forwards each to a
    /// server: the hub's and server's clocks list every timer, so all clocks together
    /// grow with the square of `n`.
    fn timers(n: usize) -> JsonLog {
        let mut log = JsonLog::new();
        for i in 0..n {
            let timer = format!("Timer({i})");
            log.add("CreateStateMachine", Details { id: Some(timer.clone()), ..Details::default() }, true);
            log.add("SendEvent", send(&timer, "Hub", i), true);
            log.add("DequeueEvent", dequeue("Hub", i), true);
            log.add("SendEvent", send("Hub", "Server", i), true);
            log.add("DequeueEvent", dequeue("Server", i), true);
        }
        log
    }

    type NamedClock = Vec<(String, i64)>;

    /// Upstream's `VectorClockGenerator` on machine names, as written before they were
    /// interned (sends and dequeues only).
    fn reference_clocks(entries: &[(&str, Details)]) -> Vec<NamedClock> {
        let mut clocks: HashMap<String, NamedClock> = HashMap::new();
        let mut unhandled: HashMap<String, NamedClock> = HashMap::new();
        let mut counts: HashMap<String, (i64, i64)> = HashMap::new();
        let mut out = Vec::new();
        for (ty, d) in entries {
            let m = d.id.clone().unwrap();
            let e = clocks.entry(m.clone()).or_insert_with(|| vec![(m.clone(), 0)]);
            e.iter_mut().find(|(x, _)| *x == m).unwrap().1 += 1;
            if *ty == "SendEvent" {
                let g = format!("_{}:_e:_{}", d.target.as_deref().unwrap(), d.payload_key);
                let c = counts.entry(g.clone()).or_insert((-1, 0));
                c.0 += 1;
                unhandled.insert(format!("{}:_{}", g, c.0), clocks[&m].clone());
            } else if *ty == "DequeueEvent" {
                let g = format!("_{}:_e:_{}", m, d.payload_key);
                let (sent, received) = counts[&g];
                if received == sent {
                    counts.remove(&g);
                } else {
                    counts.get_mut(&g).unwrap().1 += 1;
                }
                let s = unhandled.remove(&format!("{}:_{}", g, received)).unwrap();
                let local = clocks[&m].clone();
                let mut keys: Vec<String> = Vec::new();
                for (k, _) in local.iter().chain(&s) {
                    if *k != m && !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
                let e = clocks.get_mut(&m).unwrap();
                for k in keys {
                    let sv = s.iter().find(|(x, _)| *x == k).map(|p| p.1);
                    match e.iter_mut().find(|(x, _)| *x == k) {
                        Some(slot) => slot.1 = sv.map_or(slot.1, |sv| sv.max(slot.1)),
                        None => e.push((k, sv.unwrap_or(0))),
                    }
                }
            }
            out.push(clocks[&m].clone());
        }
        out
    }

    #[test]
    fn interned_clocks_match_upstreams_generator() {
        let mut entries = Vec::new();
        for i in 0..6 {
            let timer = format!("Timer({i})");
            entries.push(("SendEvent", send(&timer, "Hub", i % 2)));
            entries.push(("SendEvent", send("Server", "Hub", 0)));
            entries.push(("DequeueEvent", dequeue("Hub", i % 2)));
            entries.push(("DequeueEvent", dequeue("Hub", 0)));
            entries.push(("SendEvent", send("Hub", "Server", i)));
            entries.push(("DequeueEvent", dequeue("Server", i)));
        }
        let mut log = JsonLog::new();
        for (ty, d) in &entries {
            log.add(ty, d.clone(), true);
        }
        let expected = reference_clocks(&entries);
        assert_eq!(log.entries.len(), expected.len());
        for (e, want) in log.entries.iter().zip(expected) {
            let got: NamedClock =
                e.clock.as_ref().unwrap().iter().map(|&(k, v)| (log.vc.names[k as usize].clone(), v)).collect();
            assert_eq!(got, want, "{} of {:?}", e.ty, e.details.id);
        }
        assert_eq!(log.to_json_text(), log.to_json(true).to_pretty());
    }

    #[test]
    fn a_trace_keeps_a_bounded_number_of_clock_entries() {
        // about 4 * 3000^2 / 2 = 18 million clock entries without the budget
        let log = timers(3000);
        assert!(log.clock_entries <= CLOCK_BUDGET, "{}", log.clock_entries);
        let limit = log.entries.iter().position(|e| e.ty == "ClockLimit").expect("the budget is reached");
        assert!(log.entries[limit + 1..].iter().all(|e| e.clock.is_none()));
        // every event is still recorded, once
        assert_eq!(log.entries.iter().filter(|e| e.ty != "ClockLimit").count(), 5 * 3000);
        assert!(log.vc.clocks.iter().all(Vec::is_empty) && log.vc.unhandled_sends.is_empty());
        // a short trace keeps every clock
        let short = timers(50);
        assert!(short.entries.iter().all(|e| e.clock.is_some()));
    }
}
