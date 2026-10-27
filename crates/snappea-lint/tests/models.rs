use snappea_lint::{lint, Context, Finding, Registry, Rule};
use snappea_syntax::SourceMap;

fn findings(files: &[&str]) -> Vec<Finding> {
    let mut sm = SourceMap::new();
    for (i, text) in files.iter().enumerate() {
        sm.add(format!("model{i}.p"), *text);
    }
    lint(&sm).unwrap()
}
fn has(text: &str, id: &str) -> bool {
    findings(&[text]).iter().any(|f| f.rule == id)
}
const PASSIVE: &str = "event E; spec Passive observes E { start state S { on E goto S; } }";
const SAFE: &str = "event E: (actual: int, expected: int); spec Guard observes E { start state S { on E do (p: (actual: int, expected: int)) { assert p.actual == p.expected; } } } machine Main { start state S {} } machine Worker { start state S {} } test t [main=Main]: assert Guard in {Main, Worker};";

#[test]
fn frozen_agent_models_expose_the_observed_bad_patterns() {
    let p01 = findings(&[include_str!("models/agent-p01.p")]);
    assert!(p01.iter().any(|f| f.rule == "tautological-assert" && f.span.start > 0));
    assert!(p01.iter().any(|f| f.rule == "unselected-spec"));
    let p03 = findings(&[include_str!("models/agent-p03.p")]);
    assert!(p03.iter().any(|f| f.rule == "unused-payload-field" && f.message.contains("p.digest")));
    let p06 = findings(&[include_str!("models/agent-p06.p")]);
    assert!(p06.iter().any(|f| f.rule == "permissive-monitor" && f.message.contains("BundleTrace")));
    assert!(p06.iter().any(|f| f.rule == "unselected-spec" && f.message.contains("AtomicCommitCheck")));
}

#[test]
fn independent_observation_monitor_is_clean() {
    assert!(findings(&[SAFE]).is_empty());
}

#[test]
fn permissive_requires_total_handling_and_no_progress_obligation() {
    assert!(has(PASSIVE, "permissive-monitor"));
    assert!(has(
        "event E; spec S observes E { start state A { on E goto B; } state B { ignore E; } }",
        "permissive-monitor"
    ));
    assert!(!has("event E; spec S observes E { start state A { on E goto B; } state B {} }", "permissive-monitor"));
    assert!(!has("event E; spec S observes E { start hot state A { on E goto A; } }", "permissive-monitor"));
    assert!(!has("event E; spec S observes E { start state A { on E do { assert false; } } }", "permissive-monitor"));
}

#[test]
fn named_handlers_are_inspected() {
    assert!(has(
        "event E; spec S observes E { fun Empty() {} start state A { on E do Empty; } }",
        "permissive-monitor"
    ));
    assert!(!has(
        "event E; spec S observes E { fun Guard() { assert false; } start state A { on E do Guard; } }",
        "permissive-monitor"
    ));
    let text = "type Payload = (digest: string, rev: int); event E: Payload; spec S observes E { fun Guard(p: Payload) { assert p.rev == 1; } start state A { on E do Guard; } }";
    assert!(findings(&[text]).iter().any(|f| f.rule == "unused-payload-field" && f.message.contains("digest")));
}

#[test]
fn selection_resolves_modules_across_files_and_runnable_implementations() {
    let spec =
        "event E; spec S observes E { start state A { on E do { assert false; } } } machine Main { start state A {} }";
    let module = "module Base = { Main }; module Guarded = assert S in Base;";
    assert!(!findings(&[spec, module, "test t [main=Main]: Guarded;"]).iter().any(|f| f.rule == "unselected-spec"));
    assert!(!findings(&[spec, module, "implementation i [main=Main]: Guarded;"])
        .iter()
        .any(|f| f.rule == "unselected-spec"));
    assert!(findings(&[spec, module, "test t [main=Main]: Base;"]).iter().any(|f| f.rule == "unselected-spec"));
}

