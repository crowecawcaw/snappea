//! Diagnostics. Messages reproduce `DefaultTranslationErrorHandler` upstream verbatim so
//! that tooling built around the original compiler keeps working.

use crate::syntax::{SourceMap, Span};

/// One compiler error. `message` does not include the location prefix; see [`Diagnostic::render`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Location shown in the `[file:line:col]` prefix. `None` for errors without a location
    /// (parse errors carry their own prefix in `message`).
    pub span: Option<Span>,
    pub message: String,
    /// A suggested fix. `snappea` prints it under the message; upstream-style output omits it.
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic { span: Some(span), message: message.into(), help: None }
    }

    pub fn bare(message: impl Into<String>) -> Diagnostic {
        Diagnostic { span: None, message: message.into(), help: None }
    }

    /// Adds a suggested fix.
    pub fn help(mut self, help: impl Into<String>) -> Diagnostic {
        self.help = Some(help.into());
        self
    }

    /// The message with its `[file:line:col]` prefix, as upstream prints it after `[Error:] `.
    pub fn render(&self, sm: &SourceMap) -> String {
        match self.span {
            Some(sp) => format!("[{}] {}", sm.location(sp), self.message),
            None => self.message.clone(),
        }
    }
}

/// Renders an error as `snappea` prints it: the message, its location, the source lines
/// around it with the span underlined, and the suggested fix.
///
/// ```text
/// error: got type: string, expected: int
///   --> a.p:6:11
///   |
/// 5 |     entry {
/// 6 |       x = "hello";
///   |           ^^^^^^^
/// 7 |     }
///   |
///   = help: give it a value of the expected type, or change the declared type
/// ```
pub fn render_snippet(sm: &SourceMap, span: Option<Span>, message: &str, help: Option<&str>) -> String {
    render_coded(sm, None, span, message, help)
}

/// [`render_snippet`] with a code in the heading: `error[code]: message`.
pub fn render_coded(
    sm: &SourceMap,
    code: Option<&str>,
    span: Option<Span>,
    message: &str,
    help: Option<&str>,
) -> String {
    let mut out = match code {
        Some(c) => format!("error[{}]: {}\n", c, message),
        None => format!("error: {}\n", message),
    };
    let span = span.filter(|s| !s.is_dummy());
    let Some(span) = span else {
        if let Some(h) = help {
            out.push_str(&format!("  = help: {}\n", h));
        }
        return out;
    };
    let f = sm.file(span.file);
    let line = f.line_col(span.start).line;
    let first = line.saturating_sub(2).max(1);
    let last = (line + 1).min(f.line_count());
    let width = last.to_string().len();
    let pad = " ".repeat(width);
    out.push_str(&format!("{} --> {}\n", pad, sm.location(span)));
    out.push_str(&format!("{} |\n", pad));
    for n in first..=last {
        let text = f.line_text(n);
        out.push_str(format!("{:>w$} | {}", n, text, w = width).trim_end());
        out.push('\n');
        if n == line {
            // the span's bytes within this line; a span past the line's end is cut there
            let from = (text_offset(f, span.start) as usize).min(text.len());
            let to = (from + span.end.saturating_sub(span.start) as usize).min(text.len());
            let indent: String = text[..from].chars().map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
            let marks = text.get(from..to).map_or(0, |s| s.chars().count()).max(1);
            out.push_str(&format!("{} | {}{}\n", pad, indent, "^".repeat(marks)));
        }
    }
    if let Some(h) = help {
        out.push_str(&format!("{} |\n{} = help: {}\n", pad, pad, h));
    }
    out
}

/// Bytes between the start of `offset`'s line and `offset`.
fn text_offset(f: &crate::syntax::SourceFile, offset: u32) -> u32 {
    let before = &f.text[..(offset as usize).min(f.text.len())];
    before.rfind('\n').map_or(before.len(), |i| before.len() - i - 1) as u32
}

/// Collects errors. In collecting mode (`continue_on_error`) reporting appends; in strict mode
/// (`--strict-errors`) the first error is returned as `Err` so analysis aborts.
#[derive(Debug, Default)]
pub struct Diagnostics {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<String>,
    pub continue_on_error: bool,
}

impl Diagnostics {
    pub fn new(continue_on_error: bool) -> Diagnostics {
        Diagnostics { errors: Vec::new(), warnings: Vec::new(), continue_on_error }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// `handler.Diagnostics.Report(...)`: append, or abort when strict.
    pub fn report(&mut self, d: Diagnostic) -> Result<(), Diagnostic> {
        if self.continue_on_error {
            self.errors.push(d);
            Ok(())
        } else {
            Err(d)
        }
    }

    /// Runs one tolerant step: an error thrown inside is recorded and analysis continues
    /// with the next item (`TolerantStep` upstream).
    pub fn tolerant<T>(&mut self, r: Result<T, Diagnostic>) -> Result<Option<T>, Diagnostic> {
        match r {
            Ok(v) => Ok(Some(v)),
            Err(d) if self.continue_on_error => {
                self.errors.push(d);
                Ok(None)
            }
            Err(d) => Err(d),
        }
    }

    /// An upstream warning, printed only by `snappea-compat`: give each a lint rule in
    /// `snappea-lint` too, so `snappea` fails on it unless it is suppressed.
    pub fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    /// Errors sorted the way upstream prints them: by file name, line and column, with
    /// errors lacking a location last; insertion order breaks ties.
    pub fn sorted_errors(&self, sm: &SourceMap) -> Vec<Diagnostic> {
        let mut v: Vec<(usize, Diagnostic)> = self.errors.iter().cloned().enumerate().collect();
        v.sort_by(|(ia, a), (ib, b)| {
            let ka = sort_key(a, sm);
            let kb = sort_key(b, sm);
            ka.cmp(&kb).then(ia.cmp(ib))
        });
        v.into_iter().map(|(_, d)| d).collect()
    }
}

fn sort_key(d: &Diagnostic, sm: &SourceMap) -> (String, i64, i64) {
    match d.span {
        Some(sp) if !sp.is_dummy() => {
            let l = sm.location(sp);
            (l.file.unwrap_or_default(), l.line, l.col)
        }
        _ => ("\u{ffff}".to_string(), 0, 0),
    }
}

/// Convenience type for "thrown" errors.
pub type Fatal = Diagnostic;
