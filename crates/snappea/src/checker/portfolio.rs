//! Automatic scheduling: retain a guided corpus per scheduler and
//! spend bounded work on every arm, even when rarity gives no useful signal.
//!
//! An arm earns more schedules when its schedules reach what the run rarely reaches: a
//! machine handling an event in a state, a given number of times ([`Portfolio::rarity`]).
//! Code coverage stops growing early in a large model, so it cannot tell the arms apart;
//! rare points keep telling them apart for the whole run.
use super::{
    feedback::{ControlledRandom, DelayScheduler, FeedbackGuidedStrategy, RandomScheduler, Source},
    random::RandomValueGenerator,
    strategy::{OpInfo, PctStrategy, SchedulerStats, Strategy},
};
use rustc_hash::FxHashSet;
use std::{cell::RefCell, rc::Rc};

struct Arm {
    strategy: Box<dyn Strategy>,
    work: u64,
    reward: f64,
    runs: usize,
}
const INITIAL_OBSERVATIONS: usize = 32;
/// How one schedule resolves the model's `$` and `choose(n)`. A model injects a fault
/// with such a choice (`choose(100) < 5`, `if ($)`), at one rate for every schedule; a
/// bug that needs several faults close together, or a long stretch without any, then
/// needs many schedules. Each schedule makes either the low or the high outcomes of
/// every such choice likely, so some schedules are full of faults and others free of
/// them. All values stay possible in both.
///
/// Drawing the skew per code site instead, or leaving some schedules even, found the
/// Raft bug that needs a quiet stretch (`raft/commits-earlier-term`) less often: in 1 to
/// 4 of 10 seeds within 60 s, against 7 of 10 this way and 1 of 10 with no skew.
#[derive(Clone, Copy, PartialEq)]
enum Skew {
    Low,
    High,
}
/// Change points of the PCT arms. A bug that needs k forced handovers needs about k
/// change points: with 2 instead of 20, the upstream Paxos and Raft tutorials' bugs took
/// about four times as long at the median, and PCT with 2 alone did not find them.
const PCT_DEPTHS: [usize; 3] = [0, 10, 20];
const NAMES: [&str; 5] = ["feedback", "delay", "pct-0", "pct-10", "pct-20"];
/// The arm a run without contention uses throughout.
const FEEDBACK: usize = 0;
/// The arm a probe of a receiver with several senders starts with.
const PCT_0: usize = 2;
const _: () = assert!(matches!(NAMES[FEEDBACK].as_bytes(), b"feedback") && matches!(NAMES[PCT_0].as_bytes(), b"pct-0"));
/// A point is rare while fewer than one schedule in this many reached it.
const RARE: u64 = 100;
/// Rare points of one schedule that count: a long loop cannot buy an unlimited allocation.
const MAX_SCORE: usize = 8;
/// Distinct points counted; a model with more keeps the counts of the first ones.
const MAX_POINTS: usize = 1 << 16;

pub(crate) struct Portfolio {
    arms: Vec<Arm>,
    active: usize,
    iterations: usize,
    contention: bool,
    fan_in: usize,
    observations: usize,
    /// Times each (machine, state, event) was handled in the current schedule.
    counts: rustc_hash::FxHashMap<(u32, u32, u32), usize>,
    /// Schedules that reached each point: a (machine, state, event) and how often.
    reached: rustc_hash::FxHashMap<(u32, u32, u32, usize), u32>,
    new_failure: bool,
    failures: FxHashSet<String>,
    /// The choice about to be made is a `$` or a `choose(n)` of the model.
    model_choice: bool,
    /// The skew of the current schedule, drawn at its first such choice.
    skew: Option<Skew>,
    skew_rng: RandomValueGenerator,
    rng: RandomValueGenerator,
}
impl Portfolio {
    /// `frontier` turns on frontier guidance in the feedback-guided arms.
    pub(crate) fn new(seed: u32, unfair: usize, fair: usize, frontier: bool) -> Self {
        let guided = |bound, scheduler| {
            let s = FeedbackGuidedStrategy::new(bound, seed, scheduler)
                .with_code_guidance(true)
                .with_fresh_exploration(25)
                .with_condition_prefixes();
            Box::new(if frontier { s.with_frontier_guidance() } else { s }) as Box<dyn Strategy>
        };
        let mut strategies = vec![
            guided(fair, Box::new(RandomScheduler::new(Source::Controlled(ControlledRandom::new(seed))))),
            guided(unfair, Box::new(DelayScheduler::new(Source::Controlled(ControlledRandom::new(seed))))),
        ];
        for depth in PCT_DEPTHS {
            strategies.push(Box::new(PctStrategy::new(
                unfair,
                depth,
                Rc::new(RefCell::new(RandomValueGenerator::new(seed))),
            )));
        }
        Self {
            arms: strategies.into_iter().map(|strategy| Arm { strategy, work: 0, reward: 0.0, runs: 0 }).collect(),
            active: FEEDBACK,
            iterations: 0,
            contention: false,
            fan_in: 0,
            observations: 0,
            counts: Default::default(),
            reached: Default::default(),
            new_failure: false,
            model_choice: false,
            skew: None,
            skew_rng: RandomValueGenerator::new(seed.wrapping_add(0x51ed270b)),
            failures: Default::default(),
            rng: RandomValueGenerator::new(seed.wrapping_add(0x9e3779b9)),
        }
    }
    /// A stable dependency signal: several senders can race at one receiver.
    /// Inspect metadata already provided to the scheduler; no program-state hashes.
    pub(crate) fn observe_operations(&mut self, ops: &[OpInfo]) {
        self.observe_prefix_operation(ops, false);
    }

