//! Mutation testing must not hide real gaps or claim more than it tested: regressions
//! for likely-equivalent judgements, equivalent mutants spelled differently, untested
//! mutants and suppressions that match nothing.
mod common;

use common::snappea;

use serde_json::Value;
use snappea::host::MemHost;

#[test]
fn mutation_verdicts_do_not_depend_on_the_lint_gate() {
    let mut h = MemHost::new("/w");
    h.add_file("g.p", OPTIONAL_ANSWER);
    let mut results = Vec::new();
    for extra in [&[][..], &["--warn", "lint"][..], &["--deny", "lint"][..]] {
        let mut args = vec!["check", "mutation", "g.p", "--json"];
        args.extend_from_slice(extra);
        snappea(&mut h, &args);
        let mut m = json(&h)["mutation"].clone();
        m["seconds"] = Value::Null;
        m["first_pass_seconds"] = Value::Null;
        results.push(m);
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0], results[2]);
}

fn json(host: &MemHost) -> Value {
    serde_json::from_str(&host.stdout).unwrap_or_else(|e| panic!("{e}: {}{}", host.stdout, host.stderr))
}

fn changes(list: &Value) -> Vec<String> {
    list.as_array().unwrap().iter().map(|m| m["change"].as_str().unwrap().to_string()).collect()
}

/// A store whose write is sent in either branch of a choice: the write always happens.
const EITHER_WRITE: &str = "event eWrite: int;
event eRead: machine;
event eVal: int;

machine Store {
  var v: int;
  start state S {
    on eWrite do (x: int) { v = x; }
    on eRead do (c: machine) { send c, eVal, v; }
  }
}

