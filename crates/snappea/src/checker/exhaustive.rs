//! Exhaustive search (PEx mode): explores every behaviour of a test instead of sampling.
//!
//! The search is a depth-first search over the choice points the runtime already
//! controls: which operation runs at a scheduling point, and every `$`/`choose` value.
//! It is stateless in the sense that each schedule re-runs the test from the start,
//! replaying the choices of the path to the branch being explored, then takes the next
//! untried alternative there (so no runtime state has to be cloned).
//!
//! Visited-state caching cuts the search down: at every scheduling point the program
//! state is hashed ([`crate::checker::statehash`]) and a schedule stops when it reaches a state
//! that was already reached at the same or a smaller depth, since everything after it is
//! (or is being) explored from the earlier visit. Remembering the smallest depth keeps the
//! search complete under the depth bound.
//!
//! When the search runs out of alternatives with no bound hit, every behaviour of the
//! test within the step bound was explored: "no bug found" then means there is none (up
//! to 64-bit hash collisions). Liveness is checked for schedules that end (a hot monitor
//! at the end of a terminating schedule) and on cycles: a schedule that returns to a
//! state on its own path closes a lasso, and the runtime reports it as a liveness bug when
//! a hot monitor stays out of cold states through the cycle and the cycle is fair
//! ([`ExhaustiveStrategy::closed_cycle`]). Other cycles are pruned. A search that pruned a
//! cycle while a liveness monitor was not cold has not verified liveness: the cycle it
//! checked may differ from a fair one through the same state.
//!
//! Several workers split one search ([`Split`]). Every worker walks the top of the tree
//! the same way, down to the first choice where the path has at least
//! [`Split::TARGET_PER_WORKER`] alternatives per worker. Each such prefix belongs to one
//! worker, round-robin in the order the walk meets them; a worker cuts the prefixes it
//! does not own at their first scheduling point. Below a prefix nothing is filtered, so
//! the workers share one explored-state cache there ([`Shared`]): a state any worker
//! reached at the same or a smaller depth is explored (or being explored) by that worker,
//! and the split search is complete only when every worker's part is. The shared walk
//! prunes only cycles on the current path: the repeated state's subtree is the
//! ancestor's, which the workers explore between them.

use rustc_hash::FxHashMap;

use crate::checker::strategy::{OpInfo, SharedRandom, Strategy};

/// A permanent handoff from exhaustive search to sampling in auto mode.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AutoHandoff {
    /// One-based worker number; counts below belong to this worker.
    pub worker: usize,
    /// Completed schedules, including interleaved samples, at the handoff.
    pub after_schedules: usize,
    /// Exhaustive choice points examined at the handoff.
    pub choices: u64,
    /// Why auto abandoned exhaustive search.
    pub reason: String,
}

/// Statistics of an exhaustive search.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchStats {
    /// Permanent auto handoffs, one per worker that abandoned exhaustive search.
    pub auto_handoffs: Vec<AutoHandoff>,
    /// Distinct program states reached at scheduling points.
    pub distinct_states: usize,
    /// Schedules cut short at an already explored state.
    pub pruned_schedules: usize,
    /// Prunes that return to a state on the current execution path, rather than
    /// joining another explored path. Zero after completion establishes acyclicity.
    pub cycle_prunes: usize,
    /// Cycle prunes at a state where a liveness monitor (one with hot states) was not in
    /// a cold state, and no liveness bug was reported. Zero after completion establishes
    /// liveness for a search on one worker.
    pub live_cycle_prunes: usize,
    /// The search was split across workers, whose shared state cache can hide a cycle
    /// from the worker that would close it: liveness then needs no cycle prunes at all.
    pub split: bool,
    /// Schedules stopped by the step bound (the search is not exhaustive then); counted
    /// by the engine.
    pub depth_bound_hits: usize,
    /// Most choices made in one schedule.
    pub max_depth: usize,
    /// Every alternative was explored.
    pub finished: bool,
    /// The search was still exploring when it was stopped (by a time limit, say) rather
    /// than handed off to sampling: more time could have finished it.
    pub running: bool,
    /// Untried alternatives along the path the search was exploring when it stopped.
    pub frontier: usize,
    /// The test has hot monitor states (see [`SearchStats::liveness_unverified`]).
    pub hot_monitors: bool,
    /// Why finishing proves nothing about the test, if it does not: the program did not
    /// repeat its behaviour given the same choices, or its foreign functions keep state
    /// or values the search cannot hash.
    pub unverifiable: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Operation,
    Bool,
    Int,
}

