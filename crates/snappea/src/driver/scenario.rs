//! Scenario coverage artifacts (`ScenarioCoverageMerger`): the per-test-case
//! `*_scenario_coverage.json` written by a check, and merging them.

use std::path::{Path, PathBuf};

use crate::checker::code_coverage::Uncovered;
use crate::checker::engine::{culture_order, TestReport};
use crate::checker::jsonlog::Json;

use crate::host::{Host, HostExt, Stream};
use crate::project::pproj::list_files;

/// Suffix of upstream's report files (`<project>_<test>_scenario_coverage.json`).
pub const FILE_SUFFIX: &str = "_scenario_coverage.json";
/// Name of the report file in a `snappea check` test case folder.
pub const SNAPPEA_FILE_NAME: &str = "scenario_coverage.json";
pub const SCHEMA_VERSION: i64 = 1;

/// One scenario's entry (`ScenarioCoverageEntry`).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub satisfied: bool,
    pub satisfying_schedules: i64,
    pub distinct_satisfying_timelines: i64,
    pub max_states_visited: i64,
    pub monitor_states: i64,
}

/// `ScenarioCoverageArtifact`.
#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    pub version: i64,
    pub test_case: String,
    pub scenarios: Vec<Entry>,
}

/// `FromReport`.
pub fn from_report(report: &TestReport, test_case: &str) -> Artifact {
    Artifact {
        version: SCHEMA_VERSION,
        test_case: test_case.to_string(),
        scenarios: report
            .sorted_scenarios()
            .into_iter()
            .map(|s| Entry {
                name: s.name.clone(),
                satisfied: s.trigger_count > 0,
                satisfying_schedules: s.trigger_count as i64,
                distinct_satisfying_timelines: s.satisfying_timelines.len() as i64,
                max_states_visited: s.max_states_reached as i64,
                monitor_states: s.total_states as i64,
            })
            .collect(),
    }
}

