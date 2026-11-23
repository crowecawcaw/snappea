//! `p observe` on an in-memory host.

mod support;

use std::path::Path;

use snappea::host::{Host, MemHost};

const SPEC: &str = r#"
type tTxn = (account: int, amount: int);
event eDeposit: tTxn;
event eWithdraw: tTxn;
event eClose: int;
event eUnrelated;

spec NeverOverdrawn observes eDeposit, eWithdraw, eClose {
  var balance: int;
  start state Open {
    on eDeposit do (t: tTxn) { balance = balance + t.amount; }
    on eWithdraw do (t: tTxn) {
      balance = balance - t.amount;
      assert balance >= 0, format("account {0} overdrawn: balance {1}", t.account, balance);
    }
    on eClose goto Closed;
  }
  state Closed {}
}
"#;

const PPROJ: &str =
    "<Project><ProjectName>Bank</ProjectName><InputFiles><PFile>./PSpec/</PFile></InputFiles></Project>";

fn host() -> MemHost {
    let mut h = MemHost::new("/work");
    h.add_file("Bank.pproj", PPROJ);
    h.add_file("PSpec/Bank.p", SPEC);
    h
}

fn run(h: &mut MemHost, args: &[&str]) -> i32 {
    h.stdout.clear();
    h.stderr.clear();
    let args: Vec<String> = std::iter::once("observe").chain(args.iter().copied()).map(String::from).collect();
    snappea::compat::run(&args, h)
}

fn line(key: &str, ts: i64, event: &str, account: i64, amount: i64) -> String {
    format!(
        r#"{{"key": "{}", "timestamp": {}, "event": "{}", "payload": {{"account": {}, "amount": {}}}}}"#,
        key, ts, event, account, amount
    )
}

fn read(h: &MemHost, p: &str) -> String {
    h.read_to_string(Path::new(p)).unwrap_or_else(|e| panic!("{}: {}", p, e))
}

#[test]
fn clean_logs_pass() {
    let mut h = host();
    let log =
        [line("a", 1, "eDeposit", 1, 10), line("b", 2, "eDeposit", 2, 5), line("a", 3, "eWithdraw", 1, 10)].join("\n");
    h.add_file("logs/run.log", &log);
    let code = run(&mut h, &["--spec", "NeverOverdrawn", "-l", "logs", "-o", "out"]);
    assert_eq!(code, 0, "{}", h.stdout);
    let expected_head = "Current PObserve configuration:
PObserveConfig {
  outputDir            = out
  inputKind            = TEXT
  pproj                = /work/Bank.pproj
  parserName           = null
  specificationName    = NeverOverdrawn
  replayWindowSize     = 100
  logDelimiter         = \\n
}
-------------------------------------
Success! No bugs found.
-------------------------------------
";
    assert!(h.stdout.starts_with(expected_head), "{}", h.stdout);
    assert!(
        h.stdout.contains("3                        3                        2                        2"),
        "{}",
        h.stdout
    );
    assert!(
        h.stdout.ends_with("PObserve run completed with 0 error(s)\nRun details can be found in /work/out\n"),
        "{}",
        h.stdout
    );
    let metrics = read(&h, "out/PObserveMetrics.txt");
    assert!(metrics.contains("Event Type"), "{}", metrics);
    assert!(
        metrics.contains("eDeposit                        2                     0.000000              0"),
        "{}",
        metrics
    );
}

#[test]
fn violations_are_reported_per_key_with_replay_logs() {
    let mut h = host();
    // out of time order on purpose: each key is sorted before checking
    let log = [
        line("acct-1", 5, "eWithdraw", 1, 30),
        line("acct-1", 1, "eDeposit", 1, 20),
        line("acct-2", 2, "eDeposit", 2, 20),
        "not json".to_string(),
    ]
    .join("\n");
    h.add_file("run.log", &log);
    let code = run(&mut h, &["--spec", "NeverOverdrawn", "-l", "run.log", "-o", "out", "--replaySize", "1"]);
    assert_eq!(code, 2, "{}", h.stdout);
    assert!(
        h.stdout.contains("Parser Exception::\nException occurred while parsing log line: not json\n"),
        "{}",
        h.stdout
    );
    assert!(
        h.stdout.contains(
            "PObserve Spec Violation::\nAssertion failure: PSpec/Bank.p:14:7 account 1 overdrawn: balance -10\n"
        ),
        "{}",
        h.stdout
    );
    assert!(h.stdout.contains("PObserve run completed with 1 error(s)"), "{}", h.stdout);
    let replay = read(&h, "out/replayEvents_acct-1.log");
    assert_eq!(
        replay,
        "errorKey=acct-1\nerrorTimeStamp=5\nerrorMessage=Assertion failure: PSpec/Bank.p:14:7 account 1 overdrawn: balance -10\nreplayEvents=\n\
         PObserveEvent{partitionKey='acct-1', timestamp=1970-01-01T00:00:00.005000000Z, event=eWithdraw[PTuple_accnt_amnt[account=1, amount=30]], \
         logLine='{\"key\": \"acct-1\", \"timestamp\": 5, \"event\": \"eWithdraw\", \"payload\": {\"account\": 1, \"amount\": 30}}', customPayload=null}\n"
    );
    let parser_error = read(&h, "out/ParserError_0.txt");
    assert!(parser_error.starts_with("errorMessage=pobserve.commons.exceptions.PObserveLogParsingException: not json"));
}

