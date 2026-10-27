//! Driver facts: limits a test's driver showed in every explored schedule.
//!
//! A driver is test code that stands in for the model's environment: a test case's main
//! machine, machines declared under a `PTst` folder, and machines marked
//! `// snappea: driver`. While a test case runs, [`DriverFacts`] notes how many machines
//! of each kind are created, which events each driver instance sends to the model, and
//! the scalar payload values the driver uses. [`DriverFacts::facts`] then lists what held
//! in every schedule ("each Client sends eAcquire, then eRelease, once each"): what the
//! driver never tried, and so what the check never covered.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rustc_hash::FxHashMap;
use serde_json::{json, Value as Json};

use crate::checker::value::Value;
use crate::sema::{EventId, MachineId, MachineKind, PType, Program};
use snappea_syntax::{SourceMap, Span};

/// Distinct payload values kept per field; more means the driver varies it.
const VALUES: usize = 3;

/// The machines that drive the model, by [`MachineId`]: those declared under a `PTst`
/// folder or marked `// snappea: driver` (`marked`, their declaration spans). A test
/// case's main machine is a driver too; the checker adds it.
pub fn driver_machines(prog: &Program, sources: &SourceMap, marked: &[Span]) -> Vec<bool> {
    prog.machines
        .iter()
        .map(|m| {
            m.kind == MachineKind::Impl
                && (marked.iter().any(|s| s.file == m.span.file && s.start <= m.span.start && m.span.start < s.end)
                    || sources
                        .files()
                        .get(m.span.file.0 as usize)
                        .is_some_and(|f| f.name.split(['/', '\\']).any(|c| c.eq_ignore_ascii_case("PTst"))))
        })
        .collect()
}

/// Where a payload value was seen: an event the driver sent, or a machine it created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Site {
    Send(EventId),
    Create(MachineId),
}

/// The values one payload field took: up to [`VALUES`] of them, or more.
#[derive(Debug, Clone, PartialEq)]
enum Seen {
    Few(BTreeSet<String>),
    Many,
}

impl Seen {
    fn add(&mut self, v: String) {
        if let Seen::Few(s) = self {
            s.insert(v);
            if s.len() > VALUES {
                *self = Seen::Many;
            }
        }
    }

    fn merge(&mut self, other: &Seen) {
        match other {
            Seen::Many => *self = Seen::Many,
            Seen::Few(s) => {
                for v in s {
                    self.add(v.clone());
                }
            }
        }
    }
}

/// How many times the instances of a driver machine sent one event in a schedule.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Count {
    /// Instances that sent it at least once, in schedules that ran to the end.
    senders: u64,
    /// Fewest times an instance that sent it did, in schedules that ran to the end.
    min: Option<u32>,
    /// Most times an instance sent it.
    max: u32,
}

/// One fact: what held in every schedule the test case explored.
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    /// `instances`, `sequence`, `count`, `order` or `values`.
    pub kind: &'static str,
    /// A stable id, e.g. `Client.eAcquire.count`.
    pub id: String,
    /// What held, e.g. `each Client sends eAcquire at most twice`.
    pub text: String,
    /// What to do if the real environment can do more.
    pub hint: &'static str,
}

impl Fact {
    pub fn to_json(&self) -> Json {
        json!({"kind": self.kind, "id": self.id, "text": self.text, "hint": self.hint})
    }
}

/// What a test case's driver did over the schedules explored so far (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct DriverFacts {
    /// Per machine: whether it drives the model.
    driver: Vec<bool>,
    /// The test case's main machine.
    main: Option<MachineId>,
    /// Schedules recorded.
    pub schedules: usize,
    /// Schedules stopped by the step bound: the driver may have gone on.
    pub truncated: usize,
    /// Per machine: fewest instances created in a schedule that ran to the end, and most
    /// in any schedule.
    instances: Vec<(Option<u32>, u32)>,
    /// Per driver machine: instances seen over the schedules that ran to the end.
    driver_instances: BTreeMap<MachineId, u64>,
    /// Per driver machine and event it sent the model.
    sends: BTreeMap<(MachineId, EventId), Count>,
    /// `(m, a, b)`: an instance of driver machine `m` sent `b` after `a`.
    after: BTreeSet<(MachineId, EventId, EventId)>,
    /// `(m, a, b)`: an instance of `m` sent both `a` and `b`.
    both: BTreeSet<(MachineId, EventId, EventId)>,
    /// Payload values, per site and field (`None`: the whole payload).
    values: BTreeMap<(Site, Option<usize>), Seen>,
    /// The payload type of each site, without type aliases.
    types: FxHashMap<Site, Arc<PType>>,
    /// This schedule: machines created, per machine.
    created: Vec<u32>,
    /// This schedule: per driver instance, its machine and the events it sent the model.
    inputs: FxHashMap<u64, (MachineId, Vec<EventId>)>,
}

