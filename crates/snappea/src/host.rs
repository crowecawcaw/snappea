//! The environment the commands run in: console, file system, clock and entropy.
//!
//! Commands never touch `std::fs`, `std::time` or `std::process` directly so that the crate
//! also runs in a browser. [`MemHost`] is an in-memory implementation for tests and wasm;
//! `snappea::cli` supplies a native one.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

pub use crate::checker::observe_inputs::{LiveRead, LiveSource};

/// Console stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}

/// Console colors upstream uses (`ConsoleColor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Red,
    Yellow,
    Cyan,
}

/// Everything the commands need from their environment. File system methods behave like
/// their `std::fs` namesakes unless documented otherwise.
pub trait Host {
    /// Writes console text (no newline added).
    fn write(&mut self, stream: Stream, text: &str, color: Option<Color>);
    /// Reads one complete stdin line, or `None` at EOF. Live observe hosts override this.
    fn read_stdin_line(&self) -> io::Result<Option<String>> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "stdin is unavailable on this host"))
    }
    /// The current directory (absolute).
    fn cwd(&self) -> PathBuf;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    /// Creates or replaces a file, creating its parent directories first.
    fn write_file(&mut self, path: &Path, contents: &str) -> io::Result<()>;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    /// Entries (full paths) of a directory, sorted.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn create_dir_all(&mut self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&mut self, path: &Path) -> io::Result<()>;
    /// Removes a file. The default reports it unsupported.
    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, format!("cannot remove '{}' on this host", path.display())))
    }
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()>;
    /// Seconds since an arbitrary fixed point (for elapsed-time reporting).
    fn now(&self) -> f64;
    /// Peak memory use of the process in gigabytes, or 0 if unknown.
    fn max_memory_gb(&self) -> f64;
    /// A fresh random seed (used when `--seed` is not given).
    fn random_seed(&mut self) -> u32;
    /// Seconds since the Unix epoch (wall-clock time, for timestamped names), or 0 if unknown.
    fn unix_time(&self) -> f64;
    /// Threads for checking schedules in parallel; `None` (the default) when the host has
    /// none, and checks then run a single worker.
    fn threads(&self) -> Option<Threads> {
        None
    }
    /// Whether stdout is a terminal: commands then print text rather than JSON by default.
    fn stdout_is_terminal(&self) -> bool {
        false
    }
    /// Whether stderr is a terminal: `snappea check` then shows a live status line there.
    fn stderr_is_terminal(&self) -> bool {
        false
    }
    /// The value of environment variable `name`, if it is set.
    fn env_var(&self, _name: &str) -> Option<String> {
        None
    }
    /// Receives a snapshot of a running check, a few times a second, when
    /// [`CheckConfig::progress`](crate::driver::check::CheckConfig::progress) is set. The
    /// default ignores it.
    fn progress(&mut self, _p: &crate::driver::progress::Progress) {}
    /// Whether errors get a `hint:` line with a suggested next step. Only `snappea`
    /// commands turn it on; upstream-compatible output keeps upstream's exact text.
    fn shows_hints(&self) -> bool {
        false
    }
    /// Opens live observation inputs (`--live`). The default reads them one after another,
    /// stdin through [`Host::read_stdin_line`] and files to their current end; hosts that
    /// can wait on several streams at once override it.
    fn open_live(&self, inputs: &[LiveInput]) -> Box<dyn LiveSource + '_> {
        Box::new(SequentialLive { host: self, inputs: inputs.to_vec(), current: 0, lines: None })
    }
}

/// One `--live` input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveInput {
    /// `-`: standard input.
    Stdin,
    /// A file, named pipe or other readable path (absolute).
    Path(PathBuf),
}

/// [`Host::open_live`]'s default: each input in turn, to its end.
struct SequentialLive<'a, H: Host + ?Sized> {
    host: &'a H,
    inputs: Vec<LiveInput>,
    current: usize,
    /// The remaining lines of the current file input.
    lines: Option<std::vec::IntoIter<String>>,
}