#[test]
fn report_write_failures_are_errors() {
    let mut h = host();
    h.add_file("run.log", &[line("a", 1, "eDeposit", 1, 10), line("a", 2, "eWithdraw", 1, 30)].join("\n"));
    h.create_dir_all(Path::new("out/replayEvents_a.log")).unwrap();
    assert_eq!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "run.log", "-o", "out"]), 1, "{}", h.stdout);
    assert!(h.stdout.contains(".. writing error log into file /work/out/replayEvents_a.log\n"), "{}", h.stdout);
    assert_eq!(
        h.stderr,
        "Error: Could not write file '/work/out/replayEvents_a.log': Is a directory: '/work/out/replayEvents_a.log'\n"
    );
    assert!(!h.stdout.contains("PObserve run completed"), "{}", h.stdout);
}

#[test]
fn parser_errors_alone_exit_with_4() {
    let mut h = host();
    h.add_file("run.log", &format!("{}\n{{oops", line("k", 1, "eDeposit", 1, 1)));
    assert_eq!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "run.log", "-o", "out"]), 4, "{}", h.stdout);
    assert!(h.stdout.contains("PObserve run completed with 0 error(s)"), "{}", h.stdout);
}

#[test]
fn unhandled_events_and_out_of_order_inputs_are_internal_errors() {
    let mut h = host();
    h.add_file(
        "run.log",
        &[r#"{"key":"k","timestamp":1,"event":"eClose","payload":7}"#.to_string(), line("k", 2, "eDeposit", 1, 1)]
            .join("\n"),
    );
    assert_eq!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "run.log", "-o", "out"]), 3, "{}", h.stdout);
    assert!(h.stdout.contains("prt.State[Closed] missing event handler for eDeposit\n"), "{}", h.stdout);
    assert!(
        h.stdout.contains("Failed with an unknown exception:: null\nPlease report this issue to the P team\n"),
        "{}",
        h.stdout
    );
    let unknown = read(&h, "out/UnknownError_0.txt");
    assert!(
        unknown.starts_with(
            "errorMessage=prt.State prt.State[Closed] has no handler for class Bank.pobserve.PEvents$eDeposit"
        ),
        "{}",
        unknown
    );

    h.add_file("run.log", &[line("k", 2, "eDeposit", 1, 1), line("k", 1, "eDeposit", 1, 1)].join("\n"));
    assert_eq!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "run.log", "-o", "out2", "--assumeInputsAreSorted"]), 3);
    assert!(
        h.stdout.contains("PObserve Event Out of Order Exception::\nEncountered an out of order event.\n"),
        "{}",
        h.stdout
    );
    assert!(h.stdout.contains("Timestamp of last processed event: 1970-01-01T00:00:00.002Z\n"), "{}", h.stdout);
}

#[test]
fn keys_filter_and_parser_configuration() {
    let mut h = host();
    let log = [
        r#"{"msg": {"kind": "eWithdraw", "acct": 9, "at": "2024-02-02T17:27:34.309Z", "body": [9, 100]}}"#,
        r#"{"msg": {"kind": "eWithdraw", "acct": 8, "at": "2024-02-02T17:27:35Z", "body": {"account": 8, "amount": 1}}}"#,
    ]
    .join("\n");
    h.add_file("run.log", &log);
    let pc = r#"{"event": "msg.kind", "key": "msg.acct", "timestamp": "msg.at", "payload": "msg.body"}"#;
    let code = run(&mut h, &["--spec", "NeverOverdrawn", "-l", "run.log", "-o", "out", "-pc", pc, "--keys", "9"]);
    assert_eq!(code, 2, "{}", h.stdout);
    let replay = read(&h, "out/replayEvents_9.log");
    assert!(replay.contains("errorTimeStamp=1706894854309\n"), "{}", replay);
    assert!(replay.contains("timestamp=2024-02-02T17:27:34.309000000Z"), "{}", replay);
    assert!(!h.is_file(Path::new("out/replayEvents_8.log")));
}

