//! In-process P monitor checks for Rust application tests.

#![doc = include_str!("README.md")]

mod config;
mod errors;
mod routing;

pub use crate::checker::observe::UnknownEvents;
pub use crate::checker::observe_report::ObserveCoverageReport;
pub use crate::emit::{json, Emitter, Event, Time, Value, EVENTS_DIR_ENV};
pub use config::Mode;
pub use errors::{CheckError, ErrorKind, SetupError, SpecViolation};
pub use routing::BindGuard;

use std::ffi::OsString;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use crate::checker::observe::{
    split_entries, ErrorKind as CoreKind, ObserveError, ObserveOptions, ObservedEvent, ParsedEntry, Parser, Report,
    Spec, Timestamp,
};
use crate::checker::observe_inputs::{add_log_paths, log_paths, StdFiles};
use crate::sema::{MachineKind, PType, Program};
use crate::syntax::SourceMap;

use config::config;
use errors::REPORT_EVENTS;

const SPEC_FILES_ENV: &str = "SNAPPEA_SPEC_FILES";
const COVERAGE_ENV: &str = "SNAPPEA_COVERAGE";
const ARTIFACTS_ENV: &str = "SNAPPEA_ARTIFACTS";
const MODE_ENV: &str = "SNAPPEA_MODE";

/// Input name of the in-process events, replaced by each event's `src` in messages.
const RECORDING_INPUT: &str = "<recording>";

/// Compiled P spec files and the options to check them with. Compile once (for example
/// in a `LazyLock`) and share: clones are cheap, and builders return adjusted copies.
#[derive(Debug, Clone)]
pub struct Model {
    program: Arc<Program>,
    options: Options,
}

#[derive(Debug, Clone)]
struct Options {
    specs: Option<Vec<String>>,
    exclude: Vec<String>,
    strict: Option<bool>,
    check_hot: bool,
    unknown_events: UnknownEvents,
    coverage: Option<PathBuf>,
    file_coverage: Option<PathBuf>,
    file_mode: Option<Mode>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            specs: None,
            exclude: Vec::new(),
            strict: None,
            check_hot: false,
            unknown_events: UnknownEvents::Error,
            coverage: None,
            file_coverage: None,
            file_mode: None,
        }
    }
}

