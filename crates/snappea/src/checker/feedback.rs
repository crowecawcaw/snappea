//! POS and feedback-guided strategies (`Strategies/Feedback`): the `pos` strategy
//! (`ScheduleAndInputStrategy` + `POSScheduler`) and `feedback`, `feedbackpct`,
//! `feedbackpos` (`FeedbackGuidedStrategy` over a random, PCT or POS scheduler with
//! mutable, replayable random choices).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use crate::checker::random::DotNetRandom;
use crate::checker::runtime::SchedKind;
use crate::checker::strategy::{OpInfo, SharedRandom, Strategy};

/// A failure site, independent of formatted values and machine instance numbers.
pub(crate) fn failure_class(bug: &str) -> String {
    if let Some(rest) = bug.strip_prefix("Assertion Failed: ") {
        let end = rest
            .char_indices()
            .find_map(|(end, c)| {
                if !c.is_whitespace() {
                    return None;
                }
                let mut location = rest[..end].rsplitn(3, ':');
                location.next()?.parse::<usize>().ok()?;
                location.next()?.parse::<usize>().ok()?;
                Some(end)
            })
            .unwrap_or(rest.len());
        return format!("assertion:{}", &rest[..end]);
    }
    let mut key = String::new();
    let mut number = false;
    for c in bug.chars() {
        if c.is_ascii_digit() {
            if !number {
                key.push('#');
            }
            number = true;
        } else {
            key.push(c);
            number = false;
        }
    }
    key
}

// ---- random sources ----------------------------------------------------------------

/// `RandomChoices<T>`: a replayable, mutable list of random values drawn on demand.
#[derive(Clone)]
struct RandomChoices<T: Copy> {
    random: Rc<RefCell<DotNetRandom>>,
    pos: usize,
    data: Vec<T>,
    generate: fn(&mut DotNetRandom) -> T,
}

impl<T: Copy> RandomChoices<T> {
    fn new(random: Rc<RefCell<DotNetRandom>>, generate: fn(&mut DotNetRandom) -> T) -> Self {
        RandomChoices { random, pos: 0, data: Vec::new(), generate }
    }

    /// The consumed choices, at their end or (`rewind`) their start.
    fn prefix(&self, rewind: bool) -> Self {
        RandomChoices {
            random: self.random.clone(),
            pos: if rewind { 0 } else { self.pos },
            data: self.data[..self.pos].to_vec(),
            generate: self.generate,
        }
    }

    fn next(&mut self) -> T {
        if self.pos == self.data.len() {
            let v = self.generate_new();
            self.data.push(v);
        }
        let v = self.data[self.pos];
        self.pos += 1;
        v
    }

    fn generate_new(&self) -> T {
        (self.generate)(&mut self.random.borrow_mut())
    }

    /// `Utils.MutateRandomChoices(choices, meanMutationCount, meanMutationSize, random)`.
    fn mutate(&self, mean_count: i64, mean_size: i64, legacy: bool, suffix: bool) -> Self {
        let n = self.data.len() as i64;
        let mean_count = (n / 3).min(mean_count).max(1);
        let mean_size = (n / 3).min(mean_size).max(1);
        let mut out =
            RandomChoices { random: self.random.clone(), pos: 0, data: self.data.clone(), generate: self.generate };
        if n == 0 && !legacy {
            return out;
        }
        // Resample a suffix as well as changing individual choices. This preserves a
        // useful prefix while allowing the continuation to change length and shape.
        if suffix && !legacy && self.random.borrow_mut().next_below(2) == 0 {
            let offset = self.random.borrow_mut().next_below(n as i32) as usize;
            out.data.truncate(offset);
            return out;
        }
        let geometric = if legacy { sample_geometric_upstream } else { sample_geometric };
        let r = self.random.borrow_mut().next_double();
        let mut mutations = geometric((1.0f32 / mean_count as f32) as f64, r);
        while mutations > 0 {
            mutations -= 1;
            let offset = self.random.borrow_mut().next_below(out.data.len() as i32) as usize;
            let r = self.random.borrow_mut().next_double();
            let size = geometric((1.0f32 / mean_size as f32) as f64, r);
            let mut i = offset;
            while (i as i64) < offset as i64 + size as i64 {
                if i >= out.data.len() {
                    break;
                }
                out.data[i] = out.generate_new();
                i += 1;
            }
        }
        out
    }
}

/// Geometric distribution on 1.., including its degenerate p=1 case. Mutation
/// segments of mean one must change one choice, rather than silently changing none.
fn sample_geometric(p: f64, random: f64) -> i32 {
    if p == 1.0 {
        return 1;
    }
    sample_geometric_upstream(p, random).max(1)
}

/// `Utils.SampleGeometric`, including its p=1 zero-length mutation bug. Keep the
/// upstream random stream for snappea-compat; native checking uses the corrected form.
fn sample_geometric_upstream(p: f64, random: f64) -> i32 {
    let r = ((1.0 - random).ln() / (1.0 - p).ln()).ceil();
    if r.is_nan() {
        0
    } else {
        r as i32
    }
}

/// `ControlledRandom`: random ints and doubles recorded so a schedule can be mutated.
#[derive(Clone)]
pub struct ControlledRandom {
    ints: RandomChoices<i32>,
    doubles: RandomChoices<f64>,
    legacy_mutations: bool,
    local_mutations: bool,
}

impl ControlledRandom {
    /// `new ControlledRandom(new System.Random(seed))`.
    pub fn new(seed: u32) -> ControlledRandom {
        ControlledRandom::from_random(Rc::new(RefCell::new(DotNetRandom::new(seed as i32))))
    }

    fn from_random(r: Rc<RefCell<DotNetRandom>>) -> ControlledRandom {
        ControlledRandom {
            ints: RandomChoices::new(r.clone(), |r| r.next()),
            doubles: RandomChoices::new(r, |r| r.next_double()),
            legacy_mutations: false,
            local_mutations: false,
        }
    }

    pub fn next_below(&mut self, max: i32) -> i32 {
        if max == 0 {
            return 0;
        }
        self.ints.next() % max
    }