#[test]
fn json_traces() {
    let mut h = host();
    let trace = r#"[
      {"type": "CreateStateMachine", "details": {"log": "created"}},
      {"type": "SendEvent", "details": {"log": "send 1", "event": "eDeposit", "payload": {"account": 1, "amount": 3}}},
      {"type": "Announce", "details": {"log": "announce 2", "event": "PImplementation.eWithdraw", "payload": {"account": 1, "amount": 4}}}
    ]"#;
    h.add_file("trace.json", trace);
    let code = run(&mut h, &["--spec", "NeverOverdrawn", "-l", "trace.json", "-o", "out", "-ik", "json"]);
    assert_eq!(code, 2, "{}", h.stdout);
    assert!(h.stdout.contains("  inputKind            = JSON\n"));
    assert!(read(&h, "out/replayEvents_.log").contains("logLine='announce 2'"));
}

#[test]
fn command_line_errors() {
    let mut h = host();
    h.add_file("run.log", "");
    let usage = |h: &MemHost| h.stdout.contains("Usage: p observe [options]");
    let cases: &[(&[&str], &str)] = &[
        (&["-l", "run.log"], "The following option is required: [--spec]"),
        // JCommander's required option comes first
        (&["-l", "run.log", "--socket-mode"], "The following option is required: [--spec]"),
        (&["--spec", "NeverOverdrawn", "--socket-mode"], "Socket mode (--socket-mode) is not supported by p observe."),
        (
            &["--spec", "NeverOverdrawn", "--jars", "x.jar"],
            "Was passed main parameter '--jars' but no main parameter was defined in your arg class",
        ),
        (&["--spec", "Bad.Name"], "The specified class name spec (Bad.Name) is invalid."),
        (
            &["--spec", "NeverOverdrawn"],
            "No log files specified. Please provide log file or directory with --logs parameter.",
        ),
        (&["--spec", "NeverOverdrawn", "-l", "nope"], "The specified log location(nope) does not exist."),
        (
            &["--spec", "NeverOverdrawn", "-l", "run.log", "--replaySize", "0"],
            "The specified replaySize (0) is not valid (Must be an integer > 0)",
        ),
        (&["--spec", "NeverOverdrawn", "-l", "run.log", "-ik", "xml"], "The specified input kind (xml) is not valid."),
        (&["--spec", "NeverOverdrawn", "-l", "run.log", "--keys"], "Expected a value after parameter --keys"),
        (&["--spec", "Missing", "-l", "run.log"], "SPECIFICATION_SUPPLIER not found in the specified jar."),
        (&["--spec", "NeverOverdrawn", "-l", "run.log", "-p", "Nope"], "PARSER not found in the specified jar."),
    ];
    for (args, msg) in cases {
        assert_eq!(run(&mut h, args), 1, "{:?}", args);
        assert!(h.stdout.starts_with(&format!("Failed parsing ::\n{}\n", msg)), "{:?}\n{}", args, h.stdout);
        assert!(usage(&h));
    }
    assert_eq!(run(&mut h, &["-h"]), 0);
    assert!(h.stdout.starts_with("Usage: p observe [options]") && !h.stdout.contains("--jars"));

    // no project to take the spec from
    let mut bare = MemHost::new("/work");
    bare.add_file("run.log", "");
    assert_eq!(run(&mut bare, &["--spec", "S", "-l", "run.log"]), 1);
    assert!(bare.stdout.contains("No P project file (*.pproj) found in the current folder"), "{}", bare.stdout);
    // type errors are shown
    bare.add_file("bad.p", "spec S observes e { start state A {} }");
    assert_eq!(run(&mut bare, &["--spec", "S", "-l", "run.log", "-pf", "bad.p"]), 1);
    assert!(bare.stdout.contains("[Error:]"), "{}", bare.stdout);
}

#[test]
fn custom_parsers_plug_in() {
    use snappea::checker::observe::{ObservedEvent, Parser, Spec, Timestamp};
    use snappea::checker::value::Value;

    struct Csv;
    impl Parser for Csv {
        fn name(&self) -> &str {
            "Csv"
        }
        fn parse(&self, entry: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
            let f: Vec<&str> = entry.split(',').collect();
            let n = |i: usize| f[i].trim().parse::<i64>().map_err(|e| e.to_string());
            let ev = spec.tuple_event(
                f[1],
                Timestamp::from_millis(n(0)?),
                f[2],
                vec![Value::Int(n(3)?), Value::Int(n(4)?)],
                Some(entry),
            )?;
            Ok(vec![ev])
        }
    }
    let mut h = host();
    h.add_file("run.csv", "1,k,eDeposit,1,5\n2,k,eWithdraw,1,6");
    let args: Vec<String> =
        ["--spec", "NeverOverdrawn", "-l", "run.csv", "-o", "out"].iter().map(|s| s.to_string()).collect();
    let code = snappea::compat::observe::run_with(&args, &mut h, vec![Box::new(Csv)]);
    assert_eq!(code, 2, "{}", h.stdout);
    assert!(h.stdout.contains("account 1 overdrawn: balance -1"), "{}", h.stdout);
}

#[test]
fn regression_parsers_compile() {
    assert_eq!(support::pobserve_parsers::all().len(), 6);
}

