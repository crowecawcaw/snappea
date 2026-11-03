//! Read-only diagram data from the real parser, including source links.
use serde_json::{json, Value};
use snappea::syntax::{self, ast::*, SourceMap, Span};
use std::collections::BTreeMap;

fn source(sm: &SourceMap, span: Span) -> Value {
    let loc = sm.location(span);
    json!({"file": loc.file, "line": loc.line, "col": loc.col, "text": sm.text(span)})
}

pub fn inspect(files: &BTreeMap<String, String>) -> Result<Value, String> {
    let mut sm = SourceMap::new();
    let mut programs = Vec::new();
    for (path, text) in files.iter().filter(|(p, _)| p.ends_with(".p")) {
        let id = sm.add(path.trim_start_matches("/work/"), text);
        programs.push(syntax::parse(id, text).map_err(|e| format!("{path}: {e:?}"))?);
    }
    let globals: Vec<&FunDecl> = programs
        .iter()
        .flat_map(|p| &p.decls)
        .filter_map(|d| match d {
            TopDecl::Fun(f) => Some(f),
            _ => None,
        })
        .collect();
    let mut machines = Vec::new();
    let mut tests = Vec::new();
    for d in programs.iter().flat_map(|p| &p.decls) {
        match d {
            TopDecl::Machine(m) => {
                let funs: Vec<&FunDecl> = m.funs.iter().chain(globals.iter().copied()).collect();
                let mut states = Vec::new();
                let mut rules = Vec::new();
                for s in &m.states {
                    let mut actions = Vec::new();
                    for item in &s.items {
                        let (label, handler, target) = match &item.kind {
                            StateItemKind::Entry(h) => ("entry".into(), Some(h), None),
                            StateItemKind::Exit(h) => ("exit".into(), Some(h), None),
                            StateItemKind::OnDo { events, handler } => (names(events), Some(handler), None),
                            StateItemKind::OnGoto { events, target, with } => (names(events), with.as_ref(), Some(target.name.clone())),
                            StateItemKind::Defer(events) => (format!("defer {}", names(events)), None, None),
                            StateItemKind::Ignore(events) => (format!("ignore {}", names(events)), None, None),
                        };
                        let mut targets = Vec::new();
                        if let Some(t) = target { targets.push(json!({"state":t,"conditional":false,"source":source(&sm,item.span)})); }
                        if let Some(h) = handler { walk_handler(h, &funs, &sm, &mut Vec::new(), &mut targets, &mut rules, false); }
                        actions.push(json!({"label":label,"source":source(&sm,item.span),"targets":targets}));
                    }
                    states.push(json!({"name":s.name.name,"start":s.is_start,"hot":s.temperature==Temperature::Hot,"source":source(&sm,s.name.span),"actions":actions}));
                }
                machines.push(json!({"name":m.name.name,"kind":match m.kind {MachineKind::Impl=>"machine",MachineKind::Spec=>"spec",MachineKind::Scenario=>"scenario"},"source":source(&sm,m.name.span),"variables":m.vars.iter().flat_map(|v|v.names.iter().map(|n|json!({"name":n.name,"type":sm.text(v.ty.span)}))).collect::<Vec<_>>(),"states":states,"rules":rules}));
            }
            TopDecl::SafetyTest(t) => tests.push(json!({"name":t.name.name,"main":t.main.name,"source":source(&sm,t.span),"composition":sm.text(t.expr.span)})),
            _ => {}
        }
    }
    Ok(json!({"machines":machines,"tests":tests}))
}
fn names(events: &[Ident]) -> String {
    events.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ")
}
fn walk_handler(
    h: &Handler,
    funs: &[&FunDecl],
    sm: &SourceMap,
    visiting: &mut Vec<String>,
    targets: &mut Vec<Value>,
    rules: &mut Vec<Value>,
    conditional: bool,
) {
    match h {
        Handler::Anon(a) => walk(&a.body.stmts, funs, sm, visiting, targets, rules, conditional),
        Handler::Named(n) => call(&n.name, funs, sm, visiting, targets, rules, conditional),
    }
}
fn call(
    name: &str,
    funs: &[&FunDecl],
    sm: &SourceMap,
    visiting: &mut Vec<String>,
    targets: &mut Vec<Value>,
    rules: &mut Vec<Value>,
    conditional: bool,
) {
    if visiting.iter().any(|n| n == name) {
        return;
    }
    if let Some(f) = funs.iter().find(|f| f.name.name == name) {
        if let Some(b) = &f.body {
            visiting.push(name.into());
            walk(&b.stmts, funs, sm, visiting, targets, rules, conditional);
            visiting.pop();
        }
    }
}
fn walk(
    stmts: &[Stmt],
    funs: &[&FunDecl],
    sm: &SourceMap,
    visiting: &mut Vec<String>,
    targets: &mut Vec<Value>,
    rules: &mut Vec<Value>,
    conditional: bool,
) {
    for s in stmts {
        match &s.kind {
            StmtKind::Goto { state, .. } => {
                targets.push(json!({"state":state.name,"conditional":conditional,"source":source(sm,s.span)}))
            }
            StmtKind::Assert { cond, .. } => {
                let rule = json!({"condition":sm.text(cond.span),"source":source(sm,s.span)});
                if !rules.contains(&rule) {
                    rules.push(rule);
                }
            }
            StmtKind::Compound(stmts) => walk(stmts, funs, sm, visiting, targets, rules, conditional),
            StmtKind::If { cond, then_branch, else_branch } => {
                let start = targets.len();
                walk(std::slice::from_ref(then_branch.as_ref()), funs, sm, visiting, targets, rules, true);
                add_guard(&mut targets[start..], sm.text(cond.span));
                if let Some(b) = else_branch {
                    let start = targets.len();
                    walk(std::slice::from_ref(b.as_ref()), funs, sm, visiting, targets, rules, true);
                    add_guard(&mut targets[start..], &format!("!({})", sm.text(cond.span)));
                }
            }
            StmtKind::While { body, .. } | StmtKind::Foreach { body, .. } => {
                walk(std::slice::from_ref(body.as_ref()), funs, sm, visiting, targets, rules, true)
            }
            StmtKind::Receive { cases } => {
                for c in cases {
                    walk(&c.handler.body.stmts, funs, sm, visiting, targets, rules, true);
                }
            }
            StmtKind::FunCall { fun, .. } => call(&fun.name, funs, sm, visiting, targets, rules, conditional),
            _ => {}
        }
    }
}
fn add_guard(targets: &mut [Value], guard: &str) {
    for target in targets {
        let combined = match target.get("guard").and_then(Value::as_str) {
            Some(inner) => format!("({guard}) && ({inner})"),
            None => guard.to_string(),
        };
        target["guard"] = json!(combined);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagrams_follow_syntax_and_named_handlers() {
        let files = BTreeMap::from([(
            "/work/model.p".into(),
            r#"// state Fake { }
        machine Main { fun move() { if (true) { goto Done; } }
          start state Init { on Go do move; }
          state Done { entry { assert false, "goto Fake;"; } }
        } test tc [main=Main]: {Main};"#
                .into(),
        )]);
        let v = inspect(&files).unwrap();
        assert_eq!(v["machines"][0]["states"].as_array().unwrap().len(), 2);
        assert_eq!(v["machines"][0]["states"][0]["actions"][0]["targets"][0]["state"], "Done");
        assert_eq!(v["machines"][0]["states"][0]["actions"][0]["targets"][0]["conditional"], true);
        assert_eq!(v["machines"][0]["rules"][0]["condition"], "false");
        assert_eq!(v["tests"][0]["main"], "Main");
        assert_eq!(v["machines"][0]["states"][0]["actions"][0]["targets"][0]["guard"], "true");
    }
    #[test]
    fn invalid_edits_do_not_produce_a_partial_diagram() {
        assert!(inspect(&BTreeMap::from([("broken.p".into(), "machine Main {".into())])).is_err());
    }
}