    /// Fresh choices drawing from the same underlying generator.
    pub fn new_like(&self) -> ControlledRandom {
        let mut out =
            ControlledRandom::from_random(self.ints.random.clone()).with_legacy_mutations(self.legacy_mutations);
        out.local_mutations = self.local_mutations;
        out
    }

    pub fn mutate(&self) -> ControlledRandom {
        ControlledRandom {
            ints: self.ints.mutate(
                if self.local_mutations { 1 } else { 5 },
                1,
                self.legacy_mutations,
                self.local_mutations,
            ),
            doubles: self.doubles.mutate(
                if self.local_mutations { 1 } else { 5 },
                1,
                self.legacy_mutations,
                self.local_mutations,
            ),
            legacy_mutations: self.legacy_mutations,
            local_mutations: self.local_mutations,
        }
    }

    /// The consumed choices only (copied, not the unconsumed suffix), positioned at their
    /// end or, with `rewind`, at their start for a replay.
    fn prefix(&self, rewind: bool) -> Self {
        ControlledRandom {
            ints: self.ints.prefix(rewind),
            doubles: self.doubles.prefix(rewind),
            legacy_mutations: self.legacy_mutations,
            local_mutations: self.local_mutations,
        }
    }

    /// Choices consumed so far: the size of [`ControlledRandom::prefix`].
    fn consumed(&self) -> usize {
        self.ints.pos + self.doubles.pos
    }

    fn choice_count(&self) -> usize {
        self.ints.data.len() + self.doubles.data.len()
    }

    /// Preserve upstream's zero-length mutations for seed compatibility.
    pub fn with_legacy_mutations(mut self, legacy: bool) -> Self {
        self.legacy_mutations = legacy;
        self
    }
}

/// The random source of a scheduler: the engine's generator (`pos`) or controlled choices
/// (feedback strategies).
#[derive(Clone)]
pub enum Source {
    Shared(SharedRandom),
    Controlled(ControlledRandom),
}

impl Source {
    fn next_below(&mut self, max: i32) -> i32 {
        match self {
            Source::Shared(r) => r.borrow_mut().next_below(max),
            Source::Controlled(c) => c.next_below(max),
        }
    }

    fn mutate(&self) -> Source {
        match self {
            Source::Controlled(c) => Source::Controlled(c.mutate()),
            other => other.clone(),
        }
    }

    fn new_like(&self) -> Source {
        match self {
            Source::Controlled(c) => Source::Controlled(c.new_like()),
            other => other.clone(),
        }
    }
}

// ---- schedulers ---------------------------------------------------------------------

/// `IScheduler`.
pub trait Scheduler {
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64>;
    fn prepare_next_iteration(&mut self) -> bool;
    /// Whether a complete schedule from this scheduler supports temperature checks.
    fn is_fair(&self) -> bool {
        true
    }
    /// Keep deliberate interventions in separate frontier buckets.
    fn context(&self) -> u64 {
        0
    }
    fn mutate(&self) -> Box<dyn Scheduler>;
    fn new_like(&self) -> Box<dyn Scheduler>;
    /// Replayable consumed prefix, and the number of retained random choices.
    fn checkpoint(&self, _rewind: bool) -> Option<(Box<dyn Scheduler>, usize)> {
        None
    }
    /// The number of choices [`Scheduler::checkpoint`] would retain, without copying them.
    fn consumed_choices(&self) -> Option<usize> {
        None
    }
}

fn highest_priority(prioritized: &[u64], choices: &[&OpInfo]) -> Option<u64> {
    prioritized.iter().copied().find(|p| choices.iter().any(|c| c.id == *p))
}

/// `RandomScheduler`.
pub struct RandomScheduler {
    rng: Source,
}

impl RandomScheduler {
    pub fn new(rng: Source) -> RandomScheduler {
        RandomScheduler { rng }
    }
}

impl Scheduler for RandomScheduler {
    fn next_operation(&mut self, _current: u64, ops: &[OpInfo]) -> Option<u64> {
        // no allocation: this runs at every scheduling step
        let mut enabled = ops.iter().filter(|o| o.enabled);
        match ops.iter().filter(|o| o.enabled).count() {
            0 => None,
            1 => enabled.next().map(|o| o.id),
            n => enabled.nth(self.rng.next_below(n as i32) as usize).map(|o| o.id),
        }
    }
    fn prepare_next_iteration(&mut self) -> bool {
        true
    }
    fn mutate(&self) -> Box<dyn Scheduler> {
        Box::new(RandomScheduler { rng: self.rng.mutate() })
    }
    fn new_like(&self) -> Box<dyn Scheduler> {
        Box::new(RandomScheduler { rng: self.rng.new_like() })
    }
    fn checkpoint(&self, rewind: bool) -> Option<(Box<dyn Scheduler>, usize)> {
        let Source::Controlled(random) = &self.rng else { return None };
        let prefix = random.prefix(rewind);
        let count = prefix.choice_count();
        Some((Box::new(RandomScheduler::new(Source::Controlled(prefix))), count))
    }
    fn consumed_choices(&self) -> Option<usize> {
        let Source::Controlled(random) = &self.rng else { return None };
        Some(random.consumed())
    }
}

