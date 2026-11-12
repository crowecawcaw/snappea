//! Activity coverage (`ReportActivityCoverage`): which events each state received and
//! sent, which handled events were never exercised, and the state transitions observed —
//! the `<file>.coverage.txt` report of upstream's `ActivityCoverageReporter`.
//!
//! Upstream assembles this from two runtime loggers, `ControlledRuntimeLogEventCoverage`
//! and `ControlledRuntimeLogGraphBuilder` (with machine instances collapsed to their type).
//! The hooks below are called at the same points as those loggers' callbacks, and keep the
//! same quirks (one global "dequeued" event, monitor do-actions named with their
//! temperature, ...), with nodes and events interned for speed.

use std::collections::{HashMap, HashSet};

use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::sema::{EventId, HandlerAction, MachineId, Program, StateId, Temperature};

use crate::checker::engine::culture_order;
use crate::checker::runtime::{Machine, Monitor, Runtime, GOTO_EVENT};
use crate::checker::value::event_cs_name;

/// Node and event naming tables for a program (built once).
#[derive(Debug, Clone)]
pub struct CoverageTables {
    /// Node id strings (`PImplementation.Machine.State`, possibly with `[hot]`/`[cold]`).
    pub nodes: Vec<String>,
    /// Plain node of each state (indexed by `StateId`).
    plain: Vec<u32>,
    /// Node of each state named with its temperature (`CurrentStateNameWithTemperature`).
    tempered: Vec<u32>,
    /// Per machine, the `<Machine>.StateMachine` node that upstream's `OnReceiveEvent`
    /// links to (it passes the node category where the state name belongs).
    receive_target: Vec<u32>,
}

impl CoverageTables {
    pub fn new(prog: &Program) -> CoverageTables {
        let mut nodes = Vec::new();
        let mut index: HashMap<String, u32> = HashMap::new();
        let mut intern = |s: String| -> u32 {
            *index.entry(s.clone()).or_insert_with(|| {
                nodes.push(s);
                (nodes.len() - 1) as u32
            })
        };
        let mut plain = vec![0; prog.states.len()];
        let mut tempered = vec![0; prog.states.len()];
        let mut receive_target = Vec::with_capacity(prog.machines.len());
        for m in &prog.machines {
            for s in &m.states {
                let st = prog.state(*s);
                let base = format!("PImplementation.{}.{}", m.cs_name, st.name);
                plain[s.idx()] = intern(base.clone());
                let suffix = match st.temperature {
                    Temperature::Hot => "[hot]",
                    Temperature::Cold => "[cold]",
                    _ => "",
                };
                tempered[s.idx()] = intern(format!("{}{}", base, suffix));
            }
            receive_target.push(intern(format!("PImplementation.{}.StateMachine", m.cs_name)));
        }
        CoverageTables { nodes, plain, tempered, receive_target }
    }

    pub(crate) fn plain(&self, s: StateId) -> u32 {
        self.plain[s.idx()]
    }

    fn tempered(&self, s: StateId) -> u32 {
        self.tempered[s.idx()]
    }
}

/// A set of interned pairs, updated on every event (its order is never observed).
pub type PairSet = FxHashSet<(u32, u32)>;

/// Coverage accumulated over all schedules (`CoverageInfo`).
#[derive(Debug, Clone, Default)]
pub struct CoverageInfo {
    /// Declared machines and monitors, in declaration order, with whether each is a monitor.
    pub declared: Vec<(MachineId, bool)>,
    /// `(state node, event)` pairs received / sent.
    pub received: PairSet,
    pub sent: PairSet,
    /// Transition links `(source node, target node)` of the coverage graph.
    pub links: PairSet,
    /// Monitor states entered, including initial and terminal states.
    pub monitor_states: FxHashSet<StateId>,
    /// Actual consecutive monitor state entries (not event graph links).
    pub monitor_transitions: FxHashSet<(StateId, StateId)>,
    /// Consecutive state transitions on one machine/monitor instance in one schedule.
    pub transition_pairs: FxHashSet<(StateId, StateId, StateId)>,
    /// Code coverage probes that ran (see `Op::Probe`), as a bit set.
    pub probes: ProbeSet,
    /// Outcomes of short-circuit Boolean expression edges, including assignments
    /// and returns outside the reported `if`/`while` condition coverage.
    pub condition_edges: FxHashSet<(u32, usize, bool)>,
}

