//! Semantic analysis for P programs: scope building, type checking, machine and module
//! system validation. Produces a typed [`Program`] the checker interprets.
//!
//! The pass structure and every diagnostic message mirror the upstream P compiler
//! (`Plang.Compiler.TypeChecker`) so that the same models produce the same errors.
//! No I/O: callers load sources into a [`SourceMap`].

pub(crate) mod analyzer;
pub(crate) mod check;
pub(crate) mod collect;
pub(crate) mod csharp;
pub mod diag;
pub mod errors;
pub(crate) mod modules;
pub mod program;
pub(crate) mod scope;
pub mod types;
pub(crate) mod validate;

pub use diag::{Diagnostic, Diagnostics};
pub use program::*;
pub use types::{PType, PermissionOrigin, TypeKind};

use crate::syntax::ast;
use crate::syntax::{FileId, SourceMap};
use analyzer::Analyzer;

/// Analysis options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Collect every error (upstream default) instead of stopping at the first one
    /// (`--strict-errors`).
    pub continue_on_error: bool,
    /// Accept PVerifier-only syntax.
    pub verifier: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options { continue_on_error: true, verifier: false }
    }
}

/// The errors of a failed analysis, in the shape upstream prints them: an error that aborted
/// analysis (strict mode, or a non-tolerant pass) followed by the collected ones.
#[derive(Debug, Clone, Default)]
pub struct AnalysisError {
    pub thrown: Option<Diagnostic>,
    pub collected: Vec<Diagnostic>,
    pub warnings: Vec<String>,
}

impl AnalysisError {
    /// Messages in print order (sorted by location, thrown error first).
    pub fn messages(&self, sm: &SourceMap) -> Vec<String> {
        self.diagnostics(sm).iter().map(|e| e.render(sm)).collect()
    }

    /// The errors in print order (sorted by location, thrown error first).
    pub fn diagnostics(&self, sm: &SourceMap) -> Vec<Diagnostic> {
        let mut out: Vec<Diagnostic> = self.thrown.iter().cloned().collect();
        let d = Diagnostics { errors: self.collected.clone(), warnings: Vec::new(), continue_on_error: true };
        out.extend(d.sorted_errors(sm));
        out
    }
}

/// A successful analysis.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub program: Program,
    pub warnings: Vec<String>,
}

/// Front-end failure of [`compile`].
#[derive(Debug, Clone)]
pub enum CompileError {
    /// A lexer or parser error; `message` is `line L:C msg` as upstream formats it.
    /// `lexer` is true for token recognition errors, which upstream reports
    /// without first retrying the parse in LL mode.
    Parse {
        file: FileId,
        message: String,
        lexer: bool,
        /// Where the error is.
        span: crate::syntax::Span,
    },
    /// PVerifier-only syntax used with the checker backend (`NotSupportedException` upstream).
    NotSupported {
        file: FileId,
        line: usize,
        col: usize,
        keyword: String,
    },
    Analysis(AnalysisError),
}

/// Parses and analyzes every file of `sm`.
pub fn compile(sm: &SourceMap, opts: &Options) -> Result<Analysis, CompileError> {
    let mut programs = Vec::new();
    for (i, f) in sm.files().iter().enumerate() {
        let id = FileId(i as u32);
        let tokens = crate::syntax::lexer::lex(id, &f.text).map_err(|e| {
            let lc = f.line_col(e.span.start);
            CompileError::Parse {
                file: id,
                message: format!("line {}:{} {}", lc.line, lc.col - 1, e.message),
                lexer: true,
                span: e.span,
            }
        })?;
        if !opts.verifier {
            if let Some(t) = tokens.iter().find(|t| t.kind.is_verifier_only()) {
                let lc = f.line_col(t.span.start);
                return Err(CompileError::NotSupported {
                    file: id,
                    line: lc.line as usize,
                    col: (lc.col - 1) as usize,
                    keyword: sm.text(t.span).to_string(),
                });
            }
        }
        let p = crate::syntax::parse(id, &f.text).map_err(|e| {
            let lc = f.line_col(e.span.start);
            CompileError::Parse {
                file: id,
                message: format!("line {}:{} {}", lc.line, lc.col - 1, e.message),
                lexer: false,
                span: e.span,
            }
        })?;
        programs.push(p);
    }
    analyze(sm, &programs, opts).map_err(CompileError::Analysis)
}