/// Suppress one runnable actor for a sampled interval. Choices, including the
/// intervention, live in the mutable random prefix so checkpoints can replay it.
#[derive(Clone)]
pub(crate) struct DelayScheduler {
    rng: Source,
    initialized: bool,
    trial: Rc<std::cell::Cell<usize>>,
    plan: usize,
    target: Option<u64>,
    candidate: Option<u64>,
    start: usize,
    duration: usize,
    opportunities: usize,
    elapsed: usize,
    released: bool,
}
impl DelayScheduler {
    pub(crate) fn new(rng: Source) -> Self {
        Self {
            rng,
            initialized: false,
            trial: Rc::new(std::cell::Cell::new(0)),
            plan: 0,
            target: None,
            candidate: None,
            start: 0,
            duration: 0,
            opportunities: 0,
            elapsed: 0,
            released: false,
        }
    }
}
impl Scheduler for DelayScheduler {
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        let enabled: Vec<_> = ops.iter().filter(|o| o.enabled).map(|o| o.id).collect();
        if enabled.is_empty() {
            return None;
        }
        if self.candidate.is_none() {
            self.candidate = ops.first().map(|op| op.id + (self.plan % 16) as u64);
        }
        if enabled.len() > 1 && !self.initialized {
            // Cover each actor with a long initial delay before spending trials
            // on shorter/later delays. This avoids repeatedly choosing one actor.
            self.start = if self.plan < 16 { 0 } else { [0, 1, 4, 16][(self.plan / 16) % 4] };
            self.duration = if self.plan < 16 { 1024 } else { [16, 64, 256, 1024][(self.plan / 64) % 4] };
            self.initialized = true;
        }
        if enabled.len() > 1 && !self.released {
            if self.target.is_none() && self.opportunities >= self.start {
                self.target = self.candidate.filter(|id| enabled.contains(id));
            }
            self.opportunities += 1;
        }
        let held = self.target.filter(|_| !self.released);
        if let Some(target) = held {
            // Never manufacture a deadlock. If no other actor can run, release it.
            if self.elapsed >= self.duration || (enabled.len() == 1 && enabled[0] == target) {
                self.released = true;
            } else {
                self.elapsed += 1;
            }
        }
        let choices: Vec<_> = enabled.into_iter().filter(|id| self.released || Some(*id) != held).collect();
        // Alternate stable ordering with random continuations. Stable ordering
        // explores long runs of another actor without additional lucky choices.
        let draw = self.rng.next_below(choices.len() as i32) as usize;
        // Let an unheld creator finish making its actors before exploring their
        // orders. Otherwise high-id-first simply executes every child at birth.
        if choices.contains(&current) && ops.iter().any(|o| o.id == current && o.kind == SchedKind::Create) {
            return Some(current);
        }
        let index = match if self.plan < 32 { self.plan % 2 } else { 2 } {
            0 => 0,
            1 => choices.len() - 1,
            _ => draw,
        };
        Some(choices[index])
    }
    fn prepare_next_iteration(&mut self) -> bool {
        true
    }
    fn is_fair(&self) -> bool {
        false
    }
    fn context(&self) -> u64 {
        // The target and start remain distinct after release, too: a successful
        // continuation must not evict a still-stalled prefix from another plan.
        self.target.or(self.candidate).map_or(0, |id| {
            (id + 1).wrapping_mul(4099)
                ^ ((self.start as u64) << 32)
                ^ ((self.duration as u64) << 40)
                ^ (((self.plan % 2) as u64) << 56)
                ^ ((self.released as u64) << 63)
        })
    }
    fn mutate(&self) -> Box<dyn Scheduler> {
        let mut next = Self::new(self.rng.mutate());
        next.plan = self.plan;
        next.trial = self.trial.clone();
        Box::new(next)
    }
    fn new_like(&self) -> Box<dyn Scheduler> {
        let mut next = Self::new(self.rng.new_like());
        self.trial.set(self.trial.get() + 1);
        next.plan = self.trial.get();
        next.trial = self.trial.clone();
        Box::new(next)
    }
    fn checkpoint(&self, rewind: bool) -> Option<(Box<dyn Scheduler>, usize)> {
        let Source::Controlled(random) = &self.rng else {
            return None;
        };
        let prefix = random.prefix(rewind);
        let count = prefix.choice_count();
        let mut scheduler = if rewind { Self::new(Source::Controlled(prefix.clone())) } else { self.clone() };
        scheduler.rng = Source::Controlled(prefix);
        scheduler.plan = self.plan;
        scheduler.trial = self.trial.clone();
        Some((Box::new(scheduler), count))
    }
    fn consumed_choices(&self) -> Option<usize> {
        let Source::Controlled(random) = &self.rng else {
            return None;
        };
        Some(random.consumed())
    }
}

/// `POSScheduler`: partial order sampling; sends to the same receiver race, everything
/// else is preferred.
pub struct PosScheduler {
    rng: Source,
    prioritized: Vec<u64>,
}

impl PosScheduler {
    pub fn new(rng: Source) -> PosScheduler {
        PosScheduler { rng, prioritized: Vec::new() }
    }

    fn prioritized_operation(&mut self, ops: &[&OpInfo], current: u64) -> u64 {
        if self.prioritized.is_empty() {
            self.prioritized.push(current);
        }
        for op in ops {
            if !self.prioritized.contains(&op.id) {
                let idx = self.rng.next_below(self.prioritized.len() as i32) as usize + 1;
                self.prioritized.insert(idx, op.id);
            }
        }
        let non_racing: Vec<&OpInfo> = ops.iter().copied().filter(|o| o.kind != SchedKind::Send).collect();
        match non_racing.len() {
            0 => {}
            1 => return non_racing[0].id,
            _ => {
                if let Some(n) = highest_priority(&self.prioritized, &non_racing) {
                    return n;
                }
            }
        }
        highest_priority(&self.prioritized, ops).expect("an enabled operation is prioritized")
    }
}

impl Scheduler for PosScheduler {
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        let enabled: Vec<&OpInfo> = ops.iter().filter(|o| o.enabled).collect();
        if enabled.is_empty() {
            return None;
        }
        let next = self.prioritized_operation(&enabled, current);
        let info = enabled.iter().find(|o| o.id == next).copied().copied().unwrap();
        if info.kind == SchedKind::Send {
            for op in &enabled {
                if op.kind == SchedKind::Send && op.receiver == info.receiver {
                    self.prioritized.retain(|p| *p != op.id);
                }
            }
            self.prioritized.retain(|p| *p != next);
        }
        Some(next)
    }
    fn prepare_next_iteration(&mut self) -> bool {
        self.prioritized.clear();
        true
    }
    fn mutate(&self) -> Box<dyn Scheduler> {
        Box::new(PosScheduler::new(self.rng.mutate()))
    }
    fn new_like(&self) -> Box<dyn Scheduler> {
        Box::new(PosScheduler::new(self.rng.new_like()))
    }
}

/// `PCTScheduler` (the feedback variant of PCT).
pub struct PctScheduler {
    rng: Source,
    steps: i64,
    max_switch_points: usize,
    schedule_length: i64,
    prioritized: Vec<u64>,
    change_points: BTreeSet<i64>,
}

