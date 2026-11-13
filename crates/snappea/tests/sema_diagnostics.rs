//! Type errors on small programs.

fn errors(src: &str) -> Vec<String> {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("t.p", src);
    match snappea::sema::compile(&sm, &snappea::sema::Options::default()) {
        Ok(_) => Vec::new(),
        Err(snappea::sema::CompileError::Analysis(a)) => a.messages(&sm),
        Err(e) => panic!("not a type error: {e:?}"),
    }
}

#[test]
fn format_indices_wrap_like_csharp_int() {
    // 4294967296 wraps to 0, so one argument is expected.
    assert_eq!(errors(r#"fun f() { print format("{4294967296}", 1); }"#), Vec::<String>::new());
    // 99999999999999999999 wraps to 1661992959.
    assert_eq!(
        errors(r#"fun f() { print format("{99999999999999999999}", 1); }"#),
        ["[t.p:1:17] goto, function or constructor call expected 1661992960 arguments, got 1"]
    );
}

#[test]
fn pairwise_needs_two_parameters() {
    let src = "param x: int;
               machine Main { start state S { } }
               test param (x in [1, 2]) pairwise tc [main=Main]: { Main };";
    assert_eq!(errors(src), ["[t.p:3:41] invalid twise number at t.p:3:16: twise number 2 is greater than 1."]);
}

fn analysis(src: &str) -> (snappea::syntax::SourceMap, snappea::sema::AnalysisError) {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("t.p", src);
    match snappea::sema::compile(&sm, &snappea::sema::Options::default()) {
        Err(snappea::sema::CompileError::Analysis(a)) => (sm, a),
        r => panic!("not a type error: {r:?}"),
    }
}

#[test]
fn unknown_names_suggest_close_identifiers() {
    let src = "event eA;\nmachine Main {\n  start state S {\n    entry { send this, eB; }\n  }\n}\n";
    let (sm, a) = analysis(src);
    // the upstream message is unchanged; the suggestion is a separate field
    assert_eq!(a.messages(&sm), ["[t.p:4:24] could not find name 'eB'"]);
    let d = &a.diagnostics(&sm)[0];
    assert_eq!(d.help.as_deref(), Some("did you mean 'eA'? Otherwise declare the name before using it"));
    let text = snappea::sema::diag::render_snippet(&sm, d.span, &d.message, d.help.as_deref());
    assert_eq!(
        text,
        "error: could not find name 'eB'
  --> t.p:4:24
  |
2 | machine Main {
3 |   start state S {
4 |     entry { send this, eB; }
  |                        ^^
5 |   }
  |
  = help: did you mean 'eA'? Otherwise declare the name before using it
"
    );
}

#[test]
fn one_letter_names_get_no_suggestion() {
    let (sm, a) = analysis("machine M { start state S { entry { x = 1; } } }");
    assert_eq!(
        a.diagnostics(&sm)[0].help.as_deref(),
        Some("declare the variable before using it, or check the spelling")
    );
}

#[test]
fn every_type_error_suggests_a_fix() {
    let (sm, a) = analysis("machine M { var x: int; start state S { entry { x = \"s\"; } } }");
    assert!(a.diagnostics(&sm).iter().all(|d| d.help.is_some()));
}

#[test]
fn an_unclosed_test_module_points_to_per_variant_drivers() {
    // a shared helper creates either variant, so both are in every test's module
    let src = "machine Server { start state S { } }
               machine MemoryOnlyServer { start state S { } }
               fun Start(memoryOnly: bool) { if (memoryOnly) { new MemoryOnlyServer(); } else { new Server(); } }
               machine Main { start state S { entry { Start(false); } } }
               test tcFixed [main=Main]: { Main, Server };";
    let (sm, a) = analysis(src);
    let messages = a.messages(&sm);
    assert!(messages[0].contains("interface MemoryOnlyServer is created but not implemented"), "{messages:?}");
    let help = a.diagnostics(&sm)[0].help.clone().unwrap();
    assert!(help.contains("do not add it") && help.contains("`Run(new Server(..))`"), "{help}");
}
