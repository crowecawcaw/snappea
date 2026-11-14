//! Bug reports as `snappea` shows them ([`Config::snappea_reports`]): runtime errors in
//! P terms with their source location and a fix, and the fields of the specs a liveness
//! or deadlock report is about. `snappea-compat` keeps upstream's text instead.
//!
//! [`Config::snappea_reports`]: crate::checker::runtime::Config::snappea_reports

use crate::checker::runtime::Exception;
use crate::checker::value::{event_cs_name, Value};
use crate::sema::diag::render_snippet;
use crate::sema::types::PType;
use crate::sema::{MachineId, Program};
use crate::syntax::Span;

/// A spec's fields as `name = value` pairs, e.g. `open = {0: Connection(3)}, unclosed = 0`
/// (see [`p_value`]); empty for a spec without fields.
pub fn spec_fields(prog: &Program, spec: MachineId, fields: &[Value]) -> String {
    prog.machine(spec)
        .fields
        .iter()
        .filter_map(|v| {
            let d = prog.var(*v);
            let value = fields.get(d.slot as usize)?;
            Some(format!("{} = {}", d.name, p_value(value, &d.ty, prog)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `value` of static type `ty` written as P source would write it, with enum values by
/// name: `[1, 2]` for a sequence, `{1, 2}` for a set, `{k: v}` for a map, `(a = 1, b = 2)`
/// for a named tuple.
pub fn p_value(value: &Value, ty: &PType, prog: &Program) -> String {
    let mut out = String::new();
    write_p_value(&mut out, value, ty, prog);
    out
}

fn write_p_value(out: &mut String, value: &Value, ty: &PType, prog: &Program) {
    let ty = ty.canonicalize(prog);
    let any = PType::Any;
    let list = |out: &mut String, items: &mut dyn Iterator<Item = (&Value, &PType)>| {
        for (i, (v, t)) in items.enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            write_p_value(out, v, t, prog);
        }
    };
    match (value, &ty) {
        (Value::Null, _) => out.push_str("null"),
        (Value::Int(i), PType::Enum(id)) => match prog.enums[id.idx()].elems.iter().find(|e| e.value == *i) {
            Some(e) => out.push_str(&e.name),
            None => out.push_str(&i.to_string()),
        },
        (Value::Bool(b), _) => out.push_str(if *b { "true" } else { "false" }),
        (Value::Str(s), _) => {
            out.push('"');
            out.push_str(&s.replace('"', "\\\""));
            out.push('"');
        }
        (Value::Event(e), _) => out.push_str(event_cs_name(prog, *e)),
        (Value::Tuple(vs), _) => {
            let ts: Vec<&PType> = match &ty {
                PType::Tuple(ts) => ts.iter().collect(),
                _ => vec![&any; vs.len()],
            };
            out.push('(');
            list(out, &mut vs.iter().zip(ts.into_iter().chain(std::iter::repeat(&any))));
            out.push_str(if vs.len() == 1 { ",)" } else { ")" });
        }
        (Value::NamedTuple(t), _) => {
            out.push('(');
            for (i, (n, v)) in t.names.iter().zip(t.values.iter()).enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let ft = match &ty {
                    PType::NamedTuple(fs) => fs.iter().find(|(f, _)| f == n).map(|(_, t)| t),
                    _ => None,
                };
                out.push_str(n);
                out.push_str(" = ");
                write_p_value(out, v, ft.unwrap_or(&any), prog);
            }
            out.push_str(if t.names.len() == 1 { ",)" } else { ")" });
        }
        (Value::Seq(s), _) => {
            let et = match &ty {
                PType::Seq(t) => &**t,
                _ => &any,
            };
            out.push('[');
            list(out, &mut s.iter().map(|v| (v, et)));
            out.push(']');
        }
        (Value::Set(s), _) => {
            let et = match &ty {
                PType::Set(t) => &**t,
                _ => &any,
            };
            out.push('{');
            list(out, &mut s.iter().map(|v| (v, et)));
            out.push('}');
        }
        (Value::Map(m), _) => {
            let (kt, vt) = match &ty {
                PType::Map(k, v) => (&**k, &**v),
                _ => (&any, &any),
            };
            out.push('{');
            for (i, (k, v)) in m.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_p_value(out, k, kt, prog);
                out.push_str(": ");
                write_p_value(out, v, vt, prog);
            }
            out.push('}');
        }
        (other, _) => other.write_display(out, prog),
    }
}

/// A runtime error: `error: <what> (in <context>)`, the source around `at` with the
/// failing expression underlined, and `help`.
pub fn render(prog: &Program, what: &str, context: &str, at: Option<Span>, help: &str) -> String {
    let message = if context.is_empty() { what.to_string() } else { format!("{} (in {})", what, context) };
    let help = (!help.is_empty()).then_some(help);
    render_snippet(&prog.sources, at, &message, help).trim_end().to_string()
}

/// The source text of `at`, on one line, when it is short enough to quote.
pub fn source_text(prog: &Program, at: Option<Span>) -> Option<String> {
    let at = at.filter(|s| !s.is_dummy())?;
    let text = prog.sources.text(at).split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text.trim_end_matches(';').trim().to_string();
    (!text.is_empty() && text.len() <= 80).then_some(text)
}

/// The fix for a `send` whose target is null, naming the target from the statement at
/// `at`.
pub fn null_target_help(prog: &Program, at: Option<Span>) -> String {
    let target = source_text(prog, at)
        .and_then(|s| s.strip_prefix("send ").and_then(|s| s.split(',').next()).map(|t| t.trim().to_string()));
    match target {
        Some(t) => format!(
            "'{}' is null here: it is never set, or set later; set it before this send (e.g. in the start state's entry), or guard with `if ({} != null)`",
            t, t
        ),
        None => "the target is null here: set it before this send, or guard the send with `!= null`".to_string(),
    }
}

/// `m` and `k` of an indexing expression `m[k]` (or of the first index of `m[k][j]`,
/// `m[k].f = v`).
fn indexed(src: &str) -> Option<(&str, &str)> {
    let open = src.find('[')?;
    let mut depth = 0;
    for (i, c) in src[open..].char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    let name = src[..open].trim();
                    let key = src[open + 1..open + i].trim();
                    return (!name.is_empty() && !key.is_empty()).then_some((name, key));
                }
            }
            _ => {}
        }
    }
    None
}

