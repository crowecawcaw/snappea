//! Scheduling strategies (`PChecker.SystematicTesting.Strategies`).
//!
//! The compatibility CLI preserves upstream's random streams, including its feedback
//! mutation quirks. Native feedback corrects mutation sizes and uses the fair step bound;
//! recorded schedules remain replayable regardless of which policy found them.

use std::cell::RefCell;
use std::rc::Rc;

use crate::checker::random::RandomValueGenerator;

/// A scheduling strategy, named as upstream names it (`CheckerConfiguration.SchedulingStrategy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategyKind {
    #[default]
    Random,
    /// Native exhaustive search with an adaptive scheduling portfolio; upstream defaults remain random.
    Auto,
    Pct,
    FairPct,
    Probabilistic,
    Pos,
    Feedback,
    FeedbackPct,
    FeedbackPos,
    /// Q-learning (`rl`).
    Rl,
    /// Replay of a recorded schedule.
    Replay,
    /// Exhaustive search (PEx mode), trying the alternatives in order.
    Dfs,
    /// Exhaustive search (PEx mode), trying the alternatives in a random order.
    RandomDfs,
}

impl StrategyKind {
    const ALL: [StrategyKind; 13] = [
        StrategyKind::Auto,
        StrategyKind::Random,
        StrategyKind::Pct,
        StrategyKind::FairPct,
        StrategyKind::Probabilistic,
        StrategyKind::Pos,
        StrategyKind::Feedback,
        StrategyKind::FeedbackPct,
        StrategyKind::FeedbackPos,
        StrategyKind::Rl,
        StrategyKind::Replay,
        StrategyKind::Dfs,
        StrategyKind::RandomDfs,
    ];

    pub fn name(self) -> &'static str {
        match self {
            StrategyKind::Auto => "auto",
            StrategyKind::Random => "random",
            StrategyKind::Pct => "pct",
            StrategyKind::FairPct => "fairpct",
            StrategyKind::Probabilistic => "probabilistic",
            StrategyKind::Pos => "pos",
            StrategyKind::Feedback => "feedback",
            StrategyKind::FeedbackPct => "feedbackpct",
            StrategyKind::FeedbackPos => "feedbackpos",
            StrategyKind::Rl => "rl",
            StrategyKind::Replay => "replay",
            StrategyKind::Dfs => "dfs",
            StrategyKind::RandomDfs => "random-dfs",
        }
    }

    /// Whether the checker reports the seed with the strategy (`... Checker is using
    /// 'pct' strategy (seed:7).`).
    pub fn reports_seed(self) -> bool {
        // `feedback` draws from the seed too but has always been reported without it,
        // unlike `feedbackpct` and `feedbackpos`; nothing vendored from upstream shows
        // what upstream prints for it, so the output is kept as it is.
        !matches!(self, StrategyKind::Feedback | StrategyKind::Replay | StrategyKind::Dfs)
    }

    /// Whether this is an exhaustive search (PEx mode).
    pub fn is_exhaustive(self) -> bool {
        matches!(self, StrategyKind::Dfs | StrategyKind::RandomDfs)
    }

    /// Whether workers with their own seeds can explore schedules with this strategy
    /// side by side: it learns nothing from other schedules that would make one worker's
    /// schedules worth less than a single checker's. The feedback strategies and `rl` learn
    /// from earlier schedules, `dfs`/`random-dfs` keep one search on one worker and a
    /// replay follows one schedule. `auto` qualifies: its workers split the exhaustive
    /// phase between them (see [`crate::checker::exhaustive::Split`]).
    pub fn explores_independently(self) -> bool {
        matches!(
            self,
            StrategyKind::Auto
                | StrategyKind::Random
                | StrategyKind::Pct
                | StrategyKind::FairPct
                | StrategyKind::Probabilistic
                | StrategyKind::Pos
        )
    }
}

impl std::fmt::Display for StrategyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for StrategyKind {
    type Err = String;

    fn from_str(s: &str) -> Result<StrategyKind, String> {
        StrategyKind::ALL
            .into_iter()
            .find(|k| k.name() == s)
            .ok_or_else(|| format!("unknown scheduling strategy '{}'", s))
    }
}

