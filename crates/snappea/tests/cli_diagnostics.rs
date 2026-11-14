//! What `snappea` and `snappea-compat` print for model errors: syntax and type errors,
//! runtime errors during a check, and liveness and deadlock reports.

use snappea::host::{Job, MemHost, Threads};

fn run_threads<'a>(jobs: Vec<Job<'a>>, main: &mut (dyn FnMut() + 'a)) {
    std::thread::scope(|s| {
        for job in jobs {
            s.spawn(job);
        }
        main();
    });
}

/// A host with `a.p` holding `src`.
fn host(src: &str) -> MemHost {
    let mut h = MemHost::new("/w");
    h.threads = Some(Threads { available: 2, run: run_threads });
    h.add_file("a.p", src);
    h
}

fn snappea(host: &mut MemHost, args: &[&str]) -> i32 {
    host.stdout.clear();
    host.stderr.clear();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    snappea::cli::run(&args, host)
}

fn compat(host: &mut MemHost, args: &[&str]) -> i32 {
    host.stdout.clear();
    host.stderr.clear();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    snappea::compat::run(&args, host)
}

/// `snappea check` output (stdout and stderr) for a model.
fn check(src: &str, extra: &[&str]) -> (i32, String) {
    let mut h = host(src);
    let mut args = vec!["check", "a.p", "--format", "text", "--no-mutation"];
    args.extend_from_slice(extra);
    let code = snappea(&mut h, &args);
    (code, format!("{}{}", h.stdout, h.stderr))
}

#[test]
fn a_reserved_word_used_as_a_name_is_named() {
    let (code, out) = check("machine M { var values: seq[int]; start state S {} }\n", &[]);
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error: 'values' is a reserved word (the map-values function) and cannot be used as a name"),
        "{out}"
    );
    assert!(out.contains("help: rename it, e.g. 'vals'"), "{out}");

    let (_, out) = check("event e: (to: int, packet: int);\n", &[]);
    assert!(out.contains("error: 'to' is a reserved word"), "{out}");

    // snappea-compat keeps upstream's text.
    let mut h = host("machine M { var values: seq[int]; start state S {} }\n");
    compat(&mut h, &["check", "a.p"]);
    let out = format!("{}{}", h.stdout, h.stderr);
    assert!(out.contains("mismatched input 'values' expecting Iden"), "{out}");
    assert!(!out.contains("reserved word"), "{out}");
}

#[test]
fn cast_help_suggests_to_for_conversions() {
    // `as` between an enum and int changes only the static type.
    let (code, out) = check(
        "enum tSku { A, B }\nmachine M { var t: map[int, int]; start state S { entry { var s: tSku; t[s as int] = 1; } } }\n",
        &[],
    );
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("error: types tSku and int are incomparable"), "{out}");
    assert!(
        out.contains("`as` changes the static type and does not convert a value; to convert, use `x to int`"),
        "{out}"
    );

    // P has no int-to-enum conversion; the help must not suggest `as`.
    let (code, out) = check(
        "enum tSku { A, B }\nmachine M { start state S { entry { var s: tSku; s = choose(2) to tSku; } } }\n",
        &[],
    );
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("error: coerce expects an interface, int, or float type"), "{out}");
    assert!(out.contains("P has no conversion to an enum"), "{out}");
    assert!(!out.contains(" as int"), "{out}");
}

/// `snappea-compat check` output and the bug log it writes.
fn compat_check(src: &str) -> (String, String) {
    let mut h = host(src);
    compat(&mut h, &["check", "a.p"]);
    let log = h
        .files_under("PCheckerOutput")
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "txt") && !p.to_string_lossy().contains("summary"))
        .map(|p| snappea::host::Host::read_to_string(&h, &p).unwrap_or_default())
        .collect::<String>();
    (format!("{}{}", h.stdout, h.stderr), log)
}