/// A set of code coverage probe ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbeSet {
    bits: Vec<u64>,
}

impl ProbeSet {
    pub fn insert(&mut self, id: u32) {
        let (word, bit) = (id as usize / 64, id % 64);
        if word >= self.bits.len() {
            self.bits.resize(word + 1, 0);
        }
        self.bits[word] |= 1 << bit;
    }

    fn intersection_count(&self, other: &ProbeSet) -> usize {
        self.bits.iter().zip(&other.bits).map(|(a, b)| (a & b).count_ones() as usize).sum()
    }

    pub(crate) fn len(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn contains(&self, id: u32) -> bool {
        self.bits.get(id as usize / 64).is_some_and(|w| w & (1 << (id % 64)) != 0)
    }

    pub fn union(&mut self, other: &ProbeSet) {
        if other.bits.len() > self.bits.len() {
            self.bits.resize(other.bits.len(), 0);
        }
        for (a, b) in self.bits.iter_mut().zip(&other.bits) {
            *a |= b;
        }
    }
}

impl CoverageInfo {
    /// Adds the coverage of another worker's schedules (`CoverageInfo.Merge`).
    pub fn merge(&mut self, other: CoverageInfo) {
        for d in other.declared {
            if !self.declared.contains(&d) {
                self.declared.push(d);
            }
        }
        self.received.extend(other.received);
        self.sent.extend(other.sent);
        self.links.extend(other.links);
        self.monitor_states.extend(other.monitor_states);
        self.monitor_transitions.extend(other.monitor_transitions);
        self.transition_pairs.extend(other.transition_pairs);
        self.probes.union(&other.probes);
        self.condition_edges.extend(other.condition_edges);
    }
}

/// Who put an entry in a graph-builder inbox (`EventInfo.Name/Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sender {
    Machine(usize),
    Monitor(usize),
}

#[derive(Debug, Clone, Copy)]
struct InboxEntry {
    sender: Sender,
    /// Node of the sender's state (`EventInfo.State`).
    state: u32,
    event: u32,
}

/// Per-schedule state of the coverage loggers.
#[derive(Debug, Clone, Default)]
struct CoverageRun {
    /// Last two state entries, partitioned by actor kind and instance. Reset per run.
    state_history: FxHashMap<(bool, usize), (Option<StateId>, StateId)>,
    monitor_previous: FxHashMap<usize, StateId>,
    /// `ControlledRuntimeLogEventCoverage.Dequeued` (a single, global slot upstream).
    dequeued: Option<u32>,
    /// Graph builder inboxes per machine type, split by event: every lookup is for one
    /// event, and a single inbox per type made each dequeue scan every pending entry.
    inbox: FxHashMap<(MachineId, u32), Vec<InboxEntry>>,
    /// Graph builder inboxes per monitor type.
    monitor_inbox: FxHashMap<MachineId, Vec<InboxEntry>>,
    /// Graph builder `Dequeued`, per machine instance.
    graph_dequeued: FxHashMap<usize, InboxEntry>,
}

/// Activity coverage being collected by a runtime: the coverage accumulated so far plus the
/// loggers' state for the current schedule.
#[derive(Debug, Clone)]
pub struct Coverage {
    tables: Rc<CoverageTables>,
    info: CoverageInfo,
    run: CoverageRun,
    guide_conditions: bool,
    guidance_probes: Option<Rc<ProbeSet>>,
}

