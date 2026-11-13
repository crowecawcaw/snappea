//! Where a test's schedules branch: which machines compete at scheduling points, and
//! which make `$`, `$$` and `choose` choices. A schedule count that grows tenfold after a
//! small model change usually traces back to one machine here, such as a forwarder
//! that adds a hop to every message or a fault injector with a large budget.
//!
//! Counts cover every schedule run, including the prefixes an exhaustive search replays,
//! so they describe an average schedule rather than the size of the search tree.

use crate::sema::Program;

/// Branching counts over the schedules of one test case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Branching {
    /// Schedules counted.
    pub schedules: u64,
    /// Scheduling points with two or more operations to choose from.
    pub points: u64,
    /// Per machine declaration: scheduling points at which one of its machines could go
    /// next, among two or more.
    pub ready: Vec<u64>,
    /// Per machine declaration: choices with two or more outcomes (`$`, `$$`, `choose`)
    /// its machines made.
    pub choices: Vec<u64>,
}

/// One machine's share of the branching, per schedule on average.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineBranching {
    pub machine: String,
    /// Scheduling points with a choice at which it could go next.
    pub ready: f64,
    /// Choices with two or more outcomes it made.
    pub choices: f64,
}

impl Branching {
    /// A scheduling point at which the machines of `decls` (with repeats) could go next.
    pub(crate) fn point(&mut self, decls: impl Iterator<Item = usize>) {
        self.points += 1;
        for d in decls {
            bump(&mut self.ready, d);
        }
    }

    /// A choice with two or more outcomes made by a machine of declaration `decl`.
    pub(crate) fn choice(&mut self, decl: usize) {
        bump(&mut self.choices, decl);
    }

    /// Adds another worker's counts.
    pub fn merge(&mut self, other: &Branching) {
        self.schedules += other.schedules;
        self.points += other.points;
        for (i, n) in other.ready.iter().enumerate() {
            add(&mut self.ready, i, *n);
        }
        for (i, n) in other.choices.iter().enumerate() {
            add(&mut self.choices, i, *n);
        }
    }

    /// Scheduling points with a choice, per schedule on average.
    pub fn points_per_schedule(&self) -> f64 {
        self.points as f64 / self.schedules.max(1) as f64
    }

    /// Every machine that branches, most branching first.
    pub fn machines(&self, prog: &Program) -> Vec<MachineBranching> {
        let per = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0) as f64 / self.schedules.max(1) as f64;
        let mut out: Vec<MachineBranching> = (0..self.ready.len().max(self.choices.len()))
            .filter(|&i| per(&self.ready, i) > 0.0 || per(&self.choices, i) > 0.0)
            .map(|i| MachineBranching {
                machine: prog.machines[i].name.clone(),
                ready: per(&self.ready, i),
                choices: per(&self.choices, i),
            })
            .collect();
        out.sort_by(|a, b| (b.ready + b.choices).total_cmp(&(a.ready + a.choices)).then(a.machine.cmp(&b.machine)));
        out
    }
}

fn bump(v: &mut Vec<u64>, i: usize) {
    add(v, i, 1);
}

fn add(v: &mut Vec<u64>, i: usize, n: u64) {
    if v.len() <= i {
        v.resize(i + 1, 0);
    }
    v[i] += n;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_merge_across_workers() {
        let mut a = Branching { schedules: 2, ..Default::default() };
        a.point([0, 1].into_iter());
        a.choice(1);
        let mut b = Branching { schedules: 2, ..Default::default() };
        b.point([1, 2].into_iter());
        a.merge(&b);
        assert_eq!(a.schedules, 4);
        assert_eq!(a.points, 2);
        assert_eq!(a.ready, vec![1, 2, 1]);
        assert_eq!(a.choices, vec![0, 1]);
        assert_eq!(a.points_per_schedule(), 0.5);
    }
}
