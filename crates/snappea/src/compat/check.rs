//! `p check`: upstream's options (`PCheckerOptions`) and test-case loop (`Checker.Run`), on
//! the steps in [`crate::driver::check`].

use std::path::Path;

use crate::checker::strategy::StrategyKind;
use crate::compat::argparse::{ArgParser, DataType, ParseError};
use crate::driver::check::{
    finish_options, load, read_schedule, replay, resolve_sources, select_test_case, set_output_directory,
    testing_process, CheckConfig, Loaded, Mode,
};
use crate::host::{report_and_exit, Color, Exit, Host, HostExt, Stream};

/// `PCheckerOptions` parser definition.
pub fn parser() -> ArgParser {
    let mut p = ArgParser::new(
        "p check",
        "The P checker enables systematic exploration of a specified P test case, it generates a reproducible bug-trace if a bug is found, and also allows replaying a bug-trace.",
    );
    let b = Some("Basic");
    p.group("Basic", "Basic options");
    p.add_positional(
        b,
        "path",
        "P project file (*.pproj) or P file (*.p) to check. If no sources are given, the checker uses the *.pproj in the current folder, or else the *.p files in it and its immediate subfolders",
    )
    .optional();
    p.add(b, "pproj", Some("pp"), "P project file to check (*.pproj)", DataType::Str);
    p.add(b, "pfiles", Some("pf"), "List of P files to check", DataType::Str).multi();
    p.add(b, "projname", Some("pn"), "Project name (names the output files)", DataType::Str);
    p.add(
        b,
        "mode",
        Some("md"),
        "Checker mode to use. Can be bugfinding or pex. If this option is not passed, bugfinding mode is used as default",
        DataType::Str,
    )
    .allowed(&["bugfinding", "pex"]);
    p.add(b, "testcase", Some("tc"), "Test case to explore", DataType::Str);
    p.add(b, "list-tests", None, "List all test cases and exit.", DataType::Bool);
    p.add(b, "timeout", Some("t"), "Timeout in seconds (disabled by default)", DataType::UInt);
    p.add(b, "memout", Some("m"), "Memory limit in Giga bytes (disabled by default)", DataType::Double);
    p.add(b, "outdir", Some("o"), "Dump output to directory (absolute or relative path)", DataType::Str);
    p.add(b, "verbose", Some("v"), "Enable verbose log output during exploration", DataType::Bool);
    p.add(b, "debug", Some("d"), "Enable debugging", DataType::Bool).hidden();

    let e = Some("explore");
    p.group("explore", "Systematic exploration options");
    p.add(e, "iterations", Some("i"), "Number of schedules to explore", DataType::UInt).hidden();
    p.add(e, "schedules", Some("s"), "Number of schedules to explore", DataType::UInt);
    p.add(
        e,
        "max-steps",
        Some("ms"),
        "Max scheduling steps to be explored during systematic exploration (by default 10,000 unfair and 100,000 fair steps). You can provide one or two unsigned integer values",
        DataType::UInt,
    )
    .multi();
    p.add(e, "fail-on-maxsteps", None, "Consider it a bug if the test hits the specified max-steps", DataType::Bool);
    p.add(
        e,
        "liveness-temperature-threshold",
        None,
        "Specify the liveness temperature threshold is the liveness temperature value that triggers a liveness bug",
        DataType::UInt,
    )
    .hidden();

    let s = Some("scheduling");
    p.group("scheduling", "Search prioritization options");
    p.add(s, "sch-random", None, "Choose the random scheduling strategy (this is the default)", DataType::Bool);
    p.add(s, "sch-feedback", None, "Choose the random scheduling strategy with feedback mutation", DataType::Bool);
    p.add(
        s,
        "sch-feedbackpct",
        None,
        "Choose the PCT scheduling strategy with feedback mutation (recommended feedback strategy: best rare-behavior coverage; takes the max number of priority switch points, e.g. --sch-feedbackpct 10)",
        DataType::UInt,
    );
    p.add(s, "sch-feedbackpos", None, "Choose the POS scheduling strategy with feedback mutation", DataType::Bool);
    p.add(
        s,
        "sch-probabilistic",
        Some("sp"),
        "Choose the probabilistic scheduling strategy with given probability for each scheduling decision where the probability is specified as the integer N in the equation 0.5 to the power of N.  So for N=1, the probability is 0.5, for N=2 the probability is 0.25, N=3 you get 0.125, etc.",
        DataType::UInt,
    );
    p.add(
        s,
        "sch-pct",
        None,
        "Choose the PCT scheduling strategy with given maximum number of priority switch points",
        DataType::UInt,
    );
    p.add(s, "sch-pos", None, "Choose the POS scheduling strategy", DataType::Bool);
    p.add(
        s,
        "sch-fairpct",
        None,
        "Choose the fair PCT scheduling strategy with given maximum number of priority switch points",
        DataType::UInt,
    );
    p.add(s, "sch-rl", None, "Choose the reinforcement learning (RL) scheduling strategy", DataType::Bool).hidden();
    p.add(
        s,
        "sch-pex",
        None,
        "Choose the scheduling strategy for PEx mode (options: random, dfs). (default: random)",
        DataType::Str,
    )
    .allowed(&["random", "dfs", "astar"]);
    p.add(
        s,
        "timeline-repr",
        None,
        "How the feedback-guided search represents a schedule's timeline (the diversity signal): pairwise (default), kgram, causal, hybrid",
        DataType::Str,
    )
    .allowed(&["pairwise", "kgram", "causal", "hybrid"]);
    p.add(
        s,
        "timeline-kgram",
        None,
        "Gram length for the kgram/hybrid timeline representation (default: 3)",
        DataType::UInt,
    );
    p.add(
        s,
        "timeline-payload",
        None,
        "Enrich timeline event labels with a stable hash of the event payload",
        DataType::Bool,
    );

    p.group("replay", "Replay and debug options");
    p.add(Some("replay"), "replay", Some("r"), "Schedule file to replay", DataType::Str);

    let a = Some("advanced");
    p.group("advanced", "Advanced options");
    p.add(a, "explore", None, "Keep testing until the bound (e.g. schedule or time) is reached", DataType::Bool);
    p.add(a, "seed", None, "Specify the random value generator seed", DataType::UInt);
    p.add(a, "graph-bug", None, "Output a DGML graph of the schedule that found a bug", DataType::Bool);
    p.add(a, "graph", None, "Output a DGML graph of all test schedules whether a bug was found or not", DataType::Bool);
    p.add(a, "xml-trace", None, "Specify a filename for XML runtime log output to be written to", DataType::Bool);
    p.add(
        a,
        "jvm-args",
        None,
        "Specify a concatenated list of JVM arguments to pass, each starting with a colon",
        DataType::Str,
    )
    .hidden();
    p.add(
        a,
        "checker-args",
        None,
        "Specify a concatenated list of additional checker arguments to pass, each starting with a colon",
        DataType::Str,
    )
    .hidden();
    p.add(
        a,
        "psym-args",
        None,
        "Specify a concatenated list of additional PSym-specific arguments to pass, each starting with a colon",
        DataType::Str,
    )
    .hidden();
    p
}