/// Runs semantic analysis over parsed programs (`Analyzer.AnalyzeCompilationUnit`).
pub fn analyze(sm: &SourceMap, programs: &[ast::Program], opts: &Options) -> Result<Analysis, AnalysisError> {
    let mut a = Analyzer::new(sm, opts.continue_on_error);
    let r = run(&mut a, programs);
    let thrown = r.err();
    if thrown.is_some() || a.diags.has_errors() {
        return Err(AnalysisError { thrown, collected: a.diags.errors, warnings: a.diags.warnings });
    }
    for (i, n) in a.fun_next_slot.iter().enumerate() {
        a.prog.funs[i].frame_size = *n;
    }
    let names = csharp::names(&a.prog);
    for (m, name) in names.machines.into_iter().enumerate() {
        a.prog.machines[m].cs_name = name;
    }
    for (f, name) in names.funs.into_iter().enumerate() {
        a.prog.funs[f].cs_name = name;
    }
    // Sort contents, not paths: identical sources in different checkouts have the same identity.
    // Length delimiters prevent ambiguous concatenations. All sources participate conservatively.
    use sha2::{Digest, Sha256};
    let mut sources: Vec<_> = sm.files().iter().map(|f| f.text.as_bytes()).collect();
    sources.sort();
    let mut digest = Sha256::new();
    digest.update((sources.len() as u64).to_le_bytes());
    for source in sources {
        digest.update((source.len() as u64).to_le_bytes());
        digest.update(source);
    }
    let hex: String = digest.finalize().iter().map(|b| format!("{b:02x}")).collect();
    a.prog.source_fingerprint = Some(format!("sha256:{hex}"));
    a.prog.sources = std::sync::Arc::new(sm.clone());
    Ok(Analysis { program: a.prog, warnings: a.diags.warnings })
}

fn run<'a>(a: &mut Analyzer<'a>, programs: &'a [ast::Program]) -> Result<(), Diagnostic> {
    // Step 1: declaration stubs, then declarations with types.
    for p in programs {
        a.collect_stubs(p)?;
    }
    a.prog.sort_universal_events();
    let mut machine_iter = 0usize;
    let mut cursor = collect::FunCursor::default();
    for p in programs {
        a.collect_declarations(p, &mut machine_iter, &mut cursor)?;
    }

    // Step 2a: machine validation (tolerant, per machine).
    for m in 0..a.prog.machines.len() {
        let r = a.validate_machine(MachineId(m as u32));
        a.diags.tolerant(r)?;
    }

    // Step 3: function bodies (tolerant, per function), in `Scope.GetAllMethods` order:
    // global functions, then each machine's methods in source order (named functions and
    // anonymous handlers interleaved). Receive handlers created while checking a body are
    // populated in place and skipped here.
    let declared = a.prog.funs.len();
    let mut order: Vec<FunId> =
        (0..declared as u32).map(FunId).filter(|f| a.prog.funs[f.idx()].owner.is_none()).collect();
    for m in 0..a.prog.machines.len() {
        let mut methods: Vec<FunId> = (0..declared as u32)
            .map(FunId)
            .filter(|f| a.prog.funs[f.idx()].owner == Some(MachineId(m as u32)))
            .collect();
        methods.sort_by_key(|f| a.prog.funs[f.idx()].span.start);
        order.extend(methods);
    }
    for id in order {
        let r = a.populate_fun(id).and_then(|_| a.check_all_paths_return(id));
        a.diags.tolerant(r)?;
    }
    let mut f = declared;
    while f < a.prog.funs.len() {
        let id = FunId(f as u32);
        let r = a.populate_fun(id).and_then(|_| a.check_all_paths_return(id));
        a.diags.tolerant(r)?;
        f += 1;
    }

    // Step 2b: no static handlers (tolerant, per machine).
    for m in 0..a.prog.machines.len() {
        let r = a.validate_no_static_handlers(MachineId(m as u32));
        a.diags.tolerant(r)?;
    }

    if a.diags.has_errors() {
        return Ok(());
    }

    // Steps 4-6: purity propagation, capability restrictions, control flow.
    a.propagate_flags();
    a.check_capabilities()?;
    for f in 0..a.prog.funs.len() {
        a.check_control_flow(FunId(f as u32))?;
    }

    // Step 7: creates sets.
    for m in 0..a.prog.machines.len() {
        a.infer_creates(MachineId(m as u32));
    }

    if a.diags.has_errors() {
        return Ok(());
    }

    // Steps 8-10: module system.
    a.populate_module_system()?;
    Ok(())
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;

    fn fingerprint(files: &[(&str, &str)]) -> String {
        let mut sm = SourceMap::new();
        for (name, contents) in files {
            sm.add(*name, *contents);
        }
        compile(&sm, &Options::default()).unwrap().program.source_fingerprint.unwrap()
    }

    #[test]
    fn source_identity_ignores_paths_and_load_order_but_detects_changed_guards() {
        let first = "event E: int; spec S observes E { start state A { on E do (n: int) { assert n > 0; } } }";
        let second = first.replace("n > 0", "n > 1");
        let helper = "event Other;";
        assert_eq!(
            fingerprint(&[("/a/spec.p", first), ("/a/helper.p", helper)]),
            fingerprint(&[("/b/helper.p", helper), ("/b/spec.p", first)])
        );
        assert_ne!(fingerprint(&[("spec.p", first)]), fingerprint(&[("spec.p", &second)]));
    }
}