impl Model {
    /// Reads `[package.metadata.snappea]` from the `Cargo.toml` of the crate under test
    /// (`CARGO_MANIFEST_DIR`). Paths there are relative to that file. `SNAPPEA_SPEC_FILES`
    /// overrides `spec_files`.
    pub fn load() -> Result<Model, SetupError> {
        let dir = std::env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| config("CARGO_MANIFEST_DIR is not set: run under cargo, or use Model::from_files"))?;
        Model::load_manifest(Path::new(&dir).join("Cargo.toml"))
    }

    /// [`load`](Self::load) from a given `Cargo.toml`.
    pub fn load_manifest(manifest: impl AsRef<Path>) -> Result<Model, SetupError> {
        let manifest = manifest.as_ref();
        let cfg = config::read(manifest)?;
        let files = match std::env::var_os(SPEC_FILES_ENV).filter(|v| !v.is_empty()) {
            Some(value) => {
                let values: Vec<String> =
                    std::env::split_paths(&value).map(|p| p.to_string_lossy().into_owned()).collect();
                expand_spec_files(&cwd(), &values)?
            }
            None => match &cfg.spec_files {
                Some(values) => expand_spec_files(&cfg.dir, values)?,
                None => {
                    return Err(config(format!(
                        "no P spec files: set spec_files under [package.metadata.snappea] in {}, or {SPEC_FILES_ENV}",
                        manifest.display()
                    )))
                }
            },
        };
        let mut model = Model::read_files(&files)?;
        let o = &mut model.options;
        o.specs = cfg.specs;
        o.exclude = cfg.exclude.unwrap_or_default();
        o.strict = cfg.strict;
        o.check_hot = cfg.check_hot.unwrap_or(false);
        o.unknown_events = cfg.unknown_events.unwrap_or(UnknownEvents::Error);
        o.file_coverage = cfg.coverage;
        o.file_mode = cfg.mode;
        Ok(model)
    }

    /// Compiles P files: paths, globs (`specs/*.p`), or folders (every `.p` in them),
    /// relative to the working directory.
    pub fn from_files<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> Result<Model, SetupError> {
        let values: Vec<String> = paths.into_iter().map(|p| p.as_ref().to_string_lossy().into_owned()).collect();
        Model::read_files(&expand_spec_files(&cwd(), &values)?)
    }

    /// Compiles P sources given as `(name, text)` pairs, e.g. from `include_str!`.
    pub fn compile(sources: &[(&str, &str)]) -> Result<Model, SetupError> {
        let mut map = SourceMap::new();
        for (name, source) in sources {
            map.add(*name, *source);
        }
        let analysis = crate::sema::compile(&map, &Default::default()).map_err(|e| {
            config(match e {
                crate::sema::CompileError::Parse { file, message, .. } => format!("{}: {message}", map.file(file).name),
                crate::sema::CompileError::NotSupported { file, line, col, keyword } => {
                    format!("{}:{line}:{col}: unsupported syntax '{keyword}'", map.file(file).name)
                }
                crate::sema::CompileError::Analysis(error) => error.messages(&map).join("\n"),
            })
        })?;
        // the spec rules `snappea observe` enforces
        let findings = snappea_lint::lint_observation(&map).map_err(|e| config(e.message))?;
        if !findings.is_empty() {
            let mut text: String = findings.iter().map(|f| crate::driver::lint::render(&map, f) + "\n").collect();
            text.push_str(&crate::driver::lint::summary(findings.len()));
            return Err(config(text));
        }
        Ok(Model { program: Arc::new(analysis.program), options: Options::default() })
    }

    fn read_files(files: &[PathBuf]) -> Result<Model, SetupError> {
        let cwd = cwd();
        let mut sources = Vec::new();
        for f in files {
            let text = std::fs::read_to_string(f).map_err(|e| config(format!("cannot read {}: {e}", f.display())))?;
            let name = f.strip_prefix(&cwd).unwrap_or(f).to_string_lossy().into_owned();
            sources.push((name, text));
        }
        let borrowed: Vec<(&str, &str)> = sources.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
        Model::compile(&borrowed)
    }

    /// The specs to check. Default: every spec in the files. Naming specs also makes
    /// checks strict unless [`strict`](Self::strict) says otherwise.
    pub fn specs<S: Into<String>>(mut self, specs: impl IntoIterator<Item = S>) -> Self {
        self.options.specs = Some(specs.into_iter().map(Into::into).collect());
        self
    }

    /// Specs left out of the default (all specs) selection.
    pub fn exclude<S: Into<String>>(mut self, specs: impl IntoIterator<Item = S>) -> Self {
        self.options.exclude = specs.into_iter().map(Into::into).collect();
        self
    }

    /// Require every selected spec to check at least one event. Default: on when
    /// [`specs`](Self::specs) are named, off when checking every spec.
    pub fn strict(mut self, strict: bool) -> Self {
        self.options.strict = Some(strict);
        self
    }

    /// Fail when a key's spec ends in a hot state (a liveness rule still waiting).
    pub fn check_hot(mut self, check_hot: bool) -> Self {
        self.options.check_hot = check_hot;
        self
    }

    /// What to do with event names no P file declares. Default: `Error`.
    pub fn unknown_events(mut self, policy: UnknownEvents) -> Self {
        self.options.unknown_events = policy;
        self
    }

    /// Merge each check's monitor coverage into this JSON file (the format of
    /// `snappea coverage merge-observe`). Overrides `SNAPPEA_COVERAGE`.
    pub fn coverage(mut self, path: impl Into<PathBuf>) -> Self {
        self.options.coverage = Some(path.into());
        self
    }

    /// A new recording. Every recording starts with fresh spec instances.
    /// It binds the current thread: global [`snappea::emit::emit`](crate::emit::emit) calls made on it go to this
    /// recording until it is checked or dropped.
    pub fn recording(&self) -> Recording {
        let sink = EventSink::default();
        routing::open(&sink.state);
        Recording { model: self.clone(), sink, inputs: Mutex::default(), root: OnceLock::new(), done: false }
    }

    /// Runs `body` with a new recording, then checks it, panicking with the failure
    /// output if the check fails. Put setup and cleanup inside `body`. A panic in `body`
    /// propagates unchanged, without a check. With `SNAPPEA_MODE=off`, nothing is checked.
    #[track_caller]
    pub fn check<T>(&self, body: impl FnOnce(&Recording) -> T) -> T {
        let mode = self.mode().unwrap_or_else(|e| panic!("{e}"));
        let recording = self.recording();
        let value = body(&recording);
        if mode != Mode::Off {
            if let Err(e) = recording.check() {
                panic!("{e}");
            }
        }
        value
    }

    fn mode(&self) -> Result<Mode, SetupError> {
        match std::env::var(MODE_ENV) {
            Ok(v) if !v.is_empty() => Mode::parse(&v, MODE_ENV),
            _ => Ok(self.options.file_mode.unwrap_or(Mode::Auto)),
        }
    }

    fn strict_enabled(&self) -> bool {
        self.options.strict.unwrap_or(self.options.specs.is_some())
    }

    fn coverage_path(&self) -> Option<PathBuf> {
        self.options
            .coverage
            .clone()
            .or_else(|| std::env::var_os(COVERAGE_ENV).filter(|v| !v.is_empty()).map(|v| cwd().join(v)))
            .or_else(|| self.options.file_coverage.clone())
    }

    fn select_specs(&self) -> Result<Vec<Spec<'_>>, SetupError> {
        let prog = &*self.program;
        let declared: Vec<&str> =
            prog.machines.iter().filter(|m| m.kind == MachineKind::Spec).map(|m| m.name.as_str()).collect();
        let unknown =
            |name: &str| config(format!("there is no spec named {name}; the P files declare: {}", declared.join(", ")));
        if let Some(names) = &self.options.specs {
            return names.iter().map(|n| Spec::new(prog, n).map_err(|_| unknown(n))).collect();
        }
        if let Some(x) = self.options.exclude.iter().find(|x| !declared.contains(&x.as_str())) {
            return Err(unknown(x));
        }
        let specs: Vec<Spec> = prog
            .machines
            .iter()
            .enumerate()
            .filter(|(_, m)| m.kind == MachineKind::Spec && !self.options.exclude.contains(&m.name))
            .map(|(i, _)| Spec::from_decl(prog, crate::sema::MachineId(i as u32)))
            .collect();
        if specs.is_empty() {
            return Err(config("no specs to check: the P files declare none, or all are excluded"));
        }
        Ok(specs)
    }
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_default()
}