impl Coverage {
    fn state_enter(&mut self, monitor: bool, instance: usize, state: StateId) {
        let previous = self.run.state_history.insert((monitor, instance), (None, state));
        if let Some((before, last)) = previous {
            self.run.state_history.insert((monitor, instance), (Some(last), state));
            if let Some(first) = before {
                self.info.transition_pairs.insert((first, last, state));
            }
        }
    }

    pub(crate) fn feature_count(&self) -> usize {
        self.info.condition_edges.len()
            + self.info.transition_pairs.len()
            + self
                .guidance_probes
                .as_ref()
                .map_or_else(|| self.info.probes.len(), |mask| self.info.probes.intersection_count(mask))
            + self.info.received.len()
            + self.info.sent.len()
            + self.info.links.len()
            + self.info.monitor_states.len()
            + self.info.monitor_transitions.len()
    }

    /// Continues accumulating `info` (named by `tables`) in a new schedule.
    pub fn new(tables: Rc<CoverageTables>, info: CoverageInfo) -> Coverage {
        Coverage { tables, info, run: CoverageRun::default(), guide_conditions: false, guidance_probes: None }
    }

    /// Select probes that add independent guidance. Reported atomic guard outcomes
    /// overlap expression-edge/branch features; retain them in reports without adding
    /// a second, competing source of seed novelty.
    pub(crate) fn with_guidance_probes(mut self, mask: Rc<ProbeSet>) -> Self {
        self.guidance_probes = Some(mask);
        self
    }

    pub(crate) fn with_condition_guidance(mut self, enabled: bool) -> Self {
        self.guide_conditions = enabled;
        self
    }

    pub(crate) fn condition(&mut self, fun: u32, pc: usize, outcome: bool) {
        if self.guide_conditions {
            self.info.condition_edges.insert((fun, pc, outcome));
        }
    }

    /// The coverage accumulated so far.
    pub fn into_info(self) -> CoverageInfo {
        self.info
    }

    /// Whether code coverage probe `id` ran in this schedule.
    #[inline]
    pub(crate) fn probe_ran(&self, id: u32) -> bool {
        self.info.probes.contains(id)
    }

    /// Records that code coverage probe `id` ran.
    #[inline]
    pub(crate) fn probe_hit(&mut self, id: u32) {
        self.info.probes.insert(id);
    }

    fn add_event(&mut self, target: MachineId, entry: InboxEntry) {
        self.run.inbox.entry((target, entry.event)).or_default().push(entry);
    }

    fn add_monitor_event(&mut self, target: MachineId, entry: InboxEntry) {
        self.run.monitor_inbox.entry(target).or_default().push(entry);
    }

    /// `PopEvent`: removes every matching entry, returns the earliest.
    fn pop_event(&mut self, target: MachineId, event: u32) -> Option<InboxEntry> {
        let inbox = self.run.inbox.get_mut(&(target, event))?;
        let first = inbox.first().copied();
        inbox.clear();
        first
    }

    /// Pops the latest entry of a monitor's inbox and links its state to `target`.
    fn link_popped(&mut self, decl: MachineId, target: u32) {
        if let Some(e) = self.run.monitor_inbox.get_mut(&decl).and_then(|i| i.pop()) {
            self.link(e.state, target);
        }
    }

    fn link(&mut self, a: u32, b: u32) {
        self.info.links.insert((a, b));
    }

    /// `ControlledRuntimeLogEventCoverage.OnEventHandled`.
    fn event_handled(&mut self, state: u32) {
        if let Some(ev) = self.run.dequeued.take() {
            self.info.received.insert((state, ev));
        }
    }

    /// `LinkTransition`.
    fn link_transition(&mut self, m: usize, current: u32, new_state: Option<u32>) {
        if let Some(i) = self.run.graph_dequeued.remove(&m) {
            if i.sender != Sender::Machine(m) || i.state != current {
                self.link(i.state, current);
            }
        }
        if let Some(n) = new_state {
            self.link(current, n);
        }
    }
}