impl<H: Host + ?Sized> LiveSource for SequentialLive<'_, H> {
    fn next(&mut self) -> LiveRead {
        let input = self.current;
        let line = match self.inputs.get(input) {
            None => return LiveRead::End { input },
            Some(LiveInput::Stdin) => self.host.read_stdin_line(),
            Some(LiveInput::Path(p)) => {
                if self.lines.is_none() {
                    match self.host.read_to_string(p) {
                        Ok(text) => {
                            let lines: Vec<String> = text.lines().map(String::from).collect();
                            self.lines = Some(lines.into_iter());
                        }
                        Err(e) => return LiveRead::Error { input, error: e.to_string() },
                    }
                }
                Ok(self.lines.as_mut().and_then(|l| l.next()))
            }
        };
        match line {
            Ok(Some(line)) => LiveRead::Line { input, line },
            Ok(None) => {
                self.current += 1;
                self.lines = None;
                LiveRead::End { input }
            }
            Err(e) => LiveRead::Error { input, error: e.to_string() },
        }
    }
}

/// Work for a thread of [`Threads::run`].
pub type Job<'a> = Box<dyn FnOnce() + Send + 'a>;

/// A host's threads (see [`Host::threads`]).
#[derive(Debug, Clone, Copy)]
pub struct Threads {
    /// How many workers `snappea check` uses by default (one per CPU core).
    pub available: usize,
    /// Runs each job on its own thread and `main` on the calling thread; returns once all
    /// have finished.
    pub run: for<'a> fn(jobs: Vec<Job<'a>>, main: &mut (dyn FnMut() + 'a)),
}

/// A command stopped after reporting the problem on the console; holds the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit(pub i32);

/// Prints `Error: {msg}` on stderr; returns exit code 1.
pub(crate) fn report_and_exit(host: &mut dyn Host, msg: &str) -> Exit {
    host.write(Stream::Err, "Error: ", Some(Color::Red));
    host.write(Stream::Err, msg, Some(Color::Yellow));
    host.write(Stream::Err, "\n", None);
    Exit(1)
}

/// [`report_and_exit`], followed by `hint: {hint}` when the host [shows hints](Host::shows_hints).
pub(crate) fn report_and_exit_hint(host: &mut dyn Host, msg: &str, hint: &str) -> Exit {
    let exit = report_and_exit(host, msg);
    if host.shows_hints() {
        host.write(Stream::Err, &format!("hint: {}\n", hint), None);
    }
    exit
}

/// Writes an output file the command has announced (`..... Writing {path}`), `path` being
/// relative to the current directory or absolute. A failure is reported as
/// `Error: Could not write file ...` with exit code 1.
pub(crate) fn write_output(host: &mut dyn Host, path: &str, contents: &str) -> Result<(), Exit> {
    let full = full_path(&host.cwd(), Path::new(path));
    host.write_file(&full, contents)
        .map_err(|e| report_and_exit(host, &format!("Could not write file '{}': {}", path, e)))
}

/// Convenience console helpers.
pub trait HostExt: Host {
    fn out_line(&mut self, s: &str) {
        self.write(Stream::Out, s, None);
        self.write(Stream::Out, "\n", None);
    }
    fn out_colored_line(&mut self, s: &str, c: Color) {
        self.write(Stream::Out, s, Some(c));
        self.write(Stream::Out, "\n", None);
    }
}

impl<T: Host + ?Sized> HostExt for T {}

/// A host that holds back console output, to show it (or its errors) only if needed.
#[cfg(feature = "cli")]
pub(crate) struct Buffered<'a> {
    pub inner: &'a mut dyn Host,
    pub out: Vec<(Stream, String, Option<Color>)>,
}

#[cfg(feature = "cli")]
impl<'a> Buffered<'a> {
    pub fn new(inner: &'a mut dyn Host) -> Self {
        Buffered { inner, out: Vec::new() }
    }

    /// The text written to stderr so far.
    pub fn errors(&self) -> String {
        self.out.iter().filter(|(s, ..)| *s == Stream::Err).map(|(_, t, _)| t.as_str()).collect()
    }
}

