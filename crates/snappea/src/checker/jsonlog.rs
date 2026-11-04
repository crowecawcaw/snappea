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
    pub clock: Option<Vec<(String, i64)>>,
    /// Identity of the payload for matching sends with dequeues (not serialized).
    pub payload_key: String,
}

impl Details {
    /// `replace_nulls` is the bug-trace post-processing that turns nulls into `"null"`
    /// strings and always writes a payload; the verbose log skips it.
    fn to_json(&self, replace_nulls: bool) -> Json {
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
        if let Some(c) = &self.clock {
            f.push(("clock".into(), Json::Obj(c.iter().map(|(k, v)| (k.clone(), Json::Int(*v))).collect())));
        }
        if let Some(i) = &self.inspection {
            f.push(("inspection".into(), i.clone()));
        }
        Json::Obj(f)
    }
}

/// `VectorClockGenerator`.
#[derive(Debug, Clone, Default)]
struct VectorClocks {
    clocks: HashMap<String, Vec<(String, i64)>>,
    unhandled_sends: HashMap<String, Vec<(String, i64)>>,
    /// send id -> (sent count, received count)
    send_counts: HashMap<String, (i64, i64)>,
}

fn vc_get(v: &[(String, i64)], k: &str) -> Option<i64> {
    v.iter().find(|(m, _)| m == k).map(|(_, c)| *c)
}

impl VectorClocks {
    fn merge(&mut self, machine: &str, sender: &[(String, i64)]) {
        let local = self.clocks.get(machine).cloned().unwrap_or_default();
        let mut keys: Vec<String> = Vec::new();
        for (k, _) in local.iter().chain(sender.iter()) {
            if k != machine && !keys.contains(k) {
                keys.push(k.clone());
            }
        }
        let entry = self.clocks.get_mut(machine).unwrap();
        for k in keys {
            match vc_get(entry, &k) {
                Some(lv) => {
                    if let Some(sv) = vc_get(sender, &k) {
                        let slot = entry.iter_mut().find(|(m, _)| *m == k).unwrap();
                        slot.1 = sv.max(lv);
                    }
                }
                None => entry.push((k.clone(), vc_get(sender, &k).unwrap_or(0))),
            }
        }
    }

    /// `HandleLogEntry`; `false` when upstream's handler throws (the entry is dropped).
    fn handle(&mut self, ty: &str, d: &mut Details) -> bool {
        let Some(machine) = d.id.clone().or_else(|| d.monitor.clone()).or_else(|| d.sender.clone()) else {
            return false;
        };
        let entry = self.clocks.entry(machine.clone()).or_insert_with(|| vec![(machine.clone(), 0)]);
        if let Some(slot) = entry.iter_mut().find(|(m, _)| *m == machine) {
            slot.1 += 1;
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
                    return false;
                }
                let clock = self.clocks[&machine].clone();
                self.unhandled_sends.insert(id, clock);
            }
            "MonitorProcessEvent" => {
                if let Some(s) = &d.sender {
                    let Some(sc) = self.clocks.get(s).cloned() else { return false };
                    self.merge(&machine, &sc);
                }
            }
            "ReceiveEvent" | "DequeueEvent" => {
                let general = format!("_{}:_{}:_{}", machine, d.event.as_deref().unwrap_or(""), d.payload_key);
                let Some(&(sent, received)) = self.send_counts.get(&general) else { return false };
                if received == sent {
                    self.send_counts.remove(&general);
                } else {
                    self.send_counts.get_mut(&general).unwrap().1 += 1;
                }
                let id = format!("{}:_{}", general, received);
                let Some(sc) = self.unhandled_sends.remove(&id) else { return false };
                self.merge(&machine, &sc);
            }
            _ => {}
        }
        d.clock = Some(self.clocks[&machine].clone());
        true
    }
}

/// The JSON log of one schedule.
#[derive(Debug, Clone, Default)]
pub struct JsonLog {
    inspection_count: usize,
    entries: Vec<(String, Details)>,
    vc: VectorClocks,
}

impl JsonLog {
    pub fn new() -> JsonLog {
        JsonLog::default()
    }

    /// `JsonWriter.AddToLogs`.
    pub fn add(&mut self, ty: &str, mut d: Details, update_vc: bool) {
        // Bound browser-only snapshots without dropping any ordinary trace events.
        if ty == "Inspection" {
            self.inspection_count += 1;
            if self.inspection_count > 10_000 {
                if self.inspection_count == 10_001 {
                    self.entries.push(("InspectionLimit".into(), Details { log: Some("Variable capture limit reached (10,000 snapshots). Later values remain last known; messages and failures are still recorded.".into()), ..Details::default() }));
                }
                return;
            }
        }
        if update_vc && !self.vc.handle(ty, &mut d) {
            return;
        }
        self.entries.push((ty.to_string(), d));
    }

    /// The log as a JSON array of `{type, details}` entries (see `Details::to_json`).
    pub fn to_json(&self, replace_nulls: bool) -> Json {
        Json::Arr(
            self.entries
                .iter()
                .map(|(t, d)| {
                    Json::Obj(vec![("type".into(), Json::str(t.clone())), ("details".into(), d.to_json(replace_nulls))])
                })
                .collect(),
        )
    }

    /// The `.trace.json` file contents.
    pub fn to_json_text(&self) -> String {
        self.to_json(true).to_pretty()
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
