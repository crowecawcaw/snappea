//! The native [`Host`]: the real file system, console, clock and threads.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use crate::host::{Color, Host, Job, LiveInput, LiveRead, LiveSource, MemoryUse, Stream, Threads};

/// The [`Host`] of a native process: the real file system, console, clock, environment
/// and threads. The `snappea` binary runs on it. Use it to run [`crate::driver`] or
/// [`crate::project`] functions on real files from Rust; tests and the browser use a
/// [`MemHost`](crate::host::MemHost) instead.
///
/// ```no_run
/// use snappea::driver::check::{self, CheckConfig};
/// use snappea::host::NativeHost;
///
/// let mut host = NativeHost::new(); // paths are relative to the working directory
/// let mut cfg = CheckConfig::default(); // bug traces go to PCheckerOutput/
/// cfg.input.pfiles = vec!["model.p".into()];
/// check::resolve_sources(&mut host, &mut cfg).expect("errors are printed to stderr");
/// let loaded = check::load(&mut host, &cfg).expect("errors are printed to stderr");
/// cfg.test_case_name = "tcMain".into();
/// check::set_output_directory(&mut host, &mut cfg).expect("errors are printed to stderr");
/// let outcome = check::testing_process(&mut host, &loaded, &cfg).expect("errors are printed to stderr");
/// println!("{} bug(s) in {} schedules", outcome.bugs, outcome.schedules);
/// ```
pub struct NativeHost {
    start: Instant,
    color_out: bool,
    color_err: bool,
}

impl Host for NativeHost {
    fn stderr_is_terminal(&self) -> bool {
        io::stderr().is_terminal()
    }
    fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
    fn read_stdin_line(&self) -> io::Result<Option<String>> {
        let mut line = String::new();
        match io::stdin().read_line(&mut line)? {
            0 => Ok(None),
            _ => Ok(Some(line)),
        }
    }
    fn open_live(&self, inputs: &[LiveInput]) -> Box<dyn LiveSource + '_> {
        Box::new(ThreadedLive::new(inputs))
    }
    fn write(&mut self, stream: Stream, text: &str, color: Option<Color>) {
        let use_color = match stream {
            Stream::Out => self.color_out,
            Stream::Err => self.color_err,
        };
        let code = match (use_color, color) {
            (true, Some(Color::Red)) => Some("\x1b[31m"),
            (true, Some(Color::Yellow)) => Some("\x1b[33m"),
            (true, Some(Color::Cyan)) => Some("\x1b[36m"),
            _ => None,
        };
        let mut buf = String::new();
        if let Some(c) = code {
            buf.push_str(c);
        }
        buf.push_str(text);
        if code.is_some() {
            buf.push_str("\x1b[0m");
        }
        let _ = match stream {
            Stream::Out => io::stdout().lock().write_all(buf.as_bytes()),
            Stream::Err => io::stderr().lock().write_all(buf.as_bytes()),
        };
    }

    fn cwd(&self) -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    fn write_file(&mut self, path: &Path, contents: &str) -> io::Result<()> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, contents)
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(path)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        v.sort();
        Ok(v)
    }

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn remove_dir_all(&mut self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn max_memory_gb(&self) -> f64 {
        // peak resident set size (Linux, macOS); 0 elsewhere
        #[cfg(target_os = "macos")]
        {
            peak_rss_bytes_macos() / (1024.0 * 1024.0 * 1024.0)
        }
        #[cfg(not(target_os = "macos"))]
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("VmHWM:"))
                    .and_then(|l| l.split_whitespace().nth(1).and_then(|k| k.parse::<f64>().ok()))
            })
            .map(|kb| kb / (1024.0 * 1024.0))
            .unwrap_or(0.0)
    }

    fn memory(&self) -> Option<MemoryUse> {
        #[cfg(target_os = "linux")]
        {
            linux_memory()
        }
        #[cfg(not(target_os = "linux"))]
        None
    }

    fn random_seed(&mut self) -> u32 {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let mut x = (t as u64) ^ ((std::process::id() as u64) << 32) ^ 0x9E37_79B9_7F4A_7C15;
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x as u32
    }

    fn unix_time(&self) -> f64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
    }

    fn threads(&self) -> Option<Threads> {
        Some(Threads { available: std::thread::available_parallelism().map_or(1, |n| n.get()), run: run_threads })
    }
}