#[cfg(feature = "cli")]
impl Host for Buffered<'_> {
    fn write(&mut self, stream: Stream, text: &str, color: Option<Color>) {
        self.out.push((stream, text.to_string(), color));
    }
    fn cwd(&self) -> PathBuf {
        self.inner.cwd()
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.inner.read_to_string(path)
    }
    fn write_file(&mut self, path: &Path, contents: &str) -> io::Result<()> {
        self.inner.write_file(path, contents)
    }
    fn is_file(&self, path: &Path) -> bool {
        self.inner.is_file(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.inner.read_dir(path)
    }
    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        self.inner.create_dir_all(path)
    }
    fn remove_dir_all(&mut self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(path)
    }
    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }
    fn now(&self) -> f64 {
        self.inner.now()
    }
    fn max_memory_gb(&self) -> f64 {
        self.inner.max_memory_gb()
    }
    fn random_seed(&mut self) -> u32 {
        self.inner.random_seed()
    }
    fn unix_time(&self) -> f64 {
        self.inner.unix_time()
    }
    fn threads(&self) -> Option<Threads> {
        self.inner.threads()
    }
    fn stdout_is_terminal(&self) -> bool {
        self.inner.stdout_is_terminal()
    }
    fn stderr_is_terminal(&self) -> bool {
        self.inner.stderr_is_terminal()
    }
    fn env_var(&self, name: &str) -> Option<String> {
        self.inner.env_var(name)
    }
    fn shows_hints(&self) -> bool {
        self.inner.shows_hints()
    }
    fn progress(&mut self, p: &crate::driver::progress::Progress) {
        self.inner.progress(p)
    }
}

/// A host that shows only warnings and errors: stderr, and the stdout text the commands
/// color (upstream's console colors every warning and error, never progress). Plain stdout
/// text is dropped, unless `quiet` is off.
#[cfg(feature = "cli")]
pub(crate) struct Quiet<'a> {
    pub inner: &'a mut dyn Host,
    pub quiet: bool,
    /// The last stdout text was shown, so the newline ending it is too.
    shown: bool,
    /// When set, all stdout text goes here instead.
    pub captured: Option<String>,
    /// With `record_errors`, the stderr text so far (also written to stderr).
    pub captured_err: String,
    /// With `record_errors`, the stdout text shown so far.
    pub shown_out: String,
    record_errors: bool,
    /// How progress snapshots are shown; `None` passes them on only.
    pub progress: Option<ProgressView>,
}

/// How [`Quiet`] shows progress snapshots (`snappea check --progress`).
#[cfg(feature = "cli")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ProgressView {
    /// A status line on a terminal's stderr, redrawn in place and cleared when done.
    Live { shown: bool },
    /// A plain stderr line every [`HEARTBEAT`] seconds (CI logs).
    Heartbeat { printed_at: Option<f64> },
    /// Every snapshot as one line of JSON on stderr.
    Json,
}

/// Seconds between two [`ProgressView::Heartbeat`] lines.
#[cfg(feature = "cli")]
pub(crate) const HEARTBEAT: f64 = 30.0;

/// Erases the current terminal line.
#[cfg(feature = "cli")]
const CLEAR_LINE: &str = "\r\x1b[2K";

#[cfg(feature = "cli")]
impl<'a> Quiet<'a> {
    pub fn new(inner: &'a mut dyn Host, quiet: bool) -> Self {
        Quiet {
            inner,
            quiet,
            shown: false,
            captured: None,
            captured_err: String::new(),
            shown_out: String::new(),
            record_errors: false,
            progress: None,
        }
    }

    /// Erases a shown status line before other text is written.
    fn clear_status(&mut self) {
        if let Some(ProgressView::Live { shown: shown @ true }) = &mut self.progress {
            *shown = false;
            self.inner.write(Stream::Err, CLEAR_LINE, None);
        }
    }

    /// A host that shows everything, and keeps the stderr text in [`Quiet::captured_err`].
    pub fn recording_errors(inner: &'a mut dyn Host) -> Self {
        Quiet { record_errors: true, ..Quiet::new(inner, false) }
    }