impl DriverFacts {
    /// A recorder for a test case whose drivers are `driver` (see [`driver_machines`])
    /// and its main machine.
    pub fn new(mut driver: Vec<bool>, main: MachineId) -> DriverFacts {
        let n = driver.len();
        if let Some(d) = driver.get_mut(main.idx()) {
            *d = true;
        }
        DriverFacts {
            driver,
            main: Some(main),
            instances: vec![(None, 0); n],
            created: vec![0; n],
            ..DriverFacts::default()
        }
    }

    fn is_driver(&self, m: MachineId) -> bool {
        self.driver.get(m.idx()).copied().unwrap_or(false)
    }

    /// A machine was created.
    pub fn record_create(&mut self, m: MachineId) {
        if let Some(c) = self.created.get_mut(m.idx()) {
            *c += 1;
        }
    }

    /// Machine `creator` created `m` with `payload`.
    pub fn record_create_payload(&mut self, prog: &Program, creator: MachineId, m: MachineId, payload: &Value) {
        if self.is_driver(creator) && !self.is_driver(m) {
            self.record_values(prog, Site::Create(m), &prog.machine(m).payload, payload);
        }
    }

    /// Instance `sender` (of machine `from`) sent `event` with `payload` to a `to`.
    pub fn record_send(
        &mut self,
        prog: &Program,
        sender: u64,
        from: MachineId,
        to: MachineId,
        event: EventId,
        payload: &Value,
    ) {
        if !self.is_driver(from) || self.is_driver(to) {
            return;
        }
        self.inputs.entry(sender).or_insert_with(|| (from, Vec::new())).1.push(event);
        self.record_values(prog, Site::Send(event), &prog.event(event).payload, payload);
    }

    fn record_values(&mut self, prog: &Program, site: Site, ty: &PType, payload: &Value) {
        let ty = self.types.entry(site).or_insert_with(|| Arc::new(ty.canonicalize(prog))).clone();
        match (&*ty, payload) {
            (PType::NamedTuple(fields), Value::NamedTuple(t)) => {
                for (i, (_, fty)) in fields.iter().enumerate() {
                    if let Some(v) = t.values.get(i) {
                        self.note(prog, (site, Some(i)), fty, v);
                    }
                }
            }
            (PType::Tuple(fields), Value::Tuple(t)) => {
                for (i, fty) in fields.iter().enumerate() {
                    if let Some(v) = t.get(i) {
                        self.note(prog, (site, Some(i)), fty, v);
                    }
                }
            }
            _ => self.note(prog, (site, None), &ty, payload),
        }
    }

    /// Notes a payload value, unless the field already varies too much to report.
    fn note(&mut self, prog: &Program, key: (Site, Option<usize>), ty: &PType, v: &Value) {
        if matches!(self.values.get(&key), Some(Seen::Many)) {
            return;
        }
        if let Some(text) = scalar(prog, ty, v) {
            self.values.entry(key).or_insert(Seen::Few(BTreeSet::new())).add(text);
        }
    }