impl PctScheduler {
    pub fn new(max_switch_points: usize, schedule_length: i64, rng: Source) -> PctScheduler {
        let mut s = PctScheduler {
            rng,
            steps: 0,
            max_switch_points,
            schedule_length,
            prioritized: Vec::new(),
            change_points: BTreeSet::new(),
        };
        if schedule_length != 0 {
            s.prepare_change_points(schedule_length);
        }
        s
    }

    fn prepare_change_points(&mut self, steps: i64) {
        let mut list: Vec<i64> = (0..steps).collect();
        for _ in 0..self.max_switch_points {
            let idx = self.rng.next_below(list.len() as i32) as usize;
            self.change_points.insert(list[idx]);
            list.remove(idx);
            if list.is_empty() {
                break;
            }
        }
    }

    fn move_change_point_forward(&mut self) {
        self.change_points.remove(&self.steps);
        let mut p = self.steps + 1;
        while self.change_points.contains(&p) {
            p += 1;
        }
        self.change_points.insert(p);
    }
}

impl Scheduler for PctScheduler {
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        self.steps += 1;
        let enabled: Vec<&OpInfo> = ops.iter().filter(|o| o.enabled).collect();
        if enabled.is_empty() {
            if self.change_points.contains(&self.steps) {
                self.move_change_point_forward();
            }
            return None;
        }
        if self.prioritized.is_empty() {
            self.prioritized.push(current);
        }
        for op in &enabled {
            if !self.prioritized.contains(&op.id) {
                let idx = self.rng.next_below(self.prioritized.len() as i32) as usize + 1;
                self.prioritized.insert(idx, op.id);
            }
        }
        if self.change_points.contains(&self.steps) {
            if enabled.len() == 1 {
                self.move_change_point_forward();
            } else if let Some(p) = highest_priority(&self.prioritized, &enabled) {
                self.prioritized.retain(|x| *x != p);
                self.prioritized.push(p);
            }
        }
        highest_priority(&self.prioritized, &enabled)
    }
    fn prepare_next_iteration(&mut self) -> bool {
        self.schedule_length = self.schedule_length.max(self.steps);
        self.steps = 0;
        self.prioritized.clear();
        self.change_points.clear();
        true
    }
    fn mutate(&self) -> Box<dyn Scheduler> {
        Box::new(PctScheduler::new(self.max_switch_points, self.schedule_length, self.rng.mutate()))
    }
    fn new_like(&self) -> Box<dyn Scheduler> {
        Box::new(PctScheduler::new(self.max_switch_points, self.schedule_length, self.rng.new_like()))
    }
}

// ---- strategies ---------------------------------------------------------------------

/// `ScheduleAndInputStrategy` (the `pos` strategy): a scheduler for interleavings, the
/// engine's generator for data choices.
pub struct ScheduleAndInputStrategy {
    rng: SharedRandom,
    max_steps: usize,
    steps: usize,
    scheduler: Box<dyn Scheduler>,
}

impl ScheduleAndInputStrategy {
    pub fn new(max_steps: usize, rng: SharedRandom, scheduler: Box<dyn Scheduler>) -> Self {
        ScheduleAndInputStrategy { rng, max_steps, steps: 0, scheduler }
    }
}

impl Strategy for ScheduleAndInputStrategy {
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        self.steps += 1;
        self.scheduler.next_operation(current, ops)
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
        self.scheduler.prepare_next_iteration()
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
        // sic: upstream reports this strategy as pct
        format!("pct[seed '{}']", self.rng.borrow().seed)
    }
}

struct GeneratorState {
    input: ControlledRandom,
    scheduler: Box<dyn Scheduler>,
}

type Generator = Rc<RefCell<GeneratorState>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FrontierKey {
    Delivery(u32, u32, u32, u64),
    Condition(u32, usize, bool, u64),
}

struct FrontierRecord {
    key: FrontierKey,
    count: usize,
    choices: usize,
    /// `None` once the prefix has no continuation left to sample: the record then only
    /// keeps the same prefix from being saved again, and gives its slot to a new key.
    generator: Option<Generator>,
}

#[derive(Default)]
struct Frontiers {
    counts: rustc_hash::FxHashMap<FrontierKey, usize>,
    saved: Vec<FrontierRecord>,
    retained: usize,
    parent: Option<(FrontierKey, usize, usize)>,
}

struct GeneratorRecord {
    id: usize,
    priority: f64,
    mutation_budget: i32,
    generator: Generator,
    minhash: Vec<i32>,
}

const DEFAULT_MUTATION_BUDGET: i32 = 50;

/// `FeedbackGuidedStrategy`: keeps the generators of schedules with new timelines and
/// mutates them, prioritizing the most diverse.
pub struct FeedbackGuidedStrategy {
    generator: Generator,
    max_steps: usize,
    steps: usize,
    visited_timelines: HashSet<String>,
    saved: Vec<Rc<GeneratorRecord>>,
    pending_mutations: i32,
    should_explore_new: bool,
    visited: HashSet<usize>,
    current_parent: Option<Rc<GeneratorRecord>>,
    rnd: DotNetRandom,
    next_record_id: usize,
    suite_best_reached: std::collections::HashMap<String, usize>,
    suite_satisfied: HashSet<String>,
    fresh_percent: u8,
    code_guidance: bool,
    condition_prefixes: bool,
    stalled: usize,
    native_features: HashSet<String>,
    failures: HashSet<String>,
    failure_seeds: Vec<Generator>,
    frontiers: Option<Frontiers>,
}

impl FeedbackGuidedStrategy {
    /// `max_steps` is the step bound; `seed` seeds the input generator, the
    /// scheduler's generator and the explore/exploit choices independently.
    pub fn new(max_steps: usize, seed: u32, scheduler: Box<dyn Scheduler>) -> Self {
        FeedbackGuidedStrategy {
            generator: Rc::new(RefCell::new(GeneratorState { input: ControlledRandom::new(seed), scheduler })),
            max_steps,
            steps: 0,
            visited_timelines: HashSet::new(),
            saved: Vec::new(),
            pending_mutations: 0,
            should_explore_new: false,
            visited: HashSet::new(),
            current_parent: None,
            rnd: DotNetRandom::new(seed as i32),
            next_record_id: 0,
            suite_best_reached: std::collections::HashMap::new(),
            suite_satisfied: HashSet::new(),
            fresh_percent: 0,
            code_guidance: false,
            condition_prefixes: false,
            stalled: 0,
            native_features: HashSet::new(),
            failures: HashSet::new(),
            failure_seeds: Vec::new(),
            frontiers: None,
        }
    }

