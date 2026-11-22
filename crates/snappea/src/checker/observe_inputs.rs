//! Observation inputs: turning paths, directories and glob patterns into log files, reading
//! them into entries, and merging several live streams into one checked order.
//!
//! These are shared by `snappea observe` and by in-process test helpers. File access goes
//! through [`LogFiles`], so the same code runs on the real file system ([`StdFiles`]) and on
//! the command line's virtual hosts. Live inputs come from a [`LiveSource`] that the caller
//! supplies; [`observe_live`] feeds them to [`ObserveSession`]s.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde_json::Value as Json;

use crate::checker::observe::{ObserveSession, Parser};

/// The file system operations input expansion needs.
pub trait LogFiles {
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    /// Entries (full paths) of a directory, in any order.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
}

/// [`LogFiles`] on the real file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdFiles;

impl LogFiles for StdFiles {
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        std::fs::read_dir(path)?.map(|e| e.map(|e| e.path())).collect()
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}

/// Appends the log files `value` names to `files`, skipping files already listed.
///
/// `value` is a file, a directory (its files, not searched recursively) or a glob pattern.
/// Patterns support `*`, `?`, `[abc]`, `[a-z]`, `[!abc]` within a path component and `**`
/// for any number of directories; `*`, `?` and `**` do not match names starting with `.`.
/// Each match is then read as a file or directory, in sorted order. A value that exists as
/// written is never treated as a pattern, so arguments the shell already expanded are taken
/// as given. Relative paths are resolved against `cwd`.
///
/// Errors (a missing path, a pattern that matches nothing) are the messages `snappea observe`
/// prints.
pub fn add_log_paths(fs: &dyn LogFiles, cwd: &Path, value: &str, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let path = absolute(cwd, Path::new(value));
    let matches = if fs.is_file(&path) || fs.is_dir(&path) {
        vec![path]
    } else if is_pattern(value) {
        let found = glob(fs, &path);
        if found.is_empty() {
            return Err(format!("The specified log location({}) matches no files.", value));
        }
        found
    } else {
        return Err(format!("The specified log location({}) does not exist.", value));
    };
    for m in matches {
        let found = if fs.is_dir(&m) {
            let mut v: Vec<PathBuf> =
                fs.read_dir(&m).unwrap_or_default().into_iter().filter(|f| fs.is_file(f)).collect();
            v.sort();
            v
        } else {
            vec![m]
        };
        for f in found {
            if !files.contains(&f) {
                files.push(f);
            }
        }
    }
    Ok(())
}

/// [`add_log_paths`] for each value, in order: the log files to read.
pub fn log_paths(fs: &dyn LogFiles, cwd: &Path, values: &[impl AsRef<str>]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for v in values {
        add_log_paths(fs, cwd, v.as_ref(), &mut files)?;
    }
    Ok(files)
}

