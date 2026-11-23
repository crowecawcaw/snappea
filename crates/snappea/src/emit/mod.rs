//! Emit application events for P specs, from tests and production alike.

#![doc = include_str!("README.md")]

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub use serde_json::{json, Value};

/// The environment variable naming the folder test child processes write events to.
pub const EVENTS_DIR_ENV: &str = "SNAPPEA_EVENTS_DIR";

/// Lines a background writer holds before it drops new events.
const QUEUE_CAPACITY: usize = 16_384;

/// An event time: nanoseconds since the Unix epoch. Any increasing counter works as a
/// logical clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Time(i64);

impl Time {
    /// Nanoseconds since the Unix epoch, or any increasing counter.
    pub fn from_nanos(nanos: i64) -> Time {
        Time(nanos)
    }
    /// Milliseconds since the Unix epoch; saturates on overflow.
    pub fn from_millis(millis: i64) -> Time {
        Time(millis.saturating_mul(1_000_000))
    }
    /// The wall clock.
    pub fn now() -> Time {
        SystemTime::now().into()
    }
    /// Nanoseconds since the Unix epoch.
    pub fn as_nanos(self) -> i64 {
        self.0
    }
    /// ISO-8601 UTC with nanoseconds, e.g. `2026-10-18T00:00:00.123456789Z`.
    pub fn to_iso(self) -> String {
        let secs = self.0.div_euclid(1_000_000_000);
        let nanos = self.0.rem_euclid(1_000_000_000);
        let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
        let (y, m, d) = civil_from_days(days);
        format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{nanos:09}Z", rem / 3600, rem % 3600 / 60, rem % 60)
    }
}

/// Epoch nanoseconds.
impl From<i64> for Time {
    fn from(nanos: i64) -> Time {
        Time(nanos)
    }
}

impl From<SystemTime> for Time {
    fn from(t: SystemTime) -> Time {
        let nanos = match t.duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
            Err(e) => i64::try_from(e.duration().as_nanos()).map(|n| -n).unwrap_or(i64::MIN),
        };
        Time(nanos)
    }
}

/// Time since the Unix epoch.
impl From<Duration> for Time {
    fn from(d: Duration) -> Time {
        Time(i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
    }
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// One emitted event, as written to an events file.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// The P event name.
    pub event: String,
    /// Partition key: each key is checked by its own spec instance.
    pub key: String,
    /// JSON payload: an object for a named tuple, a single value, or `null`.
    pub payload: Value,
    /// When the event happened. Orders a key's events in a check.
    pub time: Time,
    /// The caller set `time` ([`Emitter::emit_at`]) instead of the wall clock.
    pub at: bool,
    /// `file:line` of the emit call.
    pub src: Option<String>,
    /// Emission order within the process.
    pub seq: u64,
    /// ID of the emitting process.
    pub pid: u32,
}

impl Event {
    /// An event stamped now (or at `at`) with the next sequence number, recording the
    /// caller as `src`. Times from the wall clock never decrease within a process.
    #[track_caller]
    pub fn new(at: Option<Time>, event: &str, key: impl fmt::Display, payload: Value) -> Event {
        let caller = std::panic::Location::caller();
        let (time, seq) = stamp(at);
        Event {
            event: event.to_string(),
            key: key.to_string(),
            payload,
            time,
            at: at.is_some(),
            src: Some(format!("{}:{}", caller.file(), caller.line())),
            seq,
            pid: std::process::id(),
        }
    }

