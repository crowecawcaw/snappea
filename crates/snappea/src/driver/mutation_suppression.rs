//! Advisory evidence for suppressed state transitions. Static call reachability is
//! conservative: a different handler is not proof of an observable behavior change.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::checker::checked::visit_stmts;
use crate::checker::mutation::{Change, Mutant};
use crate::sema::{FunId, HandlerAction, Program, StateId, StmtKind};
use crate::syntax::Span;

use super::mutation::Verdict;

pub(super) fn advisories(prog: &Program, mutants: &[Mutant], verdicts: &[Verdict]) -> Vec<Value> {
    let sites: BTreeSet<Span> = mutants
        .iter()
        .zip(verdicts)
        .filter(|(m, v)| m.change == Change::Remove && **v == Verdict::Suppressed)
        .map(|(m, _)| m.span)
        .collect();
    if sites.is_empty() {
        return Vec::new();
    }
    let mut transitions: BTreeMap<Span, BTreeSet<(StateId, StateId)>> = BTreeMap::new();
    for (i, source) in prog.states.iter().enumerate() {
        let mut todo: Vec<FunId> = source.entry.into_iter().chain(source.exit).collect();
        for h in &source.handlers {
            match h.action {
                HandlerAction::Do(f) | HandlerAction::Goto { with: Some(f), .. } => todo.push(f),
                _ => {}
            }
        }
        let mut seen = BTreeSet::new();
        while let Some(fun) = todo.pop() {
            if !seen.insert(fun) {
                continue;
            }
            todo.extend(prog.fun(fun).callees.iter().copied());
            if let Some(body) = &prog.fun(fun).body {
                visit_stmts(body, &mut |s| match &s.kind {
                    StmtKind::Goto { state, .. }
                        if sites.contains(&s.span) && prog.state(*state).machine == source.machine =>
                    {
                        transitions.entry(s.span).or_default().insert((StateId(i as u32), *state));
                    }
                    StmtKind::Receive { cases } => todo.extend(cases.iter().map(|(_, f)| *f)),
                    _ => {}
                });
            }
        }
    }
    let mut out = Vec::new();
    for m in mutants.iter().filter(|m| sites.contains(&m.span) && m.change == Change::Remove) {
        for &(source, target) in transitions.get(&m.span).into_iter().flatten() {
            let (a, b) = (prog.state(source), prog.state(target));
            let events: BTreeSet<_> = a.handlers.iter().chain(&b.handlers).map(|h| h.event).collect();
            // Prefer a change in handling policy over two differently named functions.
            let difference = events
                .into_iter()
                .filter_map(|event| {
                    let left = a.handler_for(event).map(|h| &h.action);
                    let right = b.handler_for(event).map(|h| &h.action);
                    if same_action(left, right) {
                        return None;
                    }
                    let priority =
                        usize::from(matches!((left, right), (Some(HandlerAction::Do(_)), Some(HandlerAction::Do(_)))));
                    Some((priority, event, left, right))
                })
                .min_by_key(|(priority, event, _, _)| (*priority, *event));
            let Some((_, event, left, right)) = difference else { continue };
            let machine = &prog.machine(a.machine).name;
            let from = format!("{}.{}", machine, a.name);
            let to = format!("{}.{}", machine, b.name);
            let event = &prog.event(event).name;
            let left = action_text(prog, left);
            let right = action_text(prog, right);
            let message = format!("suppressed goto may run in {from}, but {from} and {to} declare different handling for {event}: {left} versus {right}");
            out.push(json!({
                "rule": "suppressed-state-transition", "file": m.file, "line": m.line,
                "context": m.context, "source_state": from, "target_state": to,
                "event": event, "source_action": left, "target_action": right,
                "message": message,
                "help": format!("Exercise {event} before and after the transition. Different handlers do not prove different behavior; retain the suppression only if its reason covers this case."),
            }));
        }
    }
    out
}

fn same_action(a: Option<&HandlerAction>, b: Option<&HandlerAction>) -> bool {
    match (a, b) {
        (None, None)
        | (Some(HandlerAction::Ignore), Some(HandlerAction::Ignore))
        | (Some(HandlerAction::Defer), Some(HandlerAction::Defer)) => true,
        (Some(HandlerAction::Do(a)), Some(HandlerAction::Do(b))) => a == b,
        (Some(HandlerAction::Goto { target: a, with: af }), Some(HandlerAction::Goto { target: b, with: bf })) => {
            a == b && af == bf
        }
        _ => false,
    }
}