/// `cwd.join(p)` with `.` and `..` resolved lexically.
fn absolute(cwd: &Path, p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in cwd.join(p).components() {
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

fn is_pattern(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// Deep enough for any real log tree; stops `**` following a symbolic link cycle forever.
const MAX_GLOB_DEPTH: usize = 64;

/// The existing paths matching `pattern` (absolute), sorted.
fn glob(fs: &dyn LogFiles, pattern: &Path) -> Vec<PathBuf> {
    let mut base = PathBuf::new();
    let mut parts: Vec<String> = Vec::new();
    for c in pattern.components() {
        let s = c.as_os_str().to_string_lossy();
        if parts.is_empty() && !matches!(c, Component::Normal(_)) || parts.is_empty() && !is_pattern(&s) {
            base.push(c.as_os_str());
        } else {
            parts.push(s.into_owned());
        }
    }
    let mut out = Vec::new();
    walk(fs, &base, &parts, 0, &mut out);
    out.sort();
    out.dedup();
    out
}

fn walk(fs: &dyn LogFiles, dir: &Path, parts: &[String], depth: usize, out: &mut Vec<PathBuf>) {
    let Some((part, rest)) = parts.split_first() else {
        out.push(dir.to_path_buf());
        return;
    };
    if depth > MAX_GLOB_DEPTH || !fs.is_dir(dir) {
        return;
    }
    if part == "**" {
        walk(fs, dir, rest, depth + 1, out);
        for child in fs.read_dir(dir).unwrap_or_default() {
            if fs.is_dir(&child) && !hidden(&child) {
                walk(fs, &child, parts, depth + 1, out);
            }
        }
    } else if !is_pattern(part) {
        let child = dir.join(part);
        if fs.is_file(&child) || fs.is_dir(&child) {
            walk(fs, &child, rest, depth + 1, out);
        }
    } else {
        let pattern: Vec<char> = part.chars().collect();
        for child in fs.read_dir(dir).unwrap_or_default() {
            let Some(name) = child.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            if name.starts_with('.') && !part.starts_with('.') {
                continue;
            }
            if wildcard(&pattern, &name.chars().collect::<Vec<_>>()) {
                walk(fs, &child, rest, depth + 1, out);
            }
        }
    }
}

fn hidden(p: &Path) -> bool {
    p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

/// Matches one path component against a pattern of `*`, `?` and `[...]`.
fn wildcard(p: &[char], s: &[char]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some('*') => (0..=s.len()).any(|i| wildcard(&p[1..], &s[i..])),
        Some('?') => !s.is_empty() && wildcard(&p[1..], &s[1..]),
        Some('[') => match class(&p[1..]) {
            Some((matches, len)) => s.first().is_some_and(|&c| matches(c)) && wildcard(&p[1 + len..], &s[1..]),
            None => s.first() == Some(&'[') && wildcard(&p[1..], &s[1..]),
        },
        Some(&c) => s.first() == Some(&c) && wildcard(&p[1..], &s[1..]),
    }
}

/// A `[...]` class (after the `[`): its predicate and length including the `]`.
#[allow(clippy::type_complexity)]
fn class(p: &[char]) -> Option<(Box<dyn Fn(char) -> bool + '_>, usize)> {
    let negate = matches!(p.first(), Some('!') | Some('^'));
    let start = usize::from(negate);
    // a `]` right after the opening bracket is a member, not the end
    let end = start + 1 + p.get(start + 1..)?.iter().position(|&c| c == ']')?;
    let body = &p[start..end];
    let matches = move |c: char| {
        let mut i = 0;
        let mut hit = false;
        while i < body.len() {
            if i + 2 < body.len() && body[i + 1] == '-' {
                hit |= body[i] <= c && c <= body[i + 2];
                i += 3;
            } else {
                hit |= body[i] == c;
                i += 1;
            }
        }
        hit != negate
    };
    Some((Box::new(matches), end + 1))
}

// ---- live inputs ---------------------------------------------------------------------------

/// What a [`LiveSource`] delivers next.
#[derive(Debug)]
pub enum LiveRead {
    /// A complete line (its newline removed or not) from input `input`.
    Line { input: usize, line: String },
    /// Input `input` reached EOF.
    End { input: usize },
    /// Input `input` could not be opened or read; observation stops.
    Error { input: usize, error: String },
}

/// Several newline-delimited inputs read concurrently. `next` blocks until one of them has
/// a line or ends; every input reports [`LiveRead::End`] or [`LiveRead::Error`] once.
pub trait LiveSource {
    fn next(&mut self) -> LiveRead;
}

/// Orders lines from several inputs by a global sequence number (`--sequence FIELD`).
///
/// Every non-blank line is a JSON object carrying a whole number at `field` (a dotted path).
/// Numbers start at 1 and leave no gaps across all inputs, as a database outbox assigns
/// them. A line is released once every smaller number has been, whichever input it came
/// from, so an idle input never delays the others. Duplicates, a missing or invalid
/// number, more than [`SequenceMerger::limit`] lines waiting behind a gap, and a gap still
/// open when every input has ended are errors. Each line carries a `T` through the
/// merge, such as where it came from.
#[derive(Debug, Clone)]
pub struct SequenceMerger<T = ()> {
    field: String,
    next: u64,
    pending: BTreeMap<u64, (String, T)>,
    /// The most lines held back waiting for a missing number.
    pub limit: usize,
}

impl<T> SequenceMerger<T> {
    /// Lines held back by default before a missing number fails the run.
    pub const DEFAULT_LIMIT: usize = 10_000;

    pub fn new(field: &str) -> SequenceMerger<T> {
        SequenceMerger { field: field.to_string(), next: 1, pending: BTreeMap::new(), limit: Self::DEFAULT_LIMIT }
    }

    /// Adds a line, from where `name` says; returns the lines now ready, in sequence order.
    pub fn push(&mut self, name: &str, line: &str, tag: T) -> Result<Vec<(String, T)>, String> {
        let entry = line.trim();
        if entry.is_empty() {
            return Ok(Vec::new());
        }
        let seq = serde_json::from_str::<Json>(entry)
            .ok()
            .and_then(|j| self.field.split('.').try_fold(j, |cur, part| cur.get(part).cloned()))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| format!("{}: line has no whole-number sequence field '{}': {}", name, self.field, entry))?;
        if seq < self.next || self.pending.contains_key(&seq) {
            return Err(format!("{}: duplicate sequence number {}: {}", name, seq, entry));
        }
        self.pending.insert(seq, (entry.to_string(), tag));
        if self.pending.len() > self.limit {
            return Err(format!(
                "sequence number {} is missing: {} later lines are waiting for it",
                self.next,
                self.pending.len()
            ));
        }
        let mut ready = Vec::new();
        while let Some(line) = self.pending.remove(&self.next) {
            ready.push(line);
            self.next += 1;
        }
        Ok(ready)
    }

    /// Checks that nothing is left waiting once every input has ended.
    pub fn finish(&self) -> Result<(), String> {
        match self.pending.keys().next() {
            None => Ok(()),
            Some(first) => Err(format!(
                "sequence number {} is missing: every input ended with {} later lines waiting (next is {})",
                self.next,
                self.pending.len(),
                first
            )),
        }
    }
}

/// How [`observe_live`] ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveOutcome {
    /// Every input reached EOF and nothing was left unchecked; sessions can be finished
    /// with `finish(true, ..)`. Otherwise use `finish_after_failure`.
    pub complete: bool,
}

/// Feeds live inputs to `sessions` until every input ends or a session fails.
///
/// `names` names each input in error messages, as `name:line`; sessions locate entries by
/// input index and line ([`ObserveSession::push_at`]), so give them the same names in
/// [`ObserveOptions::input_names`](crate::checker::observe::ObserveOptions::input_names). Without
/// `sequence` lines are checked as they arrive (meant for a single input); with it, in
/// sequence order. Each checked line is passed to `record`. Read errors and sequence
/// errors are reported to every session with [`ObserveSession::input_error`].
pub fn observe_live(
    sessions: &mut [ObserveSession<'_>],
    parser: &dyn Parser,
    source: &mut dyn LiveSource,
    names: &[String],
    mut sequence: Option<&mut SequenceMerger<(usize, usize)>>,
    record: &mut dyn FnMut(&str),
    clock: &mut dyn FnMut() -> f64,
) -> LiveOutcome {
    let fail = |sessions: &mut [ObserveSession<'_>], message: String| {
        for s in sessions.iter_mut() {
            s.input_error(message.clone());
        }
        LiveOutcome { complete: false }
    };
    let mut open = names.len();
    let mut lines = vec![0usize; names.len()];
    while open > 0 {
        if sessions.iter().any(|s| s.failed()) {
            return LiveOutcome { complete: false };
        }
        let name = |i: usize| names.get(i).map_or("input", |s| s.as_str());
        let ready = match source.next() {
            LiveRead::Line { input, line } => {
                let n = lines.get(input).copied().unwrap_or(0);
                if let Some(count) = lines.get_mut(input) {
                    *count += 1;
                }
                match sequence.as_deref_mut() {
                    Some(merger) => match merger.push(&format!("{}:{}", name(input), n + 1), &line, (input, n)) {
                        Ok(ready) => ready,
                        Err(m) => return fail(sessions, m),
                    },
                    None => vec![(line, (input, n))],
                }
            }
            LiveRead::End { .. } => {
                open -= 1;
                continue;
            }
            LiveRead::Error { input, error } => return fail(sessions, format!("{}: {}", name(input), error)),
        };
        for (line, (input, n)) in ready {
            if sessions.iter().any(|s| s.failed()) {
                break;
            }
            record(line.trim_end_matches(['\n', '\r']));
            for s in sessions.iter_mut() {
                s.push_at(input, n, &line, parser, clock);
            }
        }
    }
    if sessions.iter().any(|s| s.failed()) {
        return LiveOutcome { complete: false };
    }
    if let Some(Err(m)) = sequence.map(|m| m.finish()) {
        return fail(sessions, m);
    }
    LiveOutcome { complete: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wc(p: &str, s: &str) -> bool {
        wildcard(&p.chars().collect::<Vec<_>>(), &s.chars().collect::<Vec<_>>())
    }

    #[test]
    fn wildcards() {
        assert!(wc("*.log", "a.log"));
        assert!(!wc("*.log", "a.log.1"));
        assert!(wc("a?c", "abc"));
        assert!(wc("[ab]x", "bx"));
        assert!(!wc("[!ab]x", "bx"));
        assert!(wc("[a-c][0-9]", "b7"));
        assert!(wc("[]]", "]"));
        assert!(wc("[x", "[x"));
    }

    #[test]
    fn sequences_release_in_order_and_reject_duplicates_and_gaps() {
        let mut m = SequenceMerger::new("seq");
        assert_eq!(m.push("b", r#"{"seq":2}"#, 'b').unwrap(), vec![]);
        let ready = m.push("a", r#"{"seq":1}"#, 'a').unwrap();
        assert_eq!(ready, vec![(r#"{"seq":1}"#.to_string(), 'a'), (r#"{"seq":2}"#.to_string(), 'b')]);
        assert!(m.push("a", "", 'a').unwrap().is_empty());
        assert!(m.push("a:3", r#"{"seq":2}"#, 'a').unwrap_err().contains("a:3: duplicate sequence number 2"));
        assert!(m.push("a", r#"{"x":2}"#, 'a').unwrap_err().contains("no whole-number sequence field 'seq'"));
        m.push("a", r#"{"seq":4}"#, 'a').unwrap();
        assert!(m.finish().unwrap_err().contains("sequence number 3 is missing"));
        let mut nested = SequenceMerger::new("meta.n");
        nested.limit = 1;
        assert_eq!(nested.push("a", r#"{"meta":{"n":1}}"#, ()).unwrap().len(), 1);
        nested.push("a", r#"{"meta":{"n":3}}"#, ()).unwrap();
        assert!(nested.push("a", r#"{"meta":{"n":4}}"#, ()).unwrap_err().contains("2 later lines"));
    }
}