/// Identity and status of a schedulable operation as seen by a strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpInfo {
    pub id: u64,
    pub enabled: bool,
    /// Kind of the scheduling point the operation last stopped at (`AsyncOperation.Type`).
    pub kind: crate::checker::runtime::SchedKind,
    /// Target of the operation's most recent send (`AsyncOperation.MessageReceiver`).
    pub receiver: Option<u64>,
}

pub type SharedRandom = Rc<RefCell<RandomValueGenerator>>;

/// Scheduler allocation diagnostics. Work counts checker choices, not wall time.
#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    pub name: String,
    pub schedules: usize,
    pub work: u64,
    /// Decayed, capped novelty score at the end of the run (summed across workers).
    pub reward: f64,
}

pub trait Strategy {
    fn scheduler_stats(&self) -> Vec<SchedulerStats> {
        Vec::new()
    }
    /// Picks the next operation to schedule among `ops` (sorted by id). `current` is the
    /// id of the operation that reached the scheduling point.
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64>;
    fn next_bool(&mut self, max: i32) -> bool;
    fn next_int(&mut self, max: i32) -> i32;
    /// Prepares the next iteration; `false` means exploration is exhausted.
    fn prepare_next_iteration(&mut self) -> bool;
    fn scheduled_steps(&self) -> usize;
    fn has_reached_max_steps(&self) -> bool;
    fn is_fair(&self) -> bool;
    fn description(&self) -> String;
    /// Error of a replay strategy whose trace could not be followed.
    fn replay_error(&self) -> Option<String> {
        None
    }
    /// Whether the last decision could not be made (a replay that diverged from its trace,
    /// see [`Strategy::replay_error`]); the schedule then ends there.
    fn diverged(&self) -> bool {
        false
    }
    /// Feedback from a finished schedule (`IFeedbackGuidedStrategy.ObserveRunningResults`):
    /// its abstract timeline and timeline MinHash, the states each scenario reached and the
    /// scenarios it satisfied.
    fn observe_timeline(
        &mut self,
        _timeline: &str,
        _minhash: &[i32],
        _reached: &[(String, usize)],
        _satisfied: &[String],
    ) {
    }
    /// Whether [`Strategy::observe_timeline`] is used (avoids computing MinHashes otherwise).
    fn wants_feedback(&self) -> bool {
        false
    }
    /// Whether timeline MinHashes are needed (novelty-only observers skip their cost).
    fn wants_minhash(&self) -> bool {
        self.wants_feedback()
    }
    /// Whether [`Strategy::observe_state`] is used (avoids hashing the state otherwise).
    fn wants_state_hash(&self) -> bool {
        false
    }
    /// The program-state hash (see [`crate::checker::statehash`]) at the scheduling point about to
    /// be decided by [`Strategy::next_operation`]. Returning `false` ends the schedule
    /// there, without a deadlock or liveness check (the state was explored before).
    fn observe_state(&mut self, _hash: u64) -> bool {
        true
    }
    /// Independent coverage novelty history of the current scheduler phase. Coverage
    /// reports still merge every phase; revisiting an epoch restores its history.
    fn coverage_epoch(&self) -> u64 {
        0
    }
    /// Coverage delta and failure observed during the current execution.
    fn observe_progress(&mut self, _new_coverage: usize, _bug: Option<&str>) {}
    /// Enable native prefix guidance for guided scheduler arms.
    fn enable_frontier_guidance(&mut self) {}
    /// Whether event-count frontier checkpoints are used during execution.
    fn wants_frontiers(&self) -> bool {
        false
    }
    /// A short-circuit edge reached during execution, before suffix choices.
    fn observe_condition(&mut self, _fun: u32, _pc: usize, _outcome: bool) {}
    /// A delivery about to be handled, grouped by declaration rather than instance.
    fn observe_frontier(&mut self, _machine: u32, _state: u32, _event: u32) {}
    /// Validated exhaustive completion, including liveness requirements.
    fn search_complete(&self) -> bool {
        false
    }
    /// Statistics of an exhaustive search.
    fn exhaustive_stats(&self) -> Option<crate::checker::exhaustive::SearchStats> {
        None
    }
}