/// A choice point of the current path and the alternatives left there.
struct Choice {
    kind: Kind,
    options: Vec<u64>,
    taken: usize,
    /// Product of the alternative counts up to and including this choice (saturating).
    product: u64,
}

/// A schedule that returned to a state on its own path: the lasso's cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cycle {
    /// The number of choices made before the cycle starts: the schedule's state after
    /// them is the state it returned to.
    pub start: usize,
    /// Every operation enabled at every scheduling point of the cycle ran in it, so the
    /// cycle can repeat forever under a fair scheduler.
    pub fair: bool,
}

/// What the workers of a split search share: the explored-state cache (the smallest
/// depth each state was reached at) and work budget a finished worker did not use.
#[derive(Debug)]
pub struct Shared {
    shards: [std::sync::Mutex<FxHashMap<u64, usize>>; 64],
    spare_work: std::sync::atomic::AtomicU64,
}

impl Default for Shared {
    fn default() -> Self {
        Shared { shards: std::array::from_fn(|_| Default::default()), spare_work: Default::default() }
    }
}

impl Shared {
    /// Leaves `work` (choices) for workers that run out of their own.
    pub(crate) fn give_spare_work(&self, work: u64) {
        self.spare_work.fetch_add(work, std::sync::atomic::Ordering::AcqRel);
    }

    /// Takes up to `max` of the work left by finished workers.
    pub(crate) fn take_spare_work(&self, max: u64) -> u64 {
        use std::sync::atomic::Ordering;
        let mut spare = self.spare_work.load(Ordering::Acquire);
        loop {
            let take = spare.min(max);
            match self.spare_work.compare_exchange_weak(spare, spare - take, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return take,
                Err(now) => spare = now,
            }
        }
    }

    /// Records that `hash` was reached at `depth`: `Some(new)` to explore it (`new` when no
    /// worker reached it before), `None` when it was reached at the same or a smaller depth.
    fn visit(&self, hash: u64, depth: usize) -> Option<bool> {
        let mut shard = self.shards[(hash >> 58) as usize].lock().unwrap_or_else(|e| e.into_inner());
        match shard.get_mut(&hash) {
            Some(d) if *d <= depth => None,
            Some(d) => {
                *d = depth;
                Some(false)
            }
            None => {
                shard.insert(hash, depth);
                Some(true)
            }
        }
    }
}

/// This worker's part of a search split across `workers` workers (see the module docs).
#[derive(Debug, Clone)]
pub struct Split {
    workers: u64,
    index: u64,
    /// Orders the alternatives of shared choices: the same for every worker.
    rng: SharedRandom,
    /// Position of the choice that ends the current path's prefix, once it is known.
    prefix_end: Option<usize>,
    /// The current prefix belongs to this worker.
    owned: bool,
    /// Prefixes met so far.
    prefixes: u64,
    shared: std::sync::Arc<Shared>,
}

impl Split {
    /// Prefixes per worker: enough for a balanced split, few enough that each worker
    /// walking them all costs little.
    pub const TARGET_PER_WORKER: u64 = 8;

    /// Worker `index` of `workers`; `seed` and `shared` must be the same for every worker.
    pub fn new(workers: usize, index: usize, seed: u32, shared: std::sync::Arc<Shared>) -> Split {
        Split {
            workers: workers as u64,
            index: index as u64,
            rng: std::rc::Rc::new(std::cell::RefCell::new(crate::checker::random::RandomValueGenerator::new(seed))),
            prefix_end: None,
            owned: true,
            prefixes: 0,
            shared,
        }
    }

    /// What this worker shares with the others.
    pub(crate) fn shared_state(&self) -> std::sync::Arc<Shared> {
        self.shared.clone()
    }

    fn target(&self) -> u64 {
        self.workers.saturating_mul(Self::TARGET_PER_WORKER)
    }

    /// The current path just entered a new prefix.
    fn enter_prefix(&mut self) {
        self.owned = self.prefixes % self.workers == self.index;
        self.prefixes += 1;
    }

    /// The path is still in the part every worker walks.
    fn shared(&self, pos: usize) -> bool {
        self.prefix_end.is_none_or(|end| pos <= end)
    }
}