/// Spec file paths, globs and folders, relative to `base`.
fn expand_spec_files(base: &Path, values: &[String]) -> Result<Vec<PathBuf>, SetupError> {
    let mut files: Vec<PathBuf> = Vec::new();
    let is_p = |p: &Path| p.extension().is_some_and(|e| e == "p");
    for value in values {
        let path = base.join(value);
        let mut found = Vec::new();
        if path.is_dir() {
            found = std::fs::read_dir(&path)
                .map_err(|e| config(format!("cannot read {}: {e}", path.display())))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file() && is_p(p))
                .collect();
            found.sort();
        } else {
            add_log_paths(&StdFiles, base, value, &mut found).map_err(|_| {
                config(format!("spec file {value} does not exist (or matches nothing) in {}", base.display()))
            })?;
            if !path.is_file() {
                found.retain(|p| is_p(p));
            }
        }
        if found.is_empty() {
            return Err(config(format!("no .p files in {value} ({})", base.display())));
        }
        for f in found {
            if !files.contains(&f) {
                files.push(f);
            }
        }
    }
    if files.is_empty() {
        return Err(config("no P spec files given"));
    }
    Ok(files)
}

#[derive(Debug, Default)]
struct Buffer {
    events: Vec<Event>,
    closed: bool,
    emitters: Vec<Emitter>,
    /// Events from the global `snappea::emit::emit` that could not be recorded.
    errors: Vec<(ErrorKind, String)>,
}

/// A recording's in-process event sink: cloneable, `Send` and `Sync`. Clones share one
/// ordered recording. Emit after the operation commits.
#[derive(Debug, Clone, Default)]
pub struct EventSink {
    state: Arc<Mutex<Buffer>>,
}

fn closed_error() -> SetupError {
    config("emitted into a recording that was already checked: join producer threads before check()")
}

impl EventSink {
    /// Records `event` on stream `key` now. `payload` is any `Serialize` value whose JSON
    /// matches the P payload type (`json!({...})`, a struct, a scalar, or `()`). Fails
    /// after the recording was checked.
    #[track_caller]
    pub fn emit(&self, event: &str, key: impl Display, payload: impl serde::Serialize) -> Result<(), SetupError> {
        self.push(None, event, key, payload)
    }

    /// [`emit`](Self::emit) at a given time: a `SystemTime`, or `i64` epoch nanoseconds
    /// (any increasing counter). Events are checked in time order per key.
    #[track_caller]
    pub fn emit_at(
        &self,
        at: impl Into<Time>,
        event: &str,
        key: impl Display,
        payload: impl serde::Serialize,
    ) -> Result<(), SetupError> {
        self.push(Some(at.into()), event, key, payload)
    }