fn action_text(prog: &Program, action: Option<&HandlerAction>) -> String {
    match action {
        None => "no declared handler".into(),
        Some(HandlerAction::Ignore) => "ignore".into(),
        Some(HandlerAction::Defer) => "defer".into(),
        Some(HandlerAction::Do(f)) => {
            let fun = prog.fun(*f);
            if !fun.name.is_empty() {
                return format!("call {}", fun.name);
            }
            let loc = prog.sources.location(fun.span);
            format!("handler at {}:{}", loc.file.unwrap_or_default(), loc.line)
        }
        Some(HandlerAction::Goto { target, with }) => {
            let name = &prog.state(*target).name;
            with.map_or_else(
                || format!("goto {name}"),
                |f| {
                    let loc = prog.sources.location(prog.fun(f).span);
                    format!("goto {name} with action at {}:{}", loc.file.unwrap_or_default(), loc.line)
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::{compile::compile_program_with_probes, mutation};

    fn inspect(body: &str, target: &str, suppress: bool) -> Vec<Value> {
        let mut sm = crate::syntax::SourceMap::new();
        let allow = if suppress {
            "// snappea-lint: allow mutation-nested -- inspect a deliberate transition suppression\n"
        } else {
            ""
        };
        sm.add(
            "model.p",
            format!(
                r#"
            event eTimeout; event eLeave;
            machine Main {{ start state S {{ entry {{ new Node(); }} }} }}
            {allow}machine Node {{
                start state Leader {{ entry {{ {body} }} on eTimeout do Timeout; }}
                state Follower {{ {target} }}
                fun Leave() {{ goto Follower; }}
                fun Timeout() {{ print "timeout"; }}
            }}
            test tcMain [main=Main]: {{ Main, Node }};
        "#
            ),
        );
        let analysis = crate::sema::compile(&sm, &Default::default()).unwrap();
        let prog = &analysis.program;
        let probes = compile_program_with_probes(prog).probes;
        let funs = mutation::mutable_funs(prog, &prog.test_cases(), &[]);
        let mutants = mutation::mutants(prog, &funs, &probes);
        let allowed = snappea_lint::mutation_suppression_details(&sm).unwrap();
        let verdicts = mutants
            .iter()
            .map(|m| {
                if allowed.iter().any(|s| s.matches(m.span, m.change.name())) {
                    Verdict::Suppressed
                } else {
                    Verdict::NotTested
                }
            })
            .collect::<Vec<_>>();
        advisories(prog, &mutants, &verdicts)
    }

    #[test]
    fn explains_direct_helper_and_receive_transitions() {
        for body in ["goto Follower;", "Leave();", "receive { case eLeave: { Leave(); } }"] {
            let warnings = inspect(body, "ignore eTimeout;", true);
            assert_eq!(warnings.len(), 1, "{body}: {warnings:?}");
            let w = &warnings[0];
            assert_eq!(w["source_state"], "Node.Leader");
            assert_eq!(w["target_state"], "Node.Follower");
            assert_eq!(w["event"], "eTimeout");
            assert_eq!(w["source_action"], "call Timeout");
            assert_eq!(w["target_action"], "ignore");
            assert_eq!(w["file"], "model.p");
            assert!(w["line"].as_u64().unwrap() > 0);
        }
    }

    #[test]
    fn does_not_warn_for_identical_handlers_unsuppressed_or_uncalled_code() {
        assert!(inspect("Leave();", "on eTimeout do Timeout;", true).is_empty());
        assert!(inspect("Leave();", "ignore eTimeout;", false).is_empty());
        assert!(inspect(r#"print "idle";"#, "ignore eTimeout;", true).is_empty());
    }

    #[test]
    fn distinguishes_missing_and_deferred_handlers() {
        assert_eq!(inspect("Leave();", "", true)[0]["target_action"], "no declared handler");
        assert_eq!(inspect("Leave();", "defer eTimeout;", true)[0]["target_action"], "defer");
    }
}