    /// Folds the schedule just run into the facts. A schedule that `stopped` early (a bug,
    /// the step bound, or the search cutting it at an explored state) only raises the
    /// most seen: what it would have done next is unknown. `truncated` when it was the
    /// step bound, which the driver might have gone past.
    pub fn end_schedule(&mut self, stopped: bool, truncated: bool) {
        self.schedules += 1;
        self.truncated += usize::from(truncated);
        let created = std::mem::replace(&mut self.created, vec![0; self.driver.len()]);
        for (range, &c) in self.instances.iter_mut().zip(&created) {
            range.1 = range.1.max(c);
            if !stopped {
                range.0 = Some(range.0.map_or(c, |lo| lo.min(c)));
            }
        }
        for (m, &c) in created.iter().enumerate() {
            if self.driver[m] && c > 0 && !stopped {
                *self.driver_instances.entry(MachineId(m as u32)).or_default() += u64::from(c);
            }
        }
        let mut inputs: Vec<_> = std::mem::take(&mut self.inputs).into_values().collect();
        inputs.sort();
        for (m, events) in inputs {
            let mut counts: BTreeMap<EventId, u32> = BTreeMap::new();
            // the distinct events sent so far: each is followed by this one
            let mut earlier: Vec<EventId> = Vec::new();
            for &b in &events {
                *counts.entry(b).or_default() += 1;
                for &a in &earlier {
                    self.after.insert((m, a, b));
                }
                if !earlier.contains(&b) {
                    earlier.push(b);
                }
            }
            for (&a, &n) in &counts {
                let c = self.sends.entry((m, a)).or_insert(Count { senders: 0, min: None, max: n });
                c.max = c.max.max(n);
                if !stopped {
                    c.senders += 1;
                    c.min = Some(c.min.map_or(n, |lo| lo.min(n)));
                }
                for &b in counts.keys() {
                    self.both.insert((m, a, b));
                }
            }
        }
    }

    /// Adds what another worker recorded for the same test case.
    pub fn merge(&mut self, other: &DriverFacts) {
        if other.schedules == 0 {
            return;
        }
        if self.schedules == 0 {
            *self = other.clone();
            return;
        }
        self.schedules += other.schedules;
        self.truncated += other.truncated;
        for (a, b) in self.instances.iter_mut().zip(&other.instances) {
            a.0 = match (a.0, b.0) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
            a.1 = a.1.max(b.1);
        }
        for (m, n) in &other.driver_instances {
            *self.driver_instances.entry(*m).or_default() += n;
        }
        for (k, c) in &other.sends {
            match self.sends.get_mut(k) {
                Some(e) => {
                    e.senders += c.senders;
                    e.min = match (e.min, c.min) {
                        (Some(x), Some(y)) => Some(x.min(y)),
                        (x, y) => x.or(y),
                    };
                    e.max = e.max.max(c.max);
                }
                None => {
                    self.sends.insert(*k, *c);
                }
            }
        }
        self.after.extend(other.after.iter().copied());
        self.both.extend(other.both.iter().copied());
        for (k, s) in &other.values {
            match self.values.get_mut(k) {
                Some(mine) => mine.merge(s),
                None => {
                    self.values.insert(*k, s.clone());
                }
            }
        }
    }