    #[track_caller]
    fn push(
        &self,
        at: Option<Time>,
        event: &str,
        key: impl Display,
        payload: impl serde::Serialize,
    ) -> Result<(), SetupError> {
        let payload = serde_json::to_value(payload).map_err(|e| SetupError {
            kind: ErrorKind::Parser,
            message: format!("the payload of {event} cannot be serialized: {e}"),
            artifacts: None,
        })?;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err(closed_error());
        }
        // Stamped under the lock, so recording order and time order agree.
        state.events.push(Event::new(at, event, key, payload));
        Ok(())
    }

    /// An [`Emitter`] (from [`snappea::emit`](crate::emit)) that records into this sink, for application
    /// code instrumented with `snappea::emit`. Emitting through it after the check panics.
    pub fn emitter(&self) -> Emitter {
        let weak: Weak<Mutex<Buffer>> = Arc::downgrade(&self.state);
        let emitter = Emitter::callback(move |event| {
            let Some(state) = weak.upgrade() else { panic!("{}", closed_error()) };
            let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
            if state.closed {
                drop(state);
                panic!("{}", closed_error());
            }
            state.events.push(event);
        });
        self.state.lock().unwrap_or_else(|e| e.into_inner()).emitters.push(emitter.clone());
        emitter
    }

    /// Events received so far.
    pub fn len(&self) -> usize {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).events.len()
    }
    /// No events received yet.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

type LogParse = Box<dyn Fn(&str) -> Option<Value> + Send + Sync>;

enum Input {
    Dir(PathBuf),
    Log(PathBuf, Option<LogParse>),
}

/// Events recorded for one check: in-process events, the recording's events folder,
/// and any folders or logs added. Check it after cleanup, with producers joined.
/// Nothing is checked on drop.
pub struct Recording {
    model: Model,
    sink: EventSink,
    inputs: Mutex<Vec<Input>>,
    root: OnceLock<PathBuf>,
    done: bool,
}

impl std::fmt::Debug for Recording {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recording").field("events", &self.sink.len()).field("folder", &self.root.get()).finish()
    }
}

impl Recording {
    /// A handle for application code; clones share this recording.
    pub fn sink(&self) -> EventSink {
        self.sink.clone()
    }

    /// An [`Emitter`] recording into this recording (see [`EventSink::emitter`]).
    pub fn emitter(&self) -> Emitter {
        self.sink.emitter()
    }

    /// [`EventSink::emit`].
    #[track_caller]
    pub fn emit(&self, event: &str, key: impl Display, payload: impl serde::Serialize) -> Result<(), SetupError> {
        self.sink.push(None, event, key, payload)
    }

    /// [`EventSink::emit_at`].
    #[track_caller]
    pub fn emit_at(
        &self,
        at: impl Into<Time>,
        event: &str,
        key: impl Display,
        payload: impl serde::Serialize,
    ) -> Result<(), SetupError> {
        self.sink.push(Some(at.into()), event, key, payload)
    }

    /// In-process events so far.
    pub fn len(&self) -> usize {
        self.sink.len()
    }
    /// No in-process events yet.
    pub fn is_empty(&self) -> bool {
        self.sink.is_empty()
    }

    fn root(&self) -> &Path {
        self.root.get_or_init(|| {
            use std::hash::{BuildHasher, Hasher};
            static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let random = std::collections::hash_map::RandomState::new().build_hasher().finish() % 1_000_000;
            let base = std::env::var_os(ARTIFACTS_ENV)
                .filter(|v| !v.is_empty())
                .map(|v| cwd().join(v))
                .unwrap_or_else(std::env::temp_dir);
            base.join(format!("snappea-test-{}-{n}-{random:06}", std::process::id()))
        })
    }