/// Depth-first search with visited-state caching (`--sch-pex dfs`); with a random
/// generator the alternatives at each new choice point are tried in random order
/// (`--sch-pex random`, the default).
pub struct ExhaustiveStrategy {
    max_steps: usize,
    rng: Option<SharedRandom>,
    split: Option<Split>,
    path: Vec<Choice>,
    /// Alternatives allocated along `path`.
    retained: usize,
    /// Position in `path` of the current schedule.
    pos: usize,
    /// Smallest depth each state was reached at.
    visited: FxHashMap<u64, usize>,
    state_path: Vec<(usize, u64)>,
    path_states: rustc_hash::FxHashSet<u64>,
    /// The state the last [`Strategy::observe_state`] pruned as a cycle on the path.
    cycle: Option<u64>,
    stats: SearchStats,
    error: Option<String>,
}

impl ExhaustiveStrategy {
    pub fn new(max_steps: usize, rng: Option<SharedRandom>) -> Self {
        ExhaustiveStrategy {
            max_steps,
            rng,
            split: None,
            path: Vec::new(),
            retained: 0,
            pos: 0,
            visited: FxHashMap::default(),
            state_path: Vec::new(),
            path_states: rustc_hash::FxHashSet::default(),
            cycle: None,
            stats: SearchStats::default(),
            error: None,
        }
    }

    /// Explores only this worker's part of a split search.
    pub fn with_split(mut self, split: Split) -> Self {
        self.split = Some(split);
        self.stats.split = true;
        self
    }

    /// The cycle the last pruned schedule closed, if it returned to a state on its own
    /// path. Called when a liveness monitor was not cold there; counts the cycle as one
    /// liveness was not verified on (the runtime reports a bug instead when it can).
    ///
    /// Fairness is weak fairness over operations: an operation enabled at every
    /// scheduling point of the cycle must have run in it. Nondeterministic choices (`$`,
    /// `choose`) are the environment's and need not vary; `$$` choices are checked by the
    /// runtime.
    pub(crate) fn closed_cycle(&mut self) -> Option<Cycle> {
        let hash = self.cycle.take()?;
        let start = self.state_path.iter().rev().find(|(_, h)| *h == hash)?.0;
        self.stats.live_cycle_prunes += 1;
        let end = self.pos.min(self.path.len());
        let mut always: Option<Vec<u64>> = None;
        let mut ran: Vec<u64> = Vec::new();
        for c in self.path.get(start..end).unwrap_or_default() {
            if c.kind != Kind::Operation {
                continue;
            }
            ran.push(c.options[c.taken]);
            match &mut always {
                None => always = Some(c.options.clone()),
                Some(a) => a.retain(|o| c.options.contains(o)),
            }
        }
        let fair = always.is_some_and(|a| a.iter().all(|o| ran.contains(o)));
        Some(Cycle { start, fair })
    }

    /// Allocated choice alternatives retained along the current DFS path.
    pub(crate) fn retained_alternatives(&self) -> usize {
        self.retained
    }

    /// Untried alternatives remaining along the current DFS frontier.
    pub(crate) fn pending_alternatives(&self) -> usize {
        self.path.iter().map(|c| c.options.len() - c.taken - 1).sum()
    }

    /// The next decision of the current schedule: the path's choice while replaying it
    /// (checked with `valid`, without building the alternatives), else a new choice point
    /// with the alternatives `options` builds (never empty).
    fn decide(&mut self, kind: Kind, valid: impl Fn(u64) -> bool, options: impl FnOnce() -> Vec<u64>) -> u64 {
        if self.pos < self.path.len() {
            let c = &self.path[self.pos];
            let v = c.options[c.taken];
            if c.kind != kind || !valid(v) {
                // only possible if the program is not deterministic given its choices
                self.error.get_or_insert_with(|| {
                    "Exhaustive search: the program did not repeat a schedule given the same choices.".into()
                });
            }
            self.pos += 1;
            return v;
        }
        let mut options = options();
        if let Some(split) = &self.split {
            if !split.owned && !split.shared(self.pos) {
                // another worker's prefix, cut at the next scheduling point: no choice
                // point, so there is nothing to backtrack into
                self.pos += 1;
                return options[0];
            }
        }
        let shared = self.split.as_ref().is_some_and(|s| s.shared(self.pos));
        let rng = if shared { self.split.as_ref().map(|s| &s.rng) } else { self.rng.as_ref() };
        if let Some(rng) = rng {
            let mut i = options.len();
            while i > 1 {
                let j = rng.borrow_mut().next_below(i as i32) as usize;
                i -= 1;
                options.swap(i, j);
            }
        }
        let v = options[0];
        let product = self.path.last().map_or(1, |c| c.product).saturating_mul(options.len() as u64);
        self.retained += options.len();
        self.path.push(Choice { kind, options, taken: 0, product });
        if let Some(split) = self.split.as_mut().filter(|s| s.prefix_end.is_none()) {
            if product >= split.target() {
                split.prefix_end = Some(self.pos);
                split.enter_prefix();
            }
        }
        self.pos += 1;
        v
    }
}