impl Runtime<'_> {
    pub(crate) fn cov_machine_enter(&mut self, m: usize) {
        if let (Some(cov), Some(s)) = (self.coverage.as_mut(), self.machines[m].state) {
            cov.state_enter(false, m, s);
        }
    }

    /// The coverage being collected and the node of machine `m`'s state; `None` when
    /// coverage is off or the machine has not entered a state.
    fn cov_machine(&mut self, m: usize) -> Option<(&mut Coverage, u32)> {
        let cov = self.coverage.as_mut()?;
        let state = cov.tables.plain(self.machines[m].state?);
        Some((cov, state))
    }

    /// The coverage being collected and monitor `mon`'s state and declaration; `None` when
    /// coverage is off or the monitor has not entered a state.
    fn cov_monitor(&mut self, mon: usize) -> Option<(&mut Coverage, StateId, MachineId)> {
        let cov = self.coverage.as_mut()?;
        let Monitor { state: Some(s), decl, .. } = self.monitors[mon] else { return None };
        Some((cov, s, decl))
    }

    /// `ReportActivityCoverageOfStateMachine/Monitor` + `OnCreateStateMachine/Monitor`.
    pub(crate) fn cov_declare(&mut self, decl: MachineId, monitor: bool) {
        if let Some(c) = self.coverage.as_mut() {
            if !c.info.declared.iter().any(|(d, _)| *d == decl) {
                c.info.declared.push((decl, monitor));
            }
        }
    }

    /// `OnSendEvent`.
    pub(crate) fn cov_send(&mut self, sender: usize, target_decl: MachineId, e: EventId) {
        let Some((cov, state)) = self.cov_machine(sender) else { return };
        cov.info.sent.insert((state, e.0));
        cov.add_event(target_decl, InboxEntry { sender: Sender::Machine(sender), state, event: e.0 });
    }

    /// `OnRaiseEvent` (user raises and the goto pseudo event).
    pub(crate) fn cov_raise(&mut self, m: usize, e: EventId) {
        let decl = self.machines[m].decl;
        let Some((cov, state)) = self.cov_machine(m) else { return };
        cov.info.sent.insert((state, e.0));
        cov.add_event(decl, InboxEntry { sender: Sender::Machine(m), state, event: e.0 });
    }

    /// `OnDequeueEvent`.
    pub(crate) fn cov_dequeue(&mut self, m: usize, e: EventId) {
        let decl = self.machines[m].decl;
        let Some(cov) = self.coverage.as_mut() else { return };
        cov.run.dequeued = Some(e.0);
        if let Some(info) = cov.pop_event(decl, e.0) {
            cov.run.graph_dequeued.insert(m, info);
        }
    }

    /// `OnDefaultEventHandler`.
    pub(crate) fn cov_default(&mut self, m: usize) {
        let ev = self.prog.null_event.0;
        let Machine { decl, state, .. } = self.machines[m];
        let Some(cov) = self.coverage.as_mut() else { return };
        cov.run.dequeued = Some(ev);
        let Some(state) = state.map(|s| cov.tables.plain(s)) else { return };
        cov.add_event(decl, InboxEntry { sender: Sender::Machine(m), state, event: ev });
        match cov.pop_event(decl, ev) {
            Some(i) => {
                cov.run.graph_dequeued.insert(m, i);
            }
            None => {
                cov.run.graph_dequeued.remove(&m);
            }
        }
    }

    /// `OnHandleRaisedEvent`.
    pub(crate) fn cov_handle_raised(&mut self, m: usize, e: EventId) {
        let decl = self.machines[m].decl;
        let Some(cov) = self.coverage.as_mut() else { return };
        let run = &mut cov.run;
        run.dequeued = Some(e.0);
        let found = run.inbox.get(&(decl, e.0)).and_then(|inbox| inbox.last().copied());
        if let Some(info) = found {
            run.graph_dequeued.insert(m, info);
        }
    }

    /// `OnExecuteAction` (do, entry, exit and `with` actions of machines).
    pub(crate) fn cov_execute_action(&mut self, m: usize) {
        let Some((cov, state)) = self.cov_machine(m) else { return };
        cov.event_handled(state);
        cov.link_transition(m, state, None);
    }

    /// `OnGotoState`.
    pub(crate) fn cov_goto(&mut self, m: usize, target: StateId) {
        let Some((cov, state)) = self.cov_machine(m) else { return };
        cov.event_handled(state);
        let target = cov.tables.plain(target);
        cov.link_transition(m, state, Some(target));
    }

    /// `OnReceiveEvent` (a `receive` statement got its event).
    pub(crate) fn cov_receive(&mut self, m: usize, e: EventId) {
        let decl = self.machines[m].decl;
        let Some((cov, state)) = self.cov_machine(m) else { return };
        cov.info.received.insert((state, e.0));
        let found = cov.run.inbox.get_mut(&(decl, e.0)).and_then(|inbox| inbox.pop());
        if let Some(info) = found {
            let target = cov.tables.receive_target[decl.idx()];
            cov.link(info.state, target);
        }
    }

    /// `OnMonitorProcessEvent`.
    pub(crate) fn cov_monitor_process(&mut self, mon: usize, e: EventId, sender: Option<usize>) {
        let sender_state = sender.and_then(|m| self.machines[m].state);
        let Some((cov, s, decl)) = self.cov_monitor(mon) else { return };
        let state = cov.tables.plain(s);
        cov.info.received.insert((state, e.0));
        cov.add_monitor_event(decl, InboxEntry { sender: Sender::Monitor(mon), state, event: e.0 });
        if let Some(src) = sender_state {
            let src = cov.tables.plain(src);
            cov.link(src, state);
        }
    }

    /// `OnMonitorExecuteAction` (do actions; the state is named with its temperature).
    pub(crate) fn cov_monitor_do(&mut self, mon: usize) {
        let Some((cov, s, decl)) = self.cov_monitor(mon) else { return };
        let target = cov.tables.tempered(s);
        cov.link_popped(decl, target);
    }

    /// `OnMonitorRaiseEvent` (raises and gotos inside monitor actions).
    pub(crate) fn cov_monitor_raise(&mut self, mon: usize, e: EventId) {
        let Some((cov, s, decl)) = self.cov_monitor(mon) else { return };
        let state = cov.tables.plain(s);
        cov.info.sent.insert((state, e.0));
        cov.add_monitor_event(decl, InboxEntry { sender: Sender::Monitor(mon), state, event: e.0 });
    }

    /// `OnMonitorStateTransition` on entry.
    pub(crate) fn cov_monitor_enter(&mut self, mon: usize) {
        let Some((cov, s, decl)) = self.cov_monitor(mon) else { return };
        cov.state_enter(true, mon, s);
        let target = cov.tables.tempered(s);
        cov.info.monitor_states.insert(s);
        if let Some(previous) = cov.run.monitor_previous.insert(mon, s) {
            cov.info.monitor_transitions.insert((previous, s));
        }
        cov.link_popped(decl, target);
    }
}