    /// What held in every schedule, most telling first. `repeats` when the search found
    /// cycles, so the driver may send without limit: then no counts or orders are claimed.
    pub fn facts(&self, prog: &Program, repeats: bool) -> Vec<Fact> {
        let mut out = Vec::new();
        if self.schedules == 0 {
            return out;
        }
        let bounded = !repeats && self.truncated == 0;
        let drivers: BTreeSet<MachineId> = self.sends.keys().map(|(m, _)| *m).collect();
        let mut counts = Vec::new();
        let mut orders = Vec::new();
        for &m in drivers.iter().filter(|_| bounded) {
            let name = &prog.machine(m).name;
            let instances = self.driver_instances.get(&m).copied().unwrap_or(0);
            let single = self.instances.get(m.idx()).is_some_and(|r| r.1 <= 1);
            let who = if single { name.clone() } else { format!("each {}", name) };
            let events: Vec<(EventId, Count)> =
                self.sends.iter().filter(|((d, _), _)| *d == m).map(|((_, e), c)| (*e, *c)).collect();
            // the fewest sends of an instance: 0 if some instance never sent it
            let min = |c: &Count| if c.senders < instances { 0 } else { c.min.unwrap_or(0) };
            let once = events.iter().all(|(_, c)| min(c) == 1 && c.max == 1);
            let all: Vec<EventId> = events.iter().map(|(e, _)| *e).collect();
            let mut order = all.clone();
            let total = order.iter().all(|&a| {
                order.iter().all(|&b| a == b || self.after.contains(&(m, a, b)) != self.after.contains(&(m, b, a)))
            });
            if once && total {
                order.sort_by_key(|&a| all.iter().filter(|&&b| self.after.contains(&(m, b, a))).count());
                let names: Vec<&str> = order.iter().map(|e| prog.event(*e).name.as_str()).collect();
                let text = if names.len() == 1 {
                    format!("{} sends {} exactly once", who, names[0])
                } else {
                    format!("{} sends {}, once each", who, names.join(", then "))
                };
                out.push(Fact { kind: "sequence", id: format!("{}.sequence", name), text, hint: MORE_INPUTS });
                continue;
            }
            // an event sent exactly once, before (or after) every other: one fact, not a
            // count and an order per other event
            let exactly_once = |e: EventId| events.iter().any(|(x, c)| *x == e && min(c) == 1 && c.max == 1);
            let ends: Vec<(EventId, &str)> = all
                .iter()
                .filter(|&&a| exactly_once(a))
                .filter_map(|&a| {
                    if all.iter().all(|&b| !self.after.contains(&(m, b, a))) {
                        Some((a, "before"))
                    } else if all.iter().all(|&b| !self.after.contains(&(m, a, b))) {
                        Some((a, "after"))
                    } else {
                        None
                    }
                })
                .collect();
            for (e, when) in &ends {
                let ev = &prog.event(*e).name;
                orders.push(Fact {
                    kind: "order",
                    id: format!("{}.{}.{}", name, ev, if *when == "before" { "first" } else { "last" }),
                    text: format!("{} sends {} once, {} everything else", who, ev, when),
                    hint: MORE_INPUTS,
                });
            }
            let rest: Vec<(EventId, Count)> =
                events.iter().filter(|(e, _)| !ends.iter().any(|(x, _)| x == e)).copied().collect();
            for (a, _) in &rest {
                for (b, _) in &rest {
                    if a != b && self.both.contains(&(m, *a, *b)) && !self.after.contains(&(m, *a, *b)) {
                        let (a, b) = (&prog.event(*a).name, &prog.event(*b).name);
                        orders.push(Fact {
                            kind: "order",
                            id: format!("{}.{}-after-{}", name, b, a),
                            text: format!("no {} sends {} after {}", name, b, a),
                            hint: MORE_INPUTS,
                        });
                    }
                }
            }
            // events sent equally often share one fact
            let mut groups: BTreeMap<(u32, u32), Vec<&str>> = BTreeMap::new();
            for (e, c) in &rest {
                groups.entry((min(c), c.max)).or_default().push(prog.event(*e).name.as_str());
            }
            for ((lo, hi), names) in groups {
                let how = match (lo, hi) {
                    (lo, hi) if lo == hi => format!("exactly {}", times(hi)),
                    (0, hi) => format!("at most {}", times(hi)),
                    (lo, hi) => format!("{} to {} times", lo, hi),
                };
                let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
                let each = if names.len() > 1 { " each" } else { "" };
                counts.push(Fact {
                    kind: "count",
                    id: format!("{}.{}.count", name, names.join("+")),
                    text: format!("{} sends {} {}{}", who, join(&names), how, each),
                    hint: MORE_INPUTS,
                });
            }
        }
        out.extend(orders);
        if let Some(f) = self.instances_fact(prog) {
            out.push(f);
        }
        out.extend(counts);
        out.extend(self.value_facts(prog));
        out
    }

    fn instances_fact(&self, prog: &Program) -> Option<Fact> {
        let parts: Vec<String> = self
            .instances
            .iter()
            .enumerate()
            .filter(|(m, r)| Some(MachineId(*m as u32)) != self.main && r.1 > 0)
            .map(|(m, &(lo, hi))| {
                let name = &prog.machines[m].name;
                let lo = lo.unwrap_or(0);
                if lo == hi {
                    format!("{} {}", hi, name)
                } else {
                    format!("{} to {} {}", lo, hi, name)
                }
            })
            .collect();
        if parts.is_empty() {
            return None;
        }
        Some(Fact {
            kind: "instances",
            id: "instances".into(),
            text: format!("creates {}, never more", join(&parts)),
            hint: MORE_INSTANCES,
        })
    }