    /// A host that keeps all stdout text in [`Quiet::captured`].
    pub fn capturing(inner: &'a mut dyn Host) -> Self {
        Quiet { captured: Some(String::new()), record_errors: true, ..Quiet::new(inner, true) }
    }
}

#[cfg(feature = "cli")]
impl Host for Quiet<'_> {
    fn write(&mut self, stream: Stream, text: &str, color: Option<Color>) {
        if stream == Stream::Err && self.record_errors {
            self.captured_err.push_str(text);
        }
        if let Some(c) = self.captured.as_mut().filter(|_| stream == Stream::Out) {
            return c.push_str(text);
        }
        if stream == Stream::Out && self.record_errors {
            self.shown_out.push_str(text);
        }
        if stream == Stream::Err || !self.quiet {
            self.clear_status();
            return self.inner.write(stream, text, color);
        }
        if color.is_some() || (self.shown && text == "\n") {
            self.shown = true;
            self.clear_status();
            self.inner.write(stream, text, color);
        } else {
            self.shown = false;
        }
    }
    fn read_stdin_line(&self) -> io::Result<Option<String>> {
        self.inner.read_stdin_line()
    }
    fn cwd(&self) -> PathBuf {
        self.inner.cwd()
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.inner.read_to_string(path)
    }
    fn write_file(&mut self, path: &Path, contents: &str) -> io::Result<()> {
        self.inner.write_file(path, contents)
    }
    fn is_file(&self, path: &Path) -> bool {
        self.inner.is_file(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.inner.read_dir(path)
    }
    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        self.inner.create_dir_all(path)
    }
    fn remove_dir_all(&mut self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(path)
    }
    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }
    fn now(&self) -> f64 {
        self.inner.now()
    }
    fn max_memory_gb(&self) -> f64 {
        self.inner.max_memory_gb()
    }
    fn random_seed(&mut self) -> u32 {
        self.inner.random_seed()
    }
    fn unix_time(&self) -> f64 {
        self.inner.unix_time()
    }
    fn threads(&self) -> Option<Threads> {
        self.inner.threads()
    }
    fn stdout_is_terminal(&self) -> bool {
        self.inner.stdout_is_terminal()
    }
    fn env_var(&self, name: &str) -> Option<String> {
        self.inner.env_var(name)
    }
    fn stderr_is_terminal(&self) -> bool {
        self.inner.stderr_is_terminal()
    }
    /// `Quiet` wraps the host of every `snappea` command, so its errors carry hints.
    fn shows_hints(&self) -> bool {
        true
    }
    fn progress(&mut self, p: &crate::driver::progress::Progress) {
        self.inner.progress(p);
        match &mut self.progress {
            None => {}
            Some(ProgressView::Live { shown }) => {
                if p.done {
                    if *shown {
                        *shown = false;
                        self.inner.write(Stream::Err, CLEAR_LINE, None);
                    }
                } else {
                    *shown = true;
                    // a line that wraps could not be redrawn in place
                    let text: String = p.text().chars().take(79).collect();
                    self.inner.write(Stream::Err, &format!("{}{}", CLEAR_LINE, text), None);
                }
            }
            Some(ProgressView::Heartbeat { printed_at }) => {
                if p.done {
                    *printed_at = None;
                } else if p.elapsed - printed_at.unwrap_or(0.0) >= HEARTBEAT {
                    *printed_at = Some(p.elapsed);
                    self.inner.write(Stream::Err, &format!("{}\n", p.text()), None);
                }
            }
            Some(ProgressView::Json) => {
                self.inner.write(Stream::Err, &format!("{}\n", p.to_json()), None);
            }
        }
    }
    fn open_live(&self, inputs: &[LiveInput]) -> Box<dyn LiveSource + '_> {
        self.inner.open_live(inputs)
    }
}

/// A host's file system, for expanding log paths and globs.
#[cfg(feature = "cli")]
pub(crate) struct HostFiles<'a>(pub &'a dyn Host);