    /// Observes a scheduling point of the exhaustive search. Returns its fan-in, which is
    /// only computed when sampled or when `record`: a schedule resumed after this point
    /// repeats the observation with it ([`Portfolio::repeat_initial_observations`]).
    pub(crate) fn observe_prefix_operation(&mut self, ops: &[OpInfo], record: bool) -> usize {
        self.contention |= ops.iter().filter(|o| o.enabled).count() > 1;
        self.observations += 1;
        let sample = self.sample_fan_in();
        if !sample && !record {
            return 0;
        }
        let mut fan_in = 0;
        if self.fan_in < 8 {
            let mut receivers = [(0u64, 0usize); 64];
            let mut count = 0;
            for receiver in ops.iter().take(256).filter_map(|o| o.receiver) {
                if let Some((_, n)) = receivers[..count].iter_mut().find(|(id, _)| *id == receiver) {
                    *n += 1;
                    fan_in = fan_in.max(*n);
                } else if count < receivers.len() {
                    receivers[count] = (receiver, 1);
                    count += 1;
                }
            }
        }
        if sample {
            self.fan_in = self.fan_in.max(fan_in);
        }
        fan_in
    }

    fn sample_fan_in(&self) -> bool {
        self.fan_in < 8 && (self.observations <= INITIAL_OBSERVATIONS || self.observations.is_multiple_of(16))
    }

    pub(crate) fn repeat_initial_observations(&mut self, summary: &[usize]) {
        for &fan_in in summary {
            self.observations += 1;
            if self.sample_fan_in() {
                self.fan_in = self.fan_in.max(fan_in);
            }
        }
    }

