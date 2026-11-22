//! The reinforcement-learning scheduler (`QLearningStrategy`, from Coyote).
//!
//! Still random testing, but every choice is drawn from a softmax over learned Q values
//! of the (program state, choice) pairs. After each schedule the Q values along its path
//! are updated with a reward that is more negative the more often the reached state was
//! visited before, so later schedules lean towards choices that lead to new states.
//!
//! Program states are identified by [`crate::checker::statehash`] at scheduling points. A boolean
//! or integer choice is identified by the state of the last scheduling point plus its
//! position after it (upstream uses the hash of the last scheduling point as well).
//!
//! Upstream hashes states with .NET's per-process randomised hash codes, so its schedules
//! are not reproducible from the seed; here they are.

use rustc_hash::FxHashMap;

use crate::checker::strategy::{OpInfo, SharedRandom, Strategy};

const TRUE_CHOICE: u64 = u64::MAX;
const FALSE_CHOICE: u64 = u64::MAX - 1;
const MIN_INTEGER_CHOICE: u64 = u64::MAX - 2;
const LEARNING_RATE: f64 = 0.3;
const GAMMA: f64 = 0.7;
const BASIC_ACTION_REWARD: f64 = -1.0;

/// The Q values of one state, in insertion order (upstream enumerates a `Dictionary`).
#[derive(Default)]
struct QRow {
    index: FxHashMap<u64, usize>,
    entries: Vec<(u64, f64)>,
}

impl QRow {
    fn ensure(&mut self, key: u64) {
        if !self.index.contains_key(&key) {
            self.index.insert(key, self.entries.len());
            self.entries.push((key, 0.0));
        }
    }
    fn get(&self, key: u64) -> f64 {
        self.index.get(&key).map(|&i| self.entries[i].1).unwrap_or(0.0)
    }
    fn get_mut(&mut self, key: u64) -> &mut f64 {
        self.ensure(key);
        let i = self.index[&key];
        &mut self.entries[i].1
    }
}

pub struct QLearningStrategy {
    max_steps: usize,
    steps: usize,
    rng: SharedRandom,
    q_table: FxHashMap<u64, QRow>,
    /// (choice that led here, state) per step of the current schedule.
    path: Vec<(u64, u64)>,
    frequencies: FxHashMap<u64, u64>,
    previous: u64,
    /// State of the last scheduling point and the choices made since.
    last_state: u64,
    choices_since: u64,
}

impl QLearningStrategy {
    pub fn new(max_steps: usize, rng: SharedRandom) -> Self {
        QLearningStrategy {
            max_steps,
            steps: 0,
            rng,
            q_table: FxHashMap::default(),
            path: Vec::new(),
            frequencies: FxHashMap::default(),
            previous: 0,
            last_state: 0,
            choices_since: 0,
        }
    }

    fn capture(&mut self, state: u64) -> u64 {
        self.path.push((self.previous, state));
        *self.frequencies.entry(state).or_insert(0) += 1;
        state
    }

    /// State of a boolean or integer choice: the last scheduling point's, offset by the
    /// choice's position after it.
    fn choice_state(&mut self) -> u64 {
        self.choices_since += 1;
        let mut h = crate::checker::statehash::StableHasher::new();
        std::hash::Hasher::write_u64(&mut h, self.last_state);
        std::hash::Hasher::write_u64(&mut h, self.choices_since);
        std::hash::Hasher::finish(&h)
    }

    fn row(&mut self, state: u64) -> &mut QRow {
        self.q_table.entry(state).or_default()
    }

    /// Softmax sampling (`ChooseQValueIndexFromDistribution`). Shifted by the largest Q
    /// value, which leaves the distribution unchanged but cannot underflow to all zeros.
    fn choose_index(&mut self, q: &[f64]) -> usize {
        let max = q.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let weights: Vec<f64> = q.iter().map(|v| (v - max).exp()).collect();
        let sum: f64 = weights.iter().sum();
        let r = self.rng.borrow_mut().next_double();
        let mut cumulative = 0.0;
        for (i, w) in weights.iter().enumerate() {
            cumulative += w / sum;
            if cumulative >= r {
                return i;
            }
        }
        q.len().saturating_sub(1)
    }

    /// `LearnQValues`: updates the Q value of every step of the schedule just run.
    fn learn(&mut self) {
        for i in 0..self.path.len().saturating_sub(1) {
            let (_, state) = self.path[i];
            let (next_op, next_state) = self.path[i + 1];
            let max_q = self
                .q_table
                .get(&next_state)
                .map(|r| r.entries.iter().map(|e| e.1).fold(f64::MIN, f64::max))
                .unwrap_or(f64::MIN);
            let freq = self.frequencies.get(&next_state).copied().unwrap_or(0) as f64;
            let reward = BASIC_ACTION_REWARD * freq;
            let q = self.row(state).get_mut(next_op);
            *q = (1.0 - LEARNING_RATE) * *q + LEARNING_RATE * (reward + GAMMA * max_q);
        }
    }
}

impl Strategy for QLearningStrategy {
    fn next_operation(&mut self, _current: u64, ops: &[OpInfo]) -> Option<u64> {
        let enabled: Vec<u64> = ops.iter().filter(|o| o.enabled).map(|o| o.id).collect();
        if enabled.is_empty() {
            return None;
        }
        let state = self.capture(self.last_state);
        let row = self.row(state);
        for id in &enabled {
            row.ensure(*id);
        }
        // the Q values of the enabled operations, in the row's order
        let (ids, q): (Vec<u64>, Vec<f64>) =
            row.entries.iter().filter(|(k, _)| enabled.contains(k)).map(|&(k, v)| (k, v)).unzip();
        let next = ids[self.choose_index(&q)];
        self.previous = next;
        self.steps += 1;
        Some(next)
    }

    fn next_bool(&mut self, _max: i32) -> bool {
        let s = self.choice_state();
        let state = self.capture(s);
        let row = self.row(state);
        row.ensure(TRUE_CHOICE);
        row.ensure(FALSE_CHOICE);
        let q = [row.get(TRUE_CHOICE), row.get(FALSE_CHOICE)];
        let b = self.choose_index(&q) == 0;
        self.previous = if b { TRUE_CHOICE } else { FALSE_CHOICE };
        self.steps += 1;
        b
    }

    fn next_int(&mut self, max: i32) -> i32 {
        let s = self.choice_state();
        let state = self.capture(s);
        let n = max.max(0) as u64;
        let row = self.row(state);
        for i in 0..n {
            row.ensure(MIN_INTEGER_CHOICE - i);
        }
        let q: Vec<f64> = (0..n).map(|i| row.get(MIN_INTEGER_CHOICE - i)).collect();
        let v = if q.is_empty() { 0 } else { self.choose_index(&q) as i32 };
        self.previous = MIN_INTEGER_CHOICE - v as u64;
        self.steps += 1;
        v
    }

    fn prepare_next_iteration(&mut self) -> bool {
        self.learn();
        self.path.clear();
        self.previous = 0;
        self.last_state = 0;
        self.choices_since = 0;
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
        // a `RandomStrategy` upstream
        true
    }

    fn description(&self) -> String {
        format!("rl[seed '{}']", self.rng.borrow().seed)
    }

    fn wants_state_hash(&self) -> bool {
        true
    }

    fn observe_state(&mut self, hash: u64) -> bool {
        self.last_state = hash;
        self.choices_since = 0;
        true
    }
}