    /// Match the input generator's mutation semantics to the scheduler's source.
    pub fn with_legacy_mutations(self, legacy: bool) -> Self {
        self.generator.borrow_mut().input.legacy_mutations = legacy;
        self
    }

    /// Reserve a fraction of schedules for fresh choices, retaining the learned corpus
    /// and mutation budget between them. Zero preserves the original exploration policy.
    pub fn with_fresh_exploration(mut self, percent: u8) -> Self {
        self.fresh_percent = percent.min(100);
        self
    }

    /// Use coverage novelty instead of timeline MinHash; retain upstream behavior by default.
    pub fn with_code_guidance(mut self, enabled: bool) -> Self {
        self.code_guidance = enabled;
        self.generator.borrow_mut().input.local_mutations = enabled;
        self
    }

    /// Preserve input/schedule prefixes at increasing delivery counts, then sample
    /// fresh continuations. Requires a scheduler that supports checkpoints.
    pub fn with_frontier_guidance(mut self) -> Self {
        self.frontiers = Some(Frontiers::default());
        self
    }

    /// Preserve condition-edge prefixes in automatic search. Kept separate from
    /// explicit feedback's established mutation policy for controlled comparisons.
    pub(crate) fn with_condition_prefixes(mut self) -> Self {
        self.condition_prefixes = true;
        self
    }
    fn checkpoint_frontier(&mut self, key: FrontierKey, count: usize) {
        let Some(frontiers) = &mut self.frontiers else {
            return;
        };
        let old = frontiers.saved.iter().position(|record| record.key == key);
        if old.is_some_and(|index| frontiers.saved[index].count >= count) {
            return;
        }
        // a full table reuses the slot of a prefix with nothing left to sample
        let slot = old.or_else(|| {
            (frontiers.saved.len() == 256).then(|| frontiers.saved.iter().position(|r| r.generator.is_none())).flatten()
        });
        if slot.is_none() && frontiers.saved.len() == 256 {
            return;
        }
        // count before copying: this runs on every counted event
        let generator = self.generator.borrow();
        let Some(scheduling_choices) = generator.scheduler.consumed_choices() else { return };
        let choices = scheduling_choices + generator.input.consumed();
        let retained = frontiers.retained - slot.map_or(0, |index| frontiers.saved[index].choices) + choices;
        if retained > 262_144 {
            return;
        }
        let Some((scheduler, _)) = generator.scheduler.checkpoint(false) else { return };
        let input = generator.input.prefix(false);
        let record = FrontierRecord {
            key,
            count,
            choices,
            generator: Some(Rc::new(RefCell::new(GeneratorState { input, scheduler }))),
        };
        match slot {
            Some(index) => frontiers.saved[index] = record,
            None => frontiers.saved.push(record),
        }
        frontiers.retained = retained;
    }
    fn compute_diversity(&mut self, timeline: &str, hash: &[i32]) -> f64 {
        if !self.visited_timelines.insert(timeline.to_string()) {
            return 0.0;
        }
        if self.saved.is_empty() || hash.is_empty() {
            return 1.0;
        }
        let mut max_sim = 0;
        for r in &self.saved {
            let sim = hash.iter().zip(r.minhash.iter()).filter(|(a, b)| a == b).count();
            max_sim = max_sim.max(sim);
        }
        1.0 - max_sim as f64 / hash.len() as f64
    }

    fn new_generator(&self) -> Generator {
        let g = self.generator.borrow();
        Rc::new(RefCell::new(GeneratorState { input: g.input.new_like(), scheduler: g.scheduler.new_like() }))
    }

    fn mutate_generator(prev: &Generator) -> Generator {
        let g = prev.borrow();
        Rc::new(RefCell::new(GeneratorState { input: g.input.mutate(), scheduler: g.scheduler.mutate() }))
    }

    fn prepare_next_input(&mut self) {
        self.generator.borrow_mut().scheduler.prepare_next_iteration();
        if let Some(frontiers) = &mut self.frontiers {
            if let Some((key, count, prefix_choices)) = frontiers.parent.take() {
                let generator = self.generator.borrow();
                let scheduling_choices = generator.scheduler.consumed_choices().expect("checkpoint scheduler");
                let choices = scheduling_choices + generator.input.consumed();
                if choices == prefix_choices {
                    // A prefix with no further choices has no continuation to sample:
                    // release its choices and keep the record so it is not saved again.
                    if let Some(record) = frontiers.saved.iter_mut().find(|r| r.key == key && r.count == count) {
                        frontiers.retained -= record.choices;
                        record.choices = 0;
                        record.generator = None;
                    }
                }
            }
            frontiers.counts.clear();
            let available: Vec<_> = frontiers.saved.iter().filter(|record| record.generator.is_some()).collect();
            if !available.is_empty() && self.rnd.next_below(2) == 0 {
                let recent: Vec<_> = available
                    .iter()
                    .rev()
                    .filter(|r| matches!(r.key, FrontierKey::Condition(..)))
                    .take(8)
                    .copied()
                    .collect();
                let record = if !recent.is_empty() && self.rnd.next_below(4) != 0 {
                    recent[self.rnd.next_below(recent.len() as i32) as usize]
                } else {
                    available[self.rnd.next_below(available.len() as i32) as usize]
                };
                frontiers.parent = Some((record.key, record.count, record.choices));
                let parent = record.generator.as_ref().unwrap().borrow();
                self.generator = Rc::new(RefCell::new(GeneratorState {
                    input: parent.input.prefix(true),
                    scheduler: parent.scheduler.checkpoint(true).expect("frontier scheduler has checkpoints").0,
                }));
                return;
            }
        }
        if self.code_guidance {
            // Exploit recent progress promptly, share attempts with older frontiers, and
            // fall back to independent sampling when coverage stops being informative.
            let fresh = self.fresh_percent.max(if self.stalled >= 128 { 75 } else { 25 });
            if self.saved.is_empty() || self.rnd.next_below(100) < i32::from(fresh) {
                self.generator = self.new_generator();
            } else if !self.failure_seeds.is_empty() && self.rnd.next_below(2) == 0 {
                let index = self.rnd.next_below(self.failure_seeds.len() as i32) as usize;
                self.generator = Self::mutate_generator(&self.failure_seeds[index]);
            } else {
                let index = if self.rnd.next_below(4) != 0 {
                    self.saved.len() - 1
                } else {
                    self.rnd.next_below(self.saved.len() as i32) as usize
                };
                self.generator = Self::mutate_generator(&self.saved[index].generator);
            }
            return;
        }
        if self.fresh_percent > 0 && self.rnd.next_double() < f64::from(self.fresh_percent) / 100.0 {
            self.generator = self.new_generator();
            return;
        }
        if self.saved.is_empty() {
            self.generator = self.new_generator();
            return;
        }
        if self.current_parent.is_none() && !self.should_explore_new {
            let first = self.saved[0].clone();
            self.visited.insert(first.id);
            self.pending_mutations = first.mutation_budget;
            self.current_parent = Some(first);
        }
        if self.pending_mutations == 0 {
            self.should_explore_new = false;
            let next = self.saved.iter().find(|g| !self.visited.contains(&g.id)).cloned();
            match next {
                Some(g) => {
                    self.visited.insert(g.id);
                    self.pending_mutations = g.mutation_budget;
                    self.current_parent = Some(g);
                }
                None => {
                    if self.rnd.next_double() < 0.5 {
                        self.visited.clear();
                        let first = self.saved[0].clone();
                        self.visited.insert(first.id);
                        self.pending_mutations = first.mutation_budget;
                        self.current_parent = Some(first);
                    } else {
                        self.should_explore_new = true;
                        self.current_parent = None;
                        self.pending_mutations = DEFAULT_MUTATION_BUDGET;
                    }
                }
            }
        }
        self.generator = if self.should_explore_new {
            self.new_generator()
        } else {
            Self::mutate_generator(&self.current_parent.as_ref().unwrap().generator)
        };
        self.pending_mutations -= 1;
    }
}