#[cfg(feature = "cli")]
impl crate::checker::observe_inputs::LogFiles for HostFiles<'_> {
    fn is_file(&self, path: &Path) -> bool {
        self.0.is_file(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.0.is_dir(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.0.read_dir(path)
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.0.read_to_string(path)
    }
}

/// `Path.GetFullPath`: makes `p` absolute against `cwd` and resolves `.`/`..` lexically.
pub fn full_path(cwd: &Path, p: &Path) -> PathBuf {
    let joined = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
    normalize(&joined)
}

/// Lexically resolves `.` and `..` components.
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `Path.GetRelativePath(from, to)` for absolute, normalized paths.
pub fn relative_path(from: &Path, to: &Path) -> String {
    let f: Vec<_> = from.components().collect();
    let t: Vec<_> = to.components().collect();
    let common = f.iter().zip(t.iter()).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return to.to_string_lossy().to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in common..f.len() {
        parts.push("..".into());
    }
    for c in &t[common..] {
        parts.push(c.as_os_str().to_string_lossy().to_string());
    }
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

/// Whether a file name is hidden the way .NET reports it on Unix (leading dot).
pub fn is_hidden(p: &Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with('.')).unwrap_or(false)
}

#[derive(Debug, Clone)]
enum Node {
    Dir,
    File(String),
}

/// An in-memory host: a virtual file system plus captured console output. Memory use and
/// wall-clock time read as 0.
#[derive(Debug, Clone)]
pub struct MemHost {
    nodes: BTreeMap<PathBuf, Node>,
    pub cwd: PathBuf,
    pub stdout: String,
    pub stderr: String,
    /// [`Host::now`]; always 0 by default.
    pub clock: fn() -> f64,
    /// [`Host::random_seed`].
    pub seed: u32,
    /// [`Host::threads`]; `None` by default.
    pub threads: Option<Threads>,
    /// [`Host::stdout_is_terminal`]; true by default.
    pub terminal: bool,
    /// [`Host::stderr_is_terminal`]; false by default.
    pub stderr_terminal: bool,
    /// Called with each [`Host::progress`] snapshot; `None` by default.
    pub on_progress: Option<fn(&crate::driver::progress::Progress)>,
    /// [`Host::env_var`]; empty by default.
    pub env: BTreeMap<String, String>,
}

impl Default for MemHost {
    fn default() -> Self {
        MemHost::new("/work")
    }
}

impl MemHost {
    pub fn new(cwd: &str) -> MemHost {
        let mut h = MemHost {
            nodes: BTreeMap::new(),
            cwd: PathBuf::from(cwd),
            stdout: String::new(),
            stderr: String::new(),
            clock: || 0.0,
            seed: 0,
            threads: None,
            terminal: true,
            stderr_terminal: false,
            on_progress: None,
            env: BTreeMap::new(),
        };
        h.create_dir_all(Path::new(cwd)).expect("an empty file system has no files in the way");
        h
    }

    fn abs(&self, p: &Path) -> PathBuf {
        full_path(&self.cwd, p)
    }

    /// Adds a file (creating parent directories), path relative to the cwd or absolute.
    ///
    /// # Panics
    ///
    /// If the file cannot be written (a directory is in the way, or a file where a parent
    /// directory should be).
    pub fn add_file(&mut self, path: &str, contents: &str) {
        if let Err(e) = self.write_file(Path::new(path), contents) {
            panic!("cannot add {}: {}", path, e);
        }
    }

    /// All files under `dir` (absolute paths).
    pub fn files_under(&self, dir: &str) -> Vec<PathBuf> {
        let d = self.abs(Path::new(dir));
        self.nodes
            .iter()
            .filter(|(p, n)| matches!(n, Node::File(_)) && p.starts_with(&d))
            .map(|(p, _)| p.clone())
            .collect()
    }
}

fn not_found(p: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("Could not find a part of the path '{}'.", p.display()))
}

fn is_a_directory(p: &Path) -> io::Error {
    io::Error::other(format!("Is a directory: '{}'", p.display()))
}