/// What went wrong and how to fix it, for an exception thrown by P code; `src` is the
/// text of the failing expression or statement.
pub fn describe_exception(exc: &Exception, src: Option<&str>) -> (String, String) {
    let short = exc.ty.rsplit('.').next().unwrap_or(&exc.ty);
    let idx = src.and_then(indexed);
    match short {
        "KeyNotFoundException" => {
            let key = exc
                .message
                .strip_prefix("The given key '")
                .and_then(|k| k.strip_suffix("' was not present in the dictionary."))
                .unwrap_or("?");
            match idx {
                Some((map, k)) => (
                    format!("map '{}' has no key {}", map, key),
                    format!("test `{} in {}` first, or add the key before reading it", k, map),
                ),
                None => (
                    format!("the map has no key {}", key),
                    "test `key in map` first, or add the key before reading it".to_string(),
                ),
            }
        }
        "ArgumentException" if exc.message.starts_with("An item with the same key") => {
            let key = exc.message.rsplit("Key: ").next().unwrap_or("?");
            (
                format!("the map already has key {}", key),
                "to replace the value, assign `m[k] = v`; to add only new keys, test `!(k in m)` first".to_string(),
            )
        }
        "ArgumentOutOfRangeException" if exc.message.starts_with("Index must be within the bounds") => (
            "insert position is out of range".to_string(),
            "insert at an index from 0 to sizeof(s): `s += (sizeof(s), x)` appends".to_string(),
        ),
        "ArgumentOutOfRangeException" => match idx {
            Some((coll, i)) => (
                format!("index `{}` is out of range for '{}'", i, coll),
                format!("test `{} < sizeof({})` first; indexes start at 0", i, coll),
            ),
            None => ("index is out of range".to_string(), "test `i < sizeof(s)` first; indexes start at 0".to_string()),
        },
        "NullReferenceException" => (
            "a null value was used".to_string(),
            "give the variable, field or payload a value before using it; fields of machine type are null until set"
                .to_string(),
        ),
        "DivideByZeroException" => ("division by zero".to_string(), "test that the divisor is not 0 first".to_string()),
        "OverflowException" => {
            ("integer overflow".to_string(), "keep the operands within the 64-bit integer range".to_string())
        }
        "InvalidCastException" => {
            let mut types = exc.message.split('\'').skip(1).step_by(2).map(p_type_name);
            let (from, to) = (types.next().unwrap_or_default(), types.next().unwrap_or_default());
            (
                format!("cannot cast a value of type {} to {}", from, to),
                "`as` does not convert a value: check its type first, or convert with `to`".to_string(),
            )
        }
        "InvalidOperationException" if exc.message.starts_with("Collection was modified") => (
            "a collection was changed while `foreach` iterated over it".to_string(),
            "iterate over a copy, or collect the changes and apply them after the loop".to_string(),
        ),
        "FormatException" => (
            format!("invalid format string: {}", exc.message),
            "use `{0}`, `{1}`, ... for the arguments, and `{{` or `}}` for literal braces".to_string(),
        ),
        _ => (exc.message.trim_end_matches('.').to_string(), String::new()),
    }
}