impl Strategy for ExhaustiveStrategy {
    fn next_operation(&mut self, _current: u64, ops: &[OpInfo]) -> Option<u64> {
        if !ops.iter().any(|o| o.enabled) {
            return None;
        }
        Some(self.decide(
            Kind::Operation,
            |v| ops.iter().any(|o| o.enabled && o.id == v),
            || ops.iter().filter(|o| o.enabled).map(|o| o.id).collect(),
        ))
    }

    fn next_bool(&mut self, _max: i32) -> bool {
        self.decide(Kind::Bool, |v| v < 2, || vec![0, 1]) == 1
    }

    fn next_int(&mut self, max: i32) -> i32 {
        let n = max.max(1) as u64;
        self.decide(Kind::Int, |v| v < n, || (0..n).collect()) as i32
    }

    fn prepare_next_iteration(&mut self) -> bool {
        self.stats.max_depth = self.stats.max_depth.max(self.pos);
        self.pos = 0;
        self.cycle = None;
        // backtrack to the deepest choice point with an untried alternative
        while let Some(c) = self.path.last_mut() {
            if c.taken + 1 < c.options.len() {
                c.taken += 1;
                let changed = self.path.len() - 1;
                if let Some(split) = &mut self.split {
                    match split.prefix_end {
                        Some(end) if changed == end => split.enter_prefix(),
                        // the prefix ends deeper again, where the walk meets it
                        Some(end) if changed < end => split.prefix_end = None,
                        _ => {}
                    }
                }
                // Everything before the changed choice replays unchanged. Retain
                // those ancestor hashes without hashing the replayed prefix again.
                let cutoff = self.path.len();
                while self.state_path.last().is_some_and(|(depth, _)| *depth >= cutoff) {
                    let (_, hash) = self.state_path.pop().unwrap();
                    self.path_states.remove(&hash);
                }
                return true;
            }
            let c = self.path.pop().unwrap();
            self.retained -= c.options.len();
        }
        self.stats.finished = true;
        false
    }

    fn scheduled_steps(&self) -> usize {
        self.pos
    }

    fn has_reached_max_steps(&self) -> bool {
        self.max_steps != 0 && self.pos >= self.max_steps
    }

    fn is_fair(&self) -> bool {
        false
    }

    fn description(&self) -> String {
        if let Some(s) = &self.split {
            return format!("pex[split {}/{}]", s.index + 1, s.workers);
        }
        match &self.rng {
            Some(r) => format!("pex[random, seed '{}']", r.borrow().seed),
            None => "pex[dfs]".into(),
        }
    }

    fn replay_error(&self) -> Option<String> {
        self.error.clone()
    }

    fn wants_state_hash(&self) -> bool {
        // states on the path to the branch being explored are already recorded
        self.pos >= self.path.len()
    }

    fn observe_state(&mut self, hash: u64) -> bool {
        let cycle = self.path_states.contains(&hash);
        self.cycle = None;
        if cycle {
            self.stats.cycle_prunes += 1;
        }
        if let Some(split) = &self.split {
            if !split.shared(self.pos) && !split.owned {
                return false;
            }
            if cycle {
                self.cycle = Some(hash);
            }
            if split.shared(self.pos) {
                // the shared walk: only path-local (cycle) pruning, the same in every worker
                if cycle {
                    self.stats.pruned_schedules += 1;
                    return false;
                }
                self.state_path.push((self.pos, hash));
                self.path_states.insert(hash);
                return true;
            }
            // back to an ancestor, possibly in the shared walk (not in `visited`)
            let explore = !cycle
                && split
                    .shared
                    .visit(hash, self.pos)
                    .inspect(|&new| {
                        self.stats.distinct_states += usize::from(new);
                    })
                    .is_some();
            if !explore {
                self.stats.pruned_schedules += 1;
                return false;
            }
            self.state_path.push((self.pos, hash));
            self.path_states.insert(hash);
            return true;
        }
        if cycle {
            self.cycle = Some(hash);
        }
        let explore = match self.visited.get_mut(&hash) {
            Some(d) if *d <= self.pos => {
                self.stats.pruned_schedules += 1;
                false
            }
            Some(d) => {
                *d = self.pos;
                true
            }
            None => {
                self.visited.insert(hash, self.pos);
                self.stats.distinct_states += 1;
                true
            }
        };
        if explore {
            self.state_path.push((self.pos, hash));
            self.path_states.insert(hash);
        }
        explore
    }