const LOCK: &str = r#"
event eEnter: (client: string);
event eExit: (client: string);
event eRequest: (client: string);
event eGrant: (client: string);

spec MutualExclusion observes eEnter, eExit {
  var holder: string;
  start state Free {
    on eEnter do (p: (client: string)) { holder = p.client; goto Held; }
  }
  state Held {
    on eEnter do (p: (client: string)) {
      assert false, format("{0} entered while {1} is inside", p.client, holder);
    }
    on eExit goto Free;
  }
}

spec RequestsGranted observes eRequest, eGrant {
  start cold state Idle { on eRequest goto Waiting; ignore eGrant; }
  hot state Waiting { on eGrant goto Idle; ignore eRequest; }
}
"#;

fn lock_line(key: &str, ts: &str, event: &str, client: &str) -> String {
    format!(
        r#"{{"key": "{}", "timestamp": "{}", "event": "{}", "payload": {{"client": "{}"}}, "src": "x.py:1"}}"#,
        key, ts, event, client
    )
}

fn lock_host(lines: &[String]) -> MemHost {
    let mut h = MemHost::new("/work");
    h.add_file("lock.p", LOCK);
    h.add_file("run.log", &lines.join("\n"));
    h
}

#[test]
fn several_specs_in_one_run() {
    let lines = [
        lock_line("L1", "2026-10-18T00:00:00.000000001Z", "eEnter", "c1"),
        lock_line("L1", "2026-10-18T00:00:00.000000002Z", "eEnter", "c2"),
        lock_line("L1", "2026-10-18T00:00:00.000000003Z", "eRequest", "c3"),
    ];
    let mut h = lock_host(&lines);
    let base = ["-pf", "lock.p", "-l", "run.log", "-o", "out"];
    let args = |extra: &[&'static str]| -> Vec<&str> { extra.iter().copied().chain(base.iter().copied()).collect() };

    // both named: one violation, no liveness check
    assert_eq!(run(&mut h, &args(&["--spec", "MutualExclusion", "RequestsGranted"])), 2, "{}", h.stdout);
    assert!(h.stdout.contains("specificationName    = MutualExclusion, RequestsGranted"), "{}", h.stdout);
    assert!(h.stdout.contains("Checking spec RequestsGranted"));
    assert!(h.stdout.contains("c2 entered while c1 is inside"));
    assert!(h.stdout.contains("PObserve run completed with 1 error(s)"));
    assert!(read(&h, "/work/out/MutualExclusion/replayEvents_L1.log").contains("c2 entered"));
    assert!(read(&h, "/work/out/RequestsGranted/PObserveMetrics.txt").contains("eRequest"));

    // all specs, with the hot-state check
    assert_eq!(run(&mut h, &args(&["--all-specs", "--check-hot", "--json-report", "r.json"])), 2);
    let r: serde_json::Value = serde_json::from_str(&read(&h, "/work/r.json")).unwrap();
    assert_eq!(r["exit_code"], 2);
    let specs = r["specs"].as_array().unwrap();
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0]["spec"], "MutualExclusion");
    let e = &specs[0]["errors"][0];
    assert_eq!(e["kind"], "spec_violation");
    assert_eq!(e["key"], "L1");
    assert_eq!(e["message"], "Assertion failure: lock.p:14:7 c2 entered while c1 is inside");
    let evs = e["events"].as_array().unwrap();
    assert_eq!(evs.len(), 2);
    assert_eq!(evs[1]["event"], "eEnter");
    assert_eq!(evs[1]["timestamp"], "2026-10-18T00:00:00.000000002Z");
    assert!(evs[1]["log_line"].as_str().unwrap().contains(r#""src": "x.py:1""#));
    let hot = &specs[1]["errors"][0];
    assert_eq!(
        hot["message"],
        "PImplementation.RequestsGranted detected liveness bug in hot state 'Waiting' at the end of program execution."
    );

    // exclusions
    assert_eq!(run(&mut h, &args(&["--all-specs", "--exclude-specs", "MutualExclusion", "--check-hot"])), 2);
    assert!(!h.stdout.contains("Checking spec"), "{}", h.stdout);
    assert!(h.stdout.contains("hot state 'Waiting'"));
    assert_eq!(run(&mut h, &args(&["--all-specs", "--exclude-specs", "MutualExclusion"])), 0, "{}", h.stdout);
}

#[test]
fn clean_runs_write_an_empty_json_report() {
    let lines = [
        lock_line("L1", "2026-10-18T00:00:00.000000001Z", "eEnter", "c1"),
        lock_line("L1", "2026-10-18T00:00:00.000000002Z", "eExit", "c1"),
        lock_line("L1", "2026-10-18T00:00:00.000000003Z", "eRequest", "c3"),
        lock_line("L1", "2026-10-18T00:00:00.000000004Z", "eGrant", "c3"),
    ];
    let mut h = lock_host(&lines);
    let code =
        run(&mut h, &["--all-specs", "--check-hot", "--json-report", "r.json", "-pf", "lock.p", "-l", "run.log"]);
    assert_eq!(code, 0, "{}", h.stdout);
    let r: serde_json::Value = serde_json::from_str(&read(&h, "/work/r.json")).unwrap();
    assert_eq!(r["exit_code"], 0);
    assert_eq!(r["specs"][0]["events_read"], 2);
    assert_eq!(r["specs"][1]["errors"].as_array().unwrap().len(), 0);
}

#[test]
fn spec_selection_errors() {
    let mut h = lock_host(&[]);
    let cases: &[(&[&str], &str)] = &[
        (&["--spec", "MutualExclusion", "--all-specs"], "'--spec' and '--all-specs' cannot be combined"),
        (&["--spec", "MutualExclusion", "--exclude-specs", "X"], "--exclude-specs needs --all-specs."),
        (&["--all-specs", "--exclude-specs", "Nope"], "--exclude-specs: there is no spec named Nope."),
        // spec selection is reported before the input mode
        (&["--spec", "MutualExclusion", "--exclude-specs", "X", "--stdin"], "--exclude-specs needs --all-specs."),
        (
            &["--spec", "MutualExclusion", "--all-specs", "--socket-mode"],
            "'--spec' and '--all-specs' cannot be combined",
        ),
    ];
    for (args, msg) in cases {
        let args: Vec<&str> = args.iter().copied().chain(["-pf", "lock.p", "-l", "run.log"]).collect();
        assert_eq!(run(&mut h, &args), 1, "{:?}", args);
        assert!(h.stdout.starts_with(&format!("Failed parsing ::\n{}\n", msg)), "{:?}\n{}", args, h.stdout);
    }
}

const TIMED_SPEC: &str = r#"
fun Now(): int;
fun NowBest(): int;
fun Mark(): int;
fun Elapsed(m: int): int;
fun ElapsedBest(m: int): int;
event eBegin: int;
event eEnd: (now: int, elapsed: int);
spec Budget observes eBegin, eEnd {
  var entryMark: int;
  var requestMark: int;
  start state Ready {
    entry {
      assert Now() == 0 && NowBest() == 0;
      entryMark = Mark();
      assert entryMark == 0;
    }
    on eBegin do (expected: int) {
      assert Now() == expected && NowBest() == expected;
      assert Elapsed(entryMark) == expected;
      requestMark = Mark();
      assert requestMark == 1;
      goto Waiting;
    }
  }
  state Waiting {
    entry { assert Elapsed(requestMark) == 0; }
    on eEnd do (expected: (now: int, elapsed: int)) {
      assert Now() == expected.now && NowBest() == expected.now;
      assert Elapsed(requestMark) == expected.elapsed;
      assert ElapsedBest(requestMark) == expected.elapsed;
      assert Elapsed(requestMark) <= 2000,
        format("request took {0}us", Elapsed(requestMark));
    }
  }
}
"#;

fn timed_host(source: &str, log: &str) -> MemHost {
    let mut h = MemHost::new("/work");
    h.add_file("timed.p", source);
    h.add_file("run.log", log);
    h
}

fn run_timed(h: &mut MemHost, extra: &[&str]) -> i32 {
    let mut args = vec!["-pf", "timed.p", "--spec", "Budget", "-l", "run.log", "-o", "out"];
    args.extend_from_slice(extra);
    run(h, &args)
}

#[test]
fn observed_clocks_use_sorted_timestamps_and_independent_stopwatches_per_key() {
    // Deliberately shuffled, two independent keys, ISO nanoseconds and epoch millis.
    // Equal timestamps and fractions within one microsecond do not advance the clock.
    let log = r#"
{"key":"a","timestamp":"2024-02-02T17:27:34.311999999Z","event":"eEnd","payload":{"now":1706894854311999,"elapsed":1999}}
{"key":"b","timestamp":1706894855000,"event":"eBegin","payload":1706894855000000}
{"key":"a","timestamp":"2024-02-02T17:27:34.310000001Z","event":"eBegin","payload":1706894854310000}
{"key":"b","timestamp":1706894855002,"event":"eEnd","payload":{"now":1706894855002000,"elapsed":2000}}
{"key":"a","timestamp":"2024-02-02T17:27:34.310000001Z","event":"eEnd","payload":{"now":1706894854310000,"elapsed":0}}
{"key":"a","timestamp":"2024-02-02T17:27:34.310000999Z","event":"eEnd","payload":{"now":1706894854310000,"elapsed":0}}
"#;
    let mut h = timed_host(TIMED_SPEC, log);
    assert_eq!(run_timed(&mut h, &[]), 0, "{}", h.stdout);
    assert!(h.stdout.contains("Success! No bugs found."), "{}", h.stdout);
}

#[test]
fn observed_budget_violations_report_real_duration_and_replay_timestamps() {
    let log = r#"
{"key":"slow","timestamp":1706894855000,"event":"eBegin","payload":1706894855000000}
{"key":"slow","timestamp":1706894855003,"event":"eEnd","payload":{"now":1706894855003000,"elapsed":3000}}
{"key":"fast","timestamp":1706894856000,"event":"eBegin","payload":1706894856000000}
{"key":"fast","timestamp":1706894856001,"event":"eEnd","payload":{"now":1706894856001000,"elapsed":1000}}
"#;
    let mut h = timed_host(TIMED_SPEC, log);
    assert_eq!(run_timed(&mut h, &["--json-report", "r.json"]), 2, "{}", h.stdout);
    let report: serde_json::Value = serde_json::from_str(&read(&h, "r.json")).unwrap();
    assert_eq!(report["specs"][0]["verified_keys"], 1);
    let err = &report["specs"][0]["errors"][0];
    assert_eq!(err["kind"], "spec_violation");
    assert!(err["message"].as_str().unwrap().ends_with("request took 3,000us"), "{}", err);
    assert_eq!(err["events"][1]["timestamp"], "2024-02-02T17:27:35.003Z");
    assert!(read(&h, "out/replayEvents_slow.log").contains("request took 3,000us"));
}

#[test]
fn observed_time_errors_use_the_existing_diagnostics() {
    for (declarations, call, expected) in [
        ("fun Elapsed(m: int): int;", "assert Elapsed(0) == 0;", "not a handle returned by Mark()"),
        ("fun Elapse(lo: int, hi: int);", "Elapse(1, 1);", "Elapse cannot be called from a monitor"),
        ("fun ElapseCost(name: string);", "ElapseCost(\"work\");", "ElapseCost cannot be called from a monitor"),
    ] {
        let source = format!(
            "{} event e; spec Budget observes e {{ start state S {{ on e do {{ {} }} }} }}",
            declarations, call
        );
        let mut h = timed_host(&source, r#"{"event":"e","timestamp":1}"#);
        assert_eq!(run_timed(&mut h, &[]), 3, "{}", h.stdout);
        assert!(h.stdout.contains(expected), "{}", h.stdout);
    }
}

#[test]
fn observe_validates_time_signatures_even_without_events() {
    let mut h = timed_host("fun Now(x: int): int; event e; spec Budget observes e { start state S {} }", "");
    assert_eq!(run_timed(&mut h, &[]), 1, "{}", h.stdout);
    assert!(h.stdout.contains("must be declared as fun Now(): int;"), "{}", h.stdout);
}

#[test]
fn timed_observation_still_rejects_out_of_order_inputs() {
    let log = r#"
{"timestamp":10,"event":"eBegin","payload":10000}
{"timestamp":9,"event":"eEnd","payload":{"now":9000,"elapsed":-1000}}
"#;
    let mut h = timed_host(TIMED_SPEC, log);
    assert_eq!(run_timed(&mut h, &["--assumeInputsAreSorted"]), 3, "{}", h.stdout);
    assert!(h.stdout.contains("PObserve Event Out of Order Exception"), "{}", h.stdout);
    assert!(!h.stdout.contains("PObserve Spec Violation"), "{}", h.stdout);
}

#[test]
fn strict_requires_relevant_events_after_filtering_and_preserves_other_errors() {
    for log in ["", "{\"event\":\"eUnrelated\"}"] {
        let mut h = host();
        h.add_file("logs/run.log", log);
        assert_eq!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "logs"]), 0);
        assert_ne!(
            run(&mut h, &["--spec", "NeverOverdrawn", "-l", "logs", "--strict", "--json-report", "report.json"]),
            0
        );
        assert!(h.stdout.contains("checked no relevant events"), "{}", h.stdout);
        assert!(!h.stdout.contains("Please report this issue"));
        assert!(read(&h, "report.json").contains("\"kind\": \"validation\""));
    }
    let mut h = host();
    h.add_file("logs/run.log", &line("a", 1, "eDeposit", 1, 10));
    assert_eq!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "logs", "--strict"]), 0);
    assert_ne!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "logs", "--strict", "--keys", "b"]), 0);
    h.add_file("logs/run.log", "not json");
    assert_ne!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "logs", "--strict"]), 0);
    assert!(h.stdout.contains("Parser Exception"));
    assert!(!h.stdout.contains("checked no relevant events"));
}

