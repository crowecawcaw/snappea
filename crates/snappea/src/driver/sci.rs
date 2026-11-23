//! The `.sci` file: upstream serializes its `CoverageInfo` with .NET's
//! `DataContractSerializer`. This writes the same document structure (machines, states,
//! registered events and received/sent events) so the coverage data is available to other
//! tools; the coverage graph itself is omitted, so it is not byte-identical to upstream's.

use crate::checker::coverage::{CoverageInfo, CoverageTables};
use crate::sema::{HandlerAction, Program};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn event_name(prog: &Program, ev: u32) -> String {
    if ev == u32::MAX {
        return "PChecker.Runtime.StateMachines.Events.GotoStateEvent".into();
    }
    let e = crate::sema::EventId(ev);
    if e == prog.null_event {
        return "PChecker.Runtime.Events.DefaultEvent".into();
    }
    format!("PImplementation.{}", crate::checker::value::event_cs_name(prog, e))
}

fn string_set(tag: &str, items: &[String]) -> String {
    let mut out = format!("<{} xmlns:a=\"http://schemas.microsoft.com/2003/10/Serialization/Arrays\">", tag);
    for i in items {
        out.push_str(&format!("<a:string>{}</a:string>", esc(i)));
    }
    out.push_str(&format!("</{}>", tag));
    out
}

fn dict(tag: &str, entries: &[(String, Vec<String>)]) -> String {
    let mut out = format!("<{} xmlns:a=\"http://schemas.microsoft.com/2003/10/Serialization/Arrays\">", tag);
    for (k, vs) in entries {
        out.push_str(&format!("<a:KeyValueOfstringArrayOfstringty7Ep6D1><a:Key>{}</a:Key><a:Value>", esc(k)));
        for v in vs {
            out.push_str(&format!("<a:string>{}</a:string>", esc(v)));
        }
        out.push_str("</a:Value></a:KeyValueOfstringArrayOfstringty7Ep6D1>");
    }
    out.push_str(&format!("</{}>", tag));
    out
}

/// The `CoverageInfo` document.
pub(super) fn coverage_xml(prog: &Program, tables: &CoverageTables, info: &CoverageInfo) -> String {
    let full = |d: crate::sema::MachineId| format!("PImplementation.{}", prog.machine(d).cs_name);
    let machines: Vec<String> = info.declared.iter().map(|(d, _)| full(*d)).collect();
    let mut states = Vec::new();
    let mut registered = Vec::new();
    for (d, _) in &info.declared {
        let m = prog.machine(*d);
        states.push((full(*d), m.states.iter().map(|s| prog.state(*s).name.clone()).collect()));
        for s in &m.states {
            let evs: Vec<String> = prog
                .state(*s)
                .handlers
                .iter()
                .filter(|h| matches!(h.action, HandlerAction::Do(_) | HandlerAction::Goto { .. }))
                .map(|h| event_name(prog, h.event.0))
                .collect();
            if !evs.is_empty() {
                registered.push((format!("{}.{}", full(*d), prog.state(*s).name), evs));
            }
        }
    }
    let group = |set: &crate::checker::coverage::PairSet| {
        let mut by: Vec<(String, Vec<String>)> = Vec::new();
        let mut pairs: Vec<&(u32, u32)> = set.iter().collect();
        pairs.sort();
        for (n, e) in pairs {
            let k = tables.nodes[*n as usize].clone();
            let name = event_name(prog, *e);
            match by.iter_mut().find(|(kk, _)| *kk == k) {
                Some((_, v)) => v.push(name),
                None => by.push((k, vec![name])),
            }
        }
        by
    };
    let mut out = String::from(
        "<CoverageInfo xmlns=\"http://schemas.datacontract.org/2004/07/PChecker.Coverage\" xmlns:i=\"http://www.w3.org/2001/XMLSchema-instance\">",
    );
    out.push_str("<CoverageGraph i:nil=\"true\"/>");
    out.push_str("<EventInfo>");
    out.push_str(&dict("EventsReceived", &group(&info.received)));
    out.push_str(&dict("EventsSent", &group(&info.sent)));
    out.push_str("</EventInfo>");
    out.push_str(&string_set("Machines", &machines));
    out.push_str(&dict("MachinesToStates", &states));
    out.push_str(&dict("RegisteredEvents", &registered));
    out.push_str("</CoverageInfo>");
    out
}
