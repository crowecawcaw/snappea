//! Fair-cycle analysis: how an exhaustive search decides liveness.
//!
//! A liveness bug is an infinite execution, fair to the scheduler and to the model's
//! choices, through which some liveness monitor stays out of its cold states and is hot
//! again and again. In a finite state graph such an execution ends in a cycle, so the
//! search records the graph of states where a liveness monitor is not cold while it
//! explores ([`LiveGraph`]) and, once it has explored every state, looks for a fair
//! cycle in it ([`LiveGraph::analyze`]).
//!
//! Fairness has two parts:
//!
//! - **Choices.** A choice site (`$`, `$$`, `choose`, a foreign `TryRandom*`, told apart
//!   by its number of values) that is reached infinitely often takes each of its values
//!   infinitely often. A timer that keeps choosing not to fire is not a bug.
//! - **Scheduling.** An operation (a machine, or the test task) that is enabled at every
//!   scheduling point from some point on runs infinitely often: weak fairness, as the
//!   lasso check on a single cycle enforces.
//!
//! The analysis is the Emerson-Lei refinement for Streett conditions: per liveness
//! monitor, restrict the graph to states where it is not cold and transitions through
//! which it does not enter a cold state, and split it into strongly connected
//! components. A component in which some value of a choice site never occurs cannot
//! take that site infinitely often, so the transitions that take that site are removed
//! and the rest is split again. A component left with every value of every site it
//! takes is fair to the scheduler when every operation enabled at all of its states
//! runs on one of its transitions (otherwise no part of it is), and is a liveness bug
//! when the monitor is hot at one of its states. The witness is a path from the start
//! of the test to such a state and a cycle in the component from it through a
//! transition for every choice value and operation the fairness rules ask for; the
//! runtime replays it as a lasso and reports it like any other liveness bug.
//!
//! The record is kept only for tests with hot monitor states: about 30 bytes per
//! explored state (the parent it was first reached from, for the witness's path) plus
//! about 40 bytes per state and per transition where a liveness monitor is not cold. A
//! record that outgrows [`RECORD_BUDGET`] is dropped, and liveness stays unverified.

use std::collections::VecDeque;
use std::hash::Hash;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::checker::strategy::ScheduleStep;

/// States and transitions the record of one search keeps, over all its workers, before
/// it gives up on deciding liveness (about 100 MB).
pub const RECORD_BUDGET: usize = 3_000_000;

/// A nondeterministic choice: its code site, how many values it had, the value taken.
pub type ChoiceTaken = (u64, u32, u32);

/// What the runtime knows about a transition and the state it reached, for the record.
#[derive(Debug, Clone, Default)]
pub struct LiveFacts {
    /// The schedule steps of the transition: the operation that ran and its choices.
    pub steps: Vec<ScheduleStep>,
    /// The transition's nondeterministic choices with more than one value.
    pub choices: Vec<ChoiceTaken>,
    /// Liveness monitors (bit = monitor index) that entered a cold state during it.
    pub cold: u64,
    /// Liveness monitors not in a cold state at the state reached.
    pub not_cold: u64,
    /// Liveness monitors in a hot state there.
    pub hot: u64,
    /// The operations enabled there.
    pub enabled: Vec<u64>,
    /// The test has more monitors than the masks hold.
    pub too_many_monitors: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Label {
    steps: Box<[ScheduleStep]>,
    choices: Box<[ChoiceTaken]>,
    cold: u64,
}

impl Label {
    /// The operation that ran: the transition's first step.
    fn operation(&self) -> Option<u64> {
        match self.steps.first() {
            Some(ScheduleStep::Operation(id)) => Some(*id),
            _ => None,
        }
    }
}

/// How a state was first reached at its smallest depth.
#[derive(Debug, Clone, Copy)]
struct Parent {
    depth: u32,
    label: u32,
    /// The state it was reached from; [`START`] at the start of the test.
    parent: u64,
}

/// The parent of the states reached before the first scheduling point (a state hash
/// equal to it is as unlikely as any other hash collision).
const START: u64 = u64::MAX;

/// A state where a liveness monitor is not cold.
#[derive(Debug, Clone, Copy)]
struct Node {
    not_cold: u64,
    hot: u64,
    enabled: u32,
}

#[derive(Debug)]
struct Interner<T> {
    ids: FxHashMap<T, u32>,
    items: Vec<T>,
}

impl<T> Default for Interner<T> {
    fn default() -> Self {
        Interner { ids: FxHashMap::default(), items: Vec::new() }
    }
}

impl<T: Hash + Eq + Clone> Interner<T> {
    fn intern(&mut self, t: T) -> u32 {
        if let Some(id) = self.ids.get(&t) {
            return *id;
        }
        let id = self.items.len() as u32;
        self.items.push(t.clone());
        self.ids.insert(t, id);
        id
    }
}

/// One search's (or one worker's) record of the explored state graph (see the module
/// docs).
#[derive(Debug, Default)]
pub struct LiveGraph {
    limit: usize,
    tree: FxHashMap<u64, Parent>,
    nodes: FxHashMap<u64, Node>,
    edges: FxHashSet<(u64, u64, u32)>,
    labels: Interner<Label>,
    enabled: Interner<Box<[u64]>>,
    unusable: Option<String>,
}

/// What the fair-cycle analysis concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No fair cycle keeps a liveness monitor out of its cold states.
    Verified,
    /// The analysis could not decide, and why.
    Unverified(String),
    /// A fair cycle that keeps a liveness monitor hot: the schedule's first `lasso` steps
    /// reach it, the rest go around it once.
    Livelock { steps: Vec<ScheduleStep>, lasso: usize },
}