#[test]
fn strict_applies_to_every_selected_spec() {
    let mut h = host();
    h.add_file("PSpec/Other.p", "spec Other observes eUnrelated { start state Ready { on eUnrelated do {} } }");
    h.add_file("logs/run.log", &line("a", 1, "eDeposit", 1, 10));
    assert_ne!(run(&mut h, &["--all-specs", "-l", "logs", "--strict"]), 0);
    assert!(h.stdout.contains("spec 'Other' checked no relevant events"), "{}", h.stdout);
}

#[test]
fn strict_validation_does_not_mask_a_sibling_safety_failure() {
    let mut h = host();
    h.add_file("PSpec/Other.p", "spec Other observes eUnrelated { start state Ready { on eUnrelated do {} } }");
    h.add_file("logs/run.log", &line("a", 1, "eWithdraw", 1, 10));
    assert_eq!(
        run(&mut h, &["--all-specs", "-l", "logs", "--strict", "--json-report", "report.json"]),
        snappea::driver::observe::EXIT_ASSERTION
    );
    let report = read(&h, "report.json");
    assert!(report.contains("spec_violation"));
    assert!(report.contains("validation"));
}

#[test]
fn unknown_events_are_ignored_warned_or_errors_but_declared_ones_always_pass() {
    let log = [
        line("a", 1, "eDeposit", 1, 10),
        line("a", 2, "eDepost", 1, 10),
        line("a", 3, "eDepost", 1, 10),
        "{\"event\": \"eUnrelated\"}".to_string(),
    ]
    .join("\n");
    let mut h = host();
    h.add_file("logs/run.log", &log);
    let base = ["--spec", "NeverOverdrawn", "-l", "logs", "--json-report", "report.json"];
    let with = |policy: &'static str| [&base[..], &["--unknown-events", policy]].concat();

    assert_eq!(run(&mut h, &base), 0);
    let plain = h.stdout.clone();
    assert!(h.stderr.is_empty());
    assert_eq!(run(&mut h, &with("ignore")), 0);
    assert_eq!(h.stdout, plain);
    assert!(!read(&h, "report.json").contains("unknown_events"));

    assert_eq!(run(&mut h, &with("warn")), 0);
    assert_eq!(h.stdout, plain, "warnings go to stderr only");
    assert_eq!(
        h.stderr,
        "warning: Unknown event 'eDepost': not declared in the P sources (2 occurrences, first at logs/run.log:2); did you mean eDeposit?\n"
    );
    assert!(read(&h, "report.json").contains("\"warnings\""));

    assert_eq!(run(&mut h, &with("error")), snappea::driver::observe::EXIT_INTERNAL_ERROR);
    assert!(h.stdout.contains("Total Unknown Event Errors: 1"), "{}", h.stdout);
    let report = read(&h, "report.json");
    assert!(report.contains("\"kind\": \"unknown_event\""));
    assert!(!report.contains("eUnrelated"), "declared events are never unknown");

    // an unknown name with a violation: the input is untrustworthy, so it decides the status
    h.add_file("logs/run.log", &[log.clone(), line("a", 4, "eWithdraw", 1, 99)].join("\n"));
    assert_eq!(run(&mut h, &with("error")), snappea::driver::observe::EXIT_INTERNAL_ERROR);
    assert!(read(&h, "report.json").contains("spec_violation"));
    assert_eq!(run(&mut h, &with("warn")), snappea::driver::observe::EXIT_ASSERTION);
}

