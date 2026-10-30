//! Keep the multi-error benchmark's fixtures runnable as the language evolves.
use snappea::checker::engine::{Checker, CheckerOptions, Output};
use snappea::checker::strategy::StrategyKind;
use std::path::Path;
struct Quiet;
impl Output for Quiet {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, s: &str) {
        panic!("{s}")
    }
}
#[test]
fn checker_benchmark_fixtures_compile_and_safe_controls_stay_safe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/checker");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let source = std::fs::read_to_string(root.join(case["path"].as_str().unwrap())).unwrap();
        let mut sm = snappea::syntax::SourceMap::new();
        sm.add("fixture.p", source);
        let compiled =
            snappea::sema::compile(&sm, &Default::default()).unwrap_or_else(|e| panic!("{}: {e:?}", case["name"]));
        let tests = compiled.program.test_cases();
        assert_eq!(tests.len(), 1);
        if case["expected"].as_array().unwrap().is_empty() {
            let result = Checker::new(&compiled.program)
                .check(
                    &tests[0],
                    &CheckerOptions {
                        strategy: StrategyKind::Auto,
                        iterations: 256,
                        max_fair_steps: 100,
                        max_unfair_steps: 100,
                        ..Default::default()
                    },
                    &mut Quiet,
                )
                .unwrap();
            assert_eq!(result.report.num_bugs, 0, "{}", case["name"]);
            assert_eq!(result.exhaustive.is_some_and(|s| s.finished), case["expect_exhausted"].as_bool().unwrap());
        }
    }
}