    /// Whether some scheduling point so far offered more than one operation.
    pub(crate) fn contention(&self) -> bool {
        self.contention
    }
    pub(crate) fn probe_budget(&self) -> usize {
        // A receiver with n senders can see n! orders. Bound the pilot allocation
        // rather than assuming that a slowly closing DFS frontier is cheap.
        (1..=self.fan_in.min(6)).product::<usize>().clamp(32, 512)
    }
    pub(crate) fn begin_probe(&mut self) {
        if self.fan_in >= 2 && self.iterations == 0 {
            self.active = PCT_0;
        }
    }
    /// Points of the schedule that just ran which fewer than one schedule in [`RARE`]
    /// reached so far, itself included; a failure of a new kind counts as all of them.
    fn rarity(&mut self) -> usize {
        let total = self.iterations as u64 + 1;
        let mut rare = 0;
        for (&(m, s, e), &count) in &self.counts {
            let bucket = if count <= 8 { count } else { 8 + count.ilog2() as usize };
            if self.reached.len() >= MAX_POINTS && !self.reached.contains_key(&(m, s, e, bucket)) {
                continue;
            }
            let n = self.reached.entry((m, s, e, bucket)).or_default();
            *n = n.saturating_add(1);
            rare += usize::from(*n == 1 || u64::from(*n) * RARE <= total);
        }
        if std::mem::take(&mut self.new_failure) {
            rare = MAX_SCORE;
        }
        rare.min(MAX_SCORE)
    }
    /// The skew of the choice about to be made, if it is one of the model's.
    fn skew(&mut self) -> Option<Skew> {
        if !std::mem::take(&mut self.model_choice) {
            return None;
        }
        let rng = &mut self.skew_rng;
        Some(*self.skew.get_or_insert_with(|| if rng.next_below(2) == 0 { Skew::Low } else { Skew::High }))
    }
    fn current(&self) -> &dyn Strategy {
        &*self.arms[self.active].strategy
    }
    fn current_mut(&mut self) -> &mut dyn Strategy {
        &mut *self.arms[self.active].strategy
    }
}
impl Strategy for Portfolio {
    fn scheduler_stats(&self) -> Vec<SchedulerStats> {
        self.arms
            .iter()
            .enumerate()
            .map(|(i, a)| SchedulerStats {
                name: NAMES[i].into(),
                schedules: a.runs + usize::from(i == self.active && a.strategy.scheduled_steps() > 0),
                work: a.work + if i == self.active { a.strategy.scheduled_steps() as u64 } else { 0 },
                reward: a.reward,
            })
            .collect()
    }
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        self.observe_operations(ops);
        self.current_mut().next_operation(current, ops)
    }
    fn note_model_choice(&mut self) {
        self.model_choice = true;
    }
    fn next_bool(&mut self, max: i32) -> bool {
        let skew = self.skew();
        let fair = self.current_mut().next_bool(max);
        // the arm still draws, so its own choices stay in step
        match skew {
            None => fair,
            Some(Skew::Low) => self.skew_rng.next_below(8) != 0,
            Some(Skew::High) => self.skew_rng.next_below(8) == 0,
        }
    }
    fn next_int(&mut self, max: i32) -> i32 {
        let skew = self.skew();
        let even = self.current_mut().next_int(max);
        let Some(skew) = skew.filter(|_| max >= 2) else { return even };
        // the fourth power of a uniform draw: half of the draws land in the lowest 6%
        let u = (f64::from(even) + 0.5) / f64::from(max);
        let v = if skew == Skew::Low { u.powi(4) } else { 1.0 - (1.0 - u).powi(4) };
        ((v * f64::from(max)) as i32).clamp(0, max - 1)
    }
    fn prepare_next_iteration(&mut self) -> bool {
        let work = self.current().scheduled_steps().max(1) as u64;
        let score = self.rarity();
        let arm = &mut self.arms[self.active];
        arm.work = arm.work.saturating_add(work);
        arm.runs += 1;
        // Work, not just schedules, is charged to each arm. The reward is an average
        // over about the arm's last 50 schedules: rare points are rare per schedule.
        arm.reward = arm.reward * 0.98 + score as f64;
        self.current_mut().prepare_next_iteration();
        self.iterations += 1;
        self.counts.clear();
        self.skew = None;
        self.model_choice = false;
        self.active = if !self.contention {
            FEEDBACK
        } else if let Some(i) = self.arms.iter().position(|a| a.runs == 0) {
            i
        } else if self.iterations.is_multiple_of(4) {
            // An exploration floor independent of reward: coverage can be blind.
            self.arms.iter().enumerate().min_by_key(|(_, a)| a.work).unwrap().0
        } else {
            // Sample by bounded utility rather than permanently giving every
            // exploitation slot to one slightly higher score. Complementary
            // schedulers matter when the objective is all distinct failures.
            let weights: Vec<_> = self
                .arms
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    let cost = a.work as f64 / a.runs.max(1) as f64;
                    let hint =
                        if self.fan_in >= 2 && i == PCT_0 { 1.0 + 3.0 / (a.runs.max(1) as f64).sqrt() } else { 1.0 };
                    ((0.05 + a.reward * a.reward) * hint / cost.sqrt()).max(0.001)
                })
                .collect();
            let sum: f64 = weights.iter().sum();
            let mut ticket = self.rng.next_below(1_000_000) as f64 / 1_000_000.0 * sum;
            let mut chosen = weights.len() - 1;
            for (i, weight) in weights.iter().enumerate() {
                ticket -= weight;
                if ticket <= 0.0 {
                    chosen = i;
                    break;
                }
            }
            chosen
        };
        true
    }
    fn scheduled_steps(&self) -> usize {
        self.current().scheduled_steps()
    }
    fn has_reached_max_steps(&self) -> bool {
        self.current().has_reached_max_steps()
    }
    fn is_fair(&self) -> bool {
        self.current().is_fair()
    }
    fn description(&self) -> String {
        format!("auto/{}", self.current().description())
    }
    fn wants_feedback(&self) -> bool {
        true
    }
    fn wants_minhash(&self) -> bool {
        false
    }
    fn wants_frontiers(&self) -> bool {
        true
    }
    fn observe_condition(&mut self, fun: u32, pc: usize, outcome: bool) {
        self.current_mut().observe_condition(fun, pc, outcome);
    }
    fn observe_frontier(&mut self, m: u32, s: u32, e: u32) {
        *self.counts.entry((m, s, e)).or_default() += 1;
        self.arms[self.active].strategy.observe_frontier(m, s, e);
    }
    fn observe_progress(&mut self, coverage: usize, bug: Option<&str>) {
        if let Some(bug) = bug {
            self.new_failure |= self.failures.len() < 64 && self.failures.insert(super::engine::failure_site(bug));
        }
        self.current_mut().observe_progress(coverage, bug);
    }
    fn observe_timeline(&mut self, t: &str, h: &[i32], r: &[(String, usize)], s: &[String]) {
        self.current_mut().observe_timeline(t, h, r, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::runtime::SchedKind;
    #[test]
    fn blind_rewards_still_explore_every_scheduler_with_bounded_work() {
        let mut p = Portfolio::new(7, 20, 20, false);
        let ops = [
            OpInfo { id: 1, enabled: true, kind: SchedKind::Default, receiver: None },
            OpInfo { id: 2, enabled: true, kind: SchedKind::Default, receiver: None },
        ];
        for _ in 0..200 {
            for _ in 0..10 {
                p.next_operation(1, &ops);
            }
            p.observe_progress(0, None);
            p.prepare_next_iteration();
        }
        assert!(p.arms.iter().all(|a| a.runs > 0));
        assert!(p.arms.iter().all(|a| a.work > 0));
    }
}
