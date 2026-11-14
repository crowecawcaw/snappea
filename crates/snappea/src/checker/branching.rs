//! Where a test's schedules branch: which machines compete at scheduling points, and
//! which make `$`, `$$` and `choose` choices. A schedule count that grows tenfold after a
//! small model change usually traces back to one machine here, such as a forwarder
//! that adds a hop to every message or a fault injector with a large budget.
//!
//! Counts cover every eighth schedule run, including the prefixes an exhaustive search
//! replays, so they describe an average schedule rather than the size of the search tree.

use super::value::Value;
use crate::sema::{MachineId, Program, VarId};
use std::collections::BTreeMap;

/// Observed top-level collection lengths, aggregated across instances and sampled
/// states. These are not retained values, allocations, or a proof of monotonic growth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionSize {
    pub machine: String,
    pub field: String,
    pub file: String,
    pub line: u32,
    pub min: usize,
    pub max: usize,
    pub observations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SizeRange {
    machine: MachineId,
    min: usize,
    max: usize,
    observations: u64,
}

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
    state_points: u64,
    collection_sizes: BTreeMap<VarId, SizeRange>,
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
    /// Profile one in 32 scheduling points of the already sampled schedules.
    pub(crate) fn sample_sizes(&mut self) -> bool {
        let sample = self.state_points.is_multiple_of(32);
        self.state_points += 1;
        sample
    }

    /// Constant-time length reads only; never traverses or clones collection contents.
    pub(crate) fn observe_fields(&mut self, prog: &Program, decl: MachineId, values: &[Value]) {
        for id in &prog.machine(decl).fields {
            let field = prog.var(*id);
            let n = match values.get(field.slot as usize) {
                Some(Value::Map(v)) => v.len(),
                Some(Value::Set(v)) => v.len(),
                Some(Value::Seq(v)) => v.len(),
                _ => continue,
            };
            let range = self.collection_sizes.entry(*id).or_insert(SizeRange {
                machine: decl,
                min: n,
                max: n,
                observations: 0,
            });
            range.min = range.min.min(n);
            range.max = range.max.max(n);
            range.observations += 1;
        }
    }

    /// Nonempty collection fields, largest observed first, with stable tie ordering.
    pub fn collections(&self, prog: &Program) -> Vec<CollectionSize> {
        let mut out: Vec<_> = self
            .collection_sizes
            .iter()
            .filter(|(_, r)| r.max > 0)
            .map(|(id, r)| {
                let field = prog.var(*id);
                let loc = prog.sources.location(field.span);
                CollectionSize {
                    machine: prog.machine(r.machine).name.clone(),
                    field: field.name.clone(),
                    file: loc.file.unwrap_or_default(),
                    line: loc.line.max(0) as u32,
                    min: r.min,
                    max: r.max,
                    observations: r.observations,
                }
            })
            .collect();
        out.sort_by(|a, b| b.max.cmp(&a.max).then(a.machine.cmp(&b.machine)).then(a.field.cmp(&b.field)));
        out
    }

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
        self.state_points += other.state_points;
        for (id, r) in &other.collection_sizes {
            let a = self.collection_sizes.entry(*id).or_insert(SizeRange { observations: 0, ..r.clone() });
            a.min = a.min.min(r.min);
            a.max = a.max.max(r.max);
            a.observations += r.observations;
        }
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
