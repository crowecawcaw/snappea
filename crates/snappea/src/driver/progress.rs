//! Progress of a running check: one snapshot type that every interface shows.
//!
//! The driver builds a [`Progress`] a few times a second while it checks test cases or
//! runs mutation testing, and hands it to [`Host::progress`]. `snappea check` shows it as
//! a status line or JSON lines (`--progress`), and the WebAssembly build passes it to
//! JavaScript. Nothing here estimates time left: a fraction is only shown against a real
//! limit (the schedule count or the time limit).
//!
//! Turn it on with [`CheckConfig::progress`](crate::driver::check::CheckConfig::progress).
//!
//! [`Host::progress`]: crate::host::Host::progress

use serde_json::{json, Value};

use crate::host::Host;

/// Seconds between two snapshots of the same run.
pub const INTERVAL: f64 = 0.25;

/// What a running check has done so far.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Progress {
    pub phase: Phase,
    /// Seconds since this phase started (the test case, or mutation testing).
    pub elapsed: f64,
    /// The time limit of this phase in seconds, if it has one.
    pub time_limit: Option<f64>,
    /// The last snapshot of this phase: its counts are final.
    pub done: bool,
}

/// What is running.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Phase {
    /// Exploring schedules of a test case.
    Check {
        test_case: String,
        /// One-based position among the test cases of the run, of `tests`.
        test: usize,
        tests: usize,
        /// Schedules started, by all workers.
        schedules: usize,
        /// The schedule limit (`--schedules`), if there is one.
        schedule_limit: Option<usize>,
        /// Bugs found so far (more than one only with `--explore`).
        bugs: usize,
        workers: usize,
    },
    /// Running passing test cases against mutants of the code.
    Mutation {
        /// Mutants detected so far.
        detected: usize,
        /// Mutants tested (not suppressed).
        mutants: usize,
    },
}

impl Progress {
    /// One line of plain text, e.g.
    /// `Checking tcMain (2/5): 12,400 schedules, 3.1s of 10s`.
    pub fn text(&self) -> String {
        let time = match self.time_limit {
            Some(limit) => format!("{} of {}", seconds(self.elapsed), seconds(limit)),
            None => seconds(self.elapsed),
        };
        match &self.phase {
            Phase::Check { test_case, test, tests, schedules, schedule_limit, bugs, .. } => {
                let position = if *tests > 1 { format!(" ({}/{})", test, tests) } else { String::new() };
                let count = match schedule_limit {
                    Some(limit) => format!("{} of {} schedules", thousands(*schedules), thousands(*limit)),
                    None => format!("{} schedule{}", thousands(*schedules), plural(*schedules)),
                };
                let bugs = match bugs {
                    0 => String::new(),
                    n => format!(", {} bug{}", n, plural(*n)),
                };
                format!("Checking {}{}: {}{}, {}", test_case, position, count, bugs, time)
            }
            Phase::Mutation { detected, mutants } => {
                format!("Mutation testing: {} of {} mutants detected, {}", detected, mutants, time)
            }
        }
    }

    /// The snapshot as JSON, e.g. `{"phase":"check","test_case":"tcMain",...}`.
    pub fn to_json(&self) -> Value {
        let mut v = match &self.phase {
            Phase::Check { test_case, test, tests, schedules, schedule_limit, bugs, workers } => json!({
                "phase": "check",
                "test_case": test_case,
                "test": test,
                "tests": tests,
                "schedules": schedules,
                "schedule_limit": schedule_limit,
                "bugs": bugs,
                "workers": workers,
            }),
            Phase::Mutation { detected, mutants } => json!({
                "phase": "mutation",
                "detected": detected,
                "mutants": mutants,
            }),
        };
        v["elapsed"] = json!(millis(self.elapsed));
        v["time_limit"] = json!(self.time_limit.map(millis));
        v["done"] = json!(self.done);
        v
    }
}

/// Where a check case sits in its run; see
/// [`CheckConfig::progress`](crate::driver::check::CheckConfig::progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// One-based.
    pub test: usize,
    pub tests: usize,
}

impl Default for Position {
    fn default() -> Self {
        Position { test: 1, tests: 1 }
    }
}

/// Builds the snapshots of one phase and sends them to a host, at most every
/// [`INTERVAL`] seconds plus the first and the last.
#[derive(Debug, Clone)]
pub(crate) struct Tracker {
    template: Progress,
    start: f64,
    /// When the last snapshot was sent; `None` before the first.
    last: Option<f64>,
}

impl Tracker {
    pub fn new(phase: Phase, start: f64, time_limit: Option<f64>) -> Tracker {
        Tracker { template: Progress { phase, elapsed: 0.0, time_limit, done: false }, start, last: None }
    }

    /// The snapshot as it stands.
    pub fn progress(&self) -> &Progress {
        &self.template
    }

    /// Updates the counts with `update` and sends a snapshot if one is due.
    pub fn tick(&mut self, host: &mut dyn Host, update: impl FnOnce(&mut Phase)) {
        update(&mut self.template.phase);
        let now = host.now();
        if self.last.is_some_and(|t| now - t < INTERVAL) {
            return;
        }
        self.last = Some(now);
        self.template.elapsed = now - self.start;
        host.progress(&self.template);
    }

    /// Sends the last snapshot, with the final counts set by `update`.
    pub fn finish(&mut self, host: &mut dyn Host, update: impl FnOnce(&mut Phase)) {
        update(&mut self.template.phase);
        self.template.elapsed = host.now() - self.start;
        self.template.done = true;
        host.progress(&self.template);
    }
}

/// Seconds rounded to milliseconds.
fn millis(s: f64) -> f64 {
    (s * 1000.0).round() / 1000.0
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// `12400` as `12,400`.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `3.14159` as `3.1s`, `10` as `10s`, `75` as `1m15s`.
fn seconds(s: f64) -> String {
    if s < 60.0 && s.fract() == 0.0 {
        format!("{}s", s)
    } else if s < 60.0 {
        format!("{:.1}s", s)
    } else {
        let whole = s as u64;
        format!("{}m{:02}s", whole / 60, whole % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(schedules: usize, limit: Option<usize>, tests: usize) -> Progress {
        Progress {
            phase: Phase::Check {
                test_case: "tcMain".into(),
                test: 2,
                tests,
                schedules,
                schedule_limit: limit,
                bugs: 0,
                workers: 8,
            },
            elapsed: 3.12,
            time_limit: Some(10.0),
            done: false,
        }
    }

    #[test]
    fn text_names_the_test_and_its_limits() {
        assert_eq!(check(12400, None, 5).text(), "Checking tcMain (2/5): 12,400 schedules, 3.1s of 10s");
        assert_eq!(check(1, Some(1000), 1).text(), "Checking tcMain: 1 of 1,000 schedules, 3.1s of 10s");
        let m = Progress {
            phase: Phase::Mutation { detected: 3, mutants: 12 },
            elapsed: 75.0,
            time_limit: None,
            done: false,
        };
        assert_eq!(m.text(), "Mutation testing: 3 of 12 mutants detected, 1m15s");
    }

    #[test]
    fn json_has_every_field() {
        let v = check(5, None, 1).to_json();
        assert_eq!(v["phase"], "check");
        assert_eq!(v["schedules"], 5);
        assert_eq!(v["schedule_limit"], Value::Null);
        assert_eq!(v["elapsed"], 3.12);
        assert_eq!(v["time_limit"], 10.0);
        assert_eq!(v["done"], false);
    }

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