    /// The JSON line `snappea observe` reads, without the trailing newline.
    pub fn to_json(&self) -> String {
        let mut line = serde_json::Map::new();
        line.insert("event".into(), Value::String(self.event.clone()));
        line.insert("key".into(), Value::String(self.key.clone()));
        line.insert("timestamp".into(), Value::String(self.time.to_iso()));
        line.insert("payload".into(), self.payload.clone());
        if let Some(src) = &self.src {
            line.insert("src".into(), Value::String(src.clone()));
        }
        line.insert("seq".into(), self.seq.into());
        line.insert("pid".into(), self.pid.into());
        if self.at {
            line.insert("at".into(), Value::Bool(true));
        }
        Value::Object(line).to_string()
    }
}

/// The process-wide sequence number and the last wall-clock time handed out.
static CLOCK: Mutex<(u64, i64)> = Mutex::new((0, i64::MIN));

fn stamp(at: Option<Time>) -> (Time, u64) {
    let mut clock = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
    clock.0 += 1;
    let time = match at {
        Some(t) => t,
        None => {
            let now = Time::now().0.max(clock.1);
            clock.1 = now;
            Time(now)
        }
    };
    (time, clock.0)
}

/// Where emitted events go. Clones share one destination. The default is off.
#[derive(Clone, Default)]
pub struct Emitter {
    inner: Option<Arc<Inner>>,
}

struct Inner {
    sink: Mutex<Sink>,
    dropped: Arc<AtomicU64>,
}

type Callback = Box<dyn Fn(Event) + Send + Sync>;

enum Sink {
    /// This process's own file in an events folder, opened on first use and appended to
    /// synchronously, so a child's events are on disk when it exits.
    Dir {
        dir: PathBuf,
        file: Option<(u32, File)>,
    },
    /// A background writer thread.
    Queue(SyncSender<Message>),
    Callback(Callback),
}

enum Message {
    Line(String),
    Flush(mpsc::Sender<()>),
}

impl fmt::Debug for Emitter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match &self.inner {
            None => "off".to_string(),
            Some(inner) => match &*inner.sink.lock().unwrap_or_else(|e| e.into_inner()) {
                Sink::Dir { dir, .. } => format!("events dir {}", dir.display()),
                Sink::Queue(_) => "writer".into(),
                Sink::Callback(_) => "callback".into(),
            },
        };
        write!(f, "Emitter({kind})")
    }
}

impl Emitter {
    /// Emits nothing.
    pub fn off() -> Emitter {
        Emitter { inner: None }
    }

    /// Writes to `SNAPPEA_EVENTS_DIR` when it is set (a test started this process), else off.
    pub fn from_env() -> Emitter {
        match std::env::var_os(EVENTS_DIR_ENV) {
            Some(dir) if !dir.is_empty() => Emitter::events_dir(dir),
            _ => Emitter::off(),
        }
    }

    /// Appends to this process's own `<name>-<pid>-<random>.jsonl` in `dir`, in the
    /// [events folder format](https://github.com/crowecawcaw/snappea/blob/main/docs/observe.md#events-folder-format).
    pub fn events_dir(dir: impl Into<PathBuf>) -> Emitter {
        Emitter::with_sink(Sink::Dir { dir: dir.into(), file: None }, Arc::default())
    }