impl LiveGraph {
    /// An empty record that gives up past `limit` states and transitions.
    pub fn new(limit: usize) -> LiveGraph {
        LiveGraph { limit: limit.max(1), ..Default::default() }
    }

    /// Why the record cannot decide liveness, if it cannot.
    pub fn unusable(&self) -> Option<&str> {
        self.unusable.as_deref()
    }

    /// Whether the record is still kept.
    pub fn recording(&self) -> bool {
        self.unusable.is_none()
    }

    fn give_up(&mut self, why: String) {
        *self = LiveGraph { limit: self.limit, unusable: Some(why), ..Default::default() };
    }

    fn records(&self) -> usize {
        self.tree.len() + self.nodes.len() + self.edges.len()
    }

    /// Records that the search reached `hash` at `depth` from `prev` (`None`: from the start
    /// of the test) through the transition `facts` describes; `explored` when the search
    /// explores it from here (it is new, or was reached deeper before).
    pub(crate) fn record(&mut self, prev: Option<u64>, hash: u64, depth: usize, explored: bool, facts: LiveFacts) {
        if self.unusable.is_some() {
            return;
        }
        if facts.too_many_monitors {
            self.give_up("the test has more than 64 monitors".into());
            return;
        }
        let live = facts.not_cold != 0;
        let edge = live && prev.is_some_and(|p| self.nodes.contains_key(&p));
        let new_node = live && !self.nodes.contains_key(&hash);
        let deeper = self.tree.get(&hash).is_some_and(|p| p.depth as usize <= depth);
        if !edge && !new_node && (!explored || deeper) {
            return;
        }
        if new_node {
            let enabled = self.enabled.intern(facts.enabled.into_boxed_slice());
            self.nodes.insert(hash, Node { not_cold: facts.not_cold, hot: facts.hot, enabled });
        }
        let label = self.labels.intern(Label {
            steps: facts.steps.into_boxed_slice(),
            choices: facts.choices.into_boxed_slice(),
            cold: facts.cold,
        });
        if edge {
            self.edges.insert((prev.unwrap_or_default(), hash, label));
        }
        if explored && !deeper {
            let parent = prev.unwrap_or(START);
            self.tree.insert(hash, Parent { depth: depth.min(u32::MAX as usize) as u32, label, parent });
        }
        if self.records() > self.limit {
            let limit = self.limit;
            self.give_up(format!(
                "the record of states where a liveness monitor is not cold outgrew its budget of {} states and transitions",
                limit
            ));
        }
    }

