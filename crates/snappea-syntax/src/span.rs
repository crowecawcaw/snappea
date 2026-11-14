//! Source positions.

use std::fmt;

/// Identifies a source file registered in a [`SourceMap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

/// A byte range inside a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Span {
        Span { file, start, end }
    }

    /// Returns a span covering both spans (they must be in the same file).
    pub fn to(self, other: Span) -> Span {
        Span { file: self.file, start: self.start.min(other.start), end: self.end.max(other.end) }
    }

    /// A span that points nowhere; used for synthesized nodes.
    pub fn dummy() -> Span {
        Span { file: FileId(u32::MAX), start: 0, end: 0 }
    }

    pub fn is_dummy(&self) -> bool {
        self.file.0 == u32::MAX
    }

    /// Whether `inner` lies within this span, in the same file.
    pub fn contains(&self, inner: Span) -> bool {
        self.file == inner.file && self.start <= inner.start && inner.end <= self.end
    }
}

/// A resolved position: 1-based line and column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// A registered source file.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Display name for diagnostics (upstream uses the path relative to the working directory).
    pub name: String,
    /// Base name of the file (used in parse error messages).
    pub base_name: String,
    pub text: String,
    line_starts: Vec<u32>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> SourceFile {
        let name: String = name.into();
        let text: String = text.into();
        let base_name = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_string();
        let mut line_starts = vec![0u32];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        SourceFile { name, base_name, text, line_starts }
    }

    pub fn line_col(&self, offset: u32) -> LineCol {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = self.line_starts[line] as usize;
        // Upstream lexes a C# string, so ANTLR columns count UTF-16 code units.
        let col: usize =
            self.text[line_start..(offset as usize).min(self.text.len())].chars().map(char::len_utf16).sum();
        LineCol { line: line as u32 + 1, col: col as u32 + 1 }
    }

    /// Number of lines (a final line without a newline counts).
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// Text of 1-based `line`, without its line break. Empty past the end.
    pub fn line_text(&self, line: u32) -> &str {
        let Some(&start) = self.line_starts.get(line.wrapping_sub(1) as usize) else {
            return "";
        };
        let end = self.line_starts.get(line as usize).map_or(self.text.len(), |&e| e as usize);
        self.text[start as usize..end].trim_end_matches(['\n', '\r'])
    }

    pub fn slice(&self, span: Span) -> &str {
        &self.text[span.start as usize..span.end as usize]
    }
}

/// Collection of all source files of a compilation.
#[derive(Debug, Default, Clone)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> SourceMap {
        SourceMap::default()
    }

    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        self.files.push(SourceFile::new(name, text));
        FileId(self.files.len() as u32 - 1)
    }

    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn text(&self, span: Span) -> &str {
        self.file(span.file).slice(span)
    }

    /// Formats a span start as `name:line:col` (upstream's `SourceLocation.ToString`).
    pub fn location(&self, span: Span) -> Location {
        if span.is_dummy() {
            return Location { file: None, line: -1, col: -1 };
        }
        let f = self.file(span.file);
        let lc = f.line_col(span.start);
        Location { file: Some(f.name.clone()), line: lc.line as i64, col: lc.col as i64 }
    }
}

/// A human readable source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: Option<String>,
    pub line: i64,
    pub col: i64,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            None => write!(f, "<no source>:{}:{}", self.line, self.col),
            Some(name) => write!(f, "{}:{}:{}", name, self.line, self.col),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_text_strips_line_breaks() {
        let f = SourceFile::new("t.p", "a\r\nbc\nd");
        assert_eq!(f.line_count(), 3);
        assert_eq!(
            [f.line_text(1), f.line_text(2), f.line_text(3), f.line_text(4), f.line_text(0)],
            ["a", "bc", "d", "", ""]
        );
    }

    #[test]
    fn columns_count_utf16_code_units() {
        let f = SourceFile::new("t.p", "é😀x\ny");
        // `é` is one UTF-16 code unit and `😀` two, so `x` is at zero-based column 3.
        assert_eq!(f.line_col(6), LineCol { line: 1, col: 4 });
        assert_eq!(f.line_col(8), LineCol { line: 2, col: 1 });
    }
}