#[test]
fn several_log_paths_combine_into_one_input_set() {
    let mut h = host();
    h.add_file("a/1.log", &line("x", 1, "eDeposit", 1, 10));
    h.add_file("b/2.log", &line("x", 2, "eWithdraw", 1, 10));
    h.add_file("c.log", &line("x", 3, "eWithdraw", 1, 0));
    // the withdrawal alone overdraws; with the deposit from another path it does not
    assert_eq!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "b"]), 2, "{}", h.stdout);
    for args in [
        &["-l", "a", "b", "c.log"][..],
        &["-l", "a", "-l", "b/2.log", "-l", "c.log"],
        &["-l", "a", "b", "-l", "c.log", "a/1.log", "a"],
    ] {
        let args: Vec<&str> =
            ["--spec", "NeverOverdrawn", "--json-report", "r.json"].into_iter().chain(args.iter().copied()).collect();
        assert_eq!(run(&mut h, &args), 0, "{}", h.stdout);
        let report: serde_json::Value = serde_json::from_str(&read(&h, "r.json")).unwrap();
        assert_eq!(report["specs"][0]["events_read"], 3, "files read once each: {:?}", args);
    }
    let code = run(&mut h, &["--spec", "NeverOverdrawn", "-l", "a", "nope"]);
    assert_eq!(code, 1);
    assert!(h.stdout.contains("The specified log location(nope) does not exist."), "{}", h.stdout);
}