    /// This recording's events folder, created on first use. Child processes write
    /// events there when started with [`env`](Self::env) (`SNAPPEA_EVENTS_DIR`), e.g. with
    /// [`snappea::emit::emit`](crate::emit::emit). Stop or join them before checking.
    pub fn events_dir(&self) -> PathBuf {
        let dir = self.root().join("events");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            panic!("cannot create the events folder {}: {e}", dir.display());
        }
        dir
    }

    /// The current environment plus `SNAPPEA_EVENTS_DIR`, for `Command::envs`.
    pub fn env(&self) -> Vec<(OsString, OsString)> {
        let dir = self.events_dir();
        let mut env: Vec<(OsString, OsString)> = std::env::vars_os().filter(|(k, _)| k != EVENTS_DIR_ENV).collect();
        env.push((EVENTS_DIR_ENV.into(), dir.into_os_string()));
        env
    }

    /// Also check the event files in `dir`, a folder this recording did not create (a
    /// container volume, a service started elsewhere).
    pub fn add_events_dir(&self, dir: impl AsRef<Path>) -> &Self {
        self.add(Input::Dir(cwd().join(dir)))
    }

    /// Also check `path`, a log of JSON lines in the events format.
    pub fn add_log(&self, path: impl AsRef<Path>) -> &Self {
        self.add(Input::Log(cwd().join(path), None))
    }

    /// Also check `path`, turning each line into an event with `parse`: a JSON object
    /// with `event`, `key`, `timestamp` and `payload` (as in the events format), or
    /// `None` to skip the line.
    pub fn add_log_with(
        &self,
        path: impl AsRef<Path>,
        parse: impl Fn(&str) -> Option<Value> + Send + Sync + 'static,
    ) -> &Self {
        self.add(Input::Log(cwd().join(path), Some(Box::new(parse))))
    }

    fn add(&self, input: Input) -> &Self {
        self.inputs.lock().unwrap_or_else(|e| e.into_inner()).push(input);
        self
    }

    /// Checks everything recorded. Fails with a [`SpecViolation`] or a [`SetupError`];
    /// a failure keeps the recording's folder and names it.
    pub fn check(mut self) -> Result<CheckReport, CheckError> {
        self.finish(true)
    }

    /// Requires the check to break `spec` with a message matching the regular
    /// expression `pattern` (searched), and no other spec. Returns the violation;
    /// panics otherwise. A negative control: shows the spec catches a known bug.
    #[track_caller]
    pub fn expect_violation(mut self, spec: &str, pattern: &str) -> SpecViolation {
        let re = regex::Regex::new(pattern).unwrap_or_else(|e| panic!("expect_violation: bad pattern: {e}"));
        let violation = match self.finish(false) {
            Ok(_) => panic!("expected P spec {spec} to be violated (matching {pattern:?}), but the check passed"),
            Err(CheckError::Setup(e)) => panic!("expected P spec {spec} to be violated, but the check failed:\n{e}"),
            Err(CheckError::Violation(v)) => *v,
        };
        let mut all = vec![violation.clone()];
        all.extend(violation.others.iter().cloned());
        if let Some(other) = all.iter().find(|v| v.spec != spec) {
            panic!("expected only P spec {spec} to be violated, but {} was too:\n{violation}", other.spec);
        }
        match all.into_iter().find(|v| re.is_match(&v.message)) {
            Some(mut v) => {
                v.others.clear();
                v
            }
            None => panic!("P spec {spec} was violated, but no message matches {pattern:?}:\n{violation}"),
        }
    }

    fn finish(&mut self, keep_artifacts: bool) -> Result<CheckReport, CheckError> {
        self.done = true;
        routing::close(&self.sink.state);
        let (events, failed) = {
            let mut state = self.sink.state.lock().unwrap_or_else(|e| e.into_inner());
            state.closed = true;
            let dropped: u64 = state.emitters.iter().map(Emitter::dropped).sum();
            let failed = if let Some(&(kind, _)) = state.errors.first() {
                let message = state.errors.iter().map(|(_, m)| m.as_str()).collect::<Vec<_>>().join("\n");
                Some(SetupError { kind, message, artifacts: None })
            } else if dropped > 0 {
                Some(SetupError {
                    kind: ErrorKind::Parser,
                    message: format!(
                        "{dropped} event(s) emitted through an Emitter had payloads that cannot be serialized"
                    ),
                    artifacts: None,
                })
            } else {
                None
            };
            (std::mem::take(&mut state.events), failed)
        };
        let result = match failed {
            Some(e) => Err(e.into()),
            None => self.run(&events),
        };
        let root = self.root.get();
        match result {
            Ok(report) => {
                if let Some(root) = root {
                    let _ = std::fs::remove_dir_all(root);
                }
                Ok(report)
            }
            Err(mut e) => {
                if keep_artifacts && (root.is_some() || std::env::var_os(ARTIFACTS_ENV).is_some()) {
                    let root = self.root().to_path_buf();
                    let lines: String = events.iter().map(|e| e.to_json() + "\n").collect();
                    let written = std::fs::create_dir_all(&root)
                        .and_then(|_| std::fs::write(root.join("recording.jsonl"), lines));
                    if written.is_ok() {
                        e.set_artifacts(root);
                    }
                } else if let Some(root) = root {
                    let _ = std::fs::remove_dir_all(root);
                }
                Err(e)
            }
        }
    }

    fn run(&self, events: &[Event]) -> Result<CheckReport, CheckError> {
        let cwd = cwd();
        let mut inputs: Vec<Vec<String>> = vec![events.iter().map(Event::to_json).collect()];
        let mut names = vec![RECORDING_INPUT.to_string()];
        let mut dirs: Vec<PathBuf> =
            self.root.get().map(|r| r.join("events")).filter(|d| d.is_dir()).into_iter().collect();
        let mut logs = Vec::new();
        for input in std::mem::take(&mut *self.inputs.lock().unwrap_or_else(|e| e.into_inner())) {
            match input {
                Input::Dir(d) => dirs.push(d),
                Input::Log(path, parse) => logs.push((path, parse)),
            }
        }
        let display = |p: &Path| p.strip_prefix(&cwd).unwrap_or(p).to_string_lossy().into_owned();
        for dir in &dirs {
            for file in log_paths(&StdFiles, &cwd, &[dir.to_string_lossy()]).map_err(config)? {
                let text = std::fs::read(&file).map_err(|e| config(format!("cannot read {}: {e}", file.display())))?;
                inputs.push(split_entries(&String::from_utf8_lossy(&text), "\n"));
                names.push(display(&file));
            }
        }
        for (path, parse) in logs {
            let text =
                std::fs::read_to_string(&path).map_err(|e| config(format!("cannot read {}: {e}", path.display())))?;
            let entries = match parse {
                None => split_entries(&text, "\n"),
                Some(parse) => text
                    .lines()
                    .enumerate()
                    .filter_map(|(i, line)| {
                        let mut v = parse(line)?;
                        if let Some(o) = v.as_object_mut() {
                            o.entry("src").or_insert_with(|| format!("{}:{}", display(&path), i + 1).into());
                        }
                        Some(v.to_string())
                    })
                    .collect(),
            };
            inputs.push(entries);
            names.push(display(&path));
        }

        let strict = self.model.strict_enabled();
        if !strict && inputs.iter().all(|entries| entries.iter().all(|e| e.trim().is_empty())) {
            return Ok(CheckReport::default());
        }
        let specs = self.model.select_specs()?;
        let options = ObserveOptions {
            strict,
            check_hot: self.model.options.check_hot,
            unknown_events: self.model.options.unknown_events,
            input_names: names,
            ..ObserveOptions::default()
        };
        let started = Instant::now();
        let mut report = CheckReport::default();
        for spec in &specs {
            let mut r = spec.observe(&inputs, &Lines, &options, &mut || started.elapsed().as_secs_f64());
            for u in &mut r.unknown_events {
                // Point at the emit call rather than an entry number of the recording.
                if let Some(src) = u
                    .location
                    .strip_prefix(RECORDING_INPUT)
                    .and_then(|n| n.strip_prefix(':')?.parse::<usize>().ok())
                    .and_then(|n| events.get(n.checked_sub(1)?)?.src.clone())
                {
                    u.location = src;
                }
            }
            if options.unknown_events == crate::checker::observe::UnknownEvents::Warn {
                for u in &r.unknown_events {
                    let warning = format!("warning: {}", u.message());
                    if !report.warnings.contains(&warning) {
                        eprintln!("{warning}");
                        report.warnings.push(warning);
                    }
                }
            }
            report.reports.push((spec.name().to_string(), r));
        }
        let coverage = self.merge_coverage(&report);
        classify(&report)?;
        coverage?;
        Ok(report)
    }

    fn merge_coverage(&self, report: &CheckReport) -> Result<(), SetupError> {
        let Some(path) = self.model.coverage_path() else { return Ok(()) };
        let suite = merge_coverage_file(&path, &report.coverage()?)?;
        // Print the summary when this process first sees the coverage change, not on
        // every check.
        static PRINTED: Mutex<Option<Value>> = Mutex::new(None);
        let specs = suite.to_json()["specs"].clone();
        let mut printed = PRINTED.lock().unwrap_or_else(|e| e.into_inner());
        if printed.as_ref() != Some(&specs) {
            println!("{}\nsnappea coverage: {}", suite.render_text(), path.display());
            *printed = Some(specs);
        }
        Ok(())
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if !self.done {
            routing::close(&self.sink.state);
            if let Some(root) = self.root.get() {
                let _ = std::fs::remove_dir_all(root);
            }
        }
    }
}