const MISSING_KEY: &str = "machine Store {\n\
       var carts: map[int, int];\n\
       start state Open {\n\
         entry {\n\
           var cart: int;\n\
           cart = 1;\n\
           cart = carts[cart] + 1;\n\
         }\n\
       }\n\
     }\n\
     // snappea-lint: allow no-model-machine -- runtime error fixture\n\
     test tcKey [main=Store]: { Store };\n";

#[test]
fn runtime_errors_say_what_where_and_how_to_fix_them() {
    let (code, out) = check(MISSING_KEY, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("error: map 'carts' has no key 1 (in Store(1), state 'Open')"), "{out}");
    assert!(out.contains("--> a.p:7:8"), "{out}");
    assert!(out.contains("|        ^^^^^^^^^^^"), "{out}");
    assert!(out.contains("help: test `cart in carts` first, or add the key before reading it"), "{out}");
    for internal in ["KeyNotFoundException", "Anon", "System."] {
        assert!(!out.contains(internal), "{internal}: {out}");
    }

    // snappea-compat keeps upstream's text.
    let (_, log) = compat_check(MISSING_KEY);
    assert!(
        log.contains("Exception 'System.Collections.Generic.KeyNotFoundException' was thrown in Store(1) (state 'Open', action 'Anon'): The given key '1' was not present in the dictionary."),
        "{log}"
    );
}

const NULL_TARGET: &str = "event eChanged;\n\
     machine Outbox {\n\
       var db: machine;\n\
       start state Serving {\n\
         entry {\n\
           send db, eChanged;\n\
         }\n\
       }\n\
     }\n\
     // snappea-lint: allow no-model-machine -- runtime error fixture\n\
     test tcNull [main=Outbox]: { Outbox };\n";

#[test]
fn a_send_to_null_names_the_target() {
    let (code, out) = check(NULL_TARGET, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("error: cannot send eChanged to a null target (in Outbox(1), state 'Serving')"), "{out}");
    assert!(out.contains("--> a.p:6:1"), "{out}");
    assert!(out.contains("help: 'db' is null here"), "{out}");

    let (_, log) = compat_check(NULL_TARGET);
    assert!(
        log.contains("Target in send cannot be null. Machine Outbox(1) trying to send event eChanged to null target in state Serving."),
        "{log}"
    );
}

const ENUMS: &str = "enum tStatus { Held, RolledBack, Done }\n\
     event eOps: (ops: seq[tStatus]);\n\
     spec Saga observes eOps {\n\
       start state S {\n\
         on eOps do (p: (ops: seq[tStatus])) {\n\
           assert p.ops[0] == Held, format(\"saga ended {0} but {1} is still held\", RolledBack, p.ops[0]);\n\
         }\n\
       }\n\
     }\n\
     machine M {\n\
       start state S {\n\
         entry {\n\
           var ops: seq[tStatus];\n\
           ops += (0, Done);\n\
           print format(\"status {0}\", Done);\n\
           announce eOps, (ops = ops,);\n\
         }\n\
       }\n\
     }\n\
     // snappea-lint: allow no-model-machine -- enum formatting fixture\n\
     test tcEnum [main=M]: assert Saga in { M };\n";

#[test]
fn enum_values_print_by_name_in_snappea_only() {
    let mut h = host(ENUMS);
    let code = snappea(&mut h, &["check", "a.p", "--format", "text", "--no-mutation"]);
    assert_eq!(code, 1, "{}", h.stdout);
    assert!(h.stdout.contains("saga ended RolledBack but Done is still held"), "{}", h.stdout);
    let log = snappea::host::Host::read_to_string(&h, std::path::Path::new("PCheckerOutput/tcEnum/bug.txt")).unwrap();
    assert!(log.contains("<PrintLog> status Done"), "{log}");
    assert!(log.contains("with payload <ops:(Done), >"), "{log}");

    let (_, log) = compat_check(ENUMS);
    assert!(log.contains("saga ended 1 but 2 is still held"), "{log}");
    assert!(log.contains("<PrintLog> status 2"), "{log}");
}