    fn closed_cycle(&mut self) -> Option<Cycle> {
        ExhaustiveStrategy::closed_cycle(self)
    }

    fn exhaustive_stats(&self) -> Option<SearchStats> {
        // the last schedule counts even when no next iteration was prepared (a bug, the
        // schedule limit or a timeout ended the search)
        Some(SearchStats { max_depth: self.stats.max_depth.max(self.pos), ..self.stats.clone() })
    }
}

impl SearchStats {
    /// The search explored every behaviour of the test: it finished, no schedule hit the
    /// step bound, liveness was checkable, and the program was searchable.
    pub fn complete(&self) -> bool {
        self.finished && self.depth_bound_hits == 0 && !self.liveness_unverified() && self.unverifiable.is_none()
    }

    /// The search pruned a cycle that may hide a liveness bug: one at a state where a
    /// liveness monitor was not cold (with workers, any cycle). A fair cycle that keeps a
    /// hot monitor out of cold states is reported as a bug instead; when no such cycle
    /// was pruned, every cycle through such a state was checked (see the module docs).
    pub fn liveness_unverified(&self) -> bool {
        self.hot_monitors && if self.split { self.cycle_prunes > 0 } else { self.live_cycle_prunes > 0 }
    }

    /// Adds another worker's part of a split search: complete only when every part is.
    pub fn combine(&mut self, other: &SearchStats) {
        self.auto_handoffs.extend(other.auto_handoffs.iter().cloned());
        self.auto_handoffs.sort_by_key(|h| h.worker);
        self.distinct_states += other.distinct_states;
        self.pruned_schedules += other.pruned_schedules;
        self.cycle_prunes += other.cycle_prunes;
        self.live_cycle_prunes += other.live_cycle_prunes;
        self.split = true;
        self.depth_bound_hits += other.depth_bound_hits;
        self.max_depth = self.max_depth.max(other.max_depth);
        self.finished &= other.finished;
        self.running |= other.running;
        self.frontier += other.frontier;
        self.hot_monitors |= other.hot_monitors;
        if self.unverifiable.is_none() {
            self.unverifiable.clone_from(&other.unverifiable);
        }
    }