    fn value_facts(&self, prog: &Program) -> Vec<Fact> {
        let mut out = Vec::new();
        let sites: BTreeSet<Site> = self.values.keys().map(|(s, _)| *s).collect();
        for site in sites {
            let (subject, ty, id) = match site {
                Site::Send(e) => {
                    let ev = prog.event(e);
                    (format!("{} is {{}} sent with", ev.name), ev.payload.canonicalize(prog), ev.name.clone())
                }
                Site::Create(m) => {
                    let mc = prog.machine(m);
                    (
                        format!("{} is {{}} created with", mc.name),
                        mc.payload.canonicalize(prog),
                        format!("new.{}", mc.name),
                    )
                }
            };
            let mut parts = Vec::new();
            let mut varies = false;
            for ((_, field), seen) in self.values.range((site, None)..=(site, Some(usize::MAX))) {
                let Seen::Few(values) = seen else { continue };
                let (label, fty) = match (field, &ty) {
                    (Some(i), PType::NamedTuple(fs)) => (Some(fs[*i].0.clone()), fs[*i].1.clone()),
                    (Some(i), PType::Tuple(fs)) => (Some(i.to_string()), fs[*i].clone()),
                    _ => (None, ty.clone()),
                };
                if covers_domain(prog, &fty, values.len()) {
                    continue;
                }
                let values: Vec<String> = values.iter().cloned().collect();
                varies |= values.len() > 1;
                let v = if values.len() == 1 { values[0].clone() } else { join_or(&values) };
                parts.push(match label {
                    Some(l) => format!("{} = {}", l, v),
                    None => v,
                });
            }
            if parts.is_empty() {
                continue;
            }
            let subject = subject.replace("{}", if varies { "only" } else { "always" });
            let text = format!("{} {}", subject, parts.join(", "));
            out.push(Fact { kind: "values", id: format!("{}.values", id), text, hint: MORE_VALUES });
        }
        out
    }
}

const MORE_INPUTS: &str =
    "if the real environment can send more, or in other orders, extend the driver (a loop, or a choice with `$`)";
const MORE_INSTANCES: &str = "if more of these can run at once, create more in the driver";
const MORE_VALUES: &str = "if other values are possible, let the driver choose them (`choose(...)`, or `$`)";

/// A scalar payload value as the facts show it: an int, bool, enum element or string.
fn scalar(prog: &Program, ty: &PType, v: &Value) -> Option<String> {
    match (ty, v) {
        (PType::Bool, Value::Bool(b)) => Some(b.to_string()),
        (PType::Int, Value::Int(i)) => Some(i.to_string()),
        (PType::Enum(e), Value::Int(i)) => {
            prog.enums[e.idx()].elems.iter().find(|x| x.value == *i).map(|x| x.name.clone())
        }
        (PType::String, Value::Str(s)) => Some(format!("\"{}\"", s)),
        _ => None,
    }
}

/// Whether `n` distinct values are every value of `ty` (a bool or a small enum).
fn covers_domain(prog: &Program, ty: &PType, n: usize) -> bool {
    match ty {
        PType::Bool => n >= 2,
        PType::Enum(e) => n >= prog.enums[e.idx()].elems.len(),
        _ => false,
    }
}

fn times(n: u32) -> String {
    match n {
        1 => "once".into(),
        2 => "twice".into(),
        n => format!("{} times", n),
    }
}

fn join(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [a] => a.clone(),
        [rest @ .., last] => format!("{} and {}", rest.join(", "), last),
    }
}