fn enabled_ids(ops: &[OpInfo]) -> Vec<u64> {
    ops.iter().filter(|o| o.enabled).map(|o| o.id).collect()
}

// ---------------------------------------------------------------------------

pub struct RandomStrategy {
    max_steps: usize,
    steps: usize,
    rng: SharedRandom,
}

impl RandomStrategy {
    pub fn new(max_steps: usize, rng: SharedRandom) -> Self {
        RandomStrategy { max_steps, steps: 0, rng }
    }
}

impl Strategy for RandomStrategy {
    fn next_operation(&mut self, _current: u64, ops: &[OpInfo]) -> Option<u64> {
        let enabled = enabled_ids(ops);
        if enabled.is_empty() {
            return None;
        }
        let idx = self.rng.borrow_mut().next_below(enabled.len() as i32) as usize;
        self.steps += 1;
        Some(enabled[idx])
    }
    fn next_bool(&mut self, max: i32) -> bool {
        let r = self.rng.borrow_mut().next_below(max) == 0;
        self.steps += 1;
        r
    }
    fn next_int(&mut self, max: i32) -> i32 {
        let r = self.rng.borrow_mut().next_below(max);
        self.steps += 1;
        r
    }
    fn prepare_next_iteration(&mut self) -> bool {
        self.steps = 0;
        true
    }
    fn scheduled_steps(&self) -> usize {
        self.steps
    }
    fn has_reached_max_steps(&self) -> bool {
        self.max_steps != 0 && self.steps >= self.max_steps
    }
    fn is_fair(&self) -> bool {
        true
    }
    fn description(&self) -> String {
        format!("random[seed '{}']", self.rng.borrow().seed)
    }
}

// ---------------------------------------------------------------------------

pub struct ProbabilisticRandomStrategy {
    inner: RandomStrategy,
    coin_flips: usize,
}

impl ProbabilisticRandomStrategy {
    pub fn new(max_steps: usize, coin_flips: usize, rng: SharedRandom) -> Self {
        ProbabilisticRandomStrategy { inner: RandomStrategy::new(max_steps, rng), coin_flips }
    }

    fn should_current_change(&mut self) -> bool {
        for _ in 0..self.coin_flips {
            if self.inner.rng.borrow_mut().next_below(2) == 1 {
                return false;
            }
        }
        true
    }
}

impl Strategy for ProbabilisticRandomStrategy {
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        let enabled = enabled_ids(ops);
        if enabled.is_empty() {
            return None;
        }
        self.inner.steps += 1;
        if enabled.len() > 1 {
            let current_enabled = ops.iter().any(|o| o.id == current && o.enabled);
            if !self.should_current_change() && current_enabled {
                return Some(current);
            }
        }
        let idx = self.inner.rng.borrow_mut().next_below(enabled.len() as i32) as usize;
        Some(enabled[idx])
    }
    fn next_bool(&mut self, max: i32) -> bool {
        self.inner.next_bool(max)
    }
    fn next_int(&mut self, max: i32) -> i32 {
        self.inner.next_int(max)
    }
    fn prepare_next_iteration(&mut self) -> bool {
        self.inner.prepare_next_iteration()
    }
    fn scheduled_steps(&self) -> usize {
        self.inner.steps
    }
    fn has_reached_max_steps(&self) -> bool {
        self.inner.has_reached_max_steps()
    }
    fn is_fair(&self) -> bool {
        true
    }
    fn description(&self) -> String {
        format!("probabilistic[seed '{}', coin flips '{}']", self.inner.rng.borrow().seed, self.coin_flips)
    }
}

// ---------------------------------------------------------------------------

pub struct PctStrategy {
    max_steps: usize,
    steps: usize,
    max_switch_points: usize,
    schedule_length: usize,
    prioritized: Vec<u64>,
    /// Kept sorted (upstream uses a `SortedSet<int>`).
    change_points: Vec<usize>,
    rng: SharedRandom,
}

