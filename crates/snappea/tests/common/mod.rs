//! Helpers the integration tests share; each test uses some of them.
#![allow(dead_code)]

use snappea::host::{Job, MemHost};

/// Runs `snappea args` on `host` after clearing its output; returns the exit code.
pub fn snappea(host: &mut MemHost, args: &[&str]) -> i32 {
    host.stdout.clear();
    host.stderr.clear();
    snappea::cli::run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), host)
}

/// Runs `snappea-compat args` on `host` after clearing its output; returns the exit code.
pub fn compat(host: &mut MemHost, args: &[&str]) -> i32 {
    host.stdout.clear();
    host.stderr.clear();
    snappea::compat::run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), host)
}

/// [`snappea::host::Threads::run`] on scoped threads, for a [`MemHost`] with workers.
pub fn run_threads<'a>(jobs: Vec<Job<'a>>, main: &mut (dyn FnMut() + 'a)) {
    std::thread::scope(|s| {
        for job in jobs {
            s.spawn(job);
        }
        main();
    });
}