impl Artifact {
    /// Indented camelCase JSON, as `System.Text.Json` writes it.
    pub fn to_json(&self) -> String {
        Json::Obj(vec![
            ("version".into(), Json::Int(self.version)),
            ("testCase".into(), Json::str(self.test_case.clone())),
            (
                "scenarios".into(),
                Json::Arr(
                    self.scenarios
                        .iter()
                        .map(|e| {
                            Json::Obj(vec![
                                ("name".into(), Json::str(e.name.clone())),
                                ("satisfied".into(), Json::Bool(e.satisfied)),
                                ("satisfyingSchedules".into(), Json::Int(e.satisfying_schedules)),
                                ("distinctSatisfyingTimelines".into(), Json::Int(e.distinct_satisfying_timelines)),
                                ("maxStatesVisited".into(), Json::Int(e.max_states_visited)),
                                ("monitorStates".into(), Json::Int(e.monitor_states)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
        .to_pretty()
    }

    pub fn parse(text: &str) -> Result<Artifact, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let int = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        Ok(Artifact {
            version: int(&v, "version"),
            test_case: v.get("testCase").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            scenarios: v
                .get("scenarios")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .map(|e| Entry {
                            name: e.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            satisfied: e.get("satisfied").and_then(|x| x.as_bool()).unwrap_or(false),
                            satisfying_schedules: int(e, "satisfyingSchedules"),
                            distinct_satisfying_timelines: int(e, "distinctSatisfyingTimelines"),
                            max_states_visited: int(e, "maxStatesVisited"),
                            monitor_states: int(e, "monitorStates"),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

/// `Merge`: the unified, human-readable report.
pub fn merge(artifacts: &[Artifact]) -> String {
    #[derive(Default)]
    struct Agg {
        triggered: i64,
        timelines: i64,
        cases_total: i64,
        covering: Vec<String>,
        best: i64,
        total: i64,
    }
    let mut aggs: Vec<(String, Agg)> = Vec::new();
    for a in artifacts {
        let tc = if a.test_case.is_empty() { "(default)".to_string() } else { a.test_case.clone() };
        for s in &a.scenarios {
            let i = match aggs.iter().position(|(n, _)| *n == s.name) {
                Some(i) => i,
                None => {
                    aggs.push((s.name.clone(), Agg::default()));
                    aggs.len() - 1
                }
            };
            let g = &mut aggs[i].1;
            g.triggered += s.satisfying_schedules;
            g.timelines += s.distinct_satisfying_timelines;
            g.cases_total += 1;
            if s.satisfied {
                g.covering.push(tc.clone());
            }
            if s.max_states_visited > g.best {
                g.best = s.max_states_visited;
            }
            if s.monitor_states > 0 {
                g.total = s.monitor_states;
            }
        }
    }
    aggs.sort_by(|a, b| culture_order(&a.0, &b.0));
    let covered: Vec<&(String, Agg)> = aggs.iter().filter(|(_, g)| g.triggered > 0).collect();
    let gaps: Vec<&(String, Agg)> = aggs.iter().filter(|(_, g)| g.triggered == 0).collect();
    let n = artifacts.len();
    let mut out = format!("Unified scenario coverage across {} test case{}:\n", n, if n == 1 { "" } else { "s" });
    out.push_str(&format!(
        "  {}/{} scenarios covered in >=1 test case{}\n",
        covered.len(),
        aggs.len(),
        if gaps.is_empty() {
            ".".to_string()
        } else {
            format!("; {} coverage gap{}.", gaps.len(), if gaps.len() == 1 { "" } else { "s" })
        }
    ));
    for (name, g) in covered {
        out.push_str(&format!(
            "  [covered] {}: covered in {}/{} test cases ({}); {} total triggers, {} satisfying timelines (summed per test case)\n",
            name,
            g.covering.len(),
            g.cases_total,
            g.covering.join(", "),
            g.triggered,
            g.timelines
        ));
    }
    for (name, g) in gaps {
        out.push_str(&format!(
            "  [  GAP  ] {}: never covered in any of {} test case{}",
            name,
            g.cases_total,
            if g.cases_total == 1 { "" } else { "s" }
        ));
        if g.total > 0 {
            out.push_str(&format!("; best progress anywhere {}/{} states", g.best, g.total));
        }
        out.push('\n');
    }
    out
}

/// The scenarios no test case of `artifacts` satisfied, as coverage gaps at their
/// declarations: those no `coverage` suppression (scope and reason) accepts, and those one
/// does, the latter with `(suppressed: <reason>)` after what was missed.
pub fn gaps(
    prog: &crate::sema::Program,
    artifacts: &[Artifact],
    suppressions: &[(crate::syntax::Span, String)],
) -> (Vec<Uncovered>, Vec<Uncovered>) {
    let (mut open, mut accepted) = (Vec::new(), Vec::new());
    for m in prog.machines.iter().filter(|m| m.is_scenario() && !m.span.is_dummy()) {
        let entries = artifacts.iter().flat_map(|a| &a.scenarios).filter(|e| e.name == m.cs_name);
        let (mut ran, mut satisfied) = (0, false);
        for e in entries {
            ran += 1;
            satisfied |= e.satisfied;
        }
        if ran == 0 || satisfied {
            continue;
        }
        let loc = prog.sources.location(m.span);
        let line = loc.line.max(0) as u32;
        let mut gap = Uncovered {
            file: loc.file.unwrap_or_default(),
            line,
            end_line: line,
            context: m.name.clone(),
            what: format!("scenario never satisfied in {} test case{}", ran, if ran == 1 { "" } else { "s" }),
        };
        let reason = suppressions
            .iter()
            .find(|(scope, _)| scope.file == m.span.file && scope.start <= m.span.start && m.span.start < scope.end);
        match reason {
            Some((_, reason)) => {
                gap.what = format!("{} (suppressed: {})", gap.what, reason);
                accepted.push(gap);
            }
            None => open.push(gap),
        }
    }
    (open, accepted)
}

/// Whether a path lies in a rotated history directory (`<Mode>0`..`<Mode>9`).
fn in_history_dir(p: &Path) -> bool {
    p.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        let t = s.trim_end_matches(|ch: char| ch.is_ascii_digit());
        t.len() < s.len() && (t == "BugFinding" || t == "PEx")
    })
}

/// Merges the scenario coverage reports under `dir` (absolute), recursively, and prints the
/// merged report. For each test case the report in the current run's folder wins over those
/// in rotated history folders; unreadable reports are skipped with a message. Upstream
/// reads `*_scenario_coverage.json` files; `snappea_files` adds `snappea check`'s
/// `scenario_coverage.json`.
/// Prints the merge of the scenario artifacts under `dir`; returns them as JSON.
pub fn merge_dir(host: &mut dyn Host, dir: &Path, snappea_files: bool) -> Vec<serde_json::Value> {
    let mut files: Vec<PathBuf> = Vec::new();
    list_files(host, dir, None, &mut files);
    // latest artifact per test case: the current run's directory wins over history ones
    let mut latest: Vec<(String, Artifact, bool)> = Vec::new();
    for f in files {
        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let ours = name.ends_with(FILE_SUFFIX) || (snappea_files && name == SNAPPEA_FILE_NAME);
        if !ours {
            continue;
        }
        let parsed = host.read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| Artifact::parse(&t));
        let a = match parsed {
            Ok(a) => a,
            Err(e) => {
                host.out_line(&format!("... Skipping unreadable scenario artifact {}: {}", f.display(), e));
                continue;
            }
        };
        if a.version > SCHEMA_VERSION {
            host.out_line(&format!(
                "... Skipping scenario artifact {}: schema version {} is newer than this tool supports (v{}); upgrade P to merge it.",
                f.display(),
                a.version,
                SCHEMA_VERSION
            ));
            continue;
        }
        let current = !in_history_dir(&f);
        match latest.iter().position(|(k, _, _)| *k == a.test_case) {
            None => latest.push((a.test_case.clone(), a, current)),
            Some(i) => {
                if current || !latest[i].2 {
                    latest[i] = (a.test_case.clone(), a, current);
                }
            }
        }
    }
    let arts: Vec<Artifact> = latest.into_iter().map(|(_, a, _)| a).collect();
    host.write(Stream::Out, &merge(&arts), None);
    arts.iter().filter_map(|a| serde_json::from_str(&a.to_json()).ok()).collect()
}