    /// Adds another worker's record of the same search.
    pub fn absorb(&mut self, other: LiveGraph) {
        if self.unusable.is_some() {
            return;
        }
        if let Some(why) = other.unusable {
            self.give_up(why);
            return;
        }
        let labels: Vec<u32> = other.labels.items.into_iter().map(|l| self.labels.intern(l)).collect();
        let enabled: Vec<u32> = other.enabled.items.into_iter().map(|e| self.enabled.intern(e)).collect();
        for (hash, mut p) in other.tree {
            p.label = labels[p.label as usize];
            match self.tree.get(&hash) {
                // equal depths: either parent will do; prefer one independent of worker order
                Some(q) if (q.depth, q.parent) <= (p.depth, p.parent) => {}
                _ => {
                    self.tree.insert(hash, p);
                }
            }
        }
        for (hash, mut n) in other.nodes {
            n.enabled = enabled[n.enabled as usize];
            self.nodes.entry(hash).or_insert(n);
        }
        for (from, to, label) in other.edges {
            self.edges.insert((from, to, labels[label as usize]));
        }
    }

    /// Looks for a fair cycle that keeps a liveness monitor out of its cold states (see
    /// the module docs). Only meaningful once the search explored every state.
    pub fn analyze(&self) -> Verdict {
        if let Some(why) = &self.unusable {
            return Verdict::Unverified(why.clone());
        }
        let mut hashes: Vec<u64> = self.nodes.keys().copied().collect();
        hashes.sort_unstable();
        let index: FxHashMap<u64, u32> = hashes.iter().enumerate().map(|(i, h)| (*h, i as u32)).collect();
        let nodes: Vec<Node> = hashes.iter().map(|h| self.nodes[h]).collect();
        let mut edges: Vec<Edge> = self
            .edges
            .iter()
            .filter_map(|(f, t, l)| Some(Edge { from: *index.get(f)?, to: *index.get(t)?, label: *l }))
            .collect();
        // the same order whichever worker recorded what
        edges.sort_unstable_by(|a, b| {
            (a.from, a.to).cmp(&(b.from, b.to)).then_with(|| self.label(a).cmp(self.label(b)))
        });
        let g = Graph { record: self, hashes: &hashes, nodes: &nodes, edges: &edges };
        let monitors = nodes.iter().fold(0u64, |m, n| m | n.hot);
        for bit in (0..64).map(|i| 1u64 << i).filter(|b| monitors & b != 0) {
            let candidates: Vec<usize> = (0..edges.len())
                .filter(|&e| {
                    let edge = &edges[e];
                    nodes[edge.from as usize].not_cold & bit != 0
                        && nodes[edge.to as usize].not_cold & bit != 0
                        && self.label(edge).cold & bit == 0
                })
                .collect();
            if let Some(component) = g.fair_component(candidates, bit) {
                return match g.witness(&component, bit) {
                    Some((steps, lasso)) => Verdict::Livelock { steps, lasso },
                    None => Verdict::Unverified(
                        "a fair cycle keeps a liveness monitor hot, but the search did not record how to reach it"
                            .into(),
                    ),
                };
            }
        }
        Verdict::Verified
    }

    fn label(&self, e: &Edge) -> &Label {
        &self.labels.items[e.label as usize]
    }
}

#[derive(Debug, Clone, Copy)]
struct Edge {
    from: u32,
    to: u32,
    label: u32,
}

/// The record with dense node numbers, for the analysis.
struct Graph<'a> {
    record: &'a LiveGraph,
    hashes: &'a [u64],
    nodes: &'a [Node],
    edges: &'a [Edge],
}