#[test]
fn no_model_machine_checks_each_runnable_configuration() {
    assert!(has("machine Main { start state S {} } test t [main=Main]: { Main };", "no-model-machine"));
    assert!(has(
        "event E; spec Watch observes E { start state S { on E goto S; } } machine Main { start state S {} } test t [main=Main]: assert Watch in { Main };",
        "no-model-machine"
    ));
    assert!(!has(
        "machine Main { start state S {} } machine Model { start state S {} } test t [main=Main]: { Main, Model };",
        "no-model-machine"
    ));
    // A declared implementation outside this test module is not selected behavior.
    assert!(has(
        "machine Main { start state S {} } machine Model { start state S {} } test t [main=Main]: { Main };",
        "no-model-machine"
    ));
    assert!(has("machine Main { start state S {} } implementation i [main=Main]: { Main };", "no-model-machine"));
    assert!(!has(
        "machine Main { start state S {} } machine Model { start state S {} } implementation i [main=Main]: { Main, Model };",
        "no-model-machine"
    ));
    // Implementations without a checker entry are not runnable test cases.
    assert!(!has("machine Main { start state S {} } implementation i: { Main };", "no-model-machine"));
}

#[test]
fn no_model_machine_resolves_aliases_compositions_and_wrappers() {
    let source = "machine Main { start state S {} } machine Model { start state S {} } module MainModule = { Main }; module ModelModule = { Model }; module Base = compose MainModule, ModelModule; module Alias = main Model in Base; test t [main=Main]: Alias;";
    assert!(!has(source, "no-model-machine"));

    let source = "machine Main { start state S {} } machine Model { start state S {} } module Base = { Main, Model }; module Alias = Base; test t [main=Main]: Alias;";
    assert!(!has(source, "no-model-machine"));

    let files = [
        "machine Main { start state S {} } machine Model { start state S {} }",
        "module Base = { Main, Model };",
        "module Alias = main Model in Base; test t [main=Main]: Alias;",
    ];
    let fs = findings(&files);
    assert!(!fs.iter().any(|f| f.rule == "no-model-machine"));
}

#[test]
fn no_model_machine_is_per_test_and_scoped_suppression_does_not_leak() {
    let two_tests = "event E; spec Watch observes E { start state S { on E goto S; } } machine Main { start state S {} } machine Other { start state S {} } test trace [main=Main]: assert Watch in { Main }; test modeled [main=Other]: { Main, Other };";
    let fs = findings(&[two_tests]);
    assert_eq!(fs.iter().filter(|f| f.rule == "no-model-machine").count(), 1);

    let suppressed = "machine Main { start state S {} }\n// snappea-lint: allow no-model-machine -- fixed trace fixture\ntest trace [main=Main]: { Main };\ntest sibling [main=Main]: { Main };";
    let fs = findings(&[suppressed]);
    assert_eq!(fs.iter().filter(|f| f.rule == "no-model-machine").count(), 1);
    assert!(!fs.iter().any(|f| f.rule == "invalid-suppression"));

    let implementation = "machine Main { start state S {} }\n// snappea-lint: allow no-model-machine -- trace-only implementation fixture\nimplementation trace [main=Main]: { Main };\nimplementation sibling [main=Main]: { Main };";
    let fs = findings(&[implementation]);
    assert_eq!(fs.iter().filter(|f| f.rule == "no-model-machine").count(), 1);
    assert!(!fs.iter().any(|f| f.rule == "invalid-suppression"));
}

#[test]
fn no_model_machine_uses_checker_implicit_main_conditions() {
    assert!(has("machine Main { start state S {} }", "no-model-machine"));
    let suppressed =
        "// snappea-lint: allow no-model-machine -- trace-only default model\nmachine Main { start state S {} }";
    assert!(!findings(&[suppressed]).iter().any(|f| f.rule == "no-model-machine"));
    assert!(!has("machine Main { start state S {} } machine Worker { start state S {} }", "no-model-machine"));
    // Once a test exists, the checker does not synthesize the default implementation.
    assert!(!has(
        "machine Main { start state S {} } machine Worker { start state S {} } test t [main=Main]: { Main, Worker };",
        "no-model-machine"
    ));
    // Observation-only sources without a runnable test do not invent a Main test.
    assert!(!has("event E; spec Watch observes E { start state S { on E goto S; } }", "no-model-machine"));
}

fn block(body: &str) -> String {
    format!("machine Main {{ var x: int; var y: int; fun Mutate(): bool {{ x = 3; return true; }} start state S {{ entry {{ {body} }} }} }}")
}