    /// Appends JSON lines to `path`, creating it and its folder if needed.
    pub fn file(path: impl AsRef<Path>) -> std::io::Result<Emitter> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Emitter::writer(file))
    }

    /// JSON lines on standard output.
    pub fn stdout() -> Emitter {
        Emitter::writer(std::io::stdout())
    }

    /// JSON lines to `out`, written by a background thread. Each line is one `write_all`
    /// followed by a flush.
    pub fn writer(mut out: impl Write + Send + 'static) -> Emitter {
        let dropped = Arc::<AtomicU64>::default();
        let (tx, rx) = mpsc::sync_channel::<Message>(QUEUE_CAPACITY);
        let counter = dropped.clone();
        let spawned = std::thread::Builder::new().name("snappea-emit".into()).spawn(move || {
            for message in rx {
                match message {
                    Message::Line(line) => {
                        if out.write_all(line.as_bytes()).and_then(|_| out.flush()).is_err() {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Message::Flush(done) => {
                        let _ = out.flush();
                        let _ = done.send(());
                    }
                }
            }
        });
        if spawned.is_err() {
            // The receiver is gone, so every event counts as dropped.
            return Emitter::with_sink(Sink::Queue(mpsc::sync_channel(0).0), dropped);
        }
        Emitter::with_sink(Sink::Queue(tx), dropped)
    }

    /// Calls `f` with each event, under the emitter's lock (so in emission order). `f`
    /// must not emit to the same emitter.
    pub fn callback(f: impl Fn(Event) + Send + Sync + 'static) -> Emitter {
        Emitter::with_sink(Sink::Callback(Box::new(f)), Arc::default())
    }

    fn with_sink(sink: Sink, dropped: Arc<AtomicU64>) -> Emitter {
        Emitter { inner: Some(Arc::new(Inner { sink: Mutex::new(sink), dropped })) }
    }

    /// `false` when emitting does nothing. Check it before building costly payloads.
    pub fn is_on(&self) -> bool {
        self.inner.is_some()
    }

    /// Events not written: the queue was full, a write failed, or the payload could
    /// not be serialized.
    pub fn dropped(&self) -> u64 {
        self.inner.as_ref().map_or(0, |i| i.dropped.load(Ordering::Relaxed))
    }

    /// Records `event` on stream `key` now. `payload` is any `Serialize` value whose JSON
    /// matches the P payload type (`json!({...})`, a struct, a scalar, or `()`).
    #[track_caller]
    pub fn emit(&self, event: &str, key: impl fmt::Display, payload: impl serde::Serialize) {
        self.emit_inner(None, event, key, payload)
    }

    /// [`emit`](Self::emit) at a given time instead of the wall clock.
    #[track_caller]
    pub fn emit_at(&self, at: impl Into<Time>, event: &str, key: impl fmt::Display, payload: impl serde::Serialize) {
        self.emit_inner(Some(at.into()), event, key, payload)
    }

    #[track_caller]
    fn emit_inner(&self, at: Option<Time>, event: &str, key: impl fmt::Display, payload: impl serde::Serialize) {
        if let Some(inner) = &self.inner {
            inner.emit(at, event, key, payload);
        }
    }

    /// Waits up to `timeout` for a background writer to write everything emitted so far,
    /// and returns whether it did. Other sinks write synchronously.
    pub fn flush(&self, timeout: Duration) -> bool {
        let Some(inner) = &self.inner else { return true };
        let tx = match &*inner.sink.lock().unwrap_or_else(|e| e.into_inner()) {
            Sink::Queue(tx) => tx.clone(),
            _ => return true,
        };
        let deadline = Instant::now() + timeout;
        let (done, wait) = mpsc::channel();
        let mut message = Message::Flush(done);
        loop {
            match tx.try_send(message) {
                Ok(()) => break,
                Err(TrySendError::Disconnected(_)) => return false,
                Err(TrySendError::Full(m)) => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    message = m;
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        wait.recv_timeout(deadline.saturating_duration_since(Instant::now())).is_ok()
    }
}

impl Inner {
    #[track_caller]
    fn emit(&self, at: Option<Time>, event: &str, key: impl fmt::Display, payload: impl serde::Serialize) {
        let Ok(payload) = serde_json::to_value(payload) else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let key = key.to_string();
        let mut sink = self.sink.lock().unwrap_or_else(|e| e.into_inner());
        // Stamp under the sink's lock, so lines are written in time order.
        let event = Event::new(at, event, key, payload);
        Inner::deliver(&mut sink, &self.dropped, event);
    }

    /// Writes an already stamped event.
    fn send(&self, event: Event) {
        let mut sink = self.sink.lock().unwrap_or_else(|e| e.into_inner());
        Inner::deliver(&mut sink, &self.dropped, event);
    }

    fn deliver(sink: &mut Sink, dropped: &AtomicU64, event: Event) {
        let written = match sink {
            Sink::Callback(f) => {
                f(event);
                true
            }
            Sink::Queue(tx) => match tx.try_send(Message::Line(event.to_json() + "\n")) {
                Ok(()) => true,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
            },
            Sink::Dir { dir, file } => write_to_dir(dir, file, &event),
        };
        if !written {
            dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn write_to_dir(dir: &Path, file: &mut Option<(u32, File)>, event: &Event) -> bool {
    let pid = std::process::id();
    if file.as_ref().is_none_or(|(owner, _)| *owner != pid) {
        let path = dir.join(format!("{}-{pid}-{}.jsonl", process_name(), random_suffix()));
        let opened = std::fs::create_dir_all(dir).and_then(|_| OpenOptions::new().create(true).append(true).open(path));
        match opened {
            Ok(f) => *file = Some((pid, f)),
            Err(_) => return false,
        }
    }
    let Some((_, f)) = file else { return false };
    // One write per line: with O_APPEND, lines from several writers never interleave.
    f.write_all((event.to_json() + "\n").as_bytes()).is_ok()
}

/// The program's file name, safe for a file name.
fn process_name() -> String {
    let name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "process".into());
    let name: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if name.is_empty() {
        "process".into()
    } else {
        name
    }
}

fn random_suffix() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_i64(Time::now().0);
    let mut n = h.finish();
    let mut s = String::new();
    for _ in 0..6 {
        s.push(char::from(b"abcdefghijklmnopqrstuvwxyz0123456789"[(n % 36) as usize]));
        n /= 36;
    }
    s
}

/// The process-wide emitter [`emit`] uses.
struct Global {
    emitter: RwLock<Emitter>,
    /// `SNAPPEA_EVENTS_DIR` was set: a test started this process, and [`set_sink`] does nothing.
    from_env: bool,
    /// Drops counted by emitters [`set_sink`] replaced.
    retired_drops: AtomicU64,
}

static GLOBAL: OnceLock<Global> = OnceLock::new();
/// Whether the global emitter is on, so that `emit` takes no lock when it is off.
static ON: AtomicBool = AtomicBool::new(false);

fn global() -> &'static Global {
    GLOBAL.get_or_init(|| {
        let emitter = Emitter::from_env();
        let from_env = emitter.is_on();
        ON.store(from_env, Ordering::Release);
        Global { emitter: RwLock::new(emitter), from_env, retired_drops: AtomicU64::new(0) }
    })
}

/// The global emitter, or `None` when it is off.
fn current() -> Option<Emitter> {
    let global = global();
    if !ON.load(Ordering::Acquire) {
        return None;
    }
    Some(global.emitter.read().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Sends events from [`emit`] outside tests to `sink` (`Emitter::file(path)?`,
/// `Emitter::stdout()`, `Emitter::writer(w)` or `Emitter::callback(f)`), replacing the
/// earlier sink and writing what it had queued. `Emitter::off()` turns it off. In a
/// process a test started (`SNAPPEA_EVENTS_DIR` set) it does nothing: events go to the test.
pub fn set_sink(sink: Emitter) {
    let global = global();
    if global.from_env {
        return;
    }
    let on = sink.is_on();
    let old = {
        let mut current = global.emitter.write().unwrap_or_else(|e| e.into_inner());
        let old = std::mem::replace(&mut *current, sink);
        ON.store(on, Ordering::Release);
        old
    };
    old.flush(CLOSE_TIMEOUT);
    global.retired_drops.fetch_add(old.dropped(), Ordering::Relaxed);
}

/// How long [`close`] and [`set_sink`] wait for the replaced sink's queue.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Events the global emitter dropped since the process started: a full queue, a failed
/// write, or a payload that could not be serialized.
pub fn dropped() -> u64 {
    let global = global();
    let current = global.emitter.read().unwrap_or_else(|e| e.into_inner()).dropped();
    global.retired_drops.load(Ordering::Relaxed) + current
}

/// [`Emitter::flush`] on the global emitter.
pub fn flush(timeout: Duration) -> bool {
    current().is_none_or(|e| e.flush(timeout))
}

/// Writes what the global sink has queued (waiting up to 5 s) and turns it off. Call it
/// at shutdown.
pub fn close() {
    set_sink(Emitter::off())
}

/// Records `event` on stream `key` now: into an in-process `snappea::test` recording
/// while one is open, else to `SNAPPEA_EVENTS_DIR` when a test started
/// this process, else to the sink set with [`set_sink`], else nowhere.
#[track_caller]
pub fn emit(event: &str, key: impl fmt::Display, payload: impl serde::Serialize) {
    if test_hook::ROUTING.load(Ordering::Relaxed) {
        return test_hook::route(None, event, key, payload);
    }
    if let Some(emitter) = current() {
        emitter.emit(event, key, payload)
    }
}

/// [`emit`] at a given time instead of the wall clock.
#[track_caller]
pub fn emit_at(at: impl Into<Time>, event: &str, key: impl fmt::Display, payload: impl serde::Serialize) {
    if test_hook::ROUTING.load(Ordering::Relaxed) {
        return test_hook::route(Some(at.into()), event, key, payload);
    }
    if let Some(emitter) = current() {
        emitter.emit_at(at, event, key, payload)
    }
}

/// Lets `snappea::test` recordings receive the events application code emits through the
/// global [`emit`].
pub(crate) mod test_hook {
    use super::*;

    /// Receives each global event (`Err` names an event whose payload could not be
    /// serialized). Returns the event back when no recording takes it; it then goes where
    /// [`emit`] would send it.
    pub(crate) type Router = fn(Result<Event, String>) -> Option<Event>;

    pub(crate) static ROUTING: AtomicBool = AtomicBool::new(false);
    static ROUTER: OnceLock<Router> = OnceLock::new();

    /// Installs `router` (the first call wins) and turns routing on or off. While off,
    /// [`emit`] costs one more atomic load.
    #[cfg(feature = "test")]
    pub(crate) fn set_routing(router: Router, on: bool) {
        let _ = ROUTER.set(router);
        ROUTING.store(on, Ordering::Relaxed);
    }

    #[track_caller]
    pub(crate) fn route(at: Option<Time>, event: &str, key: impl fmt::Display, payload: impl serde::Serialize) {
        let Some(router) = ROUTER.get() else {
            if let Some(emitter) = current() {
                emitter.emit_inner(at, event, key, payload);
            }
            return;
        };
        let routed = match serde_json::to_value(payload) {
            Ok(payload) => router(Ok(Event::new(at, event, key, payload))),
            Err(e) => {
                let caller = std::panic::Location::caller();
                router(Err(format!(
                    "the payload of {event} ({}:{}) cannot be serialized: {e}",
                    caller.file(),
                    caller.line()
                )))
            }
        };
        if let (Some(event), Some(emitter)) = (routed, current()) {
            if let Some(inner) = &emitter.inner {
                inner.send(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_times() {
        assert_eq!(Time(0).to_iso(), "1970-01-01T00:00:00.000000000Z");
        assert_eq!(Time(1_767_225_600_000_000_001).to_iso(), "2026-01-01T00:00:00.000000001Z");
        assert_eq!(Time(-1).to_iso(), "1969-12-31T23:59:59.999999999Z");
        assert_eq!(Time::from_millis(951_782_400_000).to_iso(), "2000-02-29T00:00:00.000000000Z");
    }

    #[test]
    fn off_does_nothing() {
        let e = Emitter::off();
        assert!(!e.is_on());
        e.emit("eX", "k", json!({"a": 1}));
        assert_eq!(e.dropped(), 0);
    }

    #[test]
    fn writer_lines_and_src() {
        #[derive(Clone, Default)]
        struct Shared(Arc<Mutex<Vec<u8>>>);
        impl Write for Shared {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let out = Shared::default();
        let e = Emitter::writer(out.clone());
        e.emit("eStocked", "coffee", json!({"units": 3}));
        e.emit_at(Time::from_millis(5), "eDone", 7, ());
        assert!(e.flush(Duration::from_secs(5)));
        let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
        let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["event"], "eStocked");
        assert_eq!(lines[0]["payload"]["units"], 3);
        assert!(lines[0]["src"].as_str().unwrap().starts_with("crates/snappea/src/emit/mod.rs:"), "{}", lines[0]);
        assert!(lines[1]["seq"].as_u64() > lines[0]["seq"].as_u64());
        assert_eq!(lines[1]["key"], "7");
        assert_eq!(lines[1]["timestamp"], "1970-01-01T00:00:00.005000000Z");
        assert_eq!(lines[1]["payload"], Value::Null);
        assert_eq!(lines[1]["at"], true);
    }

    #[test]
    fn events_dir_appends_one_file_per_process() {
        let dir = std::env::temp_dir().join(format!("snappea-emit-test-{}", random_suffix()));
        let e = Emitter::events_dir(&dir);
        e.emit("a", "k", 1);
        e.clone().emit("b", "k", 2);
        let files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|f| f.unwrap().path()).collect();
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.contains(&format!("-{}-", std::process::id())) && name.ends_with(".jsonl"), "{name}");
        assert_eq!(std::fs::read_to_string(&files[0]).unwrap().lines().count(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failures_are_counted_not_raised() {
        let e = Emitter::events_dir("/dev/null/not-a-dir");
        e.emit("a", "k", 1);
        assert_eq!(e.dropped(), 1);
        let mut bad = std::collections::HashMap::new();
        bad.insert((1, 2), 3);
        let e = Emitter::callback(|_| {});
        e.emit("a", "k", bad);
        assert_eq!(e.dropped(), 1);
    }

    #[test]
    fn global_sink_is_replaceable() {
        if std::env::var_os(EVENTS_DIR_ENV).is_some() {
            return; // set_sink does nothing under a test's events folder
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        let shared = seen.clone();
        set_sink(Emitter::callback(move |e| shared.lock().unwrap().push(e.event)));
        emit("a", "k", 1);
        let mut bad = std::collections::HashMap::new();
        bad.insert((1, 2), 3);
        let before = dropped();
        emit("bad", "k", bad);
        set_sink(Emitter::callback(|_| {}));
        assert_eq!(dropped(), before + 1, "drops survive replacing the sink");
        close();
        emit("b", "k", 2);
        assert!(flush(Duration::from_secs(1)));
        assert_eq!(*seen.lock().unwrap(), vec!["a".to_string()]);
    }

    #[test]
    fn wall_clock_never_decreases() {
        let a = Event::new(None, "a", "", Value::Null);
        let b = Event::new(None, "b", "", Value::Null);
        assert!(b.time >= a.time && b.seq > a.seq);
    }
}