impl Graph<'_> {
    fn label(&self, e: usize) -> &Label {
        self.record.label(&self.edges[e])
    }

    fn enabled(&self, node: u32) -> &[u64] {
        &self.record.enabled.items[self.nodes[node as usize].enabled as usize]
    }

    /// The transitions inside each strongly connected component of the graph `edges`
    /// form, for components with at least one (Tarjan's algorithm, iteratively).
    fn components(&self, edges: &[usize]) -> Vec<Vec<usize>> {
        const NONE: u32 = u32::MAX;
        let mut dense = vec![NONE; self.nodes.len()];
        let mut n = 0usize;
        for &e in edges {
            for v in [self.edges[e].from, self.edges[e].to] {
                if dense[v as usize] == NONE {
                    dense[v as usize] = n as u32;
                    n += 1;
                }
            }
        }
        let local = |v: u32| dense[v as usize];
        // successors in CSR form
        let mut start = vec![0usize; n + 1];
        for &e in edges {
            start[local(self.edges[e].from) as usize + 1] += 1;
        }
        for i in 0..n {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut succ = vec![0u32; edges.len()];
        for &e in edges {
            let f = local(self.edges[e].from) as usize;
            succ[fill[f]] = local(self.edges[e].to);
            fill[f] += 1;
        }
        const UNSEEN: u32 = u32::MAX;
        let mut order = vec![UNSEEN; n];
        let mut low = vec![0u32; n];
        let mut on_stack = vec![false; n];
        let mut component = vec![UNSEEN; n];
        let mut stack: Vec<u32> = Vec::new();
        let mut calls: Vec<(u32, usize)> = Vec::new();
        let mut next = 0u32;
        let mut components = 0u32;
        for root in 0..n as u32 {
            if order[root as usize] != UNSEEN {
                continue;
            }
            calls.push((root, start[root as usize]));
            order[root as usize] = next;
            low[root as usize] = next;
            next += 1;
            stack.push(root);
            on_stack[root as usize] = true;
            while let Some(&mut (v, ref mut i)) = calls.last_mut() {
                let v_ = v as usize;
                if *i < start[v_ + 1] {
                    let w = succ[*i];
                    *i += 1;
                    let w_ = w as usize;
                    if order[w_] == UNSEEN {
                        order[w_] = next;
                        low[w_] = next;
                        next += 1;
                        stack.push(w);
                        on_stack[w_] = true;
                        calls.push((w, start[w_]));
                    } else if on_stack[w_] {
                        low[v_] = low[v_].min(order[w_]);
                    }
                    continue;
                }
                calls.pop();
                if let Some(&(parent, _)) = calls.last() {
                    low[parent as usize] = low[parent as usize].min(low[v_]);
                }
                if low[v_] == order[v_] {
                    while let Some(w) = stack.pop() {
                        on_stack[w as usize] = false;
                        component[w as usize] = components;
                        if w == v {
                            break;
                        }
                    }
                    components += 1;
                }
            }
        }
        let mut inside: Vec<Vec<usize>> = vec![Vec::new(); components as usize];
        for &e in edges {
            let c = component[local(self.edges[e].from) as usize];
            if c == component[local(self.edges[e].to) as usize] {
                inside[c as usize].push(e);
            }
        }
        inside.retain(|c| !c.is_empty());
        inside
    }

    /// A component of `edges` that is fair to choices and to the scheduler and in which
    /// the monitor `bit` is hot somewhere: its transitions (Emerson-Lei refinement).
    fn fair_component(&self, edges: Vec<usize>, bit: u64) -> Option<Vec<usize>> {
        let mut work = vec![edges];
        while let Some(set) = work.pop() {
            for component in self.components(&set) {
                // a site some value of which never occurs here cannot recur forever
                let mut values: FxHashMap<(u64, u32), FxHashSet<u32>> = FxHashMap::default();
                for &e in &component {
                    for &(site, count, value) in self.label(e).choices.iter() {
                        values.entry((site, count)).or_default().insert(value);
                    }
                }
                let partial: FxHashSet<(u64, u32)> =
                    values.into_iter().filter(|((_, count), v)| v.len() < *count as usize).map(|(k, _)| k).collect();
                if !partial.is_empty() {
                    let rest: Vec<usize> = component
                        .into_iter()
                        .filter(|&e| !self.label(e).choices.iter().any(|&(s, c, _)| partial.contains(&(s, c))))
                        .collect();
                    if !rest.is_empty() {
                        work.push(rest);
                    }
                    continue;
                }
                let states = self.states(&component);
                if !self.unstarved(&component, &states).is_empty() {
                    continue;
                }
                if states.iter().any(|&n| self.nodes[n as usize].hot & bit != 0) {
                    return Some(component);
                }
            }
        }
        None
    }

    /// The states of a component, in order.
    fn states(&self, component: &[usize]) -> Vec<u32> {
        let mut states: Vec<u32> = component.iter().map(|&e| self.edges[e].from).collect();
        states.sort_unstable();
        states.dedup();
        states
    }

    /// Operations enabled at every state of the component that run on none of its
    /// transitions: a cycle through all of it starves them.
    fn unstarved(&self, component: &[usize], states: &[u32]) -> Vec<u64> {
        let mut always: Vec<u64> = self.enabled(states[0]).to_vec();
        for &n in &states[1..] {
            let here = self.enabled(n);
            always.retain(|o| here.contains(o));
        }
        always.retain(|o| !component.iter().any(|&e| self.label(e).operation() == Some(*o)));
        always
    }

    /// The witness of a fair component: a schedule from the start of the test to a state
    /// where the monitor `bit` is hot, then once around a fair cycle through it; and the
    /// length of the path before the cycle.
    fn witness(&self, component: &[usize], bit: u64) -> Option<(Vec<ScheduleStep>, usize)> {
        let states = self.states(component);
        let depth = |n: u32| self.record.tree.get(&self.hashes[n as usize]).map_or(u32::MAX, |p| p.depth);
        let start =
            states.iter().copied().filter(|&n| self.nodes[n as usize].hot & bit != 0).min_by_key(|&n| (depth(n), n))?;
        let mut out: FxHashMap<u32, Vec<usize>> = FxHashMap::default();
        for &e in component {
            out.entry(self.edges[e].from).or_default().push(e);
        }
        // what the cycle must contain: every value of every choice site, and for every
        // operation enabled in the component, a transition that runs it or a state
        // where it is not enabled
        enum Need {
            Edge(usize),
            State(u32),
        }
        let mut needs: Vec<Need> = Vec::new();
        let mut covered: FxHashSet<(u64, u32, u32)> = FxHashSet::default();
        for &e in component {
            for &c in self.label(e).choices.iter() {
                if covered.insert(c) {
                    needs.push(Need::Edge(e));
                }
            }
        }
        let mut operations: Vec<u64> = states.iter().flat_map(|&n| self.enabled(n).iter().copied()).collect();
        operations.sort_unstable();
        operations.dedup();
        for o in operations {
            match component.iter().find(|&&e| self.label(e).operation() == Some(o)) {
                Some(&e) => needs.push(Need::Edge(e)),
                None => needs.push(Need::State(*states.iter().find(|&&n| !self.enabled(n).contains(&o))?)),
            }
        }
        let mut cycle: Vec<usize> = Vec::new();
        let mut at = start;
        let mut taken: FxHashSet<usize> = FxHashSet::default();
        let mut visited: FxHashSet<u32> = FxHashSet::from_iter([start]);
        for need in needs {
            let (target, edge) = match need {
                Need::Edge(e) if taken.contains(&e) => continue,
                Need::State(n) if visited.contains(&n) => continue,
                Need::Edge(e) => (self.edges[e].from, Some(e)),
                Need::State(n) => (n, None),
            };
            let path = self.path(&out, at, target)?;
            for e in path.into_iter().chain(edge) {
                taken.insert(e);
                visited.insert(self.edges[e].to);
                cycle.push(e);
                at = self.edges[e].to;
            }
        }
        if cycle.is_empty() {
            let e = *out.get(&start)?.first()?;
            cycle.push(e);
            at = self.edges[e].to;
        }
        cycle.extend(self.path(&out, at, start)?);
        // the path from the start of the test, through the states' parents
        let mut parents: Vec<u32> = Vec::new();
        let mut hash = self.hashes[start as usize];
        loop {
            let p = self.record.tree.get(&hash)?;
            parents.push(p.label);
            if p.parent == START {
                break;
            }
            hash = p.parent;
            // depths fall along the parents, so this only guards against hash collisions
            if parents.len() > self.record.tree.len() {
                return None;
            }
        }
        let labels = &self.record.labels.items;
        let mut steps: Vec<ScheduleStep> =
            parents.iter().rev().flat_map(|&l| labels[l as usize].steps.iter().cloned()).collect();
        let lasso = steps.len();
        steps.extend(cycle.iter().flat_map(|&e| self.label(e).steps.iter().cloned()));
        Some((steps, lasso))
    }

    /// The shortest path of transitions in `out` from `from` to `to` (empty when they are
    /// the same state).
    fn path(&self, out: &FxHashMap<u32, Vec<usize>>, from: u32, to: u32) -> Option<Vec<usize>> {
        if from == to {
            return Some(Vec::new());
        }
        let mut via: FxHashMap<u32, usize> = FxHashMap::default();
        let mut queue = VecDeque::from([from]);
        while let Some(n) = queue.pop_front() {
            for &e in out.get(&n).map(Vec::as_slice).unwrap_or_default() {
                let next = self.edges[e].to;
                if next == from || via.contains_key(&next) {
                    continue;
                }
                via.insert(next, e);
                if next == to {
                    let mut path = vec![e];
                    let mut at = self.edges[e].from;
                    while at != from {
                        let e = via[&at];
                        path.push(e);
                        at = self.edges[e].from;
                    }
                    path.reverse();
                    return Some(path);
                }
                queue.push_back(next);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(op: u64, choice: Option<(u64, u32, u32)>, hot: bool, enabled: &[u64]) -> LiveFacts {
        let mut steps = vec![ScheduleStep::Operation(op)];
        if let Some((_, _, v)) = choice {
            steps.push(ScheduleStep::Bool(v == 1));
        }
        LiveFacts {
            steps,
            choices: choice.into_iter().collect(),
            not_cold: 1,
            hot: u64::from(hot),
            enabled: enabled.to_vec(),
            ..Default::default()
        }
    }

    /// Records the transitions `(from, to, facts)` of a graph whose start state is 1, with
    /// operations `enabled` there.
    fn graph(enabled: &[u64], edges: &[(u64, u64, LiveFacts)]) -> LiveGraph {
        let mut g = LiveGraph::new(1000);
        g.record(None, 1, 0, true, facts(0, None, true, enabled));
        for (depth, (from, to, f)) in edges.iter().enumerate() {
            g.record(Some(*from), *to, depth + 1, true, f.clone());
        }
        g
    }

    #[test]
    fn a_hot_cycle_is_a_livelock_with_a_lasso_witness() {
        let g = graph(&[0], &[(1, 2, facts(0, None, true, &[0])), (2, 1, facts(0, None, true, &[0]))]);
        let Verdict::Livelock { steps, lasso } = g.analyze() else { panic!("{:?}", g.analyze()) };
        assert_eq!(lasso, 1);
        assert_eq!(steps.len(), 3);
    }

    #[test]
    fn a_cycle_that_always_declines_a_choice_is_not_fair() {
        // 1 -$=false-> 1, 1 -$=true-> 2 (cold): the only hot cycle never takes `true`
        let site = (7, 2, 0);
        let mut g = graph(&[0], &[(1, 1, facts(0, Some(site), true, &[0]))]);
        g.record(Some(1), 3, 1, true, LiveFacts { not_cold: 0, ..facts(0, Some((7, 2, 1)), false, &[0]) });
        assert_eq!(g.analyze(), Verdict::Verified);
    }

    #[test]
    fn two_loops_that_take_both_values_are_a_fair_livelock() {
        let g = graph(
            &[0],
            &[
                (1, 2, facts(0, Some((7, 2, 0)), true, &[0])),
                (1, 3, facts(0, Some((7, 2, 1)), true, &[0])),
                (2, 1, facts(0, None, true, &[0])),
                (3, 1, facts(0, None, true, &[0])),
            ],
        );
        let Verdict::Livelock { steps, lasso } = g.analyze() else { panic!("{:?}", g.analyze()) };
        let cycle = &steps[lasso..];
        assert!(cycle.contains(&ScheduleStep::Bool(true)) && cycle.contains(&ScheduleStep::Bool(false)), "{cycle:?}");
    }

    #[test]
    fn a_cycle_that_starves_an_enabled_operation_is_not_fair() {
        let g = graph(&[0, 5], &[(1, 2, facts(0, None, true, &[0, 5])), (2, 1, facts(0, None, true, &[0, 5]))]);
        assert_eq!(g.analyze(), Verdict::Verified);
        // the operation is disabled somewhere on the cycle: fair
        let g = graph(&[0], &[(1, 2, facts(0, None, true, &[0])), (2, 1, facts(0, None, true, &[0, 5]))]);
        assert!(matches!(g.analyze(), Verdict::Livelock { .. }));
    }

    #[test]
    fn a_record_past_its_budget_cannot_decide() {
        let mut g = LiveGraph::new(3);
        for i in 1..10 {
            g.record(Some(i), i + 1, i as usize, true, facts(0, None, true, &[0]));
        }
        assert!(matches!(g.analyze(), Verdict::Unverified(_)));
    }
}