/// Runs `p check`; returns the exit code.
pub fn run(args: &[String], host: &mut dyn Host) -> i32 {
    let cfg = match parse(args, host) {
        Ok(c) => c,
        Err(Exit(code)) => return code,
    };
    match run_checker(cfg, host) {
        Ok(code) => code,
        Err(Exit(code)) => code,
    }
}

fn help_and_exit(p: &ArgParser, host: &mut dyn Host, msg: &str) -> Exit {
    host.write(Stream::Out, &p.help(), Some(Color::Cyan));
    report_and_exit(host, msg)
}

fn checker_report_and_exit(host: &mut dyn Host, msg: &str) -> Exit {
    report_and_exit(host, &format!("{} Run `p check --help` for usage help.", msg))
}

/// `PCheckerOptions.Parse`.
pub fn parse(args: &[String], host: &mut dyn Host) -> Result<CheckConfig, Exit> {
    let p = parser();
    let result = match p.parse(args) {
        Ok(r) => r,
        Err(ParseError::Help) => {
            host.write(Stream::Out, &p.help(), Some(Color::Cyan));
            return Err(Exit(1));
        }
        Err(ParseError::Invalid(m)) => return Err(help_and_exit(&p, host, &m)),
    };
    let mut cfg = CheckConfig::default();
    for a in &result {
        let v = a.value.clone();
        let uint = || v.as_ref().map(|v| v.as_uint()).unwrap_or(0);
        let string = || v.as_ref().map(|v| v.as_str().to_string()).unwrap_or_default();
        match a.long_name.as_str() {
            "outdir" => cfg.output_path = string(),
            "verbose" => cfg.verbose = true,
            "xml-trace" => cfg.xml_trace = true,
            "debug" | "memout" | "graph" | "graph-bug" | "jvm-args" | "checker-args" | "psym-args" => {}
            "timeline-repr" => cfg.timeline.repr = crate::checker::timeline::TimelineRepr::parse(&string()),
            "timeline-kgram" => cfg.timeline.kgram = uint() as usize,
            "timeline-payload" => cfg.timeline.payload = true,
            // as upstream, 0 is no timeout, and without --schedules no schedule bound either
            "timeout" => {
                let t = uint();
                cfg.timeout = (t > 0).then_some(t);
                cfg.user_set_iterations = true;
                if cfg.iterations == 1 {
                    cfg.iterations = 0;
                }
            }
            "path" => {
                let f = string();
                let l = f.to_lowercase();
                if l.ends_with(".pproj") {
                    cfg.input.pproj = Some(f);
                } else if l.ends_with(".p") {
                    cfg.input.pfiles.push(f);
                } else {
                    return Err(checker_report_and_exit(
                        host,
                        &format!("'{}' is not a P project file (*.pproj) or P file (*.p). snappea checks P sources directly; there is no compiled file.", f),
                    ));
                }
            }
            "pproj" => cfg.input.pproj = Some(string()),
            "pfiles" => cfg.input.pfiles.extend(v.as_ref().map(|v| v.as_str_list()).unwrap_or_default()),
            "projname" => cfg.input.project_name = Some(string()),
            "mode" => cfg.mode = if string().to_lowercase() == "pex" { Mode::PEx } else { Mode::BugFinding },
            "testcase" => cfg.test_case_name = string(),
            "list-tests" => cfg.list_test_cases = true,
            "seed" => cfg.seed = Some(uint()),
            "sch-random" => cfg.strategy = StrategyKind::Random,
            "sch-pos" => cfg.strategy = StrategyKind::Pos,
            "sch-feedback" => cfg.strategy = StrategyKind::Feedback,
            "sch-feedbackpos" => cfg.strategy = StrategyKind::FeedbackPos,
            "sch-rl" => cfg.strategy = StrategyKind::Rl,
            "sch-probabilistic" => (cfg.strategy, cfg.strategy_bound) = (StrategyKind::Probabilistic, uint() as usize),
            "sch-pct" => (cfg.strategy, cfg.strategy_bound) = (StrategyKind::Pct, uint() as usize),
            "sch-fairpct" => (cfg.strategy, cfg.strategy_bound) = (StrategyKind::FairPct, uint() as usize),
            "sch-feedbackpct" => (cfg.strategy, cfg.strategy_bound) = (StrategyKind::FeedbackPct, uint() as usize),
            "sch-pex" => {
                cfg.strategy = match string().as_str() {
                    "random" => StrategyKind::Random,
                    "dfs" => StrategyKind::Dfs,
                    "astar" => {
                        return Err(checker_report_and_exit(
                            host,
                            "The 'astar' scheduling strategy is not supported by snappea.",
                        ))
                    }
                    other => unreachable!("--sch-pex does not allow '{}'", other),
                }
            }
            "replay" => {
                let f = string();
                if Path::new(&f).extension().map(|e| e != "schedule").unwrap_or(true) {
                    return Err(checker_report_and_exit(
                        host,
                        "Please give a valid schedule file '--replay x', where 'x' has extension '.schedule'.",
                    ));
                }
                cfg.schedule_file = Some(f);
                cfg.strategy = StrategyKind::Replay;
            }
            "iterations" | "schedules" => {
                cfg.iterations = uint() as usize;
                cfg.user_set_iterations = true;
            }
            "explore" => cfg.full_exploration = true,
            "max-steps" => {
                let vals = v.as_ref().map(|v| v.as_uint_list()).unwrap_or_default();
                if vals.len() > 2 {
                    return Err(checker_report_and_exit(host, "Invalid number of options supplied via '--max-steps'."));
                }
                let i = vals[0];
                let j = if vals.len() == 2 {
                    cfg.user_set_max_fair_steps = true;
                    vals[1]
                } else {
                    i.wrapping_mul(10)
                };
                cfg.max_unfair_steps = i as usize;
                cfg.max_fair_steps = j as usize;
            }
            "fail-on-maxsteps" => cfg.fail_on_max_steps = true,
            // also `--liveness-temperature-threshold`: upstream parses it but has no case for it
            other => return Err(help_and_exit(&p, host, &format!("Unhandled parsed argument: '{}'", other))),
        }
    }
    resolve_sources(host, &mut cfg)?;
    finish_options(&mut cfg);
    if cfg.max_fair_steps < cfg.max_unfair_steps {
        return Err(checker_report_and_exit(host, "For the option '--max-steps N[,M]', please make sure that M >= N."));
    }
    set_output_directory(host, &mut cfg)?;
    Ok(cfg)
}