    /// The report lines of the search.
    pub fn text(&self, schedules: usize, max_steps: usize, prefix: &str) -> String {
        let mut out = format!(
            "{} Exhaustive search: explored {} distinct state{} in {} schedule{} ({} cut at an explored state), max depth {}.",
            prefix,
            self.distinct_states,
            if self.distinct_states == 1 { "" } else { "s" },
            schedules,
            if schedules == 1 { "" } else { "s" },
            self.pruned_schedules,
            self.max_depth
        );
        if let Some(why) = self.unverifiable.as_ref().filter(|_| self.finished) {
            out.push_str(&format!("\n{} Search finished, but it does not prove the absence of bugs: {}", prefix, why));
        } else if self.finished && self.depth_bound_hits == 0 && self.liveness_unverified() {
            let n = if self.split { self.cycle_prunes } else { self.live_cycle_prunes };
            out.push_str(&format!(
                "\n{} Search finished, but liveness was not verified: {} schedule{} returned to an earlier state while a liveness monitor was not cold, through a cycle that is unfair or leaves the hot state, and another cycle through that state could stay hot. `snappea check` continues such tests with fair feedback search.",
                prefix,
                n,
                if n == 1 { "" } else { "s" }
            ));
        } else if self.finished && self.depth_bound_hits == 0 {
            out.push_str(&format!("\n{} Search finished: every behaviour of the test was explored.", prefix));
        } else if self.finished {
            out.push_str(&format!(
                "\n{} Search finished, but {} schedule{} hit the max-steps bound of '{}': behaviours beyond it were not explored.",
                prefix,
                self.depth_bound_hits,
                if self.depth_bound_hits == 1 { "" } else { "s" },
                max_steps
            ));
        } else {
            out.push_str(&format!(
                "\n{} Search incomplete: stopped before exploring every behaviour (a bug, the time or schedule limit, or a search that stopped converging).",
                prefix
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    use std::rc::Rc;
    use std::sync::Arc;

    /// Every leaf of a tree of `depth` binary choices that `strategy` explores itself (not
    /// cut as another worker's).
    fn leaves(mut strategy: ExhaustiveStrategy, depth: usize) -> (BTreeSet<Vec<bool>>, ExhaustiveStrategy) {
        let mut out = BTreeSet::new();
        loop {
            let leaf: Vec<bool> = (0..depth).map(|_| strategy.next_bool(2)).collect();
            if strategy.split.as_ref().is_none_or(|s| s.owned || s.prefix_end.is_none()) {
                assert!(out.insert(leaf), "a leaf explored twice");
            }
            if !strategy.prepare_next_iteration() {
                return (out, strategy);
            }
        }
    }

    #[test]
    fn split_workers_partition_the_tree() {
        let rng = |seed| Some(Rc::new(RefCell::new(crate::checker::random::RandomValueGenerator::new(seed))));
        let (all, _) = leaves(ExhaustiveStrategy::new(0, rng(1)), 9);
        assert_eq!(all.len(), 512);
        for workers in [2, 3, 4, 7] {
            let shared = Arc::new(Shared::default());
            let mut union = BTreeSet::new();
            let mut sizes = Vec::new();
            for index in 0..workers {
                // each worker orders its own part differently; the shared walk is the same
                let split = Split::new(workers, index, 99, shared.clone());
                let (part, s) = leaves(ExhaustiveStrategy::new(0, rng(index as u32)).with_split(split), 9);
                assert!(s.stats.finished);
                sizes.push(part.len());
                assert!(part.is_disjoint(&union), "{workers} workers overlap");
                union.extend(part);
            }
            assert_eq!(union, all, "{workers} workers miss leaves");
            // round-robin over at least 8 prefixes per worker: no part is more than twice
            // another
            assert!(sizes.iter().max().unwrap() <= &(2 * sizes.iter().min().unwrap()), "{sizes:?}");
        }
    }

    #[test]
    fn replayed_choices_are_checked_without_building_the_alternatives() {
        let mut s = ExhaustiveStrategy::new(0, None);
        s.next_int(1_000_000);
        assert_eq!(s.retained_alternatives(), 1_000_000);
        assert!(s.prepare_next_iteration());
        assert_eq!(s.next_int(1_000_000), 1);
        assert!(s.replay_error().is_none());
        assert!(s.prepare_next_iteration());
        // a program that asks for a different choice than it did before
        s.next_bool(2);
        assert!(s.replay_error().is_some());
        while s.prepare_next_iteration() {}
        assert_eq!(s.retained_alternatives(), 0);
    }

    #[test]
    fn completion_needs_no_bound_hits_checkable_liveness_and_a_searchable_program() {
        let done = SearchStats { finished: true, ..Default::default() };
        assert!(done.complete());
        assert!(done.text(1, 10, "..").contains("every behaviour"));
        let bounded = SearchStats { depth_bound_hits: 1, ..done.clone() };
        assert!(!bounded.complete());
        let cyclic = SearchStats { cycle_prunes: 2, ..done.clone() };
        assert!(cyclic.complete(), "cycles are fine without hot monitors");
        let cold = SearchStats { hot_monitors: true, ..cyclic.clone() };
        assert!(cold.complete(), "every cycle was through states where liveness monitors were cold");
        let split = SearchStats { split: true, ..cold.clone() };
        assert!(!split.complete(), "workers may hide a cycle from each other");
        let live = SearchStats { live_cycle_prunes: 1, ..cold };
        assert!(!live.complete());
        assert!(live.text(1, 10, "..").contains("liveness was not verified"), "{}", live.text(1, 10, ".."));
        let foreign = SearchStats { unverifiable: Some("why".into()), ..done.clone() };
        assert!(!foreign.complete());
        assert!(foreign.text(1, 10, "..").contains("does not prove the absence of bugs: why"));
        let mut parts = done.clone();
        parts.combine(&SearchStats { finished: false, ..done });
        assert!(!parts.complete());
    }
}