fn join_or(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [a] => a.clone(),
        [rest @ .., last] => format!("{} or {}", rest.join(", "), last),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "enum Kind { Small, Large }\n\
        event eAcquire: (id: int, kind: Kind);\n\
        event eRelease;\n\
        event eGranted;\n\
        machine Server { start state S { ignore eAcquire, eRelease; } }\n\
        machine Client { start state S { ignore eGranted; } }\n\
        machine Main { start state S { } }\n\
        test tc [main=Main]: { Main, Server, Client };\n";

    fn program() -> Program {
        let mut sm = crate::syntax::SourceMap::new();
        sm.add("model.p", MODEL.to_string());
        crate::sema::compile(&sm, &crate::sema::Options::default())
            .unwrap_or_else(|_| panic!("does not compile"))
            .program
    }

    fn id(prog: &Program, name: &str) -> MachineId {
        MachineId(prog.machines.iter().position(|m| m.name == name).unwrap() as u32)
    }

    fn ev(prog: &Program, name: &str) -> EventId {
        EventId(prog.events.iter().position(|e| e.name == name).unwrap() as u32)
    }

    fn acquire(id: i64, kind: i64) -> Value {
        Value::NamedTuple(std::rc::Rc::new(crate::checker::value::NamedTuple {
            names: vec!["id".to_string(), "kind".to_string()].into(),
            values: vec![Value::Int(id), Value::Int(kind)],
        }))
    }

    /// A recorder where Client drives Server, and one schedule in which each of the
    /// `clients` (instance ids) sends `events`.
    fn schedule(d: &mut DriverFacts, prog: &Program, clients: &[u64], events: &[&str], stopped: bool) {
        let (server, client, main) = (id(prog, "Server"), id(prog, "Client"), id(prog, "Main"));
        d.record_create(main);
        d.record_create(server);
        for &c in clients {
            d.record_create(client);
            for e in events {
                let payload = if *e == "eAcquire" { acquire(c as i64, 0) } else { Value::Null };
                d.record_send(prog, c, client, server, ev(prog, e), &payload);
            }
        }
        d.end_schedule(stopped, false);
    }

    fn texts(facts: &[Fact]) -> Vec<String> {
        facts.iter().map(|f| f.text.clone()).collect()
    }

    fn recorder(prog: &Program) -> DriverFacts {
        let mut drivers = vec![false; prog.machines.len()];
        drivers[id(prog, "Client").idx()] = true;
        DriverFacts::new(drivers, id(prog, "Main"))
    }

    #[test]
    fn a_fixed_sequence_is_one_fact() {
        let prog = program();
        let mut d = recorder(&prog);
        schedule(&mut d, &prog, &[1, 2], &["eAcquire", "eRelease"], false);
        // a schedule cut short only raises maxima: it does not make sends optional
        schedule(&mut d, &prog, &[1], &["eAcquire"], true);
        assert_eq!(
            texts(&d.facts(&prog, false)),
            [
                "each Client sends eAcquire, then eRelease, once each",
                "creates 1 Server and 2 Client, never more",
                "eAcquire is only sent with id = 1 or 2, kind = Small",
            ]
        );
    }

    #[test]
    fn counts_orders_and_merged_workers() {
        let prog = program();
        let mut a = recorder(&prog);
        schedule(&mut a, &prog, &[1], &["eAcquire", "eRelease", "eAcquire", "eRelease"], false);
        let mut b = recorder(&prog);
        schedule(&mut b, &prog, &[1, 2], &["eAcquire", "eRelease"], false);
        a.merge(&b);
        assert_eq!(
            texts(&a.facts(&prog, false)),
            [
                "creates 1 Server and 1 to 2 Client, never more",
                "each Client sends eAcquire and eRelease 1 to 2 times each",
                "eAcquire is only sent with id = 1 or 2, kind = Small",
            ]
        );
        // once an instance also releases before it acquires, neither order is a limit
        let mut c = recorder(&prog);
        schedule(&mut c, &prog, &[1], &["eRelease", "eAcquire"], false);
        a.merge(&c);
        assert!(texts(&a.facts(&prog, false)).iter().all(|t| !t.contains("after")));
    }

    #[test]
    fn an_order_never_tried_is_reported() {
        let prog = program();
        let mut d = recorder(&prog);
        schedule(&mut d, &prog, &[1], &["eAcquire", "eAcquire", "eRelease"], false);
        schedule(&mut d, &prog, &[1], &["eAcquire", "eRelease", "eRelease"], false);
        let facts = d.facts(&prog, false);
        assert!(facts.iter().any(|f| f.id == "Client.eAcquire-after-eRelease"), "{:?}", texts(&facts));
        assert_eq!(facts[0].text, "no Client sends eAcquire after eRelease");
    }

    #[test]
    fn repeating_drivers_claim_no_counts() {
        let prog = program();
        let mut d = recorder(&prog);
        schedule(&mut d, &prog, &[1], &["eAcquire", "eRelease"], false);
        let facts = texts(&d.facts(&prog, true));
        assert_eq!(
            facts,
            ["creates 1 Server and 1 Client, never more", "eAcquire is always sent with id = 1, kind = Small"]
        );
        // every value of a field tried: nothing to say about it
        let mut d = recorder(&prog);
        let (server, client) = (id(&prog, "Server"), id(&prog, "Client"));
        d.record_send(&prog, 1, client, server, ev(&prog, "eAcquire"), &acquire(1, 0));
        d.record_send(&prog, 1, client, server, ev(&prog, "eAcquire"), &acquire(1, 1));
        d.end_schedule(false, false);
        assert!(texts(&d.facts(&prog, true)).contains(&"eAcquire is always sent with id = 1".to_string()));
    }
}