impl Strategy for FeedbackGuidedStrategy {
    fn enable_frontier_guidance(&mut self) {
        self.frontiers = Some(Frontiers::default());
    }
    fn wants_frontiers(&self) -> bool {
        self.frontiers.is_some()
    }
    fn observe_frontier(&mut self, machine: u32, state: u32, event: u32) {
        let Some(frontiers) = &mut self.frontiers else { return };
        let key = FrontierKey::Delivery(machine, state, event, self.generator.borrow().scheduler.context());
        if !frontiers.counts.contains_key(&key) && frontiers.counts.len() == 256 {
            return;
        }
        let count = frontiers.counts.entry(key).or_default();
        *count = count.saturating_add(1);
        // Fine-grained early stages, sparse milestones for long-running loops.
        if *count > 64 && !count.is_power_of_two() {
            return;
        }
        let count = *count;
        self.checkpoint_frontier(key, count);
    }
    fn observe_condition(&mut self, fun: u32, pc: usize, outcome: bool) {
        if !self.condition_prefixes || !self.code_guidance || self.frontiers.is_none() {
            return;
        }
        let context = self.generator.borrow().scheduler.context();
        // Coverage sites are stable keys, not repeatedly increasing event counts.
        // Keep the prefix at the feature, before unrelated suffix choices consume it.
        self.checkpoint_frontier(FrontierKey::Condition(fun, pc, outcome, context), 1);
    }
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        let r = self.generator.borrow_mut().scheduler.next_operation(current, ops);
        self.steps += 1;
        r
    }
    fn next_bool(&mut self, max: i32) -> bool {
        let r = self.generator.borrow_mut().input.next_below(max) == 0;
        self.steps += 1;
        r
    }
    fn next_int(&mut self, max: i32) -> i32 {
        let r = self.generator.borrow_mut().input.next_below(max);
        self.steps += 1;
        r
    }
    fn prepare_next_iteration(&mut self) -> bool {
        self.steps = 0;
        self.prepare_next_input();
        true
    }
    fn scheduled_steps(&self) -> usize {
        self.steps
    }
    fn has_reached_max_steps(&self) -> bool {
        self.max_steps != 0 && self.steps >= self.max_steps
    }
    fn is_fair(&self) -> bool {
        self.generator.borrow().scheduler.is_fair()
    }
    fn description(&self) -> String {
        "feedback".into()
    }
    fn wants_feedback(&self) -> bool {
        true
    }
    fn wants_minhash(&self) -> bool {
        !self.code_guidance
    }
    fn observe_progress(&mut self, new_coverage: usize, bug: Option<&str>) {
        if !self.code_guidance {
            return;
        }
        let new_failure = bug.is_some_and(|bug| self.failures.len() < 64 && self.failures.insert(failure_class(bug)));
        // Reuse failing data prefixes to vary inputs near a failure site. Pure
        // interleavings already use the branch/event corpus; repeatedly replaying a
        // failed interleaving can crowd out other delivery orders.
        if new_failure && !self.generator.borrow().input.ints.data.is_empty() {
            self.failure_seeds.push(self.generator.clone());
        }
        self.stalled = self.stalled.saturating_add(1);
        if new_coverage == 0 && !new_failure {
            return;
        }
        self.stalled = 0;
        // Coverage and timeline novelty both report progress for the same schedule;
        // save it once. Each schedule gets a new generator, so identity is the schedule.
        if self.saved.last().is_some_and(|r| Rc::ptr_eq(&r.generator, &self.generator)) {
            return;
        }
        // Only productive seeds enter the corpus; cap memory for long checks.
        if self.saved.len() == 256 {
            self.saved.remove(0);
        }
        self.saved.push(Rc::new(GeneratorRecord {
            id: self.next_record_id,
            priority: 1.,
            mutation_budget: 1,
            generator: self.generator.clone(),
            minhash: Vec::new(),
        }));
        self.next_record_id += 1;
    }
    fn observe_timeline(&mut self, timeline: &str, minhash: &[i32], reached: &[(String, usize)], satisfied: &[String]) {
        if self.code_guidance {
            let mut progress = false;
            for token in timeline.split(';').filter(|s| !s.is_empty()) {
                if self.native_features.len() < 100_000 && !self.native_features.contains(token) {
                    progress |= self.native_features.insert(token.to_string());
                }
            }
            for (name, count) in reached {
                let best = self.suite_best_reached.entry(name.clone()).or_default();
                if *count > *best {
                    *best = *count;
                    progress = true;
                }
            }
            for name in satisfied {
                progress |= self.suite_satisfied.insert(name.clone());
            }
            if progress {
                self.observe_progress(1, None);
            }
            return;
        }
        let diversity = self.compute_diversity(timeline, minhash);
        if diversity <= 0.0 {
            return;
        }
        // ScenarioSteering.NoveltyCompliance: new scenario progress boosts the priority
        let mut compliance = 0.0;
        for s in satisfied {
            if self.suite_satisfied.insert(s.clone()) {
                compliance = 1.0;
            }
        }
        for (s, n) in reached {
            if self.suite_satisfied.contains(s) {
                continue;
            }
            let best = self.suite_best_reached.get(s).copied().unwrap_or(0);
            if *n > best {
                self.suite_best_reached.insert(s.clone(), *n);
                compliance = 1.0;
            }
        }
        let priority = diversity * (1.0 + compliance);
        let budget = ((diversity * DEFAULT_MUTATION_BUDGET as f64).round_ties_even() as i32).max(1);
        let record = Rc::new(GeneratorRecord {
            id: self.next_record_id,
            priority,
            mutation_budget: budget,
            generator: self.generator.clone(),
            minhash: minhash.to_vec(),
        });
        self.next_record_id += 1;
        let mut index = 0;
        while index < self.saved.len() && priority < self.saved[index].priority {
            index += 1;
        }
        self.saved.insert(index, record);
    }
}

