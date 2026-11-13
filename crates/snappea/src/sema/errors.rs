//! Constructors for every diagnostic message, copied from `DefaultTranslationErrorHandler`.

use crate::sema::diag::Diagnostic;
use crate::sema::program::*;
use crate::sema::types::{PType, TypeKind};
use crate::syntax::{SourceMap, Span};

/// `FormatType` upstream: the "null" type is shown as "(no value)".
pub fn format_type(t: &PType, prog: &Program) -> String {
    let rep = t.original_repr(prog);
    if rep == "null" {
        "(no value)".to_string()
    } else {
        rep
    }
}

pub struct Errors<'a> {
    pub sm: &'a SourceMap,
}

impl<'a> Errors<'a> {
    fn loc(&self, span: Span) -> String {
        self.sm.location(span).to_string()
    }

    pub fn duplicate_start_state(&self, span: Span, dup: &str, orig: &str, machine: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!("state '{}' tries to replace start state '{}' in machine '{}'", dup, orig, machine),
        )
        .help("mark exactly one state of the machine `start`")
    }

    pub fn duplicate_event_action(&self, span: Span, existing: Span, state: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!("event handler in state '{}' duplicates handler at {}", state, self.loc(existing)),
        )
        .help("keep one handler for this event in this state; merge their bodies or remove one")
    }

    pub fn duplicate_state_exit(&self, span: Span, existing: Span, state: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!("exit handler in state '{}' duplicates exit handler at {}", state, self.loc(existing)),
        )
        .help("keep one `exit` block per state")
    }

    pub fn duplicate_state_entry(&self, span: Span, existing: Span, state: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!("entry handler in state '{}' duplicates entry handler at {}", state, self.loc(existing)),
        )
        .help("keep one `entry` block per state")
    }

    pub fn duplicate_declaration(&self, span: Span, dup: &str, existing: &str, existing_span: Span) -> Diagnostic {
        Diagnostic::new(span, format!("'{}' duplicates declaration '{}' at {}", dup, existing, self.loc(existing_span)))
            .help("rename one of the two declarations")
    }

    pub fn redeclare_global_param(&self, span: Span, dup: &str, existing: &str, existing_span: Span) -> Diagnostic {
        Diagnostic::new(
            span,
            format!(
                "'{}' redeclares global param '{}' (originally declared at {})",
                dup,
                existing,
                self.loc(existing_span)
            ),
        )
        .help("rename this param or remove one of the declarations")
    }

    pub fn undeclared_global_param(&self, span: Span, name: &str) -> Diagnostic {
        Diagnostic::new(span, format!("undeclared global param '{}'", name))
            .help("declare it at the top level, e.g. `param N: int;`")
    }

    pub fn modify_global_param(&self, span: Span, name: &str, decl_span: Span) -> Diagnostic {
        Diagnostic::new(span, format!("cannot modify global param '{}' (declared at {})", name, self.loc(decl_span)))
            .help("params are read-only; copy it into a local variable and change that")
    }

    pub fn invalid_twise(&self, span: Span, test_span: Span, msg: &str) -> Diagnostic {
        Diagnostic::new(span, format!("invalid twise number at {}: {}", self.loc(test_span), msg))
            .help("use a number between 1 and the number of params, e.g. `(2 wise)`")
    }

    pub fn incorrect_argument_count(&self, span: Span, actual: usize, expected: usize) -> Diagnostic {
        Diagnostic::new(
            span,
            format!("goto, function or constructor call expected {} arguments, got {}", expected, actual),
        )
        .help("pass one argument per parameter, in declaration order")
    }

    pub fn missing_declaration(&self, span: Span, kind: &str, name: &str) -> Diagnostic {
        let d = Diagnostic::new(span, format!("could not find {} '{}'", kind, name));
        match self.similar_name(name) {
            Some(s) => d.help(format!("did you mean '{}'? Otherwise declare the {} before using it", s, kind)),
            None => d.help(format!("declare the {} before using it, or check the spelling", kind)),
        }
    }

    /// The identifier in the sources closest to `name`, if one is a likely typo of it.
    fn similar_name(&self, name: &str) -> Option<String> {
        let limit = (name.chars().count() / 3).max(1);
        let mut best: Option<(usize, String)> = None;
        for (i, f) in self.sm.files().iter().enumerate() {
            let Ok(tokens) = crate::syntax::lexer::lex(crate::syntax::FileId(i as u32), &f.text) else { continue };
            for t in tokens.iter().filter(|t| t.kind == crate::syntax::token::TokenKind::Iden) {
                let cand = f.slice(t.span);
                if cand == name {
                    continue;
                }
                let d = if cand.eq_ignore_ascii_case(name) { 0 } else { edit_distance(cand, name) };
                // a short name is "close" to every other short name; require some overlap
                let shorter = cand.chars().count().min(name.chars().count());
                if d <= limit && d < shorter && best.as_ref().is_none_or(|(bd, b)| (d, cand) < (*bd, b.as_str())) {
                    best = Some((d, cand.to_string()));
                }
            }
        }
        best.map(|(_, s)| s)
    }

    pub fn circular_typedef(&self, span: Span, name: &str) -> Diagnostic {
        Diagnostic::new(span, format!("typedef '{}' has a circular dependency", name))
            .help("break the cycle: a type cannot contain itself")
    }

    pub fn duplicate_named_tuple_entry(&self, span: Span, name: &str) -> Diagnostic {
        Diagnostic::new(span, format!("named tuple has duplicate field '{}'", name))
            .help("give each field a distinct name")
    }

    pub fn illegal_type_in_coerce(&self, span: Span) -> Diagnostic {
        Diagnostic::new(span, "coerce expects an interface, int, or float type")
            .help("`to` converts to `int`, `float` or an interface: `e to int`, `n to float`, `m to I`; P has no conversion to an enum, so map an int to enum values explicitly, e.g. `if (n == 0) { s = A; } else { s = B; }`")
    }

    /// `x as T` where `x` and `T` are both int, float or enum types: `as` only
    /// changes the static type, so it cannot turn one of these into another.
    pub fn as_between_numeric_types(&self, span: Span, old: &PType, new: &PType, prog: &Program) -> Diagnostic {
        let to = new.original_repr(prog);
        self.incomparable_types(span, old, new, prog)
            .help(format!("`as` changes the static type and does not convert a value; to convert, use `x to {}`", to))
    }

    /// `x to T` between types that `to` cannot convert.
    pub fn illegal_to(&self, span: Span, old: &PType, new: &PType, prog: &Program) -> Diagnostic {
        self.incomparable_types(span, old, new, prog).help(
            "`to` converts between int and float, from an enum to int, and from a machine to an interface; \
             `as` changes only the static type, e.g. of an `any` value",
        )
    }

    pub fn illegal_interface_coerce(&self, span: Span, old: &str, new: &str, outlier: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!(
                "illegal coerce: permissions of '{}' are not a superset of '{}' (e.g., event '{}')",
                old, new, outlier
            ),
        )
        .help("coerce to an interface whose events the machine receives, or add the missing events to it")
    }

    pub fn static_function_not_allowed_as_handler(&self, span: Span, name: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!(
                "function '{}' cannot be used as a handler: global and foreign functions are not allowed as handlers",
                name
            ),
        )
        .help("declare the function inside the machine, or call it from a handler block: `on e do { f(); }`")
    }

    pub fn type_mismatch(&self, span: Span, actual: &PType, expected: &[&PType], prog: &Program) -> Diagnostic {
        Diagnostic::new(
            span,
            format!(
                "got type: {}, expected: {}",
                format_type(actual, prog),
                expected.iter().map(|t| format_type(t, prog)).collect::<Vec<_>>().join("; ")
            ),
        )
        .help("give it a value of the expected type, convert it (`e to int`, `n to float`, or `v as T` for an `any` value), or change the declared type")
    }

    pub fn type_mismatch_kinds(&self, span: Span, actual: &PType, expected: &[TypeKind], prog: &Program) -> Diagnostic {
        Diagnostic::new(
            span,
            format!(
                "got type: {}, expected: {}",
                format_type(actual, prog),
                expected.iter().map(|k| k.name()).collect::<Vec<_>>().join(", ")
            ),
        )
        .help("give it a value of one of the expected types, or convert it (`e to int`, `n to float`, or `v as T` for an `any` value)")
    }

    pub fn missing_named_tuple_entry(&self, span: Span, tuple: &PType, field: &str, prog: &Program) -> Diagnostic {
        Diagnostic::new(span, format!("named tuple type {} has no '{}' field", tuple.original_repr(prog), field))
            .help("use one of the tuple's field names, or add the field to its type")
    }

    pub fn out_of_bounds_tuple_access(&self, span: Span, tuple: &PType, index: &str, prog: &Program) -> Diagnostic {
        Diagnostic::new(span, format!("tuple type {} has no '{}' field", tuple.original_repr(prog), index))
            .help("tuple fields are numbered from 0; use an index below the tuple's size")
    }

    pub fn incomparable_types(&self, span: Span, lhs: &PType, rhs: &PType, prog: &Program) -> Diagnostic {
        Diagnostic::new(
            span,
            format!("types {} and {} are incomparable", lhs.original_repr(prog), rhs.original_repr(prog)),
        )
        .help("compare values of the same type; convert one side first, e.g. `e to int` for an enum or `n to float` for an int")
    }

    pub fn misplaced_this(&self, span: Span) -> Diagnostic {
        Diagnostic::new(span, "keyword 'this' used outside a machine")
            .help("`this` is only defined inside a machine; pass the machine in as a parameter instead")
    }

    pub fn binop_type_mismatch(&self, op_span: Span, op: &str, lhs: &PType, rhs: &PType, prog: &Program) -> Diagnostic {
        Diagnostic::new(
            op_span,
            format!(
                "operator '{}' requires both operands to be int or both float; got {} and {}",
                op,
                lhs.original_repr(prog),
                rhs.original_repr(prog)
            ),
        )
        .help("convert one operand so both match, e.g. `a + (b to float)`")
    }

    pub fn more_than_one_parameter_for_handlers(&self, span: Span, count: usize) -> Diagnostic {
        Diagnostic::new(
            span,
            format!("entry, exit, do, and goto handlers take at most 1 parameter; function expects {}", count),
        )
        .help("take a single parameter; send a tuple payload and read its fields")
    }

    pub fn illegal_choose_sub_expr_type(&self, span: Span, t: &PType, prog: &Program) -> Diagnostic {
        Diagnostic::new(
            span,
            format!(
                "'choose' expects int (max value) or a collection (seq, set, or map); got type {}",
                t.original_repr(prog)
            ),
        )
        .help("pass an int bound (`choose(10)`) or a seq, set or map (`choose(s)`)")
    }

    pub fn illegal_choose_sub_expr_value(&self, span: Span, n: i64) -> Diagnostic {
        Diagnostic::new(span, format!("'choose' expects at most 10000 choices; got {}", n))
            .help("choose from a smaller range, or choose in two steps (a bucket, then a value in it)")
    }

    pub fn illegal_function_used_in_spec(&self, span: Span, fun: &str, monitor: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!(
                "function '{}' is non-deterministic or has side effects (new/send/receive/announce); cannot be called from spec monitor '{}'",
                fun, monitor
            ),
        )
        .help("spec monitors must be deterministic: compute the value in a machine and pass it in the event payload")
    }

    pub fn emitted_null_event(&self, span: Span) -> Diagnostic {
        Diagnostic::new(span, "cannot send null events")
            .help("send a declared event; check that the event variable is set before sending")
    }

    pub fn missing_start_state(&self, span: Span, machine: &str) -> Diagnostic {
        Diagnostic::new(span, format!("machine '{}' has no start state", machine))
            .help("mark one state `start`, e.g. `start state Init { ... }`")
    }

    pub fn changed_state_mid_transition(&self, span: Span, fun: &str) -> Diagnostic {
        Diagnostic::new(span, format!("function '{}' is used as a transition handler but might change state here", fun))
        .help("move the goto or raise out of the function into the handler, or call the function from an `entry` block instead")
    }

    pub fn invalid_string_expr_format(&self, span: Span) -> Diagnostic {
        Diagnostic::new(span, "string format placeholders must contain only digits; escape braces by doubling them")
        .help(r#"use numbered placeholders like `format("{0} and {1}", a, b)`; write `{{` and `}}` for literal braces"#)
    }

    pub fn invalid_bind(&self, span: Span, msg: &str) -> Diagnostic {
        Diagnostic::new(span, format!("invalid bind: {}", msg))
    }

    pub fn not_closed(&self, span: Span, msg: &str) -> Diagnostic {
        Diagnostic::new(span, format!("module not closed: {}", msg))
            .help("add machines to the module that implement every interface it creates")
    }

    pub fn no_main(&self, span: Span, msg: &str) -> Diagnostic {
        Diagnostic::new(span, format!("illegal main machine: {}", msg))
            .help("name a machine of the module as main: `test t [main=Main]: { Main, ... };`")
    }

    pub fn invalid_assert_event(&self, span: Span, monitor: &str, event: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!(
                "invalid assert: event '{}' in observes set of '{}' is not in the sends set of the module",
                event, monitor
            ),
        )
        .help("add the event to what the module sends, or remove it from the monitor's observes list")
    }

    pub fn invalid_assert_monitor(&self, span: Span, monitor: &str) -> Diagnostic {
        Diagnostic::new(span, format!("invalid assert: spec monitor '{}' is already attached in the module", monitor))
            .help("attach the monitor once; remove the duplicate `assert`")
    }

    pub fn invalid_hide_event(&self, span: Span, msg: &str) -> Diagnostic {
        Diagnostic::new(span, format!("invalid hide event: {}", msg))
    }

    pub fn invalid_hide_interface(&self, span: Span, msg: &str) -> Diagnostic {
        Diagnostic::new(span, format!("invalid hide interface: {}", msg))
    }

    pub fn invalid_rename(&self, span: Span, msg: &str) -> Diagnostic {
        Diagnostic::new(span, format!("invalid rename: {}", msg))
    }

    pub fn invalid_composition(&self, span: Span, msg: &str) -> Diagnostic {
        Diagnostic::new(span, format!("invalid composition: {}", msg))
    }

    pub fn internal_error(&self, span: Span, msg: &str) -> Diagnostic {
        Diagnostic::new(span, format!("internal compiler error (please report): {}", msg)).help(
            "this is a snappea bug; please report it with the model at https://github.com/crowecawcaw/snappea/issues",
        )
    }

    pub fn two_start_states(&self, span: Span, machine: &str, first: &str, second: &str) -> Diagnostic {
        Diagnostic::new(span, format!("machine '{}' has two start states: '{}' and '{}'", machine, first, second))
            .help("keep `start` on exactly one state")
    }

    pub fn value_out_of_range(&self, span: Span, ty: &str) -> Diagnostic {
        Diagnostic::new(span, format!("value is out of range for type '{}'", ty)).help("use a value that fits the type")
    }

    pub fn null_transition_in_monitor(&self, span: Span, monitor: &str) -> Diagnostic {
        Diagnostic::new(span, format!("cannot transition on null event in spec monitor '{}'", monitor))
            .help("spec monitors only react to observed events; transition on a declared event")
    }

    pub fn illegal_monitor_operation(&self, span: Span, monitor: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!("'$', '$$', 'this', 'new', 'send', 'announce', 'receive', and 'pop' are not allowed in spec monitor '{}'", monitor),
        )
        .help("spec monitors only observe: move this into a machine, and announce what the monitor needs")
    }

    pub fn deferred_event_in_monitor(&self, span: Span, monitor: &str) -> Diagnostic {
        Diagnostic::new(span, format!("events cannot be deferred in spec monitor '{}'", monitor))
            .help("remove the `defer`; `ignore` the event or handle it in this state")
    }

    pub fn not_all_paths_return(&self, span: Span, fun: &str) -> Diagnostic {
        Diagnostic::new(span, format!("not all paths in function '{}' return a value", fun))
            .help("add a `return` at the end of the function, or in every branch")
    }

    pub fn expected_monitor(&self, span: Span, name: &str) -> Diagnostic {
        Diagnostic::new(span, format!("expected a spec monitor; got '{}'", name))
            .help("name a `spec` machine here, not a regular machine")
    }

    pub fn raise_in_non_void(&self, span: Span) -> Diagnostic {
        Diagnostic::new(span, "'raise' is only allowed in functions that do not return a value")
            .help("raise from a function without a return type, or return a value and let the handler raise")
    }

    pub fn change_state_in_non_void(&self, span: Span) -> Diagnostic {
        Diagnostic::new(span, "state changes are only allowed in functions that do not return a value")
            .help("goto from a function without a return type, or return a value and let the handler goto")
    }

    pub fn duplicate_receive_case(&self, span: Span, event: &str) -> Diagnostic {
        Diagnostic::new(span, format!("event '{}' appears twice in receive statement", event))
            .help("handle each event once per receive; merge the two cases")
    }

    pub fn bare_loop_control_flow(&self, span: Span, stmt: &str) -> Diagnostic {
        Diagnostic::new(span, format!("'{}' cannot appear outside a loop body", stmt))
            .help("move it inside a `while` or `foreach` loop, or use `return` to leave the function")
    }

    pub fn exit_function_cannot_take_parameters(&self, span: Span, count: usize) -> Diagnostic {
        Diagnostic::new(span, format!("exit handlers cannot take parameters; function expects {}", count))
            .help("remove the parameters; exit handlers receive no payload")
    }

    pub fn duplicate_bindings(&self, span: Span, name: &str) -> Diagnostic {
        Diagnostic::new(
            span,
            format!("interface or machine '{}' is mentioned or bound multiple times in the module", name),
        )
        .help("mention each machine and interface once per module expression")
    }

    pub fn spec_observes_incomplete_warning(&self, span: Span, event: &str, machine: &str) -> String {
        format!(
            "[Warning:] [{}] event '{}' is not in the observes list of spec monitor '{}'; the handler will never be triggered",
            self.loc(span),
            event,
            machine
        )
    }
}

/// Levenshtein distance over chars.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != *cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
