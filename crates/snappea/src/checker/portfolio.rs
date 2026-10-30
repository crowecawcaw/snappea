//! Automatic scheduling: retain a guided corpus per scheduler and
//! spend bounded work on every arm, even when novelty gives no useful signal.
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
    frontiers: FxHashSet<(u32, u32, u32, usize)>,
}
pub(crate) struct Portfolio {
    arms: Vec<Arm>,
    active: usize,
    iterations: usize,
    contention: bool,
    fan_in: usize,
    observations: usize,
    counts: rustc_hash::FxHashMap<(u32, u32, u32), usize>,
    novelty: usize,
    failures: FxHashSet<String>,
    rng: RandomValueGenerator,
}
impl Portfolio {
    pub(crate) fn new(seed: u32, unfair: usize, fair: usize) -> Self {
        let guided = |bound, scheduler| {
            Box::new(
                FeedbackGuidedStrategy::new(bound, seed, scheduler)
                    .with_code_guidance(true)
                    .with_fresh_exploration(25)
                    .with_condition_prefixes(),
            ) as Box<dyn Strategy>
        };
        let mut strategies = vec![
            guided(fair, Box::new(RandomScheduler::new(Source::Controlled(ControlledRandom::new(seed))))),
            guided(unfair, Box::new(DelayScheduler::new(Source::Controlled(ControlledRandom::new(seed))))),
        ];
        for depth in [0, 2, 10] {
            strategies.push(Box::new(PctStrategy::new(
                unfair,
                depth,
                Rc::new(RefCell::new(RandomValueGenerator::new(seed))),
            )));
        }
        Self {
            arms: strategies
                .into_iter()
                .map(|strategy| Arm { strategy, work: 0, reward: 0.0, runs: 0, frontiers: Default::default() })
                .collect(),
            active: 0,
            iterations: 0,
            contention: false,
            fan_in: 0,
            observations: 0,
            counts: Default::default(),
            novelty: 0,
            failures: Default::default(),
            rng: RandomValueGenerator::new(seed.wrapping_add(0x9e3779b9)),
        }
    }
    pub(crate) fn with_frontier_guidance(mut self, enabled: bool) -> Self {
        if enabled {
            for arm in self.arms.iter_mut().take(2) {
                arm.strategy.enable_frontier_guidance();
            }
        }
        self
    }
    /// A stable dependency signal: several senders can race at one receiver.
    /// Inspect metadata already provided to the scheduler; no program-state hashes.
    pub(crate) fn observe_operations(&mut self, ops: &[OpInfo]) {
        self.contention |= ops.iter().filter(|o| o.enabled).count() > 1;
        self.observations += 1;
        if self.fan_in >= 8 || (self.observations > 32 && !self.observations.is_multiple_of(16)) {
            return;
        }
        let mut receivers = [(0u64, 0usize); 64];
        let mut count = 0;
        for receiver in ops.iter().take(256).filter_map(|o| o.receiver) {
            if let Some((_, n)) = receivers[..count].iter_mut().find(|(id, _)| *id == receiver) {
                *n += 1;
                self.fan_in = self.fan_in.max(*n);
            } else if count < receivers.len() {
                receivers[count] = (receiver, 1);
                count += 1;
            }
        }
    }
    pub(crate) fn probe_budget(&self) -> usize {
        // A receiver with n senders can see n! orders. Bound the pilot allocation
        // rather than assuming that a slowly closing DFS frontier is cheap.
        (1..=self.fan_in.min(6)).product::<usize>().clamp(32, 512)
    }
    pub(crate) fn begin_probe(&mut self) {
        if self.fan_in >= 2 && self.iterations == 0 {
            self.active = 2;
        }
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
                name: ["feedback", "delay", "pct-0", "pct-2", "pct-10"][i].into(),
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
    fn next_bool(&mut self, max: i32) -> bool {
        self.current_mut().next_bool(max)
    }
    fn next_int(&mut self, max: i32) -> i32 {
        self.current_mut().next_int(max)
    }
    fn prepare_next_iteration(&mut self) -> bool {
        let work = self.current().scheduled_steps().max(1) as u64;
        let arm = &mut self.arms[self.active];
        arm.work = arm.work.saturating_add(work);
        arm.runs += 1;
        // Novelty is bounded per schedule; a counter or long loop cannot buy an
        // unlimited allocation. Work, not just schedules, is charged to each arm.
        arm.reward = arm.reward * 0.9 + (self.novelty.min(8) as f64);
        self.current_mut().prepare_next_iteration();
        self.iterations += 1;
        self.counts.clear();
        self.novelty = 0;
        self.active = if !self.contention {
            0
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
                    let hint = if self.fan_in >= 2 && i == 2 { 1.0 + 3.0 / (a.runs.max(1) as f64).sqrt() } else { 1.0 };
                    ((1.0 + a.reward.sqrt()) * hint / cost.sqrt()).max(0.001)
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
        let count = self.counts.entry((m, s, e)).or_default();
        *count += 1;
        let bucket = if *count <= 8 { *count } else { 8 + count.ilog2() as usize };
        let arm = &mut self.arms[self.active];
        if arm.frontiers.len() < 4096 && arm.frontiers.insert((m, s, e, bucket)) {
            self.novelty += 1;
        }
        arm.strategy.observe_frontier(m, s, e);
    }
    fn observe_progress(&mut self, coverage: usize, bug: Option<&str>) {
        self.novelty += coverage.min(8);
        if let Some(bug) = bug {
            if self.failures.len() < 64 && self.failures.insert(super::feedback::failure_class(bug)) {
                self.novelty += 8;
            }
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
        let mut p = Portfolio::new(7, 20, 20);
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