/// `TimelineObserver.GetTimelineMinhash` over a set of timeline tokens.
pub fn timeline_minhash<'a>(tokens: impl Iterator<Item = &'a str>) -> Vec<i32> {
    thread_local! {
        static COEFFICIENTS: Vec<(i32, i32)> = {
            let mut r = DotNetRandom::new(0);
            (0..50).map(|_| { let a = r.next(); let b = r.next(); (a, b) }).collect()
        };
    }
    let hashes: Vec<i32> = tokens.map(stable_hash).collect();
    COEFFICIENTS.with(|cs| {
        cs.iter()
            .map(|(a, b)| hashes.iter().map(|v| a.wrapping_mul(*v).wrapping_add(*b)).fold(i32::MAX, i32::min))
            .collect()
    })
}

/// FNV-1a over UTF-8 (`TimelineObserver.StableHash`).
pub fn stable_hash(token: &str) -> i32 {
    let mut h: u32 = 2166136261;
    for b in token.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    h as i32
}

#[cfg(test)]
mod mutation_tests {
    use super::*;

    #[test]
    fn checkpoints_drop_unconsumed_mutation_suffixes_and_rewind_only_for_replay() {
        let mut random = ControlledRandom::new(7);
        let values: Vec<_> = (0..8).map(|_| random.next_below(100)).collect();
        random.ints.pos = 3;
        let saved = random.prefix(false);
        assert_eq!(saved.ints.pos, 3);
        assert_eq!(saved.ints.data.len(), 3);
        let mut replay = saved.prefix(true);
        assert_eq!((0..3).map(|_| replay.next_below(100)).collect::<Vec<_>>(), values[..3]);
        assert_eq!(random.ints.data.len(), 8);
    }

    #[test]
    fn frontier_records_are_bounded_and_terminal_prefixes_stop_being_sampled() {
        let source = Source::Controlled(ControlledRandom::new(7));
        let mut strategy = FeedbackGuidedStrategy::new(100, 7, Box::new(RandomScheduler::new(source)))
            .with_code_guidance(true)
            .with_frontier_guidance();
        strategy.next_int(2);
        for event in 0..1000 {
            strategy.observe_frontier(0, 0, event);
        }
        assert_eq!(strategy.frontiers.as_ref().unwrap().saved.len(), 256);
        assert!(strategy.frontiers.as_ref().unwrap().retained <= 262_144);
        for _ in 0..10000 {
            strategy.prepare_next_iteration();
            strategy.next_int(2);
        }
        let frontiers = strategy.frontiers.as_ref().unwrap();
        assert!(frontiers.saved.iter().all(|record| record.generator.is_none()));
        assert_eq!(frontiers.retained, 0, "exhausted prefixes release their choices");
    }

    #[test]
    fn code_novelty_retains_seeds_even_with_identical_timelines_and_bounds_the_corpus() {
        let source = Source::Controlled(ControlledRandom::new(7));
        let mut strategy =
            FeedbackGuidedStrategy::new(100, 7, Box::new(RandomScheduler::new(source))).with_code_guidance(true);
        for _ in 0..300 {
            strategy.next_int(100);
            strategy.observe_progress(1, None);
            strategy.observe_timeline("same", &[], &[], &[]);
            strategy.prepare_next_iteration();
        }
        assert_eq!(strategy.saved.len(), 256);
        assert!(!strategy.wants_minhash());
        for _ in 0..128 {
            strategy.observe_progress(0, None);
        }
        let mut fresh = 0;
        for _ in 0..100 {
            strategy.prepare_next_iteration();
            fresh += usize::from(strategy.generator.borrow().input.ints.data.is_empty());
        }
        assert!(fresh > 50 && fresh < 100, "stalls favor fresh sampling but keep exploiting: {fresh}");
        assert_eq!(strategy.saved.len(), 256);
    }

    #[test]
    fn failure_data_seeds_are_distinct_by_site_and_do_not_need_new_coverage() {
        let source = Source::Controlled(ControlledRandom::new(7));
        let mut strategy =
            FeedbackGuidedStrategy::new(100, 7, Box::new(RandomScheduler::new(source))).with_code_guidance(true);
        strategy.next_int(100);
        strategy.observe_progress(0, Some("Assertion Failed: model.p:3:2 value 123"));
        strategy.observe_progress(0, Some("Assertion Failed: model.p:3:2 value 456"));
        assert_eq!(strategy.failure_seeds.len(), 1);
        assert_eq!(strategy.saved.len(), 1);
        strategy.observe_progress(0, Some("Assertion Failed: model.p:4:2 value 456"));
        assert_eq!(strategy.failure_seeds.len(), 2);
    }

