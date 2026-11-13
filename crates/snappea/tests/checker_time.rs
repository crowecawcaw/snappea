//! Time tracking (`design/time/PROPOSAL.md`): causal clocks, latency profiles,
//! `--timed` scheduling and schedule headers.

use std::path::Path;
use std::sync::Arc;

use snappea::checker::engine::{CheckResult, Checker, CheckerOptions};
use snappea::checker::strategy::{ScheduleTrace, StrategyKind};
use snappea::checker::time::ProfileSet;

const TIME_DECLS: &str = "
fun Now(): int;
fun NowBest(): int;
fun Elapse(lo: int, hi: int);
fun ElapseCost(name: string);
fun Mark(): int;
fun Elapsed(m: int): int;
fun ElapsedBest(m: int): int;
";

fn compile(files: &[(String, String)]) -> snappea::sema::Analysis {
    let mut sm = snappea::syntax::SourceMap::new();
    for (name, text) in files {
        sm.add(name.clone(), text.clone());
    }
    snappea::sema::compile(&sm, &snappea::sema::Options::default()).unwrap_or_else(|_| panic!("compile"))
}

fn check(a: &snappea::sema::Analysis, test: &str, opts: &CheckerOptions) -> Result<CheckResult, String> {
    let tc = a.program.test_cases().into_iter().find(|t| t.name == test).unwrap();
    Checker::new(&a.program).check(&tc, opts, &mut Vec::<String>::new())
}

fn retry_budget(edit: impl Fn(String) -> String) -> (snappea::sema::Analysis, Arc<ProfileSet>) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/time/RetryBudget");
    let mut files = Vec::new();
    for sub in ["PSrc", "PSpec", "PTst"] {
        let mut paths: Vec<_> = std::fs::read_dir(dir.join(sub)).unwrap().map(|e| e.unwrap().path()).collect();
        paths.sort();
        for p in paths.into_iter().filter(|p| p.extension().is_some_and(|e| e == "p")) {
            files.push((p.display().to_string(), edit(std::fs::read_to_string(&p).unwrap())));
        }
    }
    let profiles = ProfileSet::parse(&std::fs::read_to_string(dir.join("PTst/latency.toml")).unwrap()).unwrap();
    (compile(&files), Arc::new(profiles))
}

#[test]
fn retry_budget_finds_the_give_up_path_and_accepts_the_fix() {
    let (a, profiles) = retry_budget(|s| s);
    for timed in [false, true] {
        let opts =
            CheckerOptions { iterations: 500, seed: 1, latency: Some(profiles.clone()), timed, ..Default::default() };
        let r = check(&a, "tcRetry", &opts).unwrap();
        assert_eq!(r.report.num_bugs, 1, "timed={}", timed);
        let bug = &r.report.bug_reports[0];
        assert!(bug.contains("ok = False") && bug.contains("1050000us"), "{}", bug);
        let log = r.readable_trace.unwrap();
        assert!(log.contains("<TimeLog> 'Client(3)' clock is 1050000 us."), "{}", log);
        assert!(r.schedule_trace.unwrap().contains(&format!("--snappea-time:timed={};profile=same-az\n", timed as u8)));
    }

    let (fixed, _) = retry_budget(|s| s.replace("ms(300)", "ms(250)"));
    for timed in [false, true] {
        let opts =
            CheckerOptions { iterations: 500, seed: 1, latency: Some(profiles.clone()), timed, ..Default::default() };
        let r = check(&fixed, "tcRetry", &opts).unwrap();
        assert_eq!(r.report.num_bugs, 0, "{:?}", r.report.bug_reports);
    }
}

#[test]
fn replay_takes_time_settings_from_the_schedule() {
    let (a, profiles) = retry_budget(|s| s);
    let opts =
        CheckerOptions { iterations: 500, seed: 2, latency: Some(profiles.clone()), timed: true, ..Default::default() };
    let r = check(&a, "tcRetry", &opts).unwrap();
    let text = r.schedule_trace.unwrap();
    assert!(text.contains("--snappea-time:timed=1;profile=same-az\n"), "{}", text);
    assert!(text.contains("--snappea-latency:eReq=1000us..2000us\n"), "{}", text);
    let trace = ScheduleTrace::parse(&text).unwrap();
    assert_eq!(trace.to_text(), text);

    // no latency file and no --timed needed; other time options are ignored
    for (latency, latency_profile) in [(None, None), (Some(profiles.clone()), Some("cross-region".to_string()))] {
        let replay = CheckerOptions {
            strategy: StrategyKind::Replay,
            replay: Some(trace.clone()),
            latency,
            latency_profile,
            ..Default::default()
        };
        let again = check(&a, "tcRetry", &replay).unwrap();
        assert_eq!(again.report.bug_reports, r.report.bug_reports);
        assert!(again.replay_error.is_none());
        assert!(again.last_log.iter().any(|l| l == "<TimeLog> 'Client(3)' clock is 1050000 us."));
    }
}