fn not_a_directory(p: &Path) -> io::Error {
    io::Error::other(format!("Not a directory: '{}'", p.display()))
}

impl Host for MemHost {
    fn stdout_is_terminal(&self) -> bool {
        self.terminal
    }

    fn stderr_is_terminal(&self) -> bool {
        self.stderr_terminal
    }

    fn progress(&mut self, p: &crate::driver::progress::Progress) {
        if let Some(f) = self.on_progress {
            f(p);
        }
    }

    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn write(&mut self, stream: Stream, text: &str, _color: Option<Color>) {
        match stream {
            Stream::Out => self.stdout.push_str(text),
            Stream::Err => self.stderr.push_str(text),
        }
    }

    fn cwd(&self) -> PathBuf {
        self.cwd.clone()
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        match self.nodes.get(&self.abs(path)) {
            Some(Node::File(s)) => Ok(s.clone()),
            Some(Node::Dir) => Err(is_a_directory(path)),
            None => Err(not_found(path)),
        }
    }

    fn write_file(&mut self, path: &Path, contents: &str) -> io::Result<()> {
        let p = self.abs(path);
        if let Some(Node::Dir) = self.nodes.get(&p) {
            return Err(is_a_directory(path));
        }
        if let Some(parent) = p.parent() {
            self.create_dir_all(parent)?;
        }
        self.nodes.insert(p, Node::File(contents.to_string()));
        Ok(())
    }

    fn is_file(&self, path: &Path) -> bool {
        matches!(self.nodes.get(&self.abs(path)), Some(Node::File(_)))
    }

    fn is_dir(&self, path: &Path) -> bool {
        matches!(self.nodes.get(&self.abs(path)), Some(Node::Dir))
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let d = self.abs(path);
        if !self.is_dir(&d) {
            return Err(not_found(path));
        }
        Ok(self.nodes.keys().filter(|p| p.parent() == Some(d.as_path())).cloned().collect())
    }

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        let p = self.abs(path);
        let mut cur = PathBuf::new();
        for c in p.components() {
            cur.push(c.as_os_str());
            if let Some(Node::File(_)) = self.nodes.get(&cur) {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("File exists: '{}'", cur.display())));
            }
            self.nodes.entry(cur.clone()).or_insert(Node::Dir);
        }
        Ok(())
    }

    fn remove_dir_all(&mut self, path: &Path) -> io::Result<()> {
        let p = self.abs(path);
        match self.nodes.get(&p) {
            Some(Node::Dir) => {
                self.nodes.retain(|k, _| !k.starts_with(&p));
                Ok(())
            }
            Some(Node::File(_)) => Err(not_a_directory(path)),
            None => Err(not_found(path)),
        }
    }

    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        let p = self.abs(path);
        match self.nodes.get(&p) {
            Some(Node::File(_)) => {
                self.nodes.remove(&p);
                Ok(())
            }
            Some(Node::Dir) => Err(is_a_directory(path)),
            None => Err(not_found(path)),
        }
    }

    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        let f = self.abs(from);
        let t = self.abs(to);
        if !self.nodes.contains_key(&f) {
            return Err(not_found(from));
        }
        let moved: Vec<(PathBuf, Node)> =
            self.nodes.iter().filter(|(k, _)| k.starts_with(&f)).map(|(k, v)| (k.clone(), v.clone())).collect();
        self.nodes.retain(|k, _| !k.starts_with(&f));
        for (k, v) in moved {
            let rel = k.strip_prefix(&f).unwrap().to_path_buf();
            let dest = if rel.as_os_str().is_empty() { t.clone() } else { t.join(rel) };
            self.nodes.insert(dest, v);
        }
        Ok(())
    }

    fn now(&self) -> f64 {
        (self.clock)()
    }

    fn max_memory_gb(&self) -> f64 {
        0.0
    }

    fn random_seed(&mut self) -> u32 {
        self.seed
    }

    fn unix_time(&self) -> f64 {
        0.0
    }

    fn threads(&self) -> Option<Threads> {
        self.threads
    }
}