/// Merges `coverage` into the suite file at `path` under an exclusive lock, so several
/// test threads and processes can share one file. Returns the merged suite.
fn merge_coverage_file(path: &Path, coverage: &ObserveCoverageReport) -> Result<ObserveCoverageReport, SetupError> {
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fail = |e: String| config(format!("coverage file {}: {e}", path.display()));
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| fail(e.to_string()))?;
    }
    let name = path.file_name().ok_or_else(|| fail("not a file path".into()))?.to_string_lossy().into_owned();
    let _lock = FileLock::acquire(path.with_file_name(format!("{name}.lock"))).map_err(fail)?;
    let mut suite = match std::fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => ObserveCoverageReport::from_json_str(&text).map_err(fail)?,
        Ok(_) => ObserveCoverageReport::default(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ObserveCoverageReport::default(),
        Err(e) => return Err(fail(e.to_string())),
    };
    suite.merge(coverage).map_err(fail)?;
    let tmp = path.with_file_name(format!("{name}.{}.tmp", std::process::id()));
    let json = serde_json::to_string_pretty(&suite.to_json()).map_err(|e| fail(e.to_string()))? + "\n";
    std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, path)).map_err(|e| fail(e.to_string()))?;
    Ok(suite)
}

/// A lock file, created exclusively and removed on drop. One older than a minute is
/// taken to be left by a crashed process.
struct FileLock(PathBuf);