#[test]
fn tautology_tracks_scalar_values_and_forgets_uncertain_writes() {
    assert!(has(&block("x = y; assert x == y;"), "tautological-assert"));
    assert!(has(&block("x = 1; y = x; assert x == y;"), "tautological-assert"));
    assert!(has(&block("assert true;"), "tautological-assert"));
    assert!(!has(&block("x = y; y = 3; assert x == y;"), "tautological-assert"));
    assert!(!has(&block("x = 1; x = y; assert x == 1;"), "tautological-assert"));
    assert!(!has(&block("x = 1; if ($) { x = 2; } assert x == 1;"), "tautological-assert"));
    assert!(!has(&block("x = 1; if (Mutate()) { assert x == 1; }"), "tautological-assert"));
    assert!(!has(&block("x = 1; while ($) { x = 2; } assert x == 1;"), "tautological-assert"));
    assert!(!has("machine Main { var x: float; start state S { entry { assert x == x; } } }", "tautological-assert"));
    assert!(!has(&block("assert false;"), "tautological-assert"));
}

#[test]
fn tuple_assignments_do_not_make_payload_look_read_and_aliases_are_conservative() {
    let model = |body| {
        format!("event E: (a: int, b: int); spec S observes E {{ start state A {{ on E do (p: (a: int, b: int)) {{ {body} }} }} }}")
    };
    let fs = findings(&[&model("p.b = 1; assert p.a == 2;")]);
    assert!(fs.iter().any(|f| f.rule == "unused-payload-field" && f.message.contains("p.b")));
    assert!(!has(&model("var copy: (a: int, b: int); copy = p; assert copy.a == 2;"), "unused-payload-field"));
}

#[test]
fn suppression_is_scoped_and_unknown_ids_fail() {
    let suppressed = format!("// snappea-lint: allow permissive-monitor, unselected-spec -- trace fixture\n{PASSIVE}");
    // The comment precedes event E, not the spec, so it must not silently work.
    assert!(has(&suppressed, "invalid-suppression"));
    let suppressed = "event E;\n// snappea-lint: allow permissive-monitor, unselected-spec -- trace fixture\nspec Passive observes E { start state S { on E goto S; } }";
    assert!(findings(&[suppressed]).is_empty());
    let text = block("// snappea-lint: allow tautological-assert -- test fixture\nassert true; assert true;");
    // Directive is trailing code on its line: it must fail rather than over-suppress.
    assert!(has(&text, "invalid-suppression"));
    let text = block("\n// snappea-lint: allow tautological-assert -- fixture\nassert true;\nassert true;");
    assert_eq!(findings(&[&text]).iter().filter(|f| f.rule == "tautological-assert").count(), 1);
    assert!(has("// snappea-lint: allow typo\nmachine Main { start state S {} }", "invalid-suppression"));
    assert!(!has(&block("print \"// snappea-lint: allow typo\";"), "invalid-suppression"));
    assert!(!has("/* // snappea-lint: allow typo */ machine Main { start state S {} }", "invalid-suppression"));
}

#[test]
fn external_rules_share_the_same_suppression_and_reporting_pipeline() {
    struct Example;
    impl Rule for Example {
        fn id(&self) -> &'static str {
            "example-rule"
        }
        fn description(&self) -> &'static str {
            "Example extension"
        }
        fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
            for m in ctx.machines() {
                out.push(Finding::new(self.id(), m.name.span, "example"));
            }
        }
    }
    let mut registry = Registry::default();
    registry.register(Example);
    let mut sm = SourceMap::new();
    sm.add("test.p", "// snappea-lint: allow example-rule -- fixture\nmachine Main { start state S {} }");
    assert!(registry.check(&Context::parse(&sm).unwrap()).is_empty());
}

#[test]
fn driver_directives_mark_machines_and_malformed_ones_fail() {
    let marked = "// snappea: driver -- stands in for real clients\nmachine Client { start state S {} }\nmachine Server { start state S {} }";
    assert!(!has(marked, "invalid-directive"));
    let mut sm = SourceMap::new();
    sm.add("model.p", marked);
    let spans = snappea_lint::driver_machines(&sm).unwrap();
    assert_eq!(spans.len(), 1);
    assert!(sm.text(spans[0]).starts_with("machine Client"));
    // a typo, a directive before something else, and trailing code all fail
    assert!(has("// snappea: drivr\nmachine Client { start state S {} }", "invalid-directive"));
    assert!(has("// snappea: driver\nevent E;\nmachine Client { start state S {} }", "invalid-directive"));
    assert!(has("event E; // snappea: driver\nmachine Client { start state S {} }", "invalid-directive"));
    assert!(has("// snappea: driver\nspec S observes E { start state A {} }\nevent E;", "invalid-directive"));
    // strings and other comments do not count
    assert!(!has("// see snappea: driver docs\nmachine Client { start state S {} }", "invalid-directive"));
}
