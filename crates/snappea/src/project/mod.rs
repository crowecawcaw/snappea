//! The P sources a command works on, and how type errors are reported.
//!
//! Upstream compiles a project to a .NET assembly with `p compile` and checks that. snappea
//! interprets P directly, so `p check` reads the sources itself (from a `.pproj` file or
//! `--pfiles`) and type-checks them in memory; there is no compile step or compiled file.

use std::path::{Path, PathBuf};

use crate::host::{full_path, is_hidden, relative_path, report_and_exit_hint, Color, Exit, Host, HostExt};

pub mod pproj;

/// Project name used when no `.pproj` or `--projname` gives one (upstream's default).
pub const DEFAULT_NAME: &str = "generatedOutput";

#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    /// Absolute path.
    pub path: String,
    /// Path relative to the current directory; upstream bakes this into assert messages
    /// and diagnostics.
    pub name: String,
    pub text: String,
}

/// A project's name and sources.
#[derive(Debug, Clone, PartialEq)]
pub struct Sources {
    pub project_name: String,
    pub files: Vec<SourceFile>,
}

impl Sources {
    /// Absolute path of the file registered as `id` in [`Sources::source_map`].
    pub fn full_path(&self, id: crate::syntax::FileId) -> &str {
        &self.files[id.0 as usize].path
    }

    pub fn source_map(&self) -> crate::syntax::SourceMap {
        let mut sm = crate::syntax::SourceMap::new();
        for s in &self.files {
            sm.add(s.name.clone(), s.text.clone());
        }
        sm
    }
}

/// Where the sources come from, as given on the command line.
#[derive(Debug, Clone, Default)]
pub struct Input {
    pub pproj: Option<String>,
    pub pfiles: Vec<String>,
    pub project_name: Option<String>,
    /// Where a project file's `<IncludeProject>` paths are resolved.
    pub includes: pproj::Includes,
}

/// Resolved input files.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub project_name: String,
    pub p_files: Vec<PathBuf>,
    /// Foreign (`.cs`/`.java`) files listed by the project; not executable by snappea.
    pub foreign_files: Vec<PathBuf>,
}

/// Resolves `input` to files: the `.pproj` file, `--pfiles`, or else the `.pproj` in the
/// current folder, or else the `*.p` files in it and its immediate subfolders.
/// Problems are reported through `host`.
pub fn resolve(host: &mut dyn Host, input: &Input) -> Result<Resolved, Exit> {
    let mut r = Resolved { project_name: DEFAULT_NAME.into(), p_files: Vec::new(), foreign_files: Vec::new() };
    let pproj = match (&input.pproj, input.pfiles.is_empty()) {
        (Some(p), _) => Some(p.clone()),
        (None, true) => pproj::find_local_pproj(host),
        (None, false) => None,
    };
    let add = |r: &mut Resolved, f: PathBuf| {
        let list =
            if f.to_string_lossy().to_lowercase().ends_with(".p") { &mut r.p_files } else { &mut r.foreign_files };
        if !list.contains(&f) {
            list.push(f);
        }
    };
    if let Some(pp) = pproj {
        let proj = pproj::parse(host, &pp, input.includes)?;
        r.project_name = proj.name;
        for f in proj.input_files {
            add(&mut r, f);
        }
    }
    for f in &input.pfiles {
        let Some(full) = pproj::legal_p_file(host, f) else {
            return Err(report_and_exit_hint(
                host,
                &format!("P file {} not found.", f),
                "check the path; with no files given, the command uses the .pproj or .p files in the current folder",
            ));
        };
        add(&mut r, full);
    }
    if r.p_files.is_empty() {
        host.out_line(".. Searching for P files locally in the current folder");
        let mut files = Vec::new();
        pproj::list_files(host, &host.cwd(), Some(1), &mut files);
        for f in files {
            if is_hidden(&f) || f.extension().map(|e| e != "p").unwrap_or(true) {
                continue;
            }
            host.out_line(&format!(".. Adding P file: {}", f.display()));
            r.p_files.push(f);
        }
    }
    if r.p_files.is_empty() {
        return Err(report_and_exit_hint(
            host,
            "Could not find any *.p file.",
            "run the command in the model's folder, or pass its files: `snappea check model.pproj` or `snappea check src/*.p`",
        ));
    }
    if let Some(n) = &input.project_name {
        r.project_name = n.clone();
    }
    if !pproj::is_legal_project_name(&r.project_name) {
        return Err(report_and_exit_hint(
            host,
            &format!("{} is not a legal project name", r.project_name),
            "use letters, digits and '_', starting with a letter or '_'",
        ));
    }
    Ok(r)
}

/// Reads `files`, naming each relative to `root`.
pub fn read(host: &dyn Host, project_name: &str, files: &[PathBuf], root: &Path) -> Result<Sources, String> {
    let mut out = Vec::new();
    for f in files {
        let full = full_path(&host.cwd(), f);
        let text = host.read_to_string(&full).map_err(|e| format!("{}: {}", full.display(), e))?;
        out.push(SourceFile { path: full.to_string_lossy().to_string(), name: relative_path(root, &full), text });
    }
    Ok(Sources { project_name: project_name.into(), files: out })
}

fn write_error(host: &mut dyn Host, msg: &str) {
    host.out_colored_line(msg, Color::Red);
}