impl PctStrategy {
    pub fn new(max_steps: usize, max_switch_points: usize, rng: SharedRandom) -> Self {
        PctStrategy {
            max_steps,
            steps: 0,
            max_switch_points,
            schedule_length: 0,
            prioritized: Vec::new(),
            change_points: Vec::new(),
            rng,
        }
    }

    fn highest_priority_enabled(&self, enabled: &[u64]) -> Option<u64> {
        self.prioritized.iter().copied().find(|p| enabled.contains(p))
    }

    fn add_point(&mut self, p: usize) {
        if let Err(i) = self.change_points.binary_search(&p) {
            self.change_points.insert(i, p);
        }
    }

    fn remove_point(&mut self, p: usize) {
        if let Ok(i) = self.change_points.binary_search(&p) {
            self.change_points.remove(i);
        }
    }

    fn move_change_point_forward(&mut self) {
        self.remove_point(self.steps);
        let mut p = self.steps + 1;
        while self.change_points.binary_search(&p).is_ok() {
            p += 1;
        }
        self.add_point(p);
    }
}

impl Strategy for PctStrategy {
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        let enabled = enabled_ids(ops);
        if enabled.is_empty() {
            return None;
        }
        if self.prioritized.is_empty() {
            self.prioritized.push(current);
        }
        for op in &enabled {
            if !self.prioritized.contains(op) {
                let idx = self.rng.borrow_mut().next_below(self.prioritized.len() as i32) as usize + 1;
                self.prioritized.insert(idx, *op);
            }
        }
        if self.change_points.binary_search(&self.steps).is_ok() {
            if enabled.len() == 1 {
                self.move_change_point_forward();
            } else if let Some(p) = self.highest_priority_enabled(&enabled) {
                self.prioritized.retain(|x| *x != p);
                self.prioritized.push(p);
            }
        }
        let next = self.highest_priority_enabled(&enabled);
        self.steps += 1;
        next
    }
    fn next_bool(&mut self, max: i32) -> bool {
        let r = self.rng.borrow_mut().next_below(max) == 0;
        self.steps += 1;
        r
    }
    fn next_int(&mut self, max: i32) -> i32 {
        let r = self.rng.borrow_mut().next_below(max);
        self.steps += 1;
        r
    }
    fn prepare_next_iteration(&mut self) -> bool {
        self.schedule_length = self.schedule_length.max(self.steps);
        self.steps = 0;
        self.prioritized.clear();
        self.change_points.clear();
        let mut range: Vec<usize> = (0..self.schedule_length).collect();
        let mut idx = range.len();
        while idx > 1 {
            idx -= 1;
            let point = self.rng.borrow_mut().next_below(self.schedule_length as i32) as usize;
            range.swap(idx, point);
        }
        for p in range.into_iter().take(self.max_switch_points) {
            self.add_point(p);
        }
        true
    }
    fn scheduled_steps(&self) -> usize {
        self.steps
    }
    fn has_reached_max_steps(&self) -> bool {
        self.max_steps != 0 && self.steps >= self.max_steps
    }
    fn is_fair(&self) -> bool {
        false
    }
    fn description(&self) -> String {
        let pts: Vec<String> = self.change_points.iter().map(|p| p.to_string()).collect();
        format!(
            "pct[priority change points '{}' [{}], seed '{}']",
            self.max_switch_points,
            pts.join(", "),
            self.rng.borrow().seed
        )
    }
}

// ---------------------------------------------------------------------------

/// A prefix strategy followed by a suffix strategy once the prefix hits its bound.
pub struct ComboStrategy {
    prefix: Box<dyn Strategy>,
    suffix: Box<dyn Strategy>,
}

impl ComboStrategy {
    pub fn new(prefix: Box<dyn Strategy>, suffix: Box<dyn Strategy>) -> Self {
        ComboStrategy { prefix, suffix }
    }
}