/// Resident memory now, and the memory limit: the smallest of the cgroup limits on the
/// way from this process's cgroup to the root (v2 `memory.max`, v1
/// `memory.limit_in_bytes`) and the machine's total memory.
#[cfg(target_os = "linux")]
fn linux_memory() -> Option<MemoryUse> {
    static LIMIT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let kib = |text: &str, key: &str| -> Option<u64> {
        let line = text.lines().find(|l| l.starts_with(key))?;
        line.split_whitespace().nth(1)?.parse::<u64>().ok().map(|k| k * 1024)
    };
    let limit = *LIMIT.get_or_init(|| {
        let total = std::fs::read_to_string("/proc/meminfo").ok().and_then(|m| kib(&m, "MemTotal:"));
        let cgroups = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
        let mut limits: Vec<u64> = total.into_iter().collect();
        for line in cgroups.lines() {
            let mut parts = line.splitn(3, ':');
            let (Some(_), Some(controllers), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
            let (base, file) = if controllers.is_empty() {
                ("/sys/fs/cgroup", "memory.max")
            } else if controllers.split(',').any(|c| c == "memory") {
                ("/sys/fs/cgroup/memory", "memory.limit_in_bytes")
            } else {
                continue;
            };
            // a container may see its own cgroup as the root, so try each level up to it
            let mut dir = PathBuf::from(base).join(path.trim_start_matches('/'));
            loop {
                if let Some(v) = std::fs::read_to_string(dir.join(file)).ok().and_then(|s| s.trim().parse::<u64>().ok())
                {
                    limits.push(v);
                }
                if dir == Path::new(base) || !dir.pop() {
                    break;
                }
            }
        }
        limits.into_iter().filter(|&l| l > 0).min()
    });
    let resident = std::fs::read_to_string("/proc/self/status").ok().and_then(|s| kib(&s, "VmRSS:"))?;
    Some(MemoryUse { resident, limit: limit? })
}

/// Peak resident set size of the process from `getrusage`, in bytes (0 on failure).
#[cfg(target_os = "macos")]
fn peak_rss_bytes_macos() -> f64 {
    // `struct rusage`: two `struct timeval`s (16 bytes each on 64-bit macOS), then
    // `ru_maxrss` (bytes on macOS) and 13 more `long` counters
    #[repr(C)]
    struct Rusage {
        times: [i64; 4],
        maxrss: i64,
        rest: [i64; 13],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    const RUSAGE_SELF: i32 = 0;
    let mut usage = Rusage { times: [0; 4], maxrss: 0, rest: [0; 13] };
    // SAFETY: `usage` is a writable buffer with the size and layout of `struct rusage`
    let ok = unsafe { getrusage(RUSAGE_SELF, &mut usage) } == 0;
    if ok {
        usage.maxrss as f64
    } else {
        0.0
    }
}

/// [`Threads::run`] on OS threads.
fn run_threads<'a>(jobs: Vec<Job<'a>>, main: &mut (dyn FnMut() + 'a)) {
    std::thread::scope(|s| {
        for job in jobs {
            s.spawn(job);
        }
        main();
    });
}

/// Live inputs read on a thread each. The channel has no buffer, so a reader holds at
/// most one line that has not been checked yet; readers still blocked when observation
/// stops end with the process.
struct ThreadedLive {
    rx: mpsc::Receiver<LiveRead>,
}

impl ThreadedLive {
    fn new(inputs: &[LiveInput]) -> ThreadedLive {
        let (tx, rx) = mpsc::sync_channel(0);
        for (input, source) in inputs.iter().cloned().enumerate() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                // opening a named pipe waits for its writer
                let reader: Box<dyn BufRead> = match source {
                    LiveInput::Stdin => Box::new(io::stdin().lock()),
                    LiveInput::Path(p) => match std::fs::File::open(&p) {
                        Ok(f) => Box::new(io::BufReader::new(f)),
                        Err(e) => {
                            let _ = tx.send(LiveRead::Error { input, error: format!("cannot open: {}", e) });
                            return;
                        }
                    },
                };
                let _ = tx.send(read_lines(input, reader, &tx));
            });
        }
        ThreadedLive { rx }
    }
}

/// Sends each line of `reader`; returns how the input ended.
fn read_lines(input: usize, mut reader: Box<dyn BufRead>, tx: &mpsc::SyncSender<LiveRead>) -> LiveRead {
    loop {
        let mut buf = Vec::new();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => return LiveRead::End { input },
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf).into_owned();
                if tx.send(LiveRead::Line { input, line }).is_err() {
                    return LiveRead::End { input };
                }
            }
            Err(e) => return LiveRead::Error { input, error: e.to_string() },
        }
    }
}

impl LiveSource for ThreadedLive {
    fn next(&mut self) -> LiveRead {
        // every reader sends an End or Error before exiting, so this only fails if the
        // caller asks for more than that
        self.rx.recv().unwrap_or(LiveRead::End { input: usize::MAX })
    }
}

impl NativeHost {
    /// A host on the process's console; colors are used on terminals only.
    pub fn new() -> NativeHost {
        NativeHost {
            start: Instant::now(),
            color_out: io::stdout().is_terminal(),
            color_err: io::stderr().is_terminal(),
        }
    }
}

impl Default for NativeHost {
    fn default() -> Self {
        NativeHost::new()
    }
}