/// Prints front-end diagnostics; returns true on success. With `upstream` they are printed
/// as upstream's `Compiler.Compile` prints them, else as [`report_native`] does.
pub fn report_result(
    host: &mut dyn Host,
    src: &Sources,
    sm: &crate::syntax::SourceMap,
    r: &Result<crate::sema::Analysis, crate::sema::CompileError>,
    upstream: bool,
) -> bool {
    use crate::sema::CompileError;
    if !upstream {
        return report_native(host, sm, r);
    }
    match r {
        Err(CompileError::Parse { file, message, lexer, .. }) => {
            if !*lexer && upstream {
                // Upstream first parses with SLL prediction and a bail-out strategy;
                // any syntax error makes it announce the LL(*) retry.
                host.out_colored_line("Reverting to LL(*) parsing strategy.", Color::Yellow);
            }
            write_error(host, &format!("[Parser Error:] [{}] parse error: {}", sm.file(*file).base_name, message));
            false
        }
        Err(CompileError::NotSupported { file, line, col, keyword }) => {
            write_error(
                host,
                &format!(
                    "[NotSupportedError:] {} (line {}:{}): \"{}\" keyword is only supported by PVerifier backend.",
                    src.full_path(*file),
                    line,
                    col,
                    keyword
                ),
            );
            false
        }
        Err(CompileError::Analysis(a)) => {
            for w in &a.warnings {
                host.out_colored_line(w, Color::Yellow);
            }
            let msgs = a.messages(sm);
            let mut rest = msgs.as_slice();
            if a.thrown.is_some() {
                if let Some((first, others)) = rest.split_first() {
                    write_error(host, &format!("[Error:] {}", first));
                    rest = others;
                }
            }
            for (i, m) in rest.iter().enumerate() {
                if i > 0 {
                    write_error(host, "");
                }
                write_error(host, &format!("[Error:] {}", m));
            }
            false
        }
        Ok(a) => {
            for w in &a.warnings {
                host.out_colored_line(w, Color::Yellow);
            }
            true
        }
    }
}

/// Prints front-end diagnostics as `snappea` does: each error with its location, the source
/// around it and a suggested fix, then a count. Returns true on success.
fn report_native(
    host: &mut dyn Host,
    sm: &crate::syntax::SourceMap,
    r: &Result<crate::sema::Analysis, crate::sema::CompileError>,
) -> bool {
    use crate::sema::diag::render_snippet;
    use crate::sema::CompileError;
    let errors: Vec<String> = match r {
        Ok(a) => {
            for w in &a.warnings {
                host.out_colored_line(w, Color::Yellow);
            }
            return true;
        }
        Err(CompileError::Parse { message, span, .. }) => {
            // `message` is `line L:C detail`; the location is shown separately
            let detail =
                message.split_once(' ').and_then(|(_, m)| m.split_once(' ')).map_or(message.as_str(), |(_, m)| m);
            vec![render_snippet(sm, Some(*span), &format!("syntax error: {}", detail), Some(&parse_help(detail)))]
        }
        Err(CompileError::NotSupported { file, line, col, keyword }) => {
            let help = format!(
                "'{}' is PVerifier syntax, which snappea does not support yet; remove it to check the model",
                keyword
            );
            let at = format!("{}:{}:{}", sm.file(*file).name, line, col + 1);
            vec![render_snippet(sm, None, &format!("'{}' is not supported, at {}", keyword, at), Some(&help))]
        }
        Err(CompileError::Analysis(a)) => {
            for w in &a.warnings {
                host.out_colored_line(w, Color::Yellow);
            }
            a.diagnostics(sm).iter().map(|d| render_snippet(sm, d.span, &d.message, d.help.as_deref())).collect()
        }
    };
    for (i, e) in errors.iter().enumerate() {
        if i > 0 {
            write_error(host, "");
        }
        write_error(host, e.trim_end());
    }
    let n = errors.len();
    write_error(host, "");
    write_error(
        host,
        &format!(
            "The model has {} error{}; fix {} and run the command again.",
            n,
            if n == 1 { "" } else { "s" },
            if n == 1 { "it" } else { "them" }
        ),
    );
    false
}

/// A suggested fix for a syntax error, from its message.
pub(crate) fn parse_help(detail: &str) -> String {
    if let Some(rest) = detail.strip_prefix("missing ") {
        let token = rest.split(" at ").next().unwrap_or(rest);
        if token == "';'" {
            return "add ';' at the end of the statement before this point".into();
        }
        return format!("insert {} before this point", token);
    }
    if let Some(text) = detail.strip_prefix("token recognition error at: '") {
        return if text.starts_with('"') {
            "close the string with a matching '\"' on the same line".into()
        } else if text.starts_with("/*") {
            "close the comment with '*/'".into()
        } else {
            "remove this character; it is not part of P syntax".into()
        };
    }
    if detail.starts_with("nesting is too deep") {
        return "split the expression or block into smaller functions or local variables".into();
    }
    if let Some(input) = detail.strip_prefix("no viable alternative at input '") {
        // `(items = items)` parses as a parenthesized assignment; a one-field named
        // tuple needs a trailing comma.
        let input = input.trim_end_matches('\'');
        if input.ends_with(')') && input.contains('=') && !input.contains("==") && !input.contains(',') {
            return "a named tuple with one field needs a trailing comma: `(name = value,)`".into();
        }
    }
    if detail.starts_with("extraneous input") {
        return "remove this token, or look for an extra or missing symbol just before it".into();
    }
    "look for a missing ';', ')' or '}', or a misspelled keyword, here or on the line before".into()
}

#[cfg(test)]
mod parse_help_tests {
    use super::parse_help;

    #[test]
    fn one_field_named_tuple_suggests_trailing_comma() {
        let help = parse_help("no viable alternative at input 'items=items)'");
        assert!(help.contains("trailing comma"), "{help}");
        let other = parse_help("no viable alternative at input 'x==y)'");
        assert!(!other.contains("trailing comma"), "{other}");
    }
}
