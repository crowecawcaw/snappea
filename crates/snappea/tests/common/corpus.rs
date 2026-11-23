//! Runs the type checker over upstream's regression tests and compares each outcome with
//! the test's category (`Correct`/`DynamicError` must type-check, `StaticError` must not)
//! and, where present, with `ExpectedParametricTests.txt`. Shared by the `corpus` example
//! and the `regressions` test.

use std::path::{Path, PathBuf};

/// The result of type-checking one test directory.
pub(crate) struct Outcome {
    pub dir: PathBuf,
    pub category: String,
    /// Whether the outcome matches the category.
    pub ok: bool,
    /// Whether type checking failed.
    pub failed: bool,
    /// The errors, formatted as upstream prints them.
    pub messages: Vec<String>,
    /// The parametric test names upstream expects and those generated, both sorted; `None`
    /// when the test has no expectations or failed to type-check.
    pub params: Option<(Vec<String>, Vec<String>)>,
}

/// Type-checks every test directory under `root`, in sorted order.
pub(crate) fn run(root: &Path) -> Vec<Outcome> {
    let mut out = Vec::new();
    find_tests(root, &mut out);
    out
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|e| e.expect("directory entry").path())
        .collect();
    entries.sort();
    entries
}

fn p_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in sorted_entries(dir) {
        if e.is_dir() {
            p_files(&e, out);
        } else if e.extension().is_some_and(|x| x == "p") {
            out.push(e);
        }
    }
}

fn is_category(name: &str) -> bool {
    matches!(name, "Correct" | "DynamicError" | "StaticError")
}

fn find_tests(dir: &Path, out: &mut Vec<Outcome>) {
    // a test directory is a direct child of a category directory
    let category = dir.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).filter(|n| is_category(n));
    match category {
        Some(category) => {
            let mut files = Vec::new();
            p_files(dir, &mut files);
            if !files.is_empty() {
                out.push(check(dir, category, &files));
            }
        }
        None => {
            for e in sorted_entries(dir).into_iter().filter(|e| e.is_dir()) {
                find_tests(&e, out);
            }
        }
    }
}

fn check(dir: &Path, category: &str, files: &[PathBuf]) -> Outcome {
    let mut sm = snappea::syntax::SourceMap::new();
    for f in files {
        let bytes = std::fs::read(f).unwrap_or_else(|e| panic!("cannot read {}: {e}", f.display()));
        sm.add(f.display().to_string(), String::from_utf8_lossy(&bytes).into_owned());
    }
    let r = snappea::sema::compile(&sm, &snappea::sema::Options::default());
    let messages = match &r {
        Ok(_) => Vec::new(),
        Err(snappea::sema::CompileError::Parse { file, message, .. }) => {
            vec![format!("[Parser Error:] [{}] parse error: {}", sm.file(*file).base_name, message)]
        }
        Err(snappea::sema::CompileError::NotSupported { file, line, col, keyword }) => vec![format!(
            "[NotSupportedError:] {} (line {}:{}): \"{}\" keyword is only supported by PVerifier backend.",
            sm.file(*file).name,
            line,
            col,
            keyword
        )],
        Err(snappea::sema::CompileError::Analysis(a)) => {
            a.messages(&sm).iter().map(|m| format!("[Error:] {m}")).collect()
        }
    };
    let failed = r.is_err();
    let expected = dir.join("ExpectedParametricTests.txt");
    let params = match &r {
        Ok(a) if expected.exists() => {
            let mut want: Vec<String> = std::fs::read_to_string(&expected)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", expected.display()))
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            let mut got: Vec<String> = a.program.test_cases().iter().map(|t| t.name.clone()).collect();
            want.sort();
            got.sort();
            Some((want, got))
        }
        _ => None,
    };
    Outcome {
        dir: dir.to_path_buf(),
        category: category.to_string(),
        ok: failed == (category == "StaticError"),
        failed,
        messages,
        params,
    }
}