const RACE: &str = "
event eReq: Client;
event eResp;
event eTimeout;

machine Timer {
  start state Arming {
    entry (c: Client) { Elapse(100000, 100000); send c, eTimeout; }
  }
}

machine Server {
  start state Serving {
    on eReq do (c: Client) { Elapse(1000, 5000); send c, eResp; }
  }
}

machine Client {
  start state Init {
    entry (s: Server) { send s, eReq, this; new Timer(this); }
    on eResp goto Done;
    on eTimeout do { assert false, format(\"timed out at {0}\", Now()); }
  }
  state Done { ignore eTimeout; }
}

machine Main {
  start state Init { entry { new Client(new Server()); } }
}

test tcRace [main=Main]: { Main, Client, Server, Timer };
";

#[test]
fn timed_scheduling_drops_impossible_orders() {
    let a = compile(&[("decls.p".into(), TIME_DECLS.into()), ("race.p".into(), RACE.into())]);
    let profiles = Arc::new(ProfileSet::parse("[profile.lan]\ndefault = \"1ms..2ms\"").unwrap());
    let opts = |timed| CheckerOptions {
        iterations: 300,
        seed: 7,
        latency: Some(profiles.clone()),
        latency_profile: Some("lan".into()),
        timed,
        ..Default::default()
    };
    // untimed, the 100ms timeout may be handled before the ~10ms reply
    let r = check(&a, "tcRace", &opts(false)).unwrap();
    assert_eq!(r.report.num_bugs, 1);
    assert!(r.report.bug_reports[0].contains("timed out at 102000"), "{}", r.report.bug_reports[0]);
    let r = check(&a, "tcRace", &opts(true)).unwrap();
    assert_eq!(r.report.num_bugs, 0, "{:?}", r.report.bug_reports);
}

const STOPWATCH: &str = "
machine Main {
  start state Init {
    entry {
      var m: int;
      var n: int;
      m = Mark();
      Elapse(10, 20);
      n = Mark();
      Elapse(5, 5);
      assert Elapsed(m) == 25 && ElapsedBest(m) == 15, format(\"m: {0} {1}\", ElapsedBest(m), Elapsed(m));
      // shared history cancels: n - m is exactly the second draw
      assert Elapsed(n) == 5 && ElapsedBest(n) == 5, format(\"n: {0} {1}\", ElapsedBest(n), Elapsed(n));
      assert Now() == 25 && NowBest() == 15;
      ElapseCost(\"Query\");
      assert Elapsed(n) == 5 + 8000 && ElapsedBest(n) == 5 + 2000;
      assert Now() == 8025;
    }
  }
}

test tcStopwatch [main=Main]: { Main };
";

#[test]
fn stopwatch_functions() {
    let a = compile(&[("decls.p".into(), TIME_DECLS.into()), ("sw.p".into(), STOPWATCH.into())]);
    let profiles = Arc::new(
        ProfileSet::parse("[profile.p]\n\"cost.Query\" = \"2ms..8ms\"\n[tests]\ntcStopwatch = \"p\"").unwrap(),
    );
    let opts = CheckerOptions { latency: Some(profiles), ..Default::default() };
    let r = check(&a, "tcStopwatch", &opts).unwrap();
    assert_eq!(r.report.num_bugs, 0, "{:?}", r.report.bug_reports);
}

#[test]
fn bad_calls_and_names_are_reported() {
    let bad = "machine Main { start state Init { entry { Elapse(5, 1); } } }\ntest tcBad [main=Main]: { Main };";
    let a = compile(&[("decls.p".into(), TIME_DECLS.into()), ("bad.p".into(), bad.into())]);
    let r = check(&a, "tcBad", &CheckerOptions::default()).unwrap();
    assert!(r.report.bug_reports[0].contains("expected 0 <= lo <= hi"), "{:?}", r.report.bug_reports);

    let typo = Arc::new(ProfileSet::parse("[profile.p]\neRequest = \"1ms\"").unwrap());
    let opts = CheckerOptions { latency: Some(typo), latency_profile: Some("p".into()), ..Default::default() };
    let err = check(&a, "tcBad", &opts).unwrap_err();
    assert!(err.contains("unknown event 'eRequest'"), "{}", err);

    let no_profile = Arc::new(ProfileSet::parse("[profile.p]\ndefault = \"1ms\"").unwrap());
    let err =
        check(&a, "tcBad", &CheckerOptions { latency: Some(no_profile.clone()), ..Default::default() }).unwrap_err();
    assert!(err.contains("has no latency profile"), "{}", err);

    let unpriced =
        "machine Main { start state Init { entry { ElapseCost(\"Db\"); } } }\ntest tcCost [main=Main]: { Main };";
    let a = compile(&[("decls.p".into(), TIME_DECLS.into()), ("cost.p".into(), unpriced.into())]);
    let r = check(&a, "tcCost", &CheckerOptions::default()).unwrap();
    assert_eq!(r.report.num_bugs, 0, "no profile: costs are 0");
    let opts = CheckerOptions { latency: Some(no_profile), latency_profile: Some("p".into()), ..Default::default() };
    let r = check(&a, "tcCost", &opts).unwrap();
    assert!(r.report.bug_reports[0].contains("has no entry \"cost.Db\""), "{:?}", r.report.bug_reports);

    let wrong = "fun Now(): bool;\nmachine Main { start state Init { entry { } } }\ntest tcSig [main=Main]: { Main };";
    let a = compile(&[("sig.p".into(), wrong.into())]);
    let err = check(&a, "tcSig", &CheckerOptions::default()).unwrap_err();
    assert!(err.contains("must be declared as fun Now(): int;"), "{}", err);
}

#[test]
fn programs_without_time_are_unchanged() {
    let plain = "
event e;
machine Main {
  start state Init { entry { send this, e; } on e do { assert false; } }
}
test tcPlain [main=Main]: { Main };";
    let a = compile(&[("plain.p".into(), plain.into())]);
    let r = check(&a, "tcPlain", &CheckerOptions::default()).unwrap();
    assert_eq!(r.report.num_bugs, 1);
    assert!(!r.readable_trace.unwrap().contains("<TimeLog>"));
    assert!(!r.schedule_trace.unwrap().contains("--snappea-time"));
}

const ARRIVAL_ORDER: &str = "
event eA: int;

machine Handler {
  var seen: bool;
  start state S {
    on eA do (n: int) {
      if (!seen) { seen = true; assert n == 2, \"handled the later arrival first\"; }
    }
  }
}

machine Receiver {
  start state S {
    entry {
      receive { case eA: (n: int) { assert n == 2, \"received the later arrival first\"; } }
    }
    ignore eA;
  }
}

// sends first, but its event arrives after 300ms
machine Far { start state S { entry (r: machine) { send r, eA, 1; } } }
// sends 10ms later, arriving after 1ms
machine Near { start state S { entry (r: machine) { Elapse(10000, 10000); send r, eA, 2; } } }

machine MainHandler {
  start state S { entry { var r: machine; r = new Handler(); new Far(r); new Near(r); } }
}
machine MainReceiver {
  start state S { entry { var r: machine; r = new Receiver(); new Far(r); new Near(r); } }
}

test tcHandler [main=MainHandler]: { MainHandler, Handler, Far, Near };
test tcReceiver [main=MainReceiver]: { MainReceiver, Receiver, Far, Near };
";

#[test]
fn timed_delivery_follows_arrival_not_send_order() {
    let a = compile(&[("decls.p".into(), TIME_DECLS.into()), ("order.p".into(), ARRIVAL_ORDER.into())]);
    let profiles = Arc::new(
        ProfileSet::parse("[profile.p]\neA = \"1ms\"\n\"(Far, Handler)\" = \"300ms\"\n\"(Far, Receiver)\" = \"300ms\"")
            .unwrap(),
    );
    for test in ["tcHandler", "tcReceiver"] {
        let opts = |timed| CheckerOptions {
            iterations: 200,
            seed: 5,
            latency: Some(profiles.clone()),
            latency_profile: Some("p".into()),
            timed,
            ..Default::default()
        };
        let r = check(&a, test, &opts(false)).unwrap();
        assert_eq!(r.report.num_bugs, 1, "{} untimed", test);
        let r = check(&a, test, &opts(true)).unwrap();
        assert_eq!(r.report.num_bugs, 0, "{} timed: {:?}", test, r.report.bug_reports);
    }
}

#[test]
fn timed_delivery_explores_overlapping_arrivals() {
    // Far arrives in 0..100ms, Near (sent at 10ms) in 50..70ms: either may come first
    let a = compile(&[("decls.p".into(), TIME_DECLS.into()), ("order.p".into(), ARRIVAL_ORDER.into())]);
    let profiles = Arc::new(
        ProfileSet::parse("[profile.p]\neA = \"40ms..60ms\"\n\"(Far, Handler)\" = \"0ms..100ms\"\n\"(Far, Receiver)\" = \"0ms..100ms\"").unwrap(),
    );
    for test in ["tcHandler", "tcReceiver"] {
        let opts = CheckerOptions {
            iterations: 200,
            seed: 5,
            latency: Some(profiles.clone()),
            latency_profile: Some("p".into()),
            timed: true,
            ..Default::default()
        };
        let r = check(&a, test, &opts).unwrap();
        assert_eq!(r.report.num_bugs, 1, "{} timed", test);
        let text = r.schedule_trace.unwrap();
        let replay = CheckerOptions {
            strategy: StrategyKind::Replay,
            replay: Some(ScheduleTrace::parse(&text).unwrap()),
            ..Default::default()
        };
        assert_eq!(check(&a, test, &replay).unwrap().report.num_bugs, 1, "{} replay", test);
    }
}

const DEFAULT_RACE: &str = "
event eA;

// the default handler runs at 2ms; eA arrives at the latency of the profile
machine Poller {
  start state S {
    entry { Elapse(2000, 2000); }
    on null goto Timeout;
    on eA goto Got;
  }
  state Timeout { entry { assert !TIMEOUT_FAILS, \"timed out first\"; } ignore eA; }
  state Got { entry { assert TIMEOUT_FAILS, \"event first\"; } }
}
machine Sender { start state S { entry (p: machine) { send p, eA; } } }
machine Main { start state S { entry { new Sender(new Poller()); } } }
test tc [main=Main]: { Main, Poller, Sender };
";

#[test]
fn timed_default_handler_races_only_overlapping_events() {
    // (latency, timeout_fails, bug expected under --timed)
    for (lat, timeout_fails, bug) in [
        ("1ms..3ms", true, true),
        ("1ms..3ms", false, true),
        ("0ms..1ms", true, false),
        ("5ms", false, false),
        ("0ms..1ms", false, true),
        ("5ms", true, true),
    ] {
        let src = DEFAULT_RACE.replace("TIMEOUT_FAILS", if timeout_fails { "true" } else { "false" });
        let a = compile(&[("decls.p".into(), TIME_DECLS.into()), ("race.p".into(), src)]);
        let profiles = Arc::new(ProfileSet::parse(&format!("[profile.p]\ndefault = \"{}\"", lat)).unwrap());
        let opts = CheckerOptions {
            iterations: 200,
            seed: 3,
            latency: Some(profiles),
            latency_profile: Some("p".into()),
            timed: true,
            ..Default::default()
        };
        let r = check(&a, "tc", &opts).unwrap();
        assert_eq!(
            r.report.num_bugs, bug as usize,
            "{} timeout_fails={}: {:?}",
            lat, timeout_fails, r.report.bug_reports
        );
    }
}

const TIMED_RACE: &str = "
event eReq: int;
machine Server {
  var last: int;
  start state S { on eReq do (n: int) { last = n; } }
}
machine Client {
  start state S { entry (s: machine) { Elapse(1000, 5000); send s, eReq, 1; Elapse(1000, 2000); send s, eReq, 2; } }
}
machine Main {
  start state S { entry { var s: machine; s = new Server(); new Client(s); new Client(s); } }
}
// clocks grow with every tick, so no state repeats
machine Ticker {
  start state S { entry { send this, eReq, 0; } on eReq do (n: int) { Elapse(1000, 1000); send this, eReq, n + 1; } }
}
test tcRace [main=Main]: { Main, Server, Client };
test tcTicker [main=Ticker]: { Ticker };
";

#[test]
fn timed_models_are_searched_exhaustively_when_they_end() {
    let a = compile(&[("t.p".into(), format!("{}{}", TIME_DECLS, TIMED_RACE))]);
    for timed in [false, true] {
        let opts =
            CheckerOptions { strategy: StrategyKind::Auto, iterations: 100_000, seed: 1, timed, ..Default::default() };
        let r = check(&a, "tcRace", &opts).unwrap();
        assert!(r.time_tracked);
        let x = r.exhaustive.expect("an exhaustive search");
        assert!(x.complete(), "timed={} {:?}", timed, x);
        // the same choices with different clocks are different states
        assert!(x.distinct_states > 10, "{:?}", x);

        // a model that never ends cannot be exhausted: its clocks never repeat
        let opts = CheckerOptions { iterations: 300, max_unfair_steps: 50, max_fair_steps: 50, ..opts };
        let r = check(&a, "tcTicker", &opts).unwrap();
        assert!(r.time_tracked);
        assert!(r.exhaustive.as_ref().is_none_or(|x| !x.complete()), "{:?}", r.exhaustive);
    }
}