#[test]
fn helpers_only_assertion_messages_call_are_not_required_coverage() {
    let src = "enum tStatus { Held, Done }\n\
         event eOps: tStatus;\n\
         spec Saga observes eOps {\n\
           start state S {\n\
             on eOps do (s: tStatus) {\n\
               assert s == Done, format(\"saga ended {0}\", Name(s));\n\
             }\n\
           }\n\
           fun Name(s: tStatus): string {\n\
             if (s == Held) { return \"held\"; }\n\
             return \"done\";\n\
           }\n\
         }\n\
         machine M { start state S { entry { announce eOps, Done; } } }\n\
         // snappea-lint: allow no-model-machine -- coverage fixture\n\
         test tcEnum [main=M]: assert Saga in { M };\n";
    let (code, out) = check(src, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("Name"), "{out}");
}

const LIVENESS: &str = "event eOpen: int;\n\
     spec Lifecycle observes eOpen {\n\
       var open: set[int];\n\
       var closed: int;\n\
       start state Idle {\n\
         on eOpen do (id: int) { open += (id); goto Running; }\n\
       }\n\
       hot state Running { }\n\
     }\n\
     machine Main { start state S { entry { announce eOpen, 3; } } }\n\
     // snappea-lint: allow no-model-machine -- liveness report fixture\n\
     test tcLive [main=Main]: assert Lifecycle in { Main };\n";

#[test]
fn liveness_reports_show_the_spec_fields() {
    let (code, out) = check(LIVENESS, &["--warn", "lint"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("Lifecycle detected liveness bug in hot state 'Running' at the end of program execution.\n      Lifecycle's fields: open = {3}, closed = 0\n"),
        "{out}"
    );
    assert!(!out.contains("PImplementation."), "{out}");

    let (_, log) = compat_check(LIVENESS);
    assert!(
        log.contains(
            "PImplementation.Lifecycle detected liveness bug in hot state 'Running' at the end of program execution.\n"
        ),
        "{log}"
    );
}

const DEADLOCK: &str = "event eWork;\n\
     event eDone;\n\
     spec Finishes observes eWork, eDone {\n\
       var pending: int;\n\
       start state Idle { on eWork do { pending = pending + 1; goto Busy; } }\n\
       hot state Busy { on eDone do { goto Idle; } }\n\
     }\n\
     machine Main {\n\
       start state S {\n\
         entry {\n\
           announce eWork;\n\
           receive { case eDone: { announce eDone; } }\n\
         }\n\
       }\n\
     }\n\
     // snappea-lint: allow no-model-machine -- deadlock report fixture\n\
     test tcStuck [main=Main]: assert Finishes in { Main };\n";

#[test]
fn deadlock_reports_name_the_hot_spec() {
    let (code, out) = check(DEADLOCK, &["--warn", "lint"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("Deadlock detected. Main(1) is waiting to receive an event, but no other controlled tasks are enabled, while spec Finishes is in hot state 'Busy' (pending = 1)."),
        "{out}"
    );
    let mut h = host(DEADLOCK);
    snappea(&mut h, &["check", "a.p", "--format", "json", "--no-mutation", "--warn", "lint"]);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    let report = v["test_cases"][0]["bug"]["report"].as_str().unwrap_or_default().to_string();
    assert!(report.contains("while spec Finishes is in hot state 'Busy'"), "{}", h.stdout);
    // A deadlock has a failure identity, so expected-bug seeds can be compared.
    let identity = &v["test_cases"][0]["bug"]["identity"];
    assert_eq!(identity["kind"], "deadlock", "{}", h.stdout);
    assert_eq!(identity["machine"], "Main", "{}", h.stdout);
    assert_eq!(identity["state"], "S", "{}", h.stdout);

    let (_, log) = compat_check(DEADLOCK);
    assert!(
        log.contains(
            "Deadlock detected. Main(1) is waiting to receive an event, but no other controlled tasks are enabled.\n"
        ),
        "{log}"
    );
}