spec NonNeg observes eVal {
  start state S { on eVal do (x: int) { assert x >= 0, \"negative\"; } }
}

machine Client {
  start state S {
    entry {
      var s: Store;
      s = new Store();
      if ($) { send s, eWrite, 1; } else { send s, eWrite, 2; }
      send s, eRead, this;
    }
    ignore eVal;
  }
}

// snappea-lint: allow no-progress-spec -- regression fixture
test tc [main=Client]: assert NonNeg in { Store, Client };
";

#[test]
fn an_event_sent_in_every_branch_of_a_choice_is_not_an_optional_fault() {
    let mut h = MemHost::new("/w");
    // a removed assignment is a mutant only with `--strict`, which also fails on advisory lint
    // findings (the fixture's unused suppression): `--warn lint` keeps those from skipping it
    let check = ["check", "mutation", "w.p", "--strict", "--warn", "lint", "--json"];
    for (name, source) in [
        ("either", EITHER_WRITE.to_string()),
        // a branch that sends more than the event is not the run without it
        (
            "several",
            EITHER_WRITE.replace(
                "if ($) { send s, eWrite, 1; } else { send s, eWrite, 2; }",
                "if ($) { send s, eWrite, 1; send s, eWrite, 2; }",
            ),
        ),
    ] {
        h.add_file("w.p", &source);
        assert_eq!(snappea(&mut h, &check), 1, "{name}: {}", h.stdout);
        let m = &json(&h)["mutation"];
        assert!(changes(&m["undetected"]).contains(&"removed `v = x;`".to_string()), "{name}: {m}");
        assert_eq!(m["likely_equivalent"], serde_json::json!([]), "{name}: {m}");
    }
    // an optional write that a spec observes: the spec sees it arrive
    let observed = EITHER_WRITE
        .replace("if ($) { send s, eWrite, 1; } else { send s, eWrite, 2; }", "if ($) { send s, eWrite, 1; }")
        .replace("spec NonNeg observes eVal {", "spec NonNeg observes eVal, eWrite {")
        .replace(
            "on eVal do (x: int) { assert x >= 0, \"negative\"; }",
            "on eVal do (x: int) { assert x >= 0, \"negative\"; }\n    ignore eWrite;",
        );
    h.add_file("w.p", &observed);
    assert_eq!(snappea(&mut h, &check), 1, "{}", h.stdout);
    let m = &json(&h)["mutation"];
    assert!(changes(&m["undetected"]).contains(&"removed `v = x;`".to_string()), "{m}");
    assert_eq!(m["likely_equivalent"], serde_json::json!([]), "{m}");
    // the one optional write, unobserved, is the fault model's case
    let optional = EITHER_WRITE
        .replace("if ($) { send s, eWrite, 1; } else { send s, eWrite, 2; }", "if ($) { send s, eWrite, 1; }");
    h.add_file("w.p", &optional);
    snappea(&mut h, &check);
    let m = &json(&h)["mutation"];
    assert_eq!(changes(&m["likely_equivalent"]), ["removed `v = x;`"], "{m}");
    // without `--strict` the write is not a mutant at all
    snappea(&mut h, &["check", "mutation", "w.p", "--json"]);
    let m = &json(&h)["mutation"];
    assert_eq!(m["likely_equivalent"], serde_json::json!([]), "{m}");
    assert_eq!(changes(&m["undetected"]), ["removed `send c, eVal, v;`"], "{m}");
}

/// A crash handler of two statements, on lines of their own; the crash is optional.
const OPTIONAL_CRASH: &str = "event eCrash;
event eReq: machine;
event eResp: int;
event eAsked;
event eAnswered;
machine Server {
  var epoch: int;
  start state S {
    on eReq do (c: machine) { send c, eResp, epoch; }
    on eCrash do {
      epoch = epoch + 1;
      epoch = epoch + 1;
    }
  }
}
spec Answered observes eAsked, eAnswered {
  start cold state Idle { on eAsked goto Waiting; }
  hot state Waiting { on eAnswered goto Idle; }
}
machine Main {
  start state S {
    entry {
      var s: Server;
      s = new Server();
      if ($) { send s, eCrash; }
      announce eAsked;
      send s, eReq, this;
      receive { case eResp: (e: int) { if (e != 0) { print \"restarted\"; } announce eAnswered; } }
    }
  }
}
test tc [main=Main]: assert Answered in { Server, Main };
";

#[test]
fn cold_first_pass_finds_witnesses_and_leaves_survivors_for_adaptive_search() {
    let mut host = MemHost::new("/w");
    let choice = "if ($) { send s, eCrash; }";
    // Enough distinct crash counts for the baseline to exceed a short exhaustive turn.
    host.add_file("model.p", &OPTIONAL_CRASH.replace(choice, &choice.repeat(4)));
    for timed in [false, true] {
        // `--strict`: the optional branch only prints, so its mutants are not run by default
        let mut args = vec!["check", "mutation", "model.p", "--seed", "1", "--json", "--strict", "--warn", "lint"];
        if timed {
            args.push("--timed");
        }
        snappea(&mut host, &args);
        let report = json(&host);
        let m = &report["mutation"];
        let detections = m["first_pass_detections"].as_u64().unwrap();
        let schedules = m["first_pass_schedules"].as_u64().unwrap();
        if timed {
            assert_eq!((detections, schedules), (0, 0));
        } else {
            assert!(detections > 0 && schedules >= detections, "{m}");
            assert!(detections <= m["detected"].as_u64().unwrap());
        }
        assert!(!m["likely_equivalent"].as_array().unwrap().is_empty(), "{m}");
        assert_eq!(m["time_limit_reached"], false);
    }
}

/// `source` with `comment` on its own line before 1-based `line`, indented like it.
fn insert_before(source: &str, line: usize, comment: &str) -> String {
    let mut lines: Vec<String> = source.lines().map(str::to_string).collect();
    let indent: String = lines[line - 1].chars().take_while(|c| c.is_whitespace()).collect();
    lines.insert(line - 1, format!("{indent}{comment}"));
    lines.join("\n") + "\n"
}

#[test]
fn likely_equivalent_mutants_name_the_suppression_that_accepts_them() {
    let mut h = MemHost::new("/w");
    h.add_file("crash.p", OPTIONAL_CRASH);
    snappea(&mut h, &["check", "mutation", "crash.p", "--json", "--strict", "--warn", "lint"]);
    let m = json(&h)["mutation"].clone();
    let likely = m["likely_equivalent"].as_array().unwrap();
    assert!(!likely.is_empty(), "{m}");
    // each suggested line, added where it says, suppresses that mutant and nothing else
    for entry in likely {
        let comment = entry["suppression"].as_str().unwrap();
        assert!(comment.starts_with("// snappea-lint: allow mutation:"), "{entry}");
        assert!(entry["accept"].as_str().unwrap().contains(comment), "{entry}");
        let line = entry["line"].as_u64().unwrap() as usize;
        h.add_file("crash.p", &insert_before(OPTIONAL_CRASH, line, comment));
        snappea(&mut h, &["check", "mutation", "crash.p", "--json", "--strict", "--warn", "lint"]);
        let after = &json(&h)["mutation"];
        assert_eq!(after["suppressed"], 1, "{entry}: {after}");
        assert_eq!(after["stale_suppressions"], serde_json::json!([]), "{entry}: {after}");
        assert!(!changes(&after["likely_equivalent"]).contains(&entry["change"].as_str().unwrap().to_string()));
    }
    h.add_file("crash.p", OPTIONAL_CRASH);
    snappea(&mut h, &["--format", "text", "check", "mutation", "crash.p", "--strict", "--warn", "lint"]);
    assert!(h.stdout.contains("if you agree, add this line directly before line"), "{}", h.stdout);
    // a statement that shares its line with the handler needs a line of its own first
    let one_line = "event eCrash;\nevent eReq: machine;\nevent eResp: int;\nevent eAsked;\nevent eAnswered;\n\
        machine Server {\n  var epoch: int;\n  start state S {\n\
        on eReq do (c: machine) { send c, eResp, epoch; }\n\
        on eCrash do { epoch = epoch + 1; }\n  }\n}\n";
    let source = OPTIONAL_CRASH.replace(&OPTIONAL_CRASH[..OPTIONAL_CRASH.find("spec Answered").unwrap()], one_line);
    h.add_file("crash.p", &source);
    // (a handler of one assignment has a mutant only with `--strict`)
    snappea(&mut h, &["check", "mutation", "crash.p", "--strict", "--json"]);
    let m = &json(&h)["mutation"];
    let accept = m["likely_equivalent"][0]["accept"].as_str().unwrap_or_else(|| panic!("{m}"));
    assert!(accept.starts_with("put the statement on its own line"), "{accept}");
}

/// A server whose answer is optional: removing it, or the `if` around it, is the same.
const OPTIONAL_ANSWER: &str = "event eReq: machine;
event eResp: int;
event eSeen: int;

machine Server {
  var n: int;
  start state S {
    on eReq do (c: machine) {
      if ($) { send c, eResp, n; }
      n = n + 1;
    }
  }
}

spec Pos observes eSeen {
  start state S { on eSeen do (x: int) { assert x >= 0, \"neg\"; } }
}

machine Main {
  start state S {
    entry {
      var s: Server;
      s = new Server();
      send s, eReq, this;
      send s, eReq, this;
    }
    on eResp do (x: int) { announce eSeen, x; }
  }
}

// snappea-lint: allow no-progress-spec -- regression fixture
test tc [main=Main]: assert Pos in { Server, Main };
";

#[test]
fn removing_the_only_statement_of_a_choice_is_equivalent_like_removing_the_if() {
    let mut h = MemHost::new("/w");
    h.add_file("g.p", OPTIONAL_ANSWER);
    snappea(&mut h, &["check", "mutation", "g.p", "--json"]);
    let m = &json(&h)["mutation"];
    let equivalent = changes(&m["equivalent"]);
    assert!(equivalent.contains(&"removed `send c, eResp, n;`".to_string()), "{m}");
    assert!(equivalent.contains(&"removed the whole `if ($) { send c, eResp, n; }`".to_string()), "{m}");
    let inside: Vec<String> =
        m["undetected"].as_array().unwrap().iter().flat_map(|u| changes(&u["undetected_inside"])).collect();
    assert!(!inside.contains(&"removed `send c, eResp, n;`".to_string()), "{m}");
    // the same with the branch spelled as an `else`
    h.add_file(
        "g.p",
        &OPTIONAL_ANSWER.replace("if ($) { send c, eResp, n; }", "if ($) { } else { send c, eResp, n; }"),
    );
    snappea(&mut h, &["check", "mutation", "g.p", "--json"]);
    let m = &json(&h)["mutation"];
    assert!(changes(&m["equivalent"]).contains(&"removed `send c, eResp, n;`".to_string()), "{m}");
    // next to another statement, it is not
    h.add_file("g.p", &OPTIONAL_ANSWER.replace("{ send c, eResp, n; }", "{ send c, eResp, n; n = n + 2; }"));
    snappea(&mut h, &["check", "mutation", "g.p", "--json"]);
    let m = &json(&h)["mutation"];
    assert!(!changes(&m["equivalent"]).contains(&"removed `send c, eResp, n;`".to_string()), "{m}");
}

/// A server with a reset handler no test case ever triggers.
const UNREACHED: &str = "event eReq: machine;
event eResp: int;
event eReset;

machine Server {
  var n: int;
  start state S {
    on eReq do (c: machine) { n = n + 1; send c, eResp, n; }
    on eReset do { n = 0; n = n - 5; }
  }
}

machine Main {
  start state S {
    entry { send new Server(), eReq, this; receive { case eResp: (x: int) { assert x == 1, \"wrong\"; } } }
  }
}

// snappea-lint: allow no-progress-spec -- regression fixture
test tc [main=Main]: { Server, Main };
";

#[test]
fn mutants_in_code_no_test_case_ran_are_not_reported_as_detected() {
    let mut h = MemHost::new("/w");
    h.add_file("h.p", UNREACHED);
    // the reset handler only updates a variable, so its four mutants (its emptied body,
    // its two assignments and the swapped operator) run only with `--strict`
    {
        let (strict, unreached) = (true, 4);
        let with = |args: &[&'static str]| {
            let mut args = args.to_vec();
            if strict {
                // `--warn lint`: the fixture's unused suppression is an advisory finding
                args.extend(["--strict", "--warn", "lint"]);
            }
            args
        };
        assert_eq!(snappea(&mut h, &with(&["check", "mutation", "h.p", "--json"])), 1, "{}", h.stdout);
        let v = json(&h);
        assert_eq!(v["summary"]["status"], "mutation_incomplete", "{}", v["summary"]);
        assert_eq!(v["summary"]["mutants_unreached"], unreached, "{}", v["summary"]);
        assert_eq!(v["mutation"]["unreached"], unreached);
        assert_eq!(v["mutation"]["unreached_code"][0]["line"], 9);
        assert_eq!(v["mutation"]["complete"], true, "every mutant of the code that ran was tested");
        assert_eq!(snappea(&mut h, &with(&["--format", "text", "check", "mutation", "h.p"])), 1);
        assert!(!h.stdout.contains("Mutation testing passed"), "{}", h.stdout);
        let (count, them) = if strict { ("4 mutants", "them") } else { ("1 mutant", "it") };
        assert!(
            h.stdout.contains(&format!(
                "Not tested: {count} in code no passing test case ran, so no test case can detect {them}:\n  \
                 h.p:9  Server.S, on eReset ({count})\n"
            )),
            "{}",
            h.stdout
        );
        assert!(
            h.stdout.contains(&format!(": {count} in code no passing test case ran.\n"))
                && h.stdout.contains("\nMutation testing incomplete in "),
            "{}",
            h.stdout
        );
        // reported, without failing, under --warn mutation
        assert_eq!(snappea(&mut h, &with(&["check", "mutation", "h.p", "--warn", "mutation", "--json"])), 0);
        assert_eq!(json(&h)["warnings"]["mutation_untested"], unreached);
    }
    // a whole check leaves that code to coverage, with or without `--strict`
    assert_eq!(snappea(&mut h, &["check", "h.p", "--warn", "coverage", "--json"]), 0, "{}", h.stdout);
    assert_eq!(json(&h)["mutation"]["unreached"], 0);
    let strict = ["check", "h.p", "--warn", "coverage", "--strict", "--warn", "lint", "--json"];
    assert_eq!(snappea(&mut h, &strict), 0, "{}", h.stdout);
    assert_eq!(json(&h)["mutation"]["unreached"], 4);
    // code declared as not needing to run has no untested mutants
    h.add_file(
        "h.p",
        &UNREACHED.replace(
            "    on eReset do",
            "    // snappea-lint: allow coverage -- reset is out of scope\n    on eReset do",
        ),
    );
    assert_eq!(snappea(&mut h, &["--format", "text", "check", "mutation", "h.p"]), 0, "{}", h.stdout);
    assert!(
        h.stdout.contains("🫛 Mutation testing passed in 0.00s. Nothing left to fix: more tests are optional.\n"),
        "{}",
        h.stdout
    );
}

/// A store whose machine carries a non-nested suppression, which covers no mutant.
const MACHINE_SUPPRESSION: &str = "event eWrite: int;
event eRead: machine;
event eVal: int;

// snappea-lint: allow mutation -- legacy machine, not under test
machine Store {
  var v: int;
  start state S {
    on eWrite do (x: int) { v = x; }
    on eRead do (c: machine) { send c, eVal, v; }
  }
}

spec NonNeg observes eVal {
  start state S { on eVal do (x: int) { assert x >= 0, \"negative\"; } }
}

machine Client {
  start state S {
    entry {
      var s: Store;
      s = new Store();
      send s, eWrite, 1;
      send s, eRead, this;
    }
    ignore eVal;
  }
}

// snappea-lint: allow no-progress-spec -- regression fixture
test tc [main=Client]: assert NonNeg in { Store, Client };
";

#[test]
fn a_suppression_that_matches_nothing_says_so_and_suggests_the_nested_form() {
    let mut h = MemHost::new("/w");
    h.add_file("m.p", MACHINE_SUPPRESSION);
    assert_eq!(snappea(&mut h, &["--format", "text", "check", "mutation", "m.p"]), 1);
    assert!(h.stdout.contains("warning: m.p:6: `allow mutation` matches no mutant"), "{}", h.stdout);
    assert!(h.stdout.contains("help: did you mean `allow mutation-nested`?"), "{}", h.stdout);
    // a suppression that hides nothing hides no other kinds either
    assert!(!h.stdout.contains("blanket"), "{}", h.stdout);
    // the same before a handler, with a kind
    let handler =
        MACHINE_SUPPRESSION.replace("// snappea-lint: allow mutation -- legacy machine, not under test\n", "").replace(
            "    on eWrite do",
            "    // snappea-lint: allow mutation:remove-statement -- write checked elsewhere\n    on eWrite do",
        );
    h.add_file("m.p", &handler);
    snappea(&mut h, &["check", "mutation", "m.p", "--json"]);
    let m = &json(&h)["mutation"];
    assert_eq!(m["suppressed"], 0, "{m}");
    let help = m["stale_suppressions"][0]["help"].as_str().unwrap_or_else(|| panic!("{m}"));
    assert!(help.starts_with("did you mean `allow mutation-nested:remove-statement`?"), "{help}");
    // the nested form covers the code inside
    h.add_file("m.p", &MACHINE_SUPPRESSION.replace("allow mutation --", "allow mutation-nested --"));
    assert_eq!(snappea(&mut h, &["--format", "text", "check", "mutation", "m.p"]), 0, "{}", h.stdout);
    assert!(!h.stdout.contains("matches no mutant"), "{}", h.stdout);
    // the console no longer counts suppressed mutants; JSON does: the removed `send`, and
    // with `--strict` the removed assignment too
    assert!(h.stdout.contains("\nNo mutant ran in "), "{}", h.stdout);
    assert_eq!(snappea(&mut h, &["check", "mutation", "m.p", "--json"]), 0, "{}", h.stdout);
    assert_eq!(json(&h)["mutation"]["suppressed"], 1);
    assert_eq!(snappea(&mut h, &["check", "mutation", "m.p", "--strict", "--warn", "lint", "--json"]), 0);
    assert_eq!(json(&h)["mutation"]["suppressed"], 2);
}

#[test]
fn unobserved_announcement_is_a_gap_only_for_a_safe_payload_and_selected_suite() {
    let mut host = MemHost::new("/w");
    host.add_file(
        "a.p",
        r#"
event observed: int;
event effectful: int;
event unsafeIndex: int;
machine Server {
    var calls: int;
    var xs: seq[int];
    fun Count(): int { calls = calls + 1; return calls; }
    start state S { entry {
        xs += (0, 7);
        announce observed, 7;
        announce effectful, Count();
        announce unsafeIndex, xs[0];
        assert calls == 1;
    } }
}
machine Main { start state S { entry { new Server(); } } }
spec Watch observes observed { start state S { on observed do (n: int) { assert n == 7; } } }
test plain [main=Main]: { Main, Server };
test watched [main=Main]: assert Watch in { Main, Server };
"#,
    );
    for (selected, expected) in [("plain", true), ("watched", false)] {
        snappea(&mut host, &["check", "mutation", "a.p", selected, "--warn", "all", "--json"]);
        let report = json(&host);
        let gaps = report["mutation"]["undetected"].as_array().unwrap_or_else(|| panic!("{report} {}", host.stderr));
        let flagged: Vec<_> = gaps
            .iter()
            .flat_map(|g| std::iter::once(g).chain(g["undetected_inside"].as_array().into_iter().flatten()))
            .filter(|g| g["unobserved_announcement"].is_string())
            .collect();
        assert_eq!(flagged.len(), usize::from(expected), "{report}");
        if expected {
            assert_eq!(flagged[0]["unobserved_announcement"], "observed");
            assert_eq!(flagged[0]["schedules"], 0);
        }
    }
}