#[test]
fn log_globs_are_expanded_by_snappea() {
    let mut h = host();
    h.add_file("logs/day1/a.log", &line("x", 1, "eDeposit", 1, 10));
    h.add_file("logs/day2/deep/b.log", &line("x", 2, "eWithdraw", 1, 10));
    h.add_file("logs/day2/b.txt", &line("x", 3, "eWithdraw", 1, 10));
    h.add_file("logs/.hidden/c.log", &line("x", 4, "eWithdraw", 1, 10));
    let events = |h: &mut MemHost, pattern: &str| {
        let code = run(h, &["--spec", "NeverOverdrawn", "--json-report", "r.json", "-l", pattern]);
        let report: serde_json::Value = serde_json::from_str(&read(h, "r.json")).unwrap();
        (code, report["specs"][0]["events_read"].as_u64().unwrap())
    };
    assert_eq!(events(&mut h, "logs/**/*.log"), (0, 2));
    assert_eq!(events(&mut h, "/work/logs/day?/a.log"), (0, 1));
    assert_eq!(events(&mut h, "logs/day[12]/*.txt"), (2, 1));
    // a matched directory reads its files, as `-l dir` does
    assert_eq!(events(&mut h, "logs/day*"), (0, 2));
    assert_eq!(run(&mut h, &["--spec", "NeverOverdrawn", "-l", "logs/**/*.csv"]), 1);
    assert!(h.stdout.contains("The specified log location(logs/**/*.csv) matches no files."), "{}", h.stdout);
}

