use crate::{visit, Context, Finding, Rule};
use snappea_syntax::{ast::*, lexer, FileId, Span};

/// Mutation kinds a `mutation:<kind>` suppression may name (see `snappea check`).
pub const MUTATIONS: &[&str] = &[
    "remove-statement",
    "condition-true",
    "condition-false",
    "swap-operator",
    "remove-not",
    "flip-boolean",
    "empty-body",
    "drop-conjunct",
    "drop-disjunct",
];

/// Whether `id` names mutation testing: `mutation`, or `mutation:<kind>`.
fn is_program_rule(id: &str) -> bool {
    crate::PROGRAM_RULES.iter().any(|(r, _)| *r == id)
}

/// The scopes `// snappea-lint: allow` comments give rules of [`crate::PROGRAM_RULES`],
/// with the rule each allows.
pub fn program_suppressions(ctx: &Context<'_>) -> Vec<(Span, String)> {
    allowed_ids(directives(ctx, &|_| true).0).filter(|(_, id)| is_program_rule(id)).collect()
}

fn is_mutation(id: &str) -> bool {
    id == "mutation"
        || id == "mutation-nested"
        || id
            .strip_prefix("mutation:")
            .or_else(|| id.strip_prefix("mutation-nested:"))
            .is_some_and(|k| MUTATIONS.contains(&k))
}

pub fn apply(ctx: &Context<'_>, rules: &[Box<dyn Rule>], findings: &mut Vec<Finding>) {
    let (allowed, invalid) = directives(ctx, &|id| {
        rules.iter().any(|r| r.id() == id) || id == "coverage" || is_mutation(id) || is_program_rule(id)
    });
    findings.retain(|f| {
        !allowed
            .iter()
            .any(|(s, id, _)| id == f.rule && s.file == f.span.file && s.start <= f.span.start && s.end >= f.span.end)
    });
    findings.extend(invalid);
    findings.extend(snappea_directives(ctx).1);
}

/// The machines marked `// snappea: driver`: test code that drives the model, as the
/// spans of their declarations. Malformed directives are reported by linting, not here.
pub fn driver_machines(ctx: &Context<'_>) -> Vec<Span> {
    snappea_directives(ctx).0
}

/// Every `// snappea: driver` directive: the machine declaration each marks, and the
/// findings for malformed ones (another directive, or not before a machine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestIntent {
    pub span: Span,
    pub expected_bug: Option<String>,
    pub boundary: Option<String>,
}

pub fn test_intents(ctx: &Context<'_>) -> Vec<TestIntent> {
    snappea_directives(ctx).2
}

fn snappea_directives(ctx: &Context<'_>) -> (Vec<Span>, Vec<Finding>, Vec<TestIntent>) {
    let machines: Vec<Span> = ctx.machines().filter(|m| m.kind == MachineKind::Impl).map(|m| m.span).collect();
    let tests: Vec<Span> = ctx
        .declarations()
        .filter_map(|d| match d {
            TopDecl::SafetyTest(t) => Some(t.span),
            _ => None,
        })
        .collect();
    let mut intents: Vec<TestIntent> = Vec::new();
    let mut marked = Vec::new();
    let mut invalid = Vec::new();
    each_comment(ctx, &mut |id, text, start, comment, next| {
        let Some(directive) = comment.trim().strip_prefix("snappea:") else {
            return;
        };
        let span = Span::new(id, start as u32, (start + 2 + comment.len()) as u32);
        let bad = |message: &str| Finding::new("invalid-directive", span, message);
        let directive = directive.trim();
        let word = directive.split_whitespace().next().unwrap_or_default();
        if matches!(word, "expect-bug" | "boundary") {
            let line_start = text[..start].rfind('\n').map_or(0, |n| n + 1);
            let target = tests.iter().find(|t| t.file == id && Some(t.start) == next);
            let Some(target) = target.filter(|_| text[line_start..start].trim().is_empty()) else {
                invalid.push(bad("put the test intent directive on its own line directly before a test declaration"));
                return;
            };
            let value = if word == "expect-bug" {
                serde_json::from_str::<String>(directive[word.len()..].trim()).ok().filter(|s| !s.trim().is_empty())
            } else {
                directive[word.len()..]
                    .trim()
                    .strip_prefix("--")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            };
            let Some(value) = value else {
                invalid.push(bad("use '// snappea: expect-bug \"failure text\"' or '// snappea: boundary -- reason', with nonempty text"));
                return;
            };
            let i = intents.iter().position(|t| t.span == *target).unwrap_or_else(|| {
                intents.push(TestIntent { span: *target, expected_bug: None, boundary: None });
                intents.len() - 1
            });
            let slot = if word == "expect-bug" { &mut intents[i].expected_bug } else { &mut intents[i].boundary };
            if slot.is_some() {
                invalid.push(bad("duplicate test intent directive; declare each intent once"));
            } else {
                *slot = Some(value);
            }
            return;
        }
        let directive = directive.split_once("--").map_or(directive, |(d, _)| d).trim();
        if directive != "driver" {
            invalid.push(bad(
                "unknown snappea directive; use driver before a machine, or expect-bug/boundary before a test",
            ));
            return;
        }
        let line_start = text[..start].rfind('\n').map_or(0, |n| n + 1);
        let target = machines.iter().find(|m| m.file == id && Some(m.start) == next);
        match target {
            Some(m) if text[line_start..start].trim().is_empty() => marked.push(*m),
            _ => invalid.push(bad("put '// snappea: driver' on its own line directly before a machine declaration")),
        }
    });
    (marked, invalid, intents)
}