impl Strategy for ComboStrategy {
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        if self.prefix.has_reached_max_steps() {
            self.suffix.next_operation(current, ops)
        } else {
            self.prefix.next_operation(current, ops)
        }
    }
    fn next_bool(&mut self, max: i32) -> bool {
        if self.prefix.has_reached_max_steps() {
            self.suffix.next_bool(max)
        } else {
            self.prefix.next_bool(max)
        }
    }
    fn next_int(&mut self, max: i32) -> i32 {
        if self.prefix.has_reached_max_steps() {
            self.suffix.next_int(max)
        } else {
            self.prefix.next_int(max)
        }
    }
    fn prepare_next_iteration(&mut self) -> bool {
        let a = self.prefix.prepare_next_iteration();
        let b = self.suffix.prepare_next_iteration();
        a || b
    }
    fn scheduled_steps(&self) -> usize {
        if self.prefix.has_reached_max_steps() {
            self.suffix.scheduled_steps() + self.prefix.scheduled_steps()
        } else {
            self.prefix.scheduled_steps()
        }
    }
    fn has_reached_max_steps(&self) -> bool {
        self.suffix.has_reached_max_steps()
    }
    fn is_fair(&self) -> bool {
        self.suffix.is_fair()
    }
    fn description(&self) -> String {
        format!("combo[{},{}]", self.prefix.description(), self.suffix.description())
    }
}

// ---------------------------------------------------------------------------

/// One recorded decision of a schedule.
#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleStep {
    Operation(u64),
    Bool(bool),
    Int(i32),
}

/// A recorded schedule (`ScheduleTrace`), replayable with [`ReplayStrategy`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScheduleTrace {
    pub is_fair: bool,
    pub liveness_temperature_threshold: Option<usize>,
    pub test_name: Option<String>,
    /// `--snappea-time:<header>`: time tracking mode and profile (see `TimeOptions`).
    pub time: Option<String>,
    /// `--snappea-latency:<key>=<value>`: the resolved profile entries.
    pub latency: Vec<(String, String)>,
    pub steps: Vec<ScheduleStep>,
}

impl ScheduleTrace {
    /// The `.schedule` file format of the upstream checker.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        if self.is_fair {
            s.push_str("--fair-scheduling\n");
        }
        if let Some(t) = self.liveness_temperature_threshold {
            s.push_str(&format!("--liveness-temperature-threshold:{}\n", t));
        }
        if let Some(n) = &self.test_name {
            s.push_str(&format!("--test-method:{}\n", n));
        }
        if let Some(h) = &self.time {
            s.push_str(&format!("--snappea-time:{}\n", h));
        }
        for (k, v) in &self.latency {
            s.push_str(&format!("--snappea-latency:{}={}\n", k, v));
        }
        // steps are newline-separated, with no newline after the last (upstream format)
        for (i, st) in self.steps.iter().enumerate() {
            if i > 0 {
                s.push('\n');
            }
            match st {
                ScheduleStep::Operation(id) => s.push_str(&format!("({})", id)),
                ScheduleStep::Bool(b) => s.push_str(if *b { "True" } else { "False" }),
                ScheduleStep::Int(i) => s.push_str(&i.to_string()),
            }
        }
        s
    }

    pub fn parse(text: &str) -> Result<ScheduleTrace, String> {
        let mut t = ScheduleTrace::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line == "--fair-scheduling" {
                t.is_fair = true;
            } else if let Some(v) = line.strip_prefix("--liveness-temperature-threshold:") {
                t.liveness_temperature_threshold = Some(v.parse().map_err(|_| format!("bad threshold {v}"))?);
            } else if let Some(v) = line.strip_prefix("--test-method:") {
                t.test_name = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("--snappea-time:") {
                t.time = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("--snappea-latency:") {
                let (k, v) = v.rsplit_once('=').ok_or_else(|| format!("bad latency entry {v}"))?;
                t.latency.push((k.to_string(), v.to_string()));
            } else if line.starts_with("--") {
                continue;
            } else if let Some(v) = line.strip_prefix('(').and_then(|l| l.strip_suffix(')')) {
                t.steps.push(ScheduleStep::Operation(v.parse().map_err(|_| format!("bad operation {v}"))?));
            } else if line == "True" {
                t.steps.push(ScheduleStep::Bool(true));
            } else if line == "False" {
                t.steps.push(ScheduleStep::Bool(false));
            } else {
                t.steps.push(ScheduleStep::Int(line.parse().map_err(|_| format!("bad step {line}"))?));
            }
        }
        Ok(t)
    }
}

