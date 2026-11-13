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