/// Calls `f(file, text, start, comment, next)` for every comment of the sources, with
/// `next` the start of the token after it. Only lexer gaps are inspected, so strings
/// cannot forge comments.
/// Called with a comment's file, the file's text, the comment's start, its text after
/// `//`, and the start of the next token.
type OnComment<'f> = dyn FnMut(FileId, &str, usize, &str, Option<u32>) + 'f;

fn each_comment(ctx: &Context<'_>, f: &mut OnComment<'_>) {
    for (i, file) in ctx.sources.files().iter().enumerate() {
        let id = FileId(i as u32);
        let Ok(tokens) = lexer::lex(id, &file.text) else {
            continue;
        };
        let mut end = 0;
        for token in &tokens {
            if token.span.start > end {
                comments(&file.text, end as usize, token.span.start as usize, &mut |start, text| {
                    f(id, &file.text, start, text, Some(token.span.start))
                });
            }
            end = token.span.end;
        }
        if (end as usize) < file.text.len() {
            comments(&file.text, end as usize, file.text.len(), &mut |start, text| {
                f(id, &file.text, start, text, None)
            });
        }
    }
}

/// The mutation suppressions of the sources: the code each covers, and `mutation` or
/// `mutation:<kind>`. Malformed directives are reported by linting, not here.
pub fn mutation_suppressions(ctx: &Context<'_>) -> Vec<(Span, String)> {
    allowed_ids(directives(ctx, &|_| true).0).filter(|(_, id)| is_mutation(id)).collect()
}

/// A mutation suppression and the nested code it excludes unless explicitly opted in.
#[derive(Debug, Clone)]
pub struct MutationSuppression {
    pub span: Span,
    pub kind: String,
    pub reason: String,
    pub nested: bool,
    children: Vec<Span>,
}

impl MutationSuppression {
    pub fn matches(&self, span: Span, kind: &str) -> bool {
        let inside =
            |outer: Span, inner: Span| outer.file == inner.file && outer.start <= inner.start && inner.end <= outer.end;
        (self.kind == "mutation" || self.kind.strip_prefix("mutation:") == Some(kind))
            && inside(self.span, span)
            && (self.nested || !self.children.iter().any(|child| inside(*child, span)))
    }
}

pub fn mutation_suppression_details(ctx: &Context<'_>) -> Vec<MutationSuppression> {
    let scopes = scopes(ctx);
    directives(ctx, &|_| true)
        .0
        .into_iter()
        .filter(|(_, id, _)| is_mutation(id))
        .map(|(span, id, reason)| {
            let nested = id == "mutation-nested" || id.starts_with("mutation-nested:");
            let kind = if nested { id.replacen("mutation-nested", "mutation", 1) } else { id };
            let children = scopes
                .iter()
                .copied()
                .filter(|s| s.file == span.file && s.start >= span.start && s.end <= span.end && *s != span)
                .collect();
            MutationSuppression { span, kind, reason, nested, children }
        })
        .collect()
}

/// The source scopes intentionally omitted from `snappea check` coverage requirements,
/// with the reason each suppression gives after `--`.
pub fn coverage_suppressions(ctx: &Context<'_>, rules: &[Box<dyn Rule>]) -> Vec<(Span, String)> {
    let (allowed, _) = directives(ctx, &|id| {
        id == "coverage" || rules.iter().any(|rule| rule.id() == id) || is_mutation(id) || is_program_rule(id)
    });
    allowed.into_iter().filter(|(_, id, _)| id == "coverage").map(|(span, _, reason)| (span, reason)).collect()
}

/// A suppression: the scope, the ID it allows, and its reason (empty without `--`).
type Allowed = (Span, String, String);

fn allowed_ids(allowed: Vec<Allowed>) -> impl Iterator<Item = (Span, String)> {
    allowed.into_iter().map(|(span, id, _)| (span, id))
}

