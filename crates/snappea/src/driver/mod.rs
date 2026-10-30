//! Running the checker as a command: read a project's sources through a [`Host`], check
//! test cases or logs, and report on the console and in output files.
//!
//! [`check`] checks test cases and replays schedules, [`observe`] checks recorded logs
//! against spec monitors, and [`scenario`] reads and merges the scenario coverage reports a
//! check writes. The `snappea` and `snappea-compat` command lines are both built on these.
//!
//! [`Host`]: crate::host::Host

pub mod check;
pub mod lint;
pub mod mutation;
pub mod observe;
pub mod progress;
pub mod scenario;
mod sci;

/// `{value:0.##}`.
fn format_0_00(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}
