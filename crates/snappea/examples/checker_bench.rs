//! Fixed-oracle checker benchmark. The Python runner supplies a frozen manifest.
use serde_json::{json, Value};
use snappea::checker::{
    engine::{Checker, CheckerOptions, Output},
    strategy::StrategyKind,
};
use std::{collections::BTreeMap, time::Instant};
struct Discoveries {
    expected: Vec<(String, String)>,
    first: BTreeMap<String, usize>,
    tested: usize,
    start: Instant,
    seconds: f64,
}
impl Output for Discoveries {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, s: &str) {
        panic!("checker error: {s}")
    }
    fn schedule_finished(&mut self, n: usize, bug: Option<&str>) {
        self.tested = n;
        if let Some(bug) = bug {
            let matches: Vec<_> = self
                .expected
                .iter()
                .filter(|(_, pattern)| {
                    if bug.starts_with("Assertion Failed: ") {
                        bug.ends_with(pattern)
                    } else {
                        bug.contains(pattern)
                    }
                })
                .collect();
            assert_eq!(matches.len(), 1, "failure absent or ambiguous in independent oracle: {bug}");
            self.first.entry(matches[0].0.clone()).or_insert(n);
        }
    }
    fn cancelled(&mut self) -> bool {
        (!self.expected.is_empty() && self.first.len() == self.expected.len())
            || self.start.elapsed().as_secs_f64() >= self.seconds
    }
}
fn main() {
    let path = std::env::args().nth(1).expect("usage: checker_bench RUN_MANIFEST.json");
    let run: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let budget = run["budget"].as_u64().unwrap() as usize;
    for case in run["cases"].as_array().unwrap() {
        let budget = case["budget"].as_u64().map_or(budget, |n| n as usize);
        let mut sm = snappea::syntax::SourceMap::new();
        sm.add(format!("{}.p", case["name"].as_str().unwrap()), case["source"].as_str().unwrap());
        let compiled =
            snappea::sema::compile(&sm, &Default::default()).unwrap_or_else(|e| panic!("{}: {e:?}", case["name"]));
        let tc = compiled.program.test_cases().remove(0);
        let checker = Checker::with_code_coverage(&compiled.program);
        for label in run["strategies"].as_array().unwrap() {
            let label = label.as_str().unwrap();
            let (strategy, depth) = if let Some(depth) = label.strip_prefix("pct-") {
                (StrategyKind::Pct, depth.parse().unwrap())
            } else {
                (
                    match label {
                        "feedback" => StrategyKind::Feedback,
                        "random" => StrategyKind::Random,
                        "dfs" => StrategyKind::Dfs,
                        _ => StrategyKind::Auto,
                    },
                    0,
                )
            };
            for seed in run["seed_start"].as_u64().unwrap()
                ..run["seed_start"].as_u64().unwrap() + run["seeds"].as_u64().unwrap()
            {
                let expected = case["expected"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| (x["id"].as_str().unwrap().into(), x["match"].as_str().unwrap().into()))
                    .collect::<Vec<_>>();
                let mut out = Discoveries {
                    expected,
                    first: Default::default(),
                    tested: 0,
                    start: Instant::now(),
                    seconds: run["seconds"].as_f64().unwrap(),
                };
                let result = checker
                    .check(
                        &tc,
                        &CheckerOptions {
                            strategy,
                            strategy_bound: depth,
                            seed: seed as u32,
                            iterations: budget,
                            max_unfair_steps: run["steps"].as_u64().unwrap() as usize,
                            max_fair_steps: run["steps"].as_u64().unwrap() as usize,
                            full_exploration: true,
                            activity_coverage: true,
                            ..Default::default()
                        },
                        &mut out,
                    )
                    .unwrap();
                let all = (!out.expected.is_empty() && out.first.len() == out.expected.len())
                    .then(|| *out.first.values().max().unwrap());
                println!(
                    "{}",
                    json!({"budget":budget,"case":case["name"],"family":case["family"],"split":case["split"],"strategy":label,"seed":seed,"expected":out.expected.iter().map(|(id,_)|id).collect::<Vec<_>>(),"first":out.first,"schedules":out.tested,"schedules_to_all":all,"seconds":out.start.elapsed().as_secs_f64(),"time_censored":all.is_none() && out.start.elapsed().as_secs_f64()>=out.seconds,"exhausted":result.exhaustive.as_ref().is_some_and(|s|s.finished),"allocation":result.scheduler_stats.iter().map(|a|json!({"name":a.name,"runs":a.schedules,"work":a.work,"reward":a.reward})).collect::<Vec<_>>()})
                );
            }
        }
    }
}