/// Every `snappea-lint: allow` directive: the scope, ID and reason it allows, and the
/// findings for malformed ones (with an ID `known` rejects, or not before a declaration
/// or statement).
fn scopes(ctx: &Context<'_>) -> Vec<Span> {
    let mut scopes = Vec::new();
    for d in ctx.declarations() {
        match d {
            TopDecl::SafetyTest(test) => scopes.push(test.span),
            TopDecl::Enum { span, .. } => scopes.push(*span),
            TopDecl::RefinementTest { span, .. } | TopDecl::Implementation { span, .. } => scopes.push(*span),
            TopDecl::Machine(m) => {
                scopes.push(m.span);
                scopes.extend(m.vars.iter().map(|v| v.span));
                for s in &m.states {
                    scopes.push(s.span);
                    for i in &s.items {
                        scopes.push(i.span);
                    }
                }
                for b in visit::bodies(m) {
                    visit::stmts(&b.stmts, &mut |s| scopes.push(s.span), &mut |_| {});
                }
                for f in &m.funs {
                    scopes.push(f.span);
                }
            }
            TopDecl::Fun(f) => {
                scopes.push(f.span);
                if let Some(b) = &f.body {
                    visit::stmts(&b.stmts, &mut |s| scopes.push(s.span), &mut |_| {});
                }
            }
            _ => {}
        }
    }
    scopes
}

fn directives(ctx: &Context<'_>, known: &dyn Fn(&str) -> bool) -> (Vec<Allowed>, Vec<Finding>) {
    let scopes = scopes(ctx);
    let mut allowed = Vec::new();
    let mut invalid = Vec::new();
    for (i, file) in ctx.sources.files().iter().enumerate() {
        let id = FileId(i as u32);
        let Ok(tokens) = lexer::lex(id, &file.text) else {
            continue;
        };
        // Inspect only lexer gaps: strings and other tokens cannot forge directives.
        let mut end = 0;
        for token in &tokens {
            if token.span.start > end {
                comments(&file.text, end as usize, token.span.start as usize, &mut |start, text| {
                    let Some(text) = text.trim().strip_prefix("snappea-lint:") else {
                        return;
                    };
                    let span = Span::new(id, start as u32, (start + text.len()) as u32);
                    let bad = |message: &str| Finding::new("invalid-suppression", span, message);
                    let Some(ids) = text.trim().strip_prefix("allow ") else {
                        invalid.push(bad("expected '// snappea-lint: allow rule-id[, rule-id] -- reason'"));
                        return;
                    };
                    let (ids, reason) =
                        ids.split_once("--").map_or((ids, None), |(ids, reason)| (ids, Some(reason.trim())));
                    let ids = expand_mutation_kinds(ids.trim().split(',').map(str::trim));
                    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
                    if ids.iter().any(|id| !known(id)) {
                        invalid.push(bad("unknown or empty lint rule ID in suppression"));
                        return;
                    }
                    if ids.contains(&"coverage") && !reason.is_some_and(|reason| !reason.is_empty()) {
                        invalid.push(bad("coverage suppression needs a reason after '--'"));
                        return;
                    }
                    // Suppression must be a standalone comment before the node.
                    let line_start = file.text[..start].rfind('\n').map_or(0, |n| n + 1);
                    if !file.text[line_start..start].trim().is_empty() {
                        invalid
                            .push(bad("put the lint suppression on its own line before the declaration or statement"));
                        return;
                    }
                    let target =
                        scopes.iter().filter(|s| s.file == id && s.start == token.span.start).max_by_key(|s| s.end);
                    if let Some(target) = target {
                        for rule in ids {
                            allowed.push((*target, rule.to_string(), reason.unwrap_or_default().to_string()));
                        }
                    } else {
                        invalid.push(bad(
                            "lint suppression must precede a machine, test, implementation, enum, state, handler, field, function, or statement",
                        ));
                    }
                });
            }
            end = token.span.end;
        }
    }
    (allowed, invalid)
}

/// The IDs of one suppression, with a bare mutation kind after `mutation:<kind>` read as
/// another kind of it: `mutation:remove-statement,condition-false` allows both.
fn expand_mutation_kinds<'a>(ids: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let prefix = out
            .last()
            .and_then(|p| p.split_once(':').map(|(prefix, _)| prefix.to_string()))
            .filter(|p| p == "mutation" || p == "mutation-nested");
        if let Some(prefix) = prefix.filter(|_| MUTATIONS.contains(&id)) {
            out.push(format!("{prefix}:{id}"));
        } else {
            out.push(id.to_string());
        }
    }
    out
}

fn comments(text: &str, start: usize, end: usize, f: &mut impl FnMut(usize, &str)) {
    let bytes = text.as_bytes();
    let mut i = start;
    while i + 1 < end {
        if &bytes[i..i + 2] == b"//" {
            let a = i;
            i += 2;
            let content = i;
            while i < end && !matches!(bytes[i], b'\n' | b'\r') {
                i += 1;
            }
            f(a, &text[content..i]);
        } else if &bytes[i..i + 2] == b"/*" {
            i += 2;
            while i + 1 < end && &bytes[i..i + 2] != b"*/" {
                i += 1;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
}
