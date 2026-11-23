//! Fixed-budget, multi-bug scheduler benchmark. JSON goes to stdout.
use serde_json::json;
use snappea::checker::engine::{Checker, CheckerOptions, Output};
use snappea::checker::strategy::StrategyKind;
use std::collections::BTreeMap;

const BUGS: [&str; 3] = ["bug-alpha", "bug-beta", "bug-gamma"];
#[derive(Default)]
struct Discoveries {
    first: BTreeMap<String, usize>,
    tested: usize,
}
impl Output for Discoveries {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
    fn schedule_finished(&mut self, schedule: usize, bug: Option<&str>) {
        self.tested = schedule;
        if let Some(bug) = bug {
            let name = BUGS.iter().find(|name| bug.contains(**name)).expect("unexpected benchmark failure");
            self.first.entry(name.to_string()).or_insert(schedule);
        }
    }
    fn cancelled(&mut self) -> bool {
        self.first.len() == BUGS.len()
    }
}

fn ladder(depth: usize, opaque: bool) -> String {
    let mut body = String::from("var target: int; var i: int; var ok: bool; target = choose(3); ok = true;");
    if opaque {
        body.push_str(&format!("i = choose({}); ok = i == 0;", 1 << depth));
    } else {
        // Branch progress is visible; the failing branch itself is not necessary for learning.
        for _ in 0..depth {
            body.push_str("if (ok) { if ($) { i = i + 1; } else { ok = false; } }");
        }
    }
    for (i, bug) in BUGS.iter().enumerate() {
        body.push_str(&format!("if (ok && target == {i}) {{ assert false, \"{bug}\"; }}"));
    }
    format!("machine Main {{ start state Init {{ entry {{ {body} }} }} }} test tcMain [main=Main]: {{ Main }};")
}

fn deliveries(n: usize) -> String {
    let mut create = String::new();
    for x in 0..n {
        create.push_str(&format!("new Client((s=s, x={x}));"));
    }
    let mut checks = String::new();
    // Distinct delivery orders: ascending, descending and one rotation.
    for (b, bug) in BUGS.iter().enumerate() {
        let terms = (0..n)
            .map(|i| {
                let value = match b {
                    0 => i,
                    1 => n - 1 - i,
                    _ => (i + 1) % n,
                };
                format!("got[{i}] == {value}")
            })
            .collect::<Vec<_>>()
            .join(" && ");
        checks.push_str(&format!("if ({terms}) {{ assert false, \"{bug}\"; }}"));
    }
    format!(
        r#"event Hello: int;
      machine Server {{ var got: seq[int]; start state Init {{ on Hello do (x: int) {{
        got += (sizeof(got), x); if (sizeof(got) == {n}) {{ {checks} }}
      }} }} }}
      machine Client {{ start state Init {{ entry (p: (s: Server, x: int)) {{ send p.s, Hello, p.x; }} }} }}
      machine Main {{ start state Init {{ entry {{ var s: Server; s = new Server(); {create} }} }} }}
      test tcMain [main=Main]: {{ Main, Server, Client }};"#
    )
}

fn main() {
    let seeds: u32 = std::env::var("SEARCH_SEEDS").unwrap_or("20".into()).parse().unwrap();
    let seed_start: u32 = std::env::var("SEARCH_SEED_START").unwrap_or("0".into()).parse().unwrap();
    let budget: usize = std::env::var("SEARCH_BUDGET").unwrap_or("10000".into()).parse().unwrap();
    assert!(seeds > 0 && budget > 0);
    let mut rows = Vec::new();
    for (name, source) in [
        ("branch-8", ladder(8, false)),
        ("branch-12", ladder(12, false)),
        ("opaque-8", ladder(8, true)),
        ("delivery-4", deliveries(4)),
        ("delivery-5", deliveries(5)),
    ] {
        let mut sm = snappea::syntax::SourceMap::new();
        sm.add("benchmark.p", source);
        let compiled = snappea::sema::compile(&sm, &Default::default()).expect("benchmark compiles");
        let tc = compiled.program.test_cases().remove(0);
        let checker = Checker::with_code_coverage(&compiled.program);
        let mut strategies = vec![
            ("random", StrategyKind::Random),
            ("feedback", StrategyKind::Feedback),
            ("dfs", StrategyKind::Dfs),
            ("random-dfs", StrategyKind::RandomDfs),
            ("auto", StrategyKind::Auto),
        ];
        if std::env::var("SEARCH_PLAIN_FEEDBACK").as_deref() == Ok("1") {
            strategies.insert(2, ("feedback-plain", StrategyKind::Feedback));
        }
        for (label, strategy) in strategies {
            for seed in seed_start..seed_start.checked_add(seeds).expect("seed range overflow") {
                let start = std::time::Instant::now();
                let mut out = Discoveries::default();
                let result = checker
                    .check(
                        &tc,
                        &CheckerOptions {
                            strategy,
                            // `feedback` and `auto` measure the defaults (checkpoints on);
                            // `feedback-plain` turns them off for comparison.
                            feedback_frontier_guidance: label != "feedback-plain",
                            seed,
                            iterations: budget,
                            steps: snappea::checker::runtime::StepBounds {
                                unfair: 1000,
                                fair: 1000,
                                fair_explicit: false,
                            },
                            full_exploration: true,
                            activity_coverage: true,
                            ..Default::default()
                        },
                        &mut out,
                    )
                    .unwrap();
                let all = (out.first.len() == BUGS.len()).then(|| *out.first.values().max().unwrap());
                rows.push(json!({"workload":name, "strategy":label, "seed":seed,
                    "elapsed_seconds":start.elapsed().as_secs_f64(),
                    "schedules_tested":out.tested, "first_bug_schedules":out.first,
                    "schedules_to_all":all, "exhausted":result.exhaustive.is_some_and(|s| s.finished)}));
            }
        }
    }
    println!("{}", json!({"suite_version":1, "budget":budget, "seeds":seeds, "seed_start":seed_start, "rows":rows}));
}
