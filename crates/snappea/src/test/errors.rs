//! The errors a check returns, and their failure output.

use std::fmt;
use std::path::PathBuf;

use crate::emit::{Event, Value};

/// Events shown in failure output, counting back from the one that broke the spec.
pub(crate) const REPORT_EVENTS: usize = 20;

/// A P spec was broken by the recorded events.
#[derive(Debug, Clone)]
pub struct SpecViolation {
    /// The broken spec.
    pub spec: String,
    /// The key (stream) it broke on.
    pub key: String,
    /// The spec's assert message, or for liveness the hot state it ended in.
    pub message: String,
    /// `file:line` of the failed P assert.
    pub location: Option<String>,
    /// The key's last events (up to 20), the violating one last.
    pub events: Vec<Event>,
    /// The spec ended in a hot state (with `check_hot`).
    pub liveness: bool,
    /// Further violations from the same check.
    pub others: Vec<SpecViolation>,
    /// Folder holding the recording's events, kept because the check failed.
    pub artifacts: Option<PathBuf>,
    pub(crate) earlier: usize,
}

/// Anything that stops a check other than a violation.
#[derive(Debug, Clone)]
pub struct SetupError {
    /// The category; match on it to handle errors.
    pub kind: ErrorKind,
    /// What went wrong, for people.
    pub message: String,
    /// Folder holding the recording's events, kept because the check failed.
    pub artifacts: Option<PathBuf>,
}

/// What went wrong for a [`SetupError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Bad options or config, missing spec files, P compile errors, unknown spec name, or
    /// a global emit on an unbound thread while several recordings are open.
    Config,
    /// An event line or payload the core cannot read (including truncated lines).
    Parser,
    /// `strict` found a selected spec with no relevant event.
    Validation,
    /// An event name no loaded P file declares, under `unknown_events = error`.
    UnknownEvent,
    /// Events of a key out of time order.
    OutOfOrder,
}

impl ErrorKind {
    /// The uniform name: `config`, `parser`, `validation`, `unknown_event`, `out_of_order`.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Config => "config",
            ErrorKind::Parser => "parser",
            ErrorKind::Validation => "validation",
            ErrorKind::UnknownEvent => "unknown_event",
            ErrorKind::OutOfOrder => "out_of_order",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A failed check: a violation, or a setup error.
#[derive(Debug, Clone)]
pub enum CheckError {
    /// The events broke a spec: the application, or the spec, has a bug.
    Violation(Box<SpecViolation>),
    /// The check could not run: fix the configuration, specs or event output.
    Setup(SetupError),
}

impl CheckError {
    /// The violation, if this is one.
    pub fn violation(&self) -> Option<&SpecViolation> {
        match self {
            CheckError::Violation(v) => Some(v),
            CheckError::Setup(_) => None,
        }
    }
    /// The setup error, if this is one.
    pub fn setup(&self) -> Option<&SetupError> {
        match self {
            CheckError::Setup(e) => Some(e),
            CheckError::Violation(_) => None,
        }
    }
    pub(crate) fn set_artifacts(&mut self, path: PathBuf) {
        match self {
            CheckError::Violation(v) => v.artifacts = Some(path),
            CheckError::Setup(e) => e.artifacts = Some(path),
        }
    }
}

impl From<SetupError> for CheckError {
    fn from(e: SetupError) -> Self {
        CheckError::Setup(e)
    }
}
impl From<SpecViolation> for CheckError {
    fn from(v: SpecViolation) -> Self {
        CheckError::Violation(Box::new(v))
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckError::Violation(v) => v.fmt(f),
            CheckError::Setup(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for CheckError {}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        if let Some(path) = &self.artifacts {
            write!(f, "\n\nArtifacts: {}", path.display())?;
        }
        Ok(())
    }
}
impl std::error::Error for SetupError {}

impl fmt::Display for SpecViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.liveness {
            writeln!(f, "P spec {} was still waiting when the events ended: {}", self.spec, self.message)?;
        } else {
            writeln!(f, "P spec {} was broken: {}", self.spec, self.message)?;
        }
        writeln!(f)?;
        if self.key.is_empty() {
            writeln!(f, "  Events:")?;
        } else {
            writeln!(f, "  Events (key \"{}\"):", self.key)?;
        }
        if self.earlier > 0 {
            writeln!(f, "     ... {} earlier event(s)", self.earlier)?;
        }
        let rows: Vec<(String, &Event)> = self.events.iter().map(|e| (payload_text(&e.payload), e)).collect();
        let w_event = rows.iter().map(|(_, e)| e.event.len()).max().unwrap_or(0);
        let w_payload = rows.iter().map(|(p, _)| p.chars().count()).max().unwrap_or(0).min(48);
        for (i, (payload, e)) in rows.iter().enumerate() {
            let mut row = format!(
                "    {:>3}  {:<w_event$}  {:<w_payload$}  {}",
                self.earlier + i + 1,
                e.event,
                payload,
                e.src.as_deref().unwrap_or("")
            );
            if e.at {
                row.push_str("  (time set with emit_at)");
            }
            if i + 1 == rows.len() {
                row.push_str(if self.liveness { "  <- events ended here" } else { "  <- broke the spec here" });
            }
            writeln!(f, "{}", row.trim_end())?;
        }
        if rows.is_empty() {
            writeln!(f, "    (none)")?;
        }
        if self.location.is_some() || self.artifacts.is_some() {
            writeln!(f)?;
        }
        if let Some(location) = &self.location {
            writeln!(f, "  Spec: {location}")?;
        }
        if let Some(path) = &self.artifacts {
            writeln!(f, "  Artifacts: {}", path.display())?;
        }
        if !self.others.is_empty() {
            writeln!(f, "\n  {} more violation(s):", self.others.len())?;
            for o in &self.others {
                let key = if o.key.is_empty() { String::new() } else { format!(" (key \"{}\")", o.key) };
                writeln!(f, "    {}{key}: {}", o.spec, o.message)?;
            }
        }
        Ok(())
    }
}
impl std::error::Error for SpecViolation {}

fn payload_text(p: &Value) -> String {
    match p {
        Value::Null => String::new(),
        Value::Object(o) => o.iter().map(|(k, v)| format!("{k}={}", short(v))).collect::<Vec<_>>().join(" "),
        v => short(v),
    }
}

fn short(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    };
    if s.chars().count() <= 40 {
        s
    } else {
        s.chars().take(37).collect::<String>() + "..."
    }
}