/// The P name of a .NET runtime class, e.g. `int` for `PChecker.Runtime.Values.PInt`.
fn p_type_name(class: &str) -> String {
    let short = class.rsplit('.').next().unwrap_or(class);
    match short {
        "PBool" => "bool",
        "PInt" => "int",
        "PFloat" => "float",
        "PString" => "string",
        "PMachineValue" => "machine",
        "PSeq" => "seq",
        "PSet" => "set",
        "PMap" => "map",
        "PTuple" => "tuple",
        "PNamedTuple" => "named tuple",
        "Event" | "PHalt" | "DefaultEvent" => "event",
        "IPValue" => "any",
        other if class.starts_with("PImplementation.") => return format!("event {}", other),
        other => other,
    }
    .to_string()
}

/// What went wrong, where, and the fix, when `machine` in `state` gets `event` and the
/// state has no handler for it. `spec` names the spec or scenario monitor, `actor` the
/// machine instance (e.g. `Client(2)`). The location is the state's declaration.
pub fn unhandled_event(
    prog: &Program,
    machine: MachineId,
    actor: &str,
    state: Option<crate::sema::StateId>,
    event: crate::sema::EventId,
) -> (String, Option<Span>, String) {
    let decl = prog.machine(machine);
    let ev = &prog.event(event).name;
    let state_name = state.map(|s| prog.state(s).name.clone());
    let at = state.map(|s| prog.state(s).span);
    let in_state = state_name.as_ref().map_or(String::new(), |s| format!(" in state '{}'", s));
    if decl.is_spec() {
        let what = format!("spec {}{} has no handler for observed event '{}'", decl.name, in_state, ev);
        let help = format!(
            "a spec must handle every event it observes in every state: add `on {ev} do {{ ... }}` \
             (or `on {ev} goto S;`) to the state, or `ignore {ev};` if this state does not care about it"
        );
        return (what, at, help);
    }
    let what = format!("{}{} received event '{}' that it cannot handle", actor, in_state, ev);
    let mut help = format!(
        "handle it in the state with `on {ev} do {{ ... }}` or `on {ev} goto S;`; \
         `defer {ev};` to keep it for a later state, or `ignore {ev};` to drop it"
    );
    // a `receive` takes only its cases' events, and only while it waits
    let mut waits: Vec<(Span, Vec<crate::sema::EventId>)> = Vec::new();
    for f in crate::checker::checked::machine_funs(prog, machine) {
        if let Some(b) = &prog.fun(f).body {
            crate::checker::checked::visit_stmts(b, &mut |s| {
                if let crate::sema::StmtKind::Receive { cases } = &s.kind {
                    waits.push((s.span, cases.iter().map(|(e, _)| *e).collect()));
                }
            });
        }
    }
    waits.sort_by_key(|(sp, _)| (sp.file.0, sp.start));
    if let Some((w, _)) = waits.iter().find(|(_, evs)| evs.contains(&event)) {
        help.push_str(&format!(
            ". The `receive` at {} takes '{}' only while it waits there; this one arrived when no `receive` was waiting",
            prog.sources.location(*w),
            ev
        ));
    } else if let Some((w, evs)) = waits.first() {
        let names: Vec<String> = evs.iter().map(|e| format!("'{}'", prog.event(*e).name)).collect();
        help.push_str(&format!(
            ". The `receive` at {} waits only for {}; other events stay queued and the state handles them once it is done",
            prog.sources.location(*w),
            names.join(", ")
        ));
    }
    (what, at, help)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_name_the_map_and_the_test() {
        let exc = Exception::new(
            "System.Collections.Generic.KeyNotFoundException",
            "The given key '1' was not present in the dictionary.",
        );
        let (what, help) = describe_exception(&exc, Some("carts[cart]"));
        assert_eq!(what, "map 'carts' has no key 1");
        assert_eq!(help, "test `cart in carts` first, or add the key before reading it");
        let (what, _) = describe_exception(&exc, Some("m[a][b] = 1"));
        assert_eq!(what, "map 'm' has no key 1");
    }

    #[test]
    fn casts_name_p_types() {
        let exc = Exception::new(
            "System.InvalidCastException",
            "Unable to cast object of type 'PChecker.Runtime.Values.PInt' to type 'PChecker.Runtime.Values.PString'.",
        );
        assert_eq!(describe_exception(&exc, None).0, "cannot cast a value of type int to string");
    }
}