impl FileLock {
    fn acquire(path: PathBuf) -> Result<FileLock, String> {
        let started = Instant::now();
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(FileLock(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .is_ok_and(|t| t.elapsed().unwrap_or_default() > Duration::from_secs(60));
                    if stale {
                        let _ = std::fs::remove_file(&path);
                    } else if started.elapsed() > Duration::from_secs(120) {
                        return Err(format!("timed out waiting for {}", path.display()));
                    } else {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Turns the core reports into the first failure, by priority: unreadable input,
/// unknown event names, violations, then other setup errors.
fn classify(report: &CheckReport) -> Result<(), CheckError> {
    let mut parser: Vec<String> = Vec::new();
    let mut unknown: Vec<(String, String)> = Vec::new();
    let mut violations: Vec<SpecViolation> = Vec::new();
    let mut other: Vec<(ErrorKind, String)> = Vec::new();
    for (spec, r) in &report.reports {
        for e in &r.errors {
            match e.kind {
                CoreKind::Parser => {
                    let text = parser_message(e);
                    if !parser.contains(&text) {
                        parser.push(text);
                    }
                }
                CoreKind::UnknownEvent => {
                    for u in &r.unknown_events {
                        if !unknown.iter().any(|(name, _)| *name == u.name) {
                            unknown.push((u.name.clone(), u.message()));
                        }
                    }
                }
                CoreKind::SpecViolation | CoreKind::Unknown => violations.push(violation(spec, e)),
                CoreKind::OutOfOrder => other.push((ErrorKind::OutOfOrder, e.message.clone())),
                CoreKind::Validation => other.push((ErrorKind::Validation, e.message.clone())),
            }
        }
    }
    let setup = |kind, message| Err(CheckError::Setup(SetupError { kind, message, artifacts: None }));
    if !parser.is_empty() {
        return setup(ErrorKind::Parser, format!("Some events cannot be read:\n{}", parser.join("\n")));
    }
    if !unknown.is_empty() {
        return setup(ErrorKind::UnknownEvent, unknown.into_iter().map(|(_, m)| m).collect::<Vec<_>>().join("\n"));
    }
    if !violations.is_empty() {
        let mut first = violations.remove(0);
        first.others = violations;
        return Err(first.into());
    }
    if let Some(&(kind, _)) = other.first() {
        return setup(kind, other.into_iter().map(|(_, m)| m).collect::<Vec<_>>().join("\n"));
    }
    Ok(())
}

fn parser_message(e: &ObserveError) -> String {
    let cause = e.cause.as_deref().unwrap_or("cannot be read");
    match serde_json::from_str::<Value>(&e.message) {
        Ok(line) if line.get("event").is_some_and(Value::is_string) => {
            let src = line["src"].as_str().map(|s| format!(" ({s})")).unwrap_or_default();
            format!("  {}{src}: {cause}\n    payload: {}", line["event"].as_str().unwrap_or(""), line["payload"])
        }
        _ => format!("  {cause}\n    line: {}", e.message),
    }
}

fn violation(spec: &str, e: &ObserveError) -> SpecViolation {
    let raw = if e.message.is_empty() || e.message == "null" { e.detail.as_str() } else { e.message.as_str() };
    let (message, location, liveness) = split_message(raw);
    let key = e.replay.as_ref().map(|r| r.key.clone()).unwrap_or_default();
    let all: Vec<Event> = e.replay.iter().flat_map(|r| r.events.iter().map(event_of)).collect();
    let earlier = all.len().saturating_sub(REPORT_EVENTS);
    SpecViolation {
        spec: spec.to_string(),
        key,
        message,
        location,
        events: all[earlier..].to_vec(),
        liveness,
        others: Vec::new(),
        artifacts: None,
        earlier,
    }
}

/// `(message, P assert file:line, liveness)` of a core error message.
fn split_message(raw: &str) -> (String, Option<String>, bool) {
    if let Some(rest) = raw.strip_prefix("Assertion failure: ") {
        if let Some((loc, message)) = rest.split_once(' ') {
            let mut parts = loc.rsplitn(3, ':');
            if let (Some(col), Some(line), Some(file)) = (parts.next(), parts.next(), parts.next()) {
                if col.parse::<u32>().is_ok() && line.parse::<u32>().is_ok() {
                    return (message.to_string(), Some(format!("{file}:{line}")), false);
                }
            }
        }
        return (rest.to_string(), None, false);
    }
    if let Some((_, state)) = raw.split_once(" detected liveness bug in hot state '") {
        let state = state.split('\'').next().unwrap_or(state);
        return (format!("it ended in hot state '{state}'"), None, true);
    }
    (raw.to_string(), None, false)
}

fn event_of(o: &ObservedEvent) -> Event {
    let line: Value = o.log_line.as_deref().and_then(|l| serde_json::from_str(l).ok()).unwrap_or_default();
    let nanos =
        i64::try_from(i128::from(o.timestamp.secs) * 1_000_000_000 + i128::from(o.timestamp.nanos)).unwrap_or(i64::MAX);
    Event {
        event: line["event"].as_str().unwrap_or_default().to_string(),
        key: o.key.clone(),
        payload: line["payload"].clone(),
        time: Time::from_nanos(nanos),
        at: line["at"] == true,
        src: line["src"].as_str().map(String::from),
        seq: line["seq"].as_u64().unwrap_or(0),
        pid: line["pid"].as_u64().and_then(|p| u32::try_from(p).ok()).unwrap_or(0),
    }
}

/// Reads events-format lines. Undeclared names are reported for `unknown_events`;
/// declared events the spec does not observe are skipped before payload conversion.
struct Lines;

impl Parser for Lines {
    fn name(&self) -> &str {
        "snappea-test"
    }
    fn parse(&self, entry: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
        self.parse_entry(entry, spec).map(|p| p.events)
    }
    fn parse_entry(&self, entry: &str, spec: &Spec) -> Result<ParsedEntry, String> {
        if entry.trim().is_empty() {
            return Ok(ParsedEntry::default());
        }
        let line: Value = serde_json::from_str(entry).map_err(|e| format!("not a JSON line: {e}"))?;
        let name = line.get("event").and_then(Value::as_str).ok_or("missing event name")?;
        let Some(id) = spec.event_id(name) else {
            return Ok(ParsedEntry { events: Vec::new(), unknown_events: vec![name.to_string()] });
        };
        if !spec.observes_event(id) {
            return Ok(ParsedEntry::default());
        }
        let payload = line.get("payload").unwrap_or(&Value::Null);
        if matches!(spec.payload_type(id), PType::Null) && !payload.is_null() {
            return Err(format!("{name} has no payload, so its payload must be null"));
        }
        let value = crate::checker::observe_json::value_from_json(spec.prog, payload, spec.payload_type(id))?;
        let key = match line.get("key") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
        };
        let event = spec.event(key, timestamp(line.get("timestamp"))?, name, value, Some(entry))?;
        Ok(ParsedEntry { events: vec![event], unknown_events: Vec::new() })
    }
}

/// ISO-8601, or an integer of epoch milliseconds, as `snappea observe` reads it.
fn timestamp(v: Option<&Value>) -> Result<Timestamp, String> {
    match v {
        Some(Value::String(s)) => match s.parse::<i64>() {
            Ok(ms) => Ok(Timestamp::from_millis(ms)),
            Err(_) => crate::checker::observe_json::parse_instant(s).ok_or_else(|| format!("invalid timestamp '{s}'")),
        },
        Some(Value::Number(n)) => {
            n.as_i64().map(Timestamp::from_millis).ok_or_else(|| format!("invalid timestamp {n}"))
        }
        _ => Err("missing timestamp".into()),
    }
}

/// A passed check.
#[derive(Debug, Default)]
pub struct CheckReport {
    reports: Vec<(String, Report)>,
    warnings: Vec<String>,
}

impl CheckReport {
    /// The specs checked. Empty when nothing was emitted and the check was not strict.
    pub fn specs(&self) -> Vec<&str> {
        self.reports.iter().map(|(s, _)| s.as_str()).collect()
    }
    /// Events read, summed over the checked specs (each counts the events it observes).
    pub fn events_read(&self) -> u64 {
        self.reports.iter().map(|(_, r)| r.metrics.events_read).sum()
    }
    /// Events checked, summed over the checked specs.
    pub fn verified_events(&self) -> u64 {
        self.reports.iter().map(|(_, r)| r.metrics.verified_events()).sum()
    }
    /// `unknown_events = warn` messages.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
    /// Observed monitor coverage of this check, per spec.
    pub fn coverage(&self) -> Result<ObserveCoverageReport, SetupError> {
        let mut all = ObserveCoverageReport::default();
        for (spec, report) in &self.reports {
            let one = ObserveCoverageReport::from_observation(spec, report).map_err(config)?;
            all.merge(&one).map_err(config)?;
        }
        Ok(all)
    }
}
