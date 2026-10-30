//! Exhaust converging small models and interleave guided scheduling interventions.
//!
//! Only completed unbounded exhaustion can end checking early. Stalled or bounded
//! searches hand off to a feedback/delay/PCT portfolio. Independent scheduler
//! corpora and an exploration floor protect against misleading novelty signals.
use std::cell::RefCell;
use std::rc::Rc;

use super::exhaustive::{ExhaustiveStrategy, SearchStats, Split};
use super::portfolio::Portfolio;
use super::random::RandomValueGenerator;
use super::strategy::{OpInfo, SchedulerStats, Strategy};

const WINDOW: u64 = 1_024;
const MAX_WORK: u64 = 16_384;
const SMALL_FRONTIER: usize = 256;
const LOOKAHEAD_WORK: u64 = 4 * WINDOW;
const MAX_STATES: usize = 100_000;
const MAX_ALTERNATIVES: usize = 1_000_000;

pub(crate) struct AutoStrategy {
    dfs: Option<ExhaustiveStrategy>,
    portfolio: Portfolio,
    probing: bool,
    contention: bool,
    probe_remaining: usize,
    probed: bool,
    since_probe: usize,
    saw_bug: bool,

    parked: Option<SearchStats>,
    work: u64,
    work_deadline: u64,
    window_work: u64,
    window_states: usize,
    window_prunes: usize,
    window_frontier: Option<usize>,
    dfs_bounds: usize,
    has_hot: bool,
    interrupted: bool,
    complete: bool,
    /// This worker explores part of a split search: when its part is done, it keeps
    /// looking for bugs with feedback until the other parts are done too, and leaves
    /// its unused exhaustive work to workers whose parts are larger.
    split: Option<std::sync::Arc<super::exhaustive::Shared>>,
}
impl AutoStrategy {
    /// `exhaustive` starts with exhaustive search, of this worker's part of the tree
    /// when `split` is given.
    pub(crate) fn new(
        seed: u32,
        unfair_steps: usize,
        fair_steps: usize,
        has_hot: bool,
        exhaustive: bool,
        split: Option<Split>,
    ) -> Self {
        let shared = split.as_ref().map(|s| s.shared_state());
        Self {
            dfs: exhaustive.then(|| {
                let rng = Rc::new(RefCell::new(RandomValueGenerator::new(seed)));
                let dfs = ExhaustiveStrategy::new(unfair_steps, Some(rng));
                match split {
                    Some(split) => dfs.with_split(split),
                    None => dfs,
                }
            }),
            portfolio: Portfolio::new(seed, unfair_steps, fair_steps),
            probing: false,
            contention: false,
            probe_remaining: 0,
            probed: false,
            since_probe: 0,
            saw_bug: false,

            parked: None,
            work: 0,
            work_deadline: MAX_WORK,
            window_work: 0,
            window_states: 0,
            window_prunes: 0,
            window_frontier: None,
            dfs_bounds: 0,
            has_hot,
            interrupted: false,
            complete: false,
            split: shared,
        }
    }
    fn active(&self) -> &dyn Strategy {
        match self.dfs.as_ref().filter(|_| !self.probing) {
            Some(dfs) => dfs,
            None => &self.portfolio,
        }
    }
    pub(crate) fn with_frontier_guidance(mut self, enabled: bool) -> Self {
        if enabled {
            self.portfolio = self.portfolio.with_frontier_guidance(true);
        }
        self
    }
    fn active_mut(&mut self) -> &mut dyn Strategy {
        match self.dfs.as_mut().filter(|_| !self.probing) {
            Some(dfs) => dfs,
            None => &mut self.portfolio,
        }
    }
    fn choice(&mut self, alternatives: usize) -> bool {
        if let Some(dfs) = self.dfs.as_ref().filter(|_| !self.probing) {
            self.work += 1;
            self.window_work += 1;
            let retained = dfs.retained_alternatives();
            if self.work > self.work_deadline {
                // work other workers of a split search did not need
                let spare = self.split.as_ref().map_or(0, |s| s.take_spare_work(MAX_WORK));
                self.work_deadline = self.work_deadline.saturating_add(spare);
            }
            // Check before DFS materializes a potentially huge choice vector. Keep
            // DFS active until the schedule ends, so fairness and divergence describe
            // the schedule that actually ran.
            self.interrupted |=
                self.work > self.work_deadline || retained.saturating_add(alternatives) > MAX_ALTERNATIVES;
        }
        !self.interrupted
    }
    fn park(&mut self) {
        self.parked = self.dfs.take().and_then(|dfs| dfs.exhaustive_stats()).map(|stats| SearchStats {
            finished: false,
            depth_bound_hits: self.dfs_bounds,
            ..stats
        });
        self.interrupted = false;
        self.probing = false;
    }
}
impl Strategy for AutoStrategy {
    fn scheduler_stats(&self) -> Vec<SchedulerStats> {
        self.portfolio.scheduler_stats()
    }
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        self.contention |= ops.iter().filter(|o| o.enabled).count() > 1;
        if self.dfs.is_some() && !self.probing {
            self.portfolio.observe_operations(ops);
        }
        if !self.choice(ops.iter().filter(|o| o.enabled).count()) {
            return None;
        }
        self.active_mut().next_operation(current, ops)
    }
    fn next_bool(&mut self, max: i32) -> bool {
        self.choice(2) && self.active_mut().next_bool(max)
    }
    fn next_int(&mut self, max: i32) -> i32 {
        if !self.choice(max.max(1) as usize) {
            return 0;
        }
        self.active_mut().next_int(max)
    }
    fn prepare_next_iteration(&mut self) -> bool {
        if self.dfs.is_none() {
            return self.portfolio.prepare_next_iteration();
        }
        if self.probing {
            self.portfolio.prepare_next_iteration();
            self.probe_remaining = self.probe_remaining.saturating_sub(1);
            self.probing = self.probe_remaining > 0;
            return true;
        }
        self.dfs_bounds += usize::from(self.active().has_reached_max_steps());
        if self.interrupted {
            self.park();
            return true;
        }
        let schedule_steps = self.active().scheduled_steps();
        let dfs = self.dfs.as_mut().unwrap();
        let more = dfs.prepare_next_iteration();
        let stats = dfs.exhaustive_stats().unwrap();
        // a program that does not repeat itself given the same choices cannot be exhausted
        let repeatable = dfs.replay_error().is_none();
        if !more && stats.finished && self.dfs_bounds == 0 && (!self.has_hot || stats.cycle_prunes == 0) && repeatable {
            self.complete = true;
            let Some(shared) = &self.split else {
                return false;
            };
            shared.give_spare_work(self.work_deadline.saturating_sub(self.work));
            self.parked = Some(SearchStats { depth_bound_hits: 0, ..stats });
            self.dfs = None;
            return true;
        }
        if !more || self.dfs_bounds > 0 || !repeatable {
            self.park();
            return true;
        }
        let frontier = dfs.pending_alternatives();
        let initial_frontier = *self.window_frontier.get_or_insert(frontier);
        if self.window_work >= WINDOW {
            let states = stats.distinct_states.saturating_sub(self.window_states);
            let prunes = stats.pruned_schedules.saturating_sub(self.window_prunes);
            // A shrinking frontier or more graph closures than discoveries supports
            // continuing DFS. Novel states/coverage alone can grow forever.
            let closing = prunes > 0 && prunes >= states;
            if closing {
                // Cache closures show convergence even if a narrow DFS frontier
                // fluctuates. Renew the stall budget while that evidence continues;
                // absolute state and retained-choice bounds still cap memory.
                let renewed = self.work.saturating_add(MAX_WORK);
                // the workers of a split search share one state cache, so its closure
                // is evidence for all of them: a worker that has not seen any yet can
                // draw on it instead of giving up
                if let Some(shared) = &self.split {
                    shared.give_spare_work(renewed.saturating_sub(self.work_deadline));
                }
                self.work_deadline = renewed;
            }
            let shrinking = frontier < initial_frontier;
            let decrease = initial_frontier.saturating_sub(frontier) as u64;
            let remaining_work = self.work_deadline.saturating_sub(self.work).min(LOOKAHEAD_WORK);
            let affordable = decrease > 0
                && (frontier as u64).saturating_mul(self.window_work) <= decrease.saturating_mul(remaining_work);
            // A narrow frontier gets the bounded probe to reach deeper recovery
            // stages. A huge, slowly shrinking input range is still too expensive
            // when its observed closure rate cannot finish inside the work budget.
            if frontier > SMALL_FRONTIER && !closing && (!shrinking || !affordable) {
                self.park();
            } else {
                self.window_frontier = Some(frontier);
                self.window_states = stats.distinct_states;
                self.window_prunes = stats.pruned_schedules;
                self.window_work = 0;
            }
        }
        if self.dfs.is_some()
            && self.contention
            && !self.complete
            && !self.probed
            && (self.work >= WINDOW || schedule_steps >= 64 || self.saw_bug)
        {
            // A bounded early probe catches scheduling shapes that DFS might only
            // reach late, without doubling every converging finite search.
            self.portfolio.begin_probe();
            self.probing = true;
            self.probed = true;
            self.probe_remaining = self.portfolio.probe_budget();
        } else if self.dfs.is_some() && self.probed && !self.complete {
            // A closing state cache does not bound the remaining schedule tree.
            // Continue sampling alongside DFS so it cannot monopolize all-bug search.
            self.since_probe += 1;
            if self.since_probe >= 3 {
                self.since_probe = 0;
                self.probing = true;
                // Once a concurrent execution has failed, proving correctness is
                // no longer the task. Favor complementary samples while retaining
                // DFS as a floor for failures that need systematic enumeration.
                self.probe_remaining = if self.saw_bug { 3 } else { 1 };
            }
        }
        true
    }
    fn scheduled_steps(&self) -> usize {
        self.active().scheduled_steps()
    }
    fn has_reached_max_steps(&self) -> bool {
        self.active().has_reached_max_steps()
    }
    fn is_fair(&self) -> bool {
        self.active().is_fair()
    }
    fn description(&self) -> String {
        "auto".into()
    }
    fn diverged(&self) -> bool {
        self.interrupted || self.active().diverged()
    }
    fn replay_error(&self) -> Option<String> {
        self.active().replay_error()
    }
    fn wants_minhash(&self) -> bool {
        self.active().wants_minhash()
    }
    fn wants_feedback(&self) -> bool {
        self.active().wants_feedback()
    }
    fn observe_timeline(&mut self, timeline: &str, minhash: &[i32], reached: &[(String, usize)], satisfied: &[String]) {
        self.active_mut().observe_timeline(timeline, minhash, reached, satisfied);
    }
    fn observe_progress(&mut self, new_coverage: usize, bug: Option<&str>) {
        self.saw_bug |= bug.is_some();
        self.active_mut().observe_progress(new_coverage, bug);
    }
    fn wants_frontiers(&self) -> bool {
        self.active().wants_frontiers()
    }
    fn observe_condition(&mut self, fun: u32, pc: usize, outcome: bool) {
        self.active_mut().observe_condition(fun, pc, outcome);
    }
    fn observe_frontier(&mut self, machine: u32, state: u32, event: u32) {
        self.active_mut().observe_frontier(machine, state, event);
    }
    fn coverage_epoch(&self) -> u64 {
        u64::from(self.dfs.is_none() || self.probing)
    }
    fn wants_state_hash(&self) -> bool {
        !self.interrupted && self.active().wants_state_hash()
    }
    fn observe_state(&mut self, hash: u64) -> bool {
        if self.probing {
            return true;
        }
        let Some(dfs) = self.dfs.as_mut() else {
            return true;
        };
        if dfs.exhaustive_stats().is_some_and(|s| s.distinct_states >= MAX_STATES) {
            self.interrupted = true;
            return false;
        }
        dfs.observe_state(hash)
    }
    fn exhaustive_stats(&self) -> Option<SearchStats> {
        self.dfs
            .as_ref()
            .and_then(|dfs| dfs.exhaustive_stats())
            .map(|stats| SearchStats { finished: self.complete, depth_bound_hits: self.dfs_bounds, ..stats })
            .or_else(|| self.parked.clone())
    }
    fn search_complete(&self) -> bool {
        self.complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_search_starts_with_feedback_and_does_not_claim_exhaustion() {
        let auto = AutoStrategy::new(7, 100, 100, false, false, None);
        assert!(auto.is_fair());
        assert!(auto.wants_feedback());
        assert!(!auto.wants_minhash());
        assert!(auto.exhaustive_stats().is_none());
        assert!(!auto.search_complete());
    }

    #[test]
    fn shrinking_finite_frontier_keeps_exhausting_past_the_probe_window() {
        let mut auto = AutoStrategy::new(7, 100, 100, false, true, None);
        let mut schedules = 0;
        loop {
            for _ in 0..8 {
                auto.next_bool(2);
            }
            schedules += 1;
            if !auto.prepare_next_iteration() {
                break;
            }
            assert!(!auto.is_fair(), "a converging small tree should keep DFS");
            assert!(schedules <= 256);
        }
        assert_eq!(schedules, 256);
        assert!(auto.work > WINDOW);
        assert!(auto.search_complete());
        assert!(auto.exhaustive_stats().unwrap().finished);
    }

    #[test]
    fn slowly_shrinking_large_input_range_hands_off_instead_of_enumerating_thousands() {
        let mut auto = AutoStrategy::new(7, 0, 100, false, true, None);
        for _ in 0..WINDOW {
            auto.next_int(10_000);
            assert!(auto.prepare_next_iteration());
            if auto.is_fair() {
                break;
            }
        }
        assert!(auto.is_fair());
        assert!(auto.work < MAX_WORK);
        assert!(!auto.exhaustive_stats().unwrap().finished);
    }

    #[test]
    fn oversized_choice_hands_off_between_schedules_and_releases_dfs() {
        let mut auto = AutoStrategy::new(7, 0, 100, false, true, None);
        assert!(!auto.is_fair());
        auto.next_int(MAX_ALTERNATIVES as i32 + 1);
        assert!(auto.diverged());
        assert!(!auto.is_fair(), "the interrupted schedule still belongs to DFS");
        assert!(auto.prepare_next_iteration());
        assert!(auto.is_fair());
        assert!(!auto.diverged());
        assert_eq!(auto.coverage_epoch(), 1);
        assert!(!auto.exhaustive_stats().unwrap().finished);
        for _ in 0..10 {
            auto.next_int(10);
            assert!(auto.prepare_next_iteration());
            assert!(auto.is_fair(), "parked DFS must never compete with feedback");
        }
    }

    #[test]
    fn ever_growing_input_tree_stalls_even_without_state_hashes_or_depth_bounds() {
        let mut auto = AutoStrategy::new(7, 0, 100, false, true, None);
        for _ in 0..2 {
            for _ in 0..WINDOW {
                auto.next_bool(2);
            }
            assert!(auto.prepare_next_iteration());
            if auto.is_fair() {
                break;
            }
        }
        assert!(auto.is_fair());
        assert!(!auto.exhaustive_stats().unwrap().finished);
    }
}