/// `Checker.Run`.
pub(crate) fn run_checker(mut cfg: CheckConfig, host: &mut dyn Host) -> Result<i32, Exit> {
    if let Some(schedule_file) = cfg.schedule_file.clone() {
        host.out_line(&format!(
            "Replay option is used, checker is ignoring all other parameters and using the {} to replay the schedule",
            schedule_file
        ));
        host.out_line(&format!("... Replaying {}", schedule_file));
        let loaded = load(host, &cfg)?;
        let tc = select_test_case(host, &loaded, &cfg.test_case_name)?;
        let trace = read_schedule(host, &schedule_file)?;
        replay(host, &loaded, &tc, &mut cfg, trace)?;
        return Ok(0);
    }

    host.out_line(&format!(".. Checking {}", cfg.project_name));
    let mut exit_code = 0;
    // both modes run the same engine; PEx mode with an exhaustive search strategy
    let loaded = load(host, &cfg)?;
    let cases = fetch_test_cases(host, &loaded, &cfg);
    if !cfg.list_test_cases {
        for tc in cases {
            cfg.test_case_name = tc;
            // as upstream, the last test case sets the exit code
            exit_code = testing_process(host, &loaded, &cfg)?.exit_code();
        }
    }
    host.out_line(". Done");
    Ok(exit_code)
}

/// `TestingProcess.FetchTestCases`.
fn fetch_test_cases(host: &mut dyn Host, loaded: &Loaded, cfg: &CheckConfig) -> Vec<String> {
    let names: Vec<String> = loaded.analysis.program.test_cases().into_iter().map(|t| t.name).collect();
    if cfg.list_test_cases {
        host.out_line(&format!(".. List of test cases (total {})", names.len()));
    } else if cfg.test_case_name.is_empty() {
        return vec![String::new()];
    } else {
        host.out_line(&format!(".. Running test cases with prefix '{}':", cfg.test_case_name));
    }
    let mut out = Vec::new();
    for n in names {
        if cfg.list_test_cases {
            host.out_line(&n);
        } else if n.starts_with(&cfg.test_case_name) {
            host.out_line(&n);
            out.push(n);
        }
    }
    out
}
