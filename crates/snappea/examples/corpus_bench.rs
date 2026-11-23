//! Streams one measurement per test/strategy/seed from a repository corpus manifest.
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use snappea::checker::engine::{failure_site, Checker, CheckerOptions, Output};
use snappea::checker::strategy::StrategyKind;
use snappea::host::MemHost;

struct Observer {
    first: BTreeMap<String, usize>,
    tested: usize,
    deadline: Instant,
}
impl Output for Observer {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
    fn schedule_finished(&mut self, n: usize, bug: Option<&str>) {
        self.tested = n;
        if let Some(bug) = bug {
            self.first.entry(failure_site(bug)).or_insert(n);
        }
    }
    fn cancelled(&mut self) -> bool {
        Instant::now() >= self.deadline
    }
}
fn emit(value: Value) {
    println!("{value}");
    std::io::stdout().flush().unwrap();
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let project = &manifest["projects"][args[2].parse::<usize>().unwrap()];
    let root = manifest["root"].as_str().unwrap();
    let mut host = MemHost::new(root);
    for (name, contents) in manifest["files"].as_object().unwrap() {
        host.add_file(name, contents.as_str().unwrap());
    }
    let input = snappea::project::Input {
        pproj: project["pproj"].as_str().map(str::to_string),
        pfiles: project["pfiles"]
            .as_array()
            .map(|v| v.iter().map(|x| x.as_str().unwrap().into()).collect())
            .unwrap_or_default(),
        ..Default::default()
    };
    let resolved = match snappea::project::resolve(&mut host, &input) {
        Ok(r) => r,
        Err(_) => {
            emit(json!({"kind":"project", "status":"resolve_error", "detail":host.stderr, "stdout":host.stdout}));
            return;
        }
    };
    let sources = snappea::project::read(&host, &resolved.project_name, &resolved.p_files, Path::new(root)).unwrap();
    let sm = &sources.map;
    let compiled = match snappea::sema::compile(sm, &Default::default()) {
        Ok(a) => a,
        Err(error) => {
            let detail = match error {
                snappea::sema::CompileError::Analysis(a) => a.messages(sm).join("\n"),
                e => format!("{e:?}"),
            };
            emit(json!({"kind":"project", "status":"compile_error", "detail":detail}));
            return;
        }
    };
    let cases = compiled.program.test_cases();
    let checker = Checker::with_code_coverage(&compiled.program);
    let has_hot = compiled.program.states.iter().any(|s| s.temperature == snappea::sema::Temperature::Hot);
    let names: Vec<_> = cases.iter().map(|tc| &tc.name).collect();
    let monitors: BTreeMap<_, Vec<_>> = cases
        .iter()
        .map(|tc| {
            let (module, _) = compiled.program.test_case_module(tc);
            (
                tc.name.clone(),
                module.monitor_map.iter().map(|(id, _)| compiled.program.machine(*id).name.clone()).collect(),
            )
        })
        .collect();
    emit(json!({"kind":"project", "status":"ready", "cases":names,
        "case_monitors":monitors,
        "sources":sources.map.files().iter().map(|f| &f.name).collect::<Vec<_>>(), "has_hot":has_hot,
        "foreign_files":resolved.foreign_files}));
    let budget = manifest["budget"].as_u64().unwrap() as usize;
    let steps = manifest["steps"].as_u64().unwrap() as usize;
    let seconds = manifest["seconds_per_run"].as_f64().unwrap();
    for tc in cases {
        for name in manifest["strategies"].as_array().unwrap() {
            let label = name.as_str().unwrap();
            // `feedback` and `auto` measure the defaults (checkpoints on); `feedback-plain`
            // turns checkpoints off.
            let frontier_guidance = label != "feedback-plain";
            let strategy: StrategyKind =
                if label.starts_with("feedback-") { StrategyKind::Feedback } else { label.parse().unwrap() };
            let seed_start = manifest["seed_start"].as_u64().unwrap_or(0);
            for seed in seed_start..seed_start + manifest["seeds"].as_u64().unwrap() {
                let seed = seed as u32;
                let start = Instant::now();
                let mut out =
                    Observer { first: BTreeMap::new(), tested: 0, deadline: start + Duration::from_secs_f64(seconds) };
                let result = checker.check(
                    &tc,
                    &CheckerOptions {
                        strategy,
                        feedback_frontier_guidance: frontier_guidance,
                        seed,
                        iterations: budget,
                        max_fair_steps: steps,
                        max_unfair_steps: steps,
                        full_exploration: true,
                        activity_coverage: true,
                        ..Default::default()
                    },
                    &mut out,
                );
                match result {
                    Ok(r) => {
                        let certified = !r.cancelled && r.exhaustive.as_ref().is_some_and(|s| s.complete());
                        emit(json!({"kind":"run", "case":tc.name, "strategy":label, "seed":seed,
                            "schedules_tested":out.tested, "first_issue_schedules":out.first,
                            "cancelled":r.cancelled, "exhaustive_complete":certified,
                            "exhaustive":r.exhaustive.map(|s| json!({"finished":s.finished, "depth_bound_hits":s.depth_bound_hits, "cycle_prunes":s.cycle_prunes})),
                            "fair_bound_hits":r.report.max_fair_steps_hit_in_fair,
                            "unfair_bound_hits":r.report.max_unfair_steps_hit_in_unfair,
                            "elapsed_seconds":start.elapsed().as_secs_f64(), "status":"ok"}));
                    }
                    Err(error) => emit(
                        json!({"kind":"run", "case":tc.name, "strategy":label, "seed":seed, "status":"engine_error", "detail":error}),
                    ),
                }
            }
        }
    }
}