/// Replays a recorded schedule (`ReplayStrategy`).
pub struct ReplayStrategy {
    trace: ScheduleTrace,
    index: usize,
    is_fair: bool,
    error: Option<String>,
    /// Report the step bound as reached once this many steps were replayed (used to
    /// reproduce a schedule that ended at the bound; upstream replays never hit it).
    pub(crate) stop_at: Option<usize>,
}

impl ReplayStrategy {
    pub fn new(trace: ScheduleTrace) -> Self {
        let is_fair = trace.is_fair;
        ReplayStrategy { trace, index: 0, is_fair, error: None, stop_at: None }
    }

    /// Records why the trace cannot be followed (upstream's `ErrorText`) and returns
    /// `placeholder`, which the runtime discards: it ends the schedule (see
    /// [`Strategy::diverged`]).
    fn diverge<T>(&mut self, reason: &str, placeholder: T) -> T {
        self.error = Some(format!("Trace is not reproducible: {}", reason));
        placeholder
    }
}

impl Strategy for ReplayStrategy {
    fn next_operation(&mut self, _current: u64, ops: &[OpInfo]) -> Option<u64> {
        let enabled = enabled_ids(ops);
        if enabled.is_empty() {
            return None;
        }
        let id = match self.trace.steps.get(self.index) {
            None => return self.diverge("execution is longer than trace.", None),
            Some(ScheduleStep::Operation(id)) => *id,
            Some(_) => return self.diverge("next step is not a scheduling choice.", None),
        };
        if !enabled.contains(&id) {
            return self.diverge(&format!("cannot detect id '{}'.", id), None);
        }
        self.index += 1;
        Some(id)
    }
    fn next_bool(&mut self, _max: i32) -> bool {
        let b = match self.trace.steps.get(self.index) {
            None => return self.diverge("execution is longer than trace.", false),
            Some(ScheduleStep::Bool(b)) => *b,
            Some(ScheduleStep::Operation(_)) => {
                return self.diverge("next step is not a nondeterministic choice.", false)
            }
            Some(ScheduleStep::Int(_)) => {
                return self.diverge("next step is not a nondeterministic boolean choice.", false)
            }
        };
        self.index += 1;
        b
    }
    fn next_int(&mut self, max: i32) -> i32 {
        let i = match self.trace.steps.get(self.index) {
            None => return self.diverge("execution is longer than trace.", 0),
            Some(ScheduleStep::Int(i)) => *i,
            Some(ScheduleStep::Operation(_)) => return self.diverge("next step is not a nondeterministic choice.", 0),
            Some(ScheduleStep::Bool(_)) => {
                return self.diverge("next step is not a nondeterministic integer choice.", 0)
            }
        };
        // a recorded choice is below `max` (`Random.Next(0)` is 0)
        if !(0..max.max(1)).contains(&i) {
            return self.diverge(&format!("integer choice '{}' is not below the maximum value '{}'.", i, max), 0);
        }
        self.index += 1;
        i
    }
    fn prepare_next_iteration(&mut self) -> bool {
        false
    }
    fn scheduled_steps(&self) -> usize {
        self.index
    }
    fn has_reached_max_steps(&self) -> bool {
        self.stop_at.map(|k| self.index >= k).unwrap_or(false)
    }
    fn is_fair(&self) -> bool {
        self.is_fair
    }
    fn description(&self) -> String {
        "replay".into()
    }
    fn replay_error(&self) -> Option<String> {
        self.error.clone()
    }
    fn diverged(&self) -> bool {
        self.error.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::StrategyKind;

    #[test]
    fn strategy_names_round_trip() {
        for k in StrategyKind::ALL {
            assert_eq!(k.to_string().parse(), Ok(k));
        }
        assert!("astar".parse::<StrategyKind>().is_err());
    }
}