#[test]
fn live_inputs_merge_by_sequence_and_record_the_checked_order() {
    let ev = |seq: u64, event: &str, amount: i64| {
        format!(r#"{{"seq": {seq}, "key": "x", "event": "{event}", "payload": {{"account": 1, "amount": {amount}}}}}"#)
    };
    let live = |h: &mut MemHost, extra: &[&str]| {
        let args: Vec<&str> =
            ["--spec", "NeverOverdrawn", "--fail-fast", "--json-report", "r.json", "--live", "a", "b"]
                .into_iter()
                .chain(extra.iter().copied())
                .collect();
        run(h, &args)
    };
    let mut h = host();
    // `a` is read first, but its withdrawal comes after `b`'s deposit
    h.add_file("a", &[ev(2, "eWithdraw", 10), ev(3, "eDeposit", 1)].join("\n"));
    h.add_file("b", &format!("{}\n\n", ev(1, "eDeposit", 10)));
    assert_eq!(live(&mut h, &["--sequence", "seq", "--record", "events.jsonl"]), 0, "{}", h.stdout);
    let report: serde_json::Value = serde_json::from_str(&read(&h, "r.json")).unwrap();
    assert_eq!(report["input_complete"], true);
    assert_eq!(report["stop_reason"], "eof");
    let recorded: Vec<u64> = read(&h, "events.jsonl")
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["seq"].as_u64().unwrap())
        .collect();
    assert_eq!(recorded, [1, 2, 3]);

    assert_eq!(live(&mut h, &[]), 1);
    assert!(h.stdout.contains("several live inputs need --sequence"), "{}", h.stdout);

    // entries are located by their own input and line, not by check order
    h.add_file("b", &format!("{}\n{}", ev(1, "eDeposit", 10), r#"{"seq": 4, "key": "x", "event": "eDepost"}"#));
    assert_eq!(live(&mut h, &["--sequence", "seq", "--unknown-events", "warn"]), 0, "{}", h.stdout);
    assert!(h.stderr.contains("first at b:2)"), "{}", h.stderr);

    let failures = [
        (vec![ev(1, "eDeposit", 10), ev(3, "eDeposit", 1)], "sequence number 2 is missing"),
        (vec![ev(1, "eDeposit", 10), ev(1, "eDeposit", 1)], "duplicate sequence number 1"),
        (vec![r#"{"key": "x"}"#.to_string()], "no whole-number sequence field 'seq'"),
    ];
    for (lines, message) in failures {
        h.add_file("a", &lines.join("\n"));
        h.add_file("b", "");
        assert_eq!(live(&mut h, &["--sequence", "seq"]), 3, "{}", h.stdout);
        let report = read(&h, "r.json");
        assert!(report.contains(message), "{report}");
        assert!(report.contains("\"input_complete\": false"), "{report}");
    }
}

#[test]
fn live_option_errors() {
    let mut h = host();
    h.add_file("run.log", "");
    let cases: &[(&[&str], &str)] = &[
        (&["--stdin", "--live", "-", "--fail-fast"], "stdin can be read only once"),
        (&["--live", "a"], "requires both --stdin and --fail-fast"),
        (&["-l", "run.log", "--sequence", "seq"], "'--sequence' and '--record' apply to live input"),
        (&["-l", "run.log", "--record", "out.jsonl"], "'--sequence' and '--record' apply to live input"),
        (&["--live", "a", "-l", "run.log", "--fail-fast"], "cannot be combined with --logs"),
        (&["--live", "a", "-ik", "json", "--fail-fast"], "requires TEXT input"),
    ];
    for (args, message) in cases {
        let args: Vec<&str> = ["--spec", "NeverOverdrawn"].into_iter().chain(args.iter().copied()).collect();
        assert_eq!(run(&mut h, &args), 1, "{:?}", args);
        assert!(h.stdout.contains(message), "{:?}: {}", args, h.stdout);
    }
}