/// Fully qualified event type name, as the coverage keys use it.
fn event_full_name(prog: &Program, ev: u32) -> String {
    if ev == GOTO_EVENT.0 {
        return "PChecker.Runtime.StateMachines.Events.GotoStateEvent".into();
    }
    let e = EventId(ev);
    if e == prog.null_event {
        return "PChecker.Runtime.Events.DefaultEvent".into();
    }
    format!("PImplementation.{}", event_cs_name(prog, e))
}

fn is_builtin(prog: &Program, ev: u32) -> bool {
    ev == GOTO_EVENT.0 || EventId(ev) == prog.null_event
}

/// After the last `.` (`GetSanitizedName`, `GetStateName`).
fn after_dot(s: &str) -> &str {
    match s.rfind('.') {
        Some(i) if i > 0 => &s[i + 1..],
        _ => s,
    }
}

/// Up to the last `.` (`GetMachineId`).
fn before_dot(s: &str) -> &str {
    match s.rfind('.') {
        Some(i) if i > 0 => &s[..i],
        _ => s,
    }
}

fn sorted_names(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = names.map(|n| after_dot(&n).to_string()).collect();
    v.sort_by(|a, b| culture_order(a, b));
    v
}

fn percent(covered: usize, total: usize) -> String {
    if total == 0 {
        "100.0".into()
    } else {
        format!("{:.1}", covered as f64 * 100.0 / total as f64)
    }
}