    #[test]
    fn geometric_mutation_sizes_are_positive_even_at_mean_one() {
        for random in [0.0, 0.1, 0.5, 0.999999] {
            assert_eq!(sample_geometric(1.0, random), 1);
            assert!(sample_geometric(0.2, random) >= 1);
            assert_eq!(sample_geometric_upstream(1.0, random), 0);
        }
        assert_eq!(sample_geometric(0.5, 0.75), 2);
    }

    #[test]
    fn mutating_recorded_choices_changes_short_and_long_inputs_without_changing_parent() {
        for count in [1, 3, 30] {
            let mut source = ControlledRandom::new(7);
            for _ in 0..count {
                source.next_below(i32::MAX);
            }
            let original = source.ints.data.clone();
            let mutation = source.mutate();
            assert_ne!(mutation.ints.data, original, "{count} choices");
            assert_eq!(source.ints.data, original);
            assert_eq!(mutation.ints.pos, 0);
            assert!(mutation.doubles.data.is_empty());
        }
        // No recorded input is also safe to mutate and subsequently extend.
        let mut empty = ControlledRandom::new(7).mutate();
        assert!((0..10).contains(&empty.next_below(10)));
    }

    #[test]
    fn legacy_mutations_and_fresh_generators_keep_the_upstream_mode() {
        let mut source = ControlledRandom::new(7).with_legacy_mutations(true);
        for _ in 0..30 {
            source.next_below(i32::MAX);
        }
        let mutation = source.mutate();
        assert_eq!(mutation.ints.data, source.ints.data);
        assert!(mutation.legacy_mutations);
        assert!(source.new_like().legacy_mutations);
    }

    #[test]
    fn fresh_exploration_keeps_the_corpus_and_resumes_mutations() {
        let source = Source::Controlled(ControlledRandom::new(7));
        let mut strategy =
            FeedbackGuidedStrategy::new(100, 7, Box::new(RandomScheduler::new(source))).with_fresh_exploration(100);
        strategy.next_int(1000);
        strategy.observe_timeline("timeline", &[1, 2], &[], &[]);
        assert_eq!(strategy.saved.len(), 1);
        let saved = strategy.saved[0].clone();
        strategy.prepare_next_iteration();
        assert!(strategy.generator.borrow().input.ints.data.is_empty());
        assert!(Rc::ptr_eq(&strategy.saved[0], &saved));
        strategy = strategy.with_fresh_exploration(0);
        strategy.prepare_next_iteration();
        assert_eq!(strategy.current_parent.as_ref().unwrap().id, saved.id);
        assert!(!strategy.generator.borrow().input.ints.data.is_empty());
    }
}

#[cfg(test)]
mod delay_tests {
    use super::*;
    fn op(id: u64) -> OpInfo {
        OpInfo { id, enabled: true, kind: SchedKind::Default, receiver: None }
    }
    #[test]
    fn delay_releases_when_it_is_the_only_enabled_actor_and_stays_unfair() {
        let mut s = DelayScheduler::new(Source::Controlled(ControlledRandom::new(7)));
        assert_eq!(s.next_operation(1, &[op(1), op(2)]), Some(2));
        assert_eq!(s.next_operation(2, &[op(1)]), Some(1));
        assert!(!s.is_fair());
        assert_eq!(s.next_operation(1, &[]), None);
    }
    #[test]
    fn delay_checkpoints_replay_the_intervention_and_retain_context() {
        let mut s = DelayScheduler::new(Source::Controlled(ControlledRandom::new(7)));
        let ops = [op(1), op(2), op(3)];
        let mut expected = Vec::new();
        for _ in 0..20 {
            expected.push(s.next_operation(1, &ops));
        }
        let context = s.context();
        let (saved, count) = s.checkpoint(false).unwrap();
        assert_eq!(count, 20);
        assert_eq!(saved.context(), context);
        let (mut replay, _) = saved.checkpoint(true).unwrap();
        for choice in expected {
            assert_eq!(replay.next_operation(1, &ops), choice);
        }
        assert_eq!(replay.context(), context);
    }
    #[test]
    fn future_actor_is_delayed_when_it_becomes_runnable() {
        let mut s = DelayScheduler::new(Source::Controlled(ControlledRandom::new(7)));
        s.plan = 3;
        s.next_operation(1, &[op(1), op(2)]);
        for _ in 0..10 {
            assert_ne!(s.next_operation(2, &[op(1), op(2), op(4)]), Some(4));
        }
        assert_eq!(s.target, Some(4));
    }
}

#[cfg(test)]
mod condition_prefix_tests {
    use super::*;
    #[test]
    fn condition_checkpoint_precedes_the_unrelated_suffix() {
        let mut strategy = FeedbackGuidedStrategy::new(
            100,
            7,
            Box::new(RandomScheduler::new(Source::Controlled(ControlledRandom::new(7)))),
        )
        .with_code_guidance(true)
        .with_frontier_guidance()
        .with_condition_prefixes();
        strategy.next_bool(2);
        strategy.observe_condition(0, 42, true);
        for _ in 0..20 {
            strategy.next_bool(2);
        }
        strategy.observe_condition(0, 42, true);
        let saved = &strategy.frontiers.as_ref().unwrap().saved;
        assert_eq!(saved.len(), 1);
        assert_eq!(
            saved[0].choices, 1,
            "later executions must not replace the useful early boundary with a completed suffix"
        );
        let generator = saved[0].generator.as_ref().unwrap().borrow();
        let mut replay = generator.input.prefix(true);
        assert_eq!(replay.consumed(), 0);
        replay.next_below(2);
        assert_eq!(replay.consumed(), 1);
    }
    #[test]
    fn explicit_feedback_keeps_condition_checkpointing_opt_in() {
        let mut strategy = FeedbackGuidedStrategy::new(
            100,
            7,
            Box::new(RandomScheduler::new(Source::Controlled(ControlledRandom::new(7)))),
        )
        .with_code_guidance(true)
        .with_frontier_guidance();
        strategy.next_bool(2);
        strategy.observe_condition(0, 42, true);
        assert!(strategy.frontiers.as_ref().unwrap().saved.is_empty());
    }
}
