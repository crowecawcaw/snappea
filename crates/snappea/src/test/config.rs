//! `[package.metadata.snappea]` in a crate's `Cargo.toml`. A small reader for that one table:
//! strings, booleans and arrays of strings, which are all the keys need.

use std::path::{Path, PathBuf};

use crate::checker::observe::UnknownEvents;

use crate::test::{ErrorKind, SetupError};

const TABLE: &str = "package.metadata.snappea";
const KEYS: &[&str] = &["spec_files", "specs", "exclude", "strict", "check_hot", "unknown_events", "coverage", "mode"];
/// Concepts of docs/language-support.md that are not Rust config keys: the check runs in
/// process, so there is no `timeout`; the artifacts folder is set only by `SNAPPEA_ARTIFACTS`.
#[cfg(test)]
const NOT_KEYS: &[&str] = &["timeout", "artifacts"];

/// L3 mode (`SNAPPEA_MODE`): `off` makes [`Model::check`](crate::test::Model::check) run the body
/// without checking. Rust has no global test hook, so `auto` and `marked` both mean
/// "check where the fixture is used".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Check where the fixture is used (the default).
    Auto,
    /// Same as `Auto` in Rust.
    Marked,
    /// Run test bodies without checking.
    Off,
}

impl Mode {
    pub(crate) fn parse(s: &str, origin: &str) -> Result<Mode, SetupError> {
        match s {
            "auto" => Ok(Mode::Auto),
            "marked" => Ok(Mode::Marked),
            "off" => Ok(Mode::Off),
            _ => Err(config(format!("{origin}: mode must be auto, marked or off, not '{s}'"))),
        }
    }
}

/// Settings from the manifest, paths resolved against its folder.
#[derive(Debug, Clone, Default)]
pub(crate) struct FileConfig {
    pub dir: PathBuf,
    pub spec_files: Option<Vec<String>>,
    pub specs: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub strict: Option<bool>,
    pub check_hot: Option<bool>,
    pub unknown_events: Option<UnknownEvents>,
    /// The suite coverage file; `off` and no setting are both `None`, as Rust prints no
    /// suite summary.
    pub coverage: Option<PathBuf>,
    pub mode: Option<Mode>,
}

#[derive(Debug, Clone, PartialEq)]
enum Val {
    Str(String),
    Bool(bool),
    List(Vec<String>),
}

pub(crate) fn config(message: impl Into<String>) -> SetupError {
    SetupError { kind: ErrorKind::Config, message: message.into(), artifacts: None }
}

pub(crate) fn read(manifest: &Path) -> Result<FileConfig, SetupError> {
    let text =
        std::fs::read_to_string(manifest).map_err(|e| config(format!("cannot read {}: {e}", manifest.display())))?;
    let at = |e: String| config(format!("{} [{TABLE}]: {e}", manifest.display()));
    let mut cfg = FileConfig { dir: manifest.parent().unwrap_or(Path::new(".")).to_path_buf(), ..Default::default() };
    for (key, value) in table(&text).map_err(at)? {
        let strings = |v: Val| match v {
            Val::List(l) => Ok(l),
            _ => Err(at(format!("{key} must be a list of strings"))),
        };
        let boolean = |v: Val| match v {
            Val::Bool(b) => Ok(b),
            _ => Err(at(format!("{key} must be true or false"))),
        };
        let string = |v: Val| match v {
            Val::Str(s) => Ok(s),
            _ => Err(at(format!("{key} must be a string"))),
        };
        match key.as_str() {
            "spec_files" => cfg.spec_files = Some(strings(value)?),
            "specs" => cfg.specs = Some(strings(value)?),
            "exclude" => cfg.exclude = Some(strings(value)?),
            "strict" => cfg.strict = Some(boolean(value)?),
            "check_hot" => cfg.check_hot = Some(boolean(value)?),
            "unknown_events" => cfg.unknown_events = Some(string(value)?.parse().map_err(at)?),
            "coverage" => cfg.coverage = Some(string(value)?).filter(|s| s != "off").map(|s| cfg.dir.join(s)),
            "mode" => cfg.mode = Some(Mode::parse(&string(value)?, &manifest.display().to_string())?),
            _ => return Err(at(format!("unknown key '{key}'; expected one of {}", KEYS.join(", ")))),
        }
    }
    Ok(cfg)
}

/// The key/value pairs of `[package.metadata.snappea]`, or none if the table is absent.
fn table(text: &str) -> Result<Vec<(String, Val)>, String> {
    let mut lines = text.lines();
    let header = |line: &str| {
        let line = strip_comment(line).trim();
        let inner = line.strip_prefix('[')?.strip_suffix(']')?;
        Some(inner.split('.').map(str::trim).collect::<Vec<_>>().join("."))
    };
    if !lines.by_ref().any(|l| header(l).as_deref() == Some(TABLE)) {
        return Ok(Vec::new());
    }
    // The table's body: up to the next header.
    let body: Vec<&str> = lines.take_while(|l| !l.trim_start().starts_with('[')).collect();
    let body = body.join("\n");
    let mut p = Reader { s: &body, i: 0 };
    let mut out: Vec<(String, Val)> = Vec::new();
    loop {
        p.skip(true);
        if p.i >= p.s.len() {
            return Ok(out);
        }
        let key = p.key()?;
        p.skip(false);
        if !p.eat('=') {
            return Err(format!("expected '=' after {key}"));
        }
        p.skip(false);
        let value = p.value()?;
        if out.iter().any(|(k, _)| *k == key) {
            return Err(format!("{key} is set twice"));
        }
        out.push((key, value));
        p.skip(false);
        if p.i < p.s.len() && !p.eat('\n') {
            return Err("expected a new line after a value".into());
        }
    }
}

fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '#') => return &line[..i],
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            _ => {}
        }
    }
    line
}

struct Reader<'a> {
    s: &'a str,
    i: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += c.len_utf8();
            true
        } else {
            false
        }
    }
    /// Skips spaces and comments, and new lines too when `newlines`.
    fn skip(&mut self, newlines: bool) {
        while let Some(c) = self.peek() {
            if c == '#' {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.i += self.peek().unwrap().len_utf8();
                }
            } else if c == ' ' || c == '\t' || c == '\r' || (newlines && c == '\n') {
                self.i += 1;
            } else {
                break;
            }
        }
    }
    fn key(&mut self) -> Result<String, String> {
        if matches!(self.peek(), Some('"' | '\'')) {
            return self.string();
        }
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            self.i += 1;
        }
        if start == self.i {
            return Err(format!("expected a key at '{}'", self.s[self.i..].lines().next().unwrap_or("")));
        }
        Ok(self.s[start..self.i].to_string())
    }
    fn string(&mut self) -> Result<String, String> {
        let quote = self.peek().unwrap();
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = self.peek().ok_or("unterminated string")?;
            self.i += c.len_utf8();
            match c {
                '\n' => return Err("unterminated string".into()),
                c if c == quote => return Ok(out),
                '\\' if quote == '"' => {
                    let e = self.peek().ok_or("unterminated string")?;
                    self.i += e.len_utf8();
                    out.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        '"' | '\\' => e,
                        _ => return Err(format!("unsupported escape \\{e}")),
                    });
                }
                c => out.push(c),
            }
        }
    }
    fn value(&mut self) -> Result<Val, String> {
        match self.peek() {
            Some('"' | '\'') => Ok(Val::Str(self.string()?)),
            Some('[') => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.skip(true);
                    if self.eat(']') {
                        return Ok(Val::List(items));
                    }
                    match self.peek() {
                        Some('"' | '\'') => items.push(self.string()?),
                        _ => return Err("lists may only hold strings".into()),
                    }
                    self.skip(true);
                    if !self.eat(',') && self.peek() != Some(']') {
                        return Err("expected ',' or ']' in a list".into());
                    }
                }
            }
            _ if self.s[self.i..].starts_with("true") => {
                self.i += 4;
                Ok(Val::Bool(true))
            }
            _ if self.s[self.i..].starts_with("false") => {
                self.i += 5;
                Ok(Val::Bool(false))
            }
            _ => Err("values must be strings, true/false, or lists of strings".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_the_snappea_table() {
        let text = r#"
[package]
name = "x" # [package.metadata.snappea] in a comment
[package.metadata.snappea]
spec_files = [
  "specs/a.p", # first
  'specs/*.p',
]
specs = ["A"]
strict = false
unknown_events = "warn" # comment
[dependencies]
specs = 3
"#;
        let t = table(text).unwrap();
        assert_eq!(t[0], ("spec_files".into(), Val::List(vec!["specs/a.p".into(), "specs/*.p".into()])));
        assert_eq!(t[1], ("specs".into(), Val::List(vec!["A".into()])));
        assert_eq!(t[2], ("strict".into(), Val::Bool(false)));
        assert_eq!(t[3], ("unknown_events".into(), Val::Str("warn".into())));
        assert_eq!(t.len(), 4);
        assert!(table("[package]\nname = 'x'\n").unwrap().is_empty());
        assert!(table("[package.metadata.snappea]\nspecs = [1]\n").is_err());
        assert!(table("[package.metadata.snappea]\nspecs = ['a'] strict = true\n").is_err());
    }

    #[test]
    fn keys_match_the_concepts_table() {
        let doc =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/language-support.md")).unwrap();
        let section = doc.split("### Concepts").nth(1).unwrap().split("\n#").next().unwrap();
        let rows: Vec<(&str, &str)> =
            section.lines().filter_map(|l| l.strip_prefix("| `")?.split_once("` |")).collect();
        let mut concepts: Vec<&str> = rows.iter().map(|(k, _)| *k).collect();
        let mut keys: Vec<&str> = KEYS.iter().chain(NOT_KEYS).copied().collect();
        concepts.sort();
        keys.sort();
        assert_eq!(keys, concepts);
        for name in NOT_KEYS {
            let (_, row) = rows.iter().find(|(k, _)| k == name).unwrap();
            assert!(row.contains("Rust"), "the {name} row must state that Rust has no such key");
        }
    }

    #[test]
    fn coverage_off_is_no_file() {
        let dir = std::env::temp_dir().join(format!("snappea-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join("Cargo.toml");
        std::fs::write(&manifest, "[package.metadata.snappea]\ncoverage = \"off\"\n").unwrap();
        assert_eq!(read(&manifest).unwrap().coverage, None);
        std::fs::write(&manifest, "[package.metadata.snappea]\ncoverage = \"cov.json\"\n").unwrap();
        assert_eq!(read(&manifest).unwrap().coverage, Some(dir.join("cov.json")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