/// `ActivityCoverageReporter.WriteCoverageText`.
pub fn coverage_report(prog: &Program, tables: &CoverageTables, info: &CoverageInfo) -> String {
    let node = |n: u32| tables.nodes[n as usize].as_str();
    // registered (handled, not deferred/ignored) events per state node
    let mut registered: Vec<(MachineId, u32, Vec<u32>)> = Vec::new();
    for (decl, _) in &info.declared {
        for s in &prog.machine(*decl).states {
            let evs: Vec<u32> = prog
                .state(*s)
                .handlers
                .iter()
                .filter(|h| matches!(h.action, HandlerAction::Do(_) | HandlerAction::Goto { .. }))
                .map(|h| h.event.0)
                .collect();
            if !evs.is_empty() {
                registered.push((*decl, tables.plain(*s), evs));
            }
        }
    }
    let uncovered = |state: u32, evs: &[u32]| -> Vec<u32> {
        evs.iter().copied().filter(|e| !info.received.contains(&(state, *e))).collect::<Vec<u32>>()
    };
    let total: usize = registered.iter().map(|(_, _, e)| e.len()).sum();
    let total_uncovered: usize = registered.iter().map(|(_, s, e)| uncovered(*s, e).len()).sum();
    let mut out = String::new();
    let header = |out: &mut String, h: String| {
        out.push_str(&h);
        out.push('\n');
        out.push_str(&"=".repeat(h.chars().count()));
        out.push('\n');
    };
    header(&mut out, format!("Total event coverage: {}%", percent(total - total_uncovered, total)));
    let mut machines: Vec<(String, MachineId, bool)> = info
        .declared
        .iter()
        .map(|(d, mon)| (format!("PImplementation.{}", prog.machine(*d).cs_name), *d, *mon))
        .collect();
    machines.sort_by(|a, b| culture_order(&a.0, &b.0));
    for (full, decl, monitor) in machines {
        header(&mut out, format!("{}: {}", if monitor { "Monitor" } else { "StateMachine" }, after_dot(&full)));
        let mine: Vec<&(MachineId, u32, Vec<u32>)> = registered.iter().filter(|(d, _, _)| *d == decl).collect();
        let mt: usize = mine.iter().map(|(_, _, e)| e.len()).sum();
        let mu: usize = mine.iter().map(|(_, s, e)| uncovered(*s, e).len()).sum();
        out.push_str(&format!("Event coverage: {}%\n", percent(mt - mu, mt)));
        for s in &prog.machine(decl).states {
            let key = tables.plain(*s);
            let state_name = &prog.state(*s).name;
            let evs: Vec<u32> = mine.iter().filter(|(_, k, _)| *k == key).flat_map(|(_, _, e)| e.clone()).collect();
            let st = evs.len();
            let su = uncovered(key, &evs);
            out.push('\n');
            out.push_str(&format!(
                "\tState: {}{}\n",
                state_name,
                if st > 0 && st == su.len() { " is uncovered" } else { "" }
            ));
            if st == 0 {
                out.push_str("\t\tState has no expected events, so coverage is 100%\n");
            } else if st != su.len() {
                out.push_str(&format!("\t\tState event coverage: {}%\n", percent(st - su.len(), st)));
            }
            let key_name = node(key);
            let mut incoming: HashSet<String> = HashSet::new();
            let mut outgoing: HashSet<String> = HashSet::new();
            for (a, b) in &info.links {
                let (sa, sb) = (node(*a), node(*b));
                if before_dot(sa) == full && before_dot(sb) == full {
                    if sb == key_name {
                        incoming.insert(after_dot(sa).to_string());
                    }
                    if sa == key_name {
                        outgoing.insert(after_dot(sb).to_string());
                    }
                }
            }
            let received: HashSet<String> = info
                .received
                .iter()
                .filter(|(n, e)| *n == key && !is_builtin(prog, *e))
                .map(|(_, e)| event_full_name(prog, *e))
                .collect();
            if !received.is_empty() {
                out.push_str(&format!("\t\tEvents received: {}\n", sorted_names(received.into_iter()).join(", ")));
            }
            let sent: HashSet<String> = info
                .sent
                .iter()
                .filter(|(n, e)| *n == key && !is_builtin(prog, *e))
                .map(|(_, e)| event_full_name(prog, *e))
                .collect();
            if !sent.is_empty() {
                out.push_str(&format!("\t\tEvents sent: {}\n", sorted_names(sent.into_iter()).join(", ")));
            }
            if !su.is_empty() {
                let names = su.iter().map(|e| event_full_name(prog, *e)).collect::<HashSet<_>>();
                out.push_str(&format!("\t\tEvents not covered: {}\n", sorted_names(names.into_iter()).join(", ")));
            }
            if !incoming.is_empty() {
                out.push_str(&format!("\t\tPrevious states: {}\n", sorted_names(incoming.into_iter()).join(", ")));
            }
            if !outgoing.is_empty() {
                out.push_str(&format!("\t\tNext states: {}\n", sorted_names(outgoing.into_iter()).join(", ")));
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod guidance_tests {
    use super::*;

    #[test]
    fn expression_outcomes_and_transition_pairs_add_independent_feedback_novelty() {
        let mut sources = crate::syntax::SourceMap::new();
        sources.add(
            "model.p",
            "machine Main { start state A { entry { goto B; } } state B { entry { goto C; } } state C {} }",
        );
        let analysis = crate::sema::compile(&sources, &Default::default()).unwrap();
        let mut cov = Coverage::new(Rc::new(CoverageTables::new(&analysis.program)), CoverageInfo::default());
        cov = cov.with_guidance_probes(Rc::new(ProbeSet::default())).with_condition_guidance(true);
        let [a, b, c] = analysis.program.machines[0].states[..] else { panic!("three-state fixture") };
        cov.info.links.extend([(a.0, b.0), (b.0, c.0)]);
        let initial = cov.feature_count();
        cov.probe_hit(0); // an atomic condition's true outcome
        cov.probe_hit(1); // its false outcome, a distinct probe
        cov.probe_hit(1);
        assert_eq!(cov.feature_count(), initial, "reported guards must not duplicate expression novelty");
        assert!(cov.info.probes.contains(0) && cov.info.probes.contains(1));
        cov.condition(0, 42, true);
        cov.condition(0, 42, false);
        cov.condition(0, 42, false);
        assert_eq!(cov.feature_count(), initial + 2);
        cov.state_enter(false, 0, a);
        cov.state_enter(false, 0, b);
        cov.state_enter(false, 0, c);
        assert_eq!(cov.feature_count(), initial + 3, "the pair is novel even when both edges are covered");
        let info = cov.into_info();
        let mut next = Coverage::new(Rc::new(CoverageTables::new(&analysis.program)), info)
            .with_guidance_probes(Rc::new(ProbeSet::default()))
            .with_condition_guidance(true);
        next.state_enter(false, 0, a);
        next.state_enter(false, 0, b);
        next.state_enter(false, 0, c);
        assert_eq!(next.feature_count(), initial + 3, "repeating the pair gives no further novelty");
    }
}
