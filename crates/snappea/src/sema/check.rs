//! Expression and statement type checking (`ExprVisitor`, `StatementVisitor`,
//! `FunctionBodyVisitor` upstream).

use crate::sema::analyzer::{Analyzer, FunSrc};
use crate::sema::collect::parse_int_literal;
use crate::sema::diag::Fatal;
use crate::sema::program::*;
use crate::sema::scope::{Namespace, ScopeId};
use crate::sema::types::{PType, PermissionOrigin, TypeKind};
use crate::syntax::ast;
use crate::syntax::Span;

#[derive(Clone, Copy)]
pub struct Ctx {
    pub fun: FunId,
    pub machine: Option<MachineId>,
    pub scope: ScopeId,
}

fn err_expr(span: Span) -> Expr {
    Expr { kind: ExprKind::Error, ty: PType::Error, span }
}

fn expr(kind: ExprKind, ty: PType, span: Span) -> Expr {
    Expr { kind, ty, span }
}

/// `TypeCheckingUtils.PrintStmtNumArgs`: number of required arguments, or `None` when
/// malformed. Like upstream, indices use wrapping 32-bit arithmetic: an overflowing index
/// counts as the value it wraps to, and a negative one requires no arguments.
pub fn format_num_args(message: &str) -> Option<usize> {
    let m: Vec<char> = message.chars().collect();
    let mut max = 0i32;
    let mut i = 0usize;
    while i < m.len() {
        if m[i] == '{' {
            i += 1;
            if i >= m.len() {
                return None;
            }
            if m[i] == '{' {
                i += 1;
                continue;
            }
            let mut cur = 0i32;
            loop {
                if !m[i].is_ascii_digit() {
                    return None;
                }
                cur = cur.wrapping_mul(10).wrapping_add(m[i] as i32 - '0' as i32);
                i += 1;
                if i >= m.len() || m[i] == '}' {
                    break;
                }
            }
            if i >= m.len() {
                return None;
            }
            max = max.max(cur.wrapping_add(1));
            i += 1;
        } else if m[i] == '}' {
            i += 1;
            if i >= m.len() || m[i] != '}' {
                return None;
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    Some(max as usize)
}

/// Interprets C# escape sequences the way the generated C# source would.
pub fn unescape_csharp(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('a') => out.push('\u{7}'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('v') => out.push('\u{b}'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

impl<'a> Analyzer<'a> {
    fn check_assignable(&mut self, span: Span, expected: &PType, actual: &PType) -> Result<bool, Fatal> {
        if actual.is_error() || expected.is_error() {
            return Ok(true);
        }
        if expected.is_assignable_from(actual, &self.prog) {
            return Ok(true);
        }
        let d = self.e.type_mismatch(span, actual, &[expected], &self.prog);
        self.report(d)?;
        Ok(false)
    }

    fn is_spec_ctx(&self, ctx: Ctx) -> bool {
        ctx.machine.map(|m| self.prog.machines[m.idx()].is_spec()).unwrap_or(false)
    }

    fn mname(&self, ctx: Ctx) -> String {
        ctx.machine.map(|m| self.prog.machines[m.idx()].name.clone()).unwrap_or_default()
    }

    fn validate_payload_types(&mut self, span: Span, payload: &PType, args: &[Expr]) -> Result<(), Fatal> {
        if payload.is_error() {
            return Ok(());
        }
        if args.is_empty() {
            if !payload.is_same_type_as(&PType::Null, &self.prog) {
                let d = self.e.type_mismatch(span, &PType::Null, &[payload], &self.prog);
                self.report(d)?;
            }
        } else if args.len() == 1 {
            self.check_assignable(span, payload, &args[0].ty)?;
        } else if let PType::Tuple(ts) = payload.canonicalize(&self.prog) {
            if ts.len() != args.len() {
                let d = self.e.incorrect_argument_count(span, args.len(), ts.len());
                self.report(d)?;
                return Ok(());
            }
            for (t, a) in ts.iter().zip(args) {
                self.check_assignable(span, t, &a.ty)?;
            }
        } else {
            let d = self.e.incorrect_argument_count(span, args.len(), 1);
            self.report(d)?;
        }
        Ok(())
    }

    fn args_to_payload(&self, mut args: Vec<Expr>, span: Span) -> Option<Expr> {
        match args.len() {
            0 => None,
            1 => Some(args.pop().unwrap()),
            _ => {
                let ty = PType::Tuple(args.iter().map(|a| a.ty.clone()).collect());
                Some(expr(ExprKind::Tuple(args), ty, span))
            }
        }
    }

    fn check_args(&mut self, ctx: Ctx, args: &[ast::Expr]) -> Result<Vec<Expr>, Fatal> {
        let mut v = Vec::new();
        for a in args {
            v.push(self.check_expr(ctx, a)?);
        }
        Ok(v)
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    pub fn check_expr(&mut self, ctx: Ctx, e: &ast::Expr) -> Result<Expr, Fatal> {
        let span = e.inner;
        match &e.kind {
            ast::ExprKind::Name(n) => self.check_name(ctx, n, span),
            ast::ExprKind::FloatLit(text) => match text.parse::<f64>() {
                Ok(v) => Ok(expr(ExprKind::FloatLit(v), PType::Float, span)),
                Err(_) => {
                    let d = self.e.value_out_of_range(span, "float");
                    self.report(d)?;
                    Ok(err_expr(span))
                }
            },
            ast::ExprKind::BoolLit(b) => Ok(expr(ExprKind::BoolLit(*b), PType::Bool, span)),
            ast::ExprKind::IntLit(text) => match parse_int_literal(text) {
                Some(v) => Ok(expr(ExprKind::IntLit(v), PType::Int, span)),
                None => {
                    let d = self.e.value_out_of_range(span, "int");
                    self.report(d)?;
                    Ok(err_expr(span))
                }
            },
            ast::ExprKind::NullLit => Ok(expr(ExprKind::NullLit, PType::Null, span)),
            ast::ExprKind::Nondet | ast::ExprKind::FairNondet => {
                if self.is_spec_ctx(ctx) {
                    let d = self.e.illegal_monitor_operation(span, &self.mname(ctx));
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                self.prog.funs[ctx.fun.idx()].is_nondet = true;
                let k = if matches!(e.kind, ast::ExprKind::Nondet) { ExprKind::Nondet } else { ExprKind::FairNondet };
                Ok(expr(k, PType::Bool, span))
            }
            ast::ExprKind::Halt => {
                let h = self.prog.halt_event;
                Ok(expr(ExprKind::EventRef(h), PType::Event, span))
            }
            ast::ExprKind::This => {
                let Some(m) = ctx.machine else {
                    let d = self.e.misplaced_this(span);
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                if self.prog.machines[m.idx()].is_spec() {
                    let d = self.e.illegal_monitor_operation(span, &self.mname(ctx));
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                Ok(expr(ExprKind::This, PType::Permission(PermissionOrigin::Machine(m)), span))
            }
            ast::ExprKind::UnnamedTuple(fields) => {
                let fs = self.check_args(ctx, fields)?;
                let ty = PType::Tuple(fs.iter().map(|f| f.ty.clone()).collect());
                Ok(expr(ExprKind::Tuple(fs), ty, span))
            }
            ast::ExprKind::NamedTuple(fields) => {
                let mut names: Vec<String> = Vec::new();
                let mut vals = Vec::new();
                let mut tys = Vec::new();
                for (i, (n, v)) in fields.iter().enumerate() {
                    let val = self.check_expr(ctx, v)?;
                    let mut name = n.name.clone();
                    if names.contains(&name) {
                        let d = self.e.duplicate_named_tuple_entry(n.span, &n.name);
                        self.report(d)?;
                        name = format!("{}$dup${}", n.name, i);
                    }
                    names.push(name.clone());
                    tys.push((name, val.ty.clone()));
                    vals.push(val);
                }
                Ok(expr(ExprKind::NamedTuple(vals), PType::NamedTuple(tys), span))
            }
            ast::ExprKind::NamedTupleAccess { base, field } => {
                let b = self.check_expr(ctx, base)?;
                if b.ty.is_error() {
                    return Ok(err_expr(span));
                }
                match b.ty.canonicalize(&self.prog) {
                    PType::NamedTuple(fs) => match fs.iter().position(|(n, _)| *n == field.name) {
                        Some(i) => {
                            let ty = fs[i].1.clone();
                            Ok(expr(ExprKind::NamedTupleAccess { base: Box::new(b), index: i as u32 }, ty, span))
                        }
                        None => {
                            let d = self.e.missing_named_tuple_entry(field.span, &b.ty, &field.name, &self.prog);
                            self.report(d)?;
                            Ok(err_expr(span))
                        }
                    },
                    _ => {
                        let d = self.e.type_mismatch_kinds(
                            b.span,
                            &b.ty,
                            &[TypeKind::NamedTuple, TypeKind::Base],
                            &self.prog,
                        );
                        self.report(d)?;
                        Ok(err_expr(span))
                    }
                }
            }
            ast::ExprKind::TupleAccess { base, index, index_span } => {
                let b = self.check_expr(ctx, base)?;
                if b.ty.is_error() {
                    return Ok(err_expr(span));
                }
                let Some(idx) = parse_int_literal(index) else {
                    let d = self.e.value_out_of_range(span, "int");
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                let PType::Tuple(ts) = b.ty.canonicalize(&self.prog) else {
                    let d =
                        self.e.type_mismatch_kinds(b.span, &b.ty, &[TypeKind::Tuple, TypeKind::NamedTuple], &self.prog);
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                if idx as usize >= ts.len() {
                    let d = self.e.out_of_bounds_tuple_access(*index_span, &b.ty, index, &self.prog);
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                let ty = ts[idx as usize].clone();
                Ok(expr(ExprKind::TupleAccess { base: Box::new(b), index: idx as u32 }, ty, span))
            }
            ast::ExprKind::Index { base, index } => {
                let b = self.check_expr(ctx, base)?;
                let i = self.check_expr(ctx, index)?;
                if b.ty.is_error() || i.ty.is_error() {
                    return Ok(err_expr(span));
                }
                match b.ty.canonicalize(&self.prog) {
                    PType::Seq(el) => {
                        if !self.check_assignable(index.span, &PType::Int, &i.ty)? {
                            return Ok(err_expr(span));
                        }
                        Ok(expr(ExprKind::SeqAccess { base: Box::new(b), index: Box::new(i) }, *el, span))
                    }
                    PType::Map(k, v) => {
                        if !self.check_assignable(index.span, &k, &i.ty)? {
                            return Ok(err_expr(span));
                        }
                        Ok(expr(ExprKind::MapAccess { base: Box::new(b), key: Box::new(i) }, *v, span))
                    }
                    PType::Set(el) => {
                        if !self.check_assignable(index.span, &PType::Int, &i.ty)? {
                            return Ok(err_expr(span));
                        }
                        Ok(expr(ExprKind::SetAccess { base: Box::new(b), index: Box::new(i) }, *el, span))
                    }
                    _ => {
                        let d =
                            self.e.type_mismatch_kinds(b.span, &b.ty, &[TypeKind::Sequence, TypeKind::Map], &self.prog);
                        self.report(d)?;
                        Ok(err_expr(span))
                    }
                }
            }
            ast::ExprKind::Keys(inner) | ast::ExprKind::Values(inner) => {
                let x = self.check_expr(ctx, inner)?;
                if x.ty.is_error() {
                    return Ok(err_expr(span));
                }
                let PType::Map(k, v) = x.ty.canonicalize(&self.prog) else {
                    let d = self.e.type_mismatch_kinds(x.span, &x.ty, &[TypeKind::Map], &self.prog);
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                if matches!(e.kind, ast::ExprKind::Keys(_)) {
                    Ok(expr(ExprKind::Keys(Box::new(x)), PType::seq(*k), span))
                } else {
                    Ok(expr(ExprKind::Values(Box::new(x)), PType::seq(*v), span))
                }
            }
            ast::ExprKind::Sizeof(inner) => {
                let x = self.check_expr(ctx, inner)?;
                if x.ty.is_error() {
                    return Ok(err_expr(span));
                }
                if !matches!(x.ty.canonicalize(&self.prog), PType::Seq(_) | PType::Map(..) | PType::Set(_)) {
                    let d = self.e.type_mismatch_kinds(
                        x.span,
                        &x.ty,
                        &[TypeKind::Map, TypeKind::Sequence, TypeKind::Set],
                        &self.prog,
                    );
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                Ok(expr(ExprKind::Sizeof(Box::new(x)), PType::Int, span))
            }
            ast::ExprKind::Default(t) => {
                let ty = self.resolve_type(t, ctx.scope)?;
                let ty = ty.canonicalize(&self.prog);
                Ok(expr(ExprKind::Default, ty, span))
            }
            ast::ExprKind::Ctor { interface, args } => {
                let a = self.check_args(ctx, args)?;
                let Some(iface) = self.scopes.lookup_interface(ctx.scope, &interface.name) else {
                    let d = self.e.missing_declaration(interface.span, "interface", &interface.name);
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                if self.is_spec_ctx(ctx) {
                    let d = self.e.illegal_monitor_operation(span, &self.mname(ctx));
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                let payload_ty = self.prog.interfaces[iface.idx()].payload.clone();
                self.validate_payload_types(span, &payload_ty, &a)?;
                self.prog.funs[ctx.fun.idx()].can_create = true;
                let payload = self.args_to_payload(a, span).map(Box::new);
                Ok(expr(
                    ExprKind::Ctor { interface: iface, payload },
                    PType::Permission(PermissionOrigin::Interface(iface)),
                    span,
                ))
            }
            ast::ExprKind::FunCall { fun, args } => {
                let a = self.check_args(ctx, args)?;
                if let Some(f) = self.scopes.lookup_fun(ctx.scope, &fun.name) {
                    let params = self.prog.funs[f.idx()].params.clone();
                    if params.len() != a.len() {
                        let d = self.e.incorrect_argument_count(span, a.len(), params.len());
                        self.report(d)?;
                        return Ok(err_expr(span));
                    }
                    for (i, (p, arg)) in params.iter().zip(a.iter()).enumerate() {
                        let pt = self.prog.var_decls[p.idx()].ty.clone();
                        self.check_assignable(args[i].span, &pt, &arg.ty)?;
                    }
                    self.add_callee(ctx.fun, f);
                    let ret = self.prog.funs[f.idx()].ret.clone();
                    return Ok(expr(ExprKind::FunCall { fun: f, args: a }, ret, span));
                }
                if self.scopes.lookup_pure(ctx.scope, &fun.name).is_some() {
                    let d = self.e.internal_error(span, "pure functions are only supported by the PVerifier backend");
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                let d = self.e.missing_declaration(fun.span, "function", &fun.name);
                self.report(d)?;
                Ok(err_expr(span))
            }
            ast::ExprKind::Unary { op, operand } => {
                let x = self.check_expr(ctx, operand)?;
                if x.ty.is_error() {
                    return Ok(err_expr(span));
                }
                match op {
                    ast::UnOp::Neg => {
                        if !PType::Int.is_assignable_from(&x.ty, &self.prog)
                            && !PType::Float.is_assignable_from(&x.ty, &self.prog)
                        {
                            let d =
                                self.e.type_mismatch(operand.span, &x.ty, &[&PType::Int, &PType::Float], &self.prog);
                            self.report(d)?;
                            return Ok(err_expr(span));
                        }
                        let ty = x.ty.clone();
                        Ok(expr(ExprKind::Unary { op: UnOp::Neg, operand: Box::new(x) }, ty, span))
                    }
                    ast::UnOp::Not => {
                        if !PType::Bool.is_assignable_from(&x.ty, &self.prog) {
                            let d = self.e.type_mismatch(operand.span, &x.ty, &[&PType::Bool], &self.prog);
                            self.report(d)?;
                            return Ok(err_expr(span));
                        }
                        Ok(expr(ExprKind::Unary { op: UnOp::Not, operand: Box::new(x) }, PType::Bool, span))
                    }
                }
            }
            ast::ExprKind::Binary { op, op_span, lhs, rhs } => self.check_binary(ctx, *op, *op_span, lhs, rhs, span),
            ast::ExprKind::Cast { kind, expr: inner, ty } => self.check_cast(ctx, *kind, inner, ty, span),
            ast::ExprKind::Choose(inner) => {
                self.prog.funs[ctx.fun.idx()].is_nondet = true;
                let Some(inner) = inner else {
                    return Ok(expr(ExprKind::Choose(None), PType::Bool, span));
                };
                let x = self.check_expr(ctx, inner)?;
                if x.ty.is_error() {
                    return Ok(err_expr(span));
                }
                let ty = match x.ty.canonicalize(&self.prog) {
                    PType::Seq(el) => *el,
                    PType::Set(el) => *el,
                    PType::Map(k, _) => *k,
                    PType::Int => {
                        if let ExprKind::IntLit(v) = x.kind {
                            if v > 10000 {
                                let d = self.e.illegal_choose_sub_expr_value(span, v);
                                self.report(d)?;
                                return Ok(err_expr(span));
                            }
                        }
                        PType::Int
                    }
                    _ => {
                        let d = self.e.illegal_choose_sub_expr_type(span, &x.ty, &self.prog);
                        self.report(d)?;
                        return Ok(err_expr(span));
                    }
                };
                Ok(expr(ExprKind::Choose(Some(Box::new(x))), ty, span))
            }
            ast::ExprKind::Str { raw, raw_span, args } => {
                let base = &raw[1..raw.len() - 1];
                let needed = format_num_args(base);
                let a = self.check_args(ctx, args)?;
                let Some(needed) = needed else {
                    let d = self.e.invalid_string_expr_format(*raw_span);
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                if a.len() != needed {
                    let d = self.e.incorrect_argument_count(span, a.len(), needed);
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                Ok(expr(ExprKind::Str { base: unescape_csharp(base), args: a }, PType::String, span))
            }
            ast::ExprKind::Test { .. }
            | ast::ExprKind::Targets { .. }
            | ast::ExprKind::Flying(_)
            | ast::ExprKind::Sent(_)
            | ast::ExprKind::Quant { .. } => {
                let d = self.e.internal_error(span, "expression is only supported by the PVerifier backend");
                self.report(d)?;
                Ok(err_expr(span))
            }
        }
    }

    fn check_name(&mut self, ctx: Ctx, n: &ast::Ident, span: Span) -> Result<Expr, Fatal> {
        if let Some(v) = self.scopes.lookup_var(ctx.scope, &n.name) {
            let ty = self.prog.var_decls[v.idx()].ty.clone();
            return Ok(expr(ExprKind::Var(v), ty, span));
        }
        if let Some((en, i)) = self.scopes.lookup_enum_elem(ctx.scope, &n.name) {
            return Ok(expr(ExprKind::EnumElem(en, i), PType::Enum(en), span));
        }
        if let Some(ev) = self.scopes.lookup_event(ctx.scope, &n.name) {
            return Ok(expr(ExprKind::EventRef(ev), PType::Event, span));
        }
        if let Some(m) = self.scopes.lookup_machine(ctx.scope, &n.name) {
            if self.prog.machines[m.idx()].is_spec() {
                let d = self.e.internal_error(span, "spec references are only supported by the PVerifier backend");
                self.report(d)?;
                return Ok(err_expr(span));
            }
        }
        let d = self.e.missing_declaration(n.span, "name", &n.name);
        self.report(d)?;
        Ok(err_expr(span))
    }

    fn check_binary(
        &mut self,
        ctx: Ctx,
        op: ast::BinOp,
        op_span: Span,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        span: Span,
    ) -> Result<Expr, Fatal> {
        let l = self.check_expr(ctx, lhs)?;
        let r = self.check_expr(ctx, rhs)?;
        if l.ty.is_error() || r.ty.is_error() {
            return Ok(err_expr(span));
        }
        let prog = &self.prog;
        let both = |t: &PType| t.is_assignable_from(&l.ty, prog) && t.is_assignable_from(&r.ty, prog);
        let bin = |op: BinOp, ty: PType, l: Expr, r: Expr| {
            expr(ExprKind::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }, ty, span)
        };
        match op {
            ast::BinOp::Add => {
                if both(&PType::String) || both(&PType::Int) || both(&PType::Float) {
                    let ty = l.ty.clone();
                    return Ok(bin(BinOp::Add, ty, l, r));
                }
                let d = self.e.binop_type_mismatch(op_span, op.symbol(), &l.ty, &r.ty, &self.prog);
                self.report(d)?;
                Ok(err_expr(span))
            }
            ast::BinOp::Mul
            | ast::BinOp::Div
            | ast::BinOp::Sub
            | ast::BinOp::Lt
            | ast::BinOp::Gt
            | ast::BinOp::Ge
            | ast::BinOp::Le => {
                if both(&PType::Int) || both(&PType::Float) || both(&PType::String) {
                    let (bop, ty) = match op {
                        ast::BinOp::Mul => (BinOp::Mul, l.ty.clone()),
                        ast::BinOp::Div => (BinOp::Div, l.ty.clone()),
                        ast::BinOp::Sub => (BinOp::Sub, l.ty.clone()),
                        ast::BinOp::Lt => (BinOp::Lt, PType::Bool),
                        ast::BinOp::Gt => (BinOp::Gt, PType::Bool),
                        ast::BinOp::Ge => (BinOp::Ge, PType::Bool),
                        _ => (BinOp::Le, PType::Bool),
                    };
                    return Ok(bin(bop, ty, l, r));
                }
                let d = self.e.binop_type_mismatch(op_span, op.symbol(), &l.ty, &r.ty, &self.prog);
                self.report(d)?;
                Ok(err_expr(span))
            }
            ast::BinOp::Mod => {
                if both(&PType::Int) || both(&PType::Float) {
                    let ty = l.ty.clone();
                    return Ok(bin(BinOp::Mod, ty, l, r));
                }
                let d = self.e.incomparable_types(span, &l.ty, &r.ty, &self.prog);
                self.report(d)?;
                Ok(err_expr(span))
            }
            ast::BinOp::In => {
                let elem = match r.ty.canonicalize(&self.prog) {
                    PType::Map(k, _) => *k,
                    PType::Seq(e) => *e,
                    PType::Set(e) => *e,
                    _ => {
                        let d =
                            self.e.type_mismatch_kinds(r.span, &r.ty, &[TypeKind::Map, TypeKind::Sequence], &self.prog);
                        self.report(d)?;
                        return Ok(err_expr(span));
                    }
                };
                if !self.check_assignable(lhs.span, &elem, &l.ty)? {
                    return Ok(err_expr(span));
                }
                Ok(expr(ExprKind::Contains { item: Box::new(l), collection: Box::new(r) }, PType::Bool, span))
            }
            ast::BinOp::Eq | ast::BinOp::Ne => {
                if !l.ty.is_assignable_from(&r.ty, &self.prog) && !r.ty.is_assignable_from(&l.ty, &self.prog) {
                    let d = self.e.incomparable_types(span, &l.ty, &r.ty, &self.prog);
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                let bop = if op == ast::BinOp::Eq { BinOp::Eq } else { BinOp::Neq };
                Ok(bin(bop, PType::Bool, l, r))
            }
            ast::BinOp::And | ast::BinOp::Or | ast::BinOp::Then | ast::BinOp::Iff => {
                if !self.check_assignable(lhs.span, &PType::Bool, &l.ty)? {
                    return Ok(err_expr(span));
                }
                if !self.check_assignable(rhs.span, &PType::Bool, &r.ty)? {
                    return Ok(err_expr(span));
                }
                let bop = match op {
                    ast::BinOp::And => BinOp::And,
                    ast::BinOp::Or => BinOp::Or,
                    ast::BinOp::Then => BinOp::Then,
                    _ => BinOp::Iff,
                };
                Ok(bin(bop, PType::Bool, l, r))
            }
        }
    }

    fn check_cast(
        &mut self,
        ctx: Ctx,
        kind: ast::CastKind,
        inner: &ast::Expr,
        ty: &ast::Type,
        span: Span,
    ) -> Result<Expr, Fatal> {
        let x = self.check_expr(ctx, inner)?;
        let old = x.ty.clone();
        let new = self.resolve_type(ty, ctx.scope)?;
        if old.is_error() || new.is_error() {
            return Ok(err_expr(span));
        }
        let prog = &self.prog;
        match kind {
            ast::CastKind::As => {
                if !new.is_assignable_from(&old, prog) && !old.is_assignable_from(&new, prog) {
                    let numeric = |t: &PType| {
                        t.is_same_type_as(&PType::Int, prog)
                            || t.is_same_type_as(&PType::Float, prog)
                            || t.is_kind(prog, TypeKind::Enum)
                    };
                    let d = if numeric(&old) && numeric(&new) {
                        self.e.as_between_numeric_types(span, &old, &new, prog)
                    } else {
                        self.e.incomparable_types(span, &old, &new, prog)
                    };
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                Ok(expr(ExprKind::Cast(Box::new(x)), new, span))
            }
            ast::CastKind::To => {
                let new_is_perm = matches!(new, PType::Permission(_));
                if !(new_is_perm || new.is_same_type_as(&PType::Int, prog) || new.is_same_type_as(&PType::Float, prog))
                {
                    let d = self.e.illegal_type_in_coerce(span);
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                if old.is_same_type_as(&PType::Int, prog) {
                    if new.is_same_type_as(&PType::Int, prog) {
                        return Ok(x);
                    }
                    // Upstream also lists enum targets here, but the guard above rejects
                    // them: P has no int-to-enum conversion.
                    if new.is_same_type_as(&PType::Float, prog) {
                        return Ok(expr(ExprKind::Coerce(Box::new(x)), new, span));
                    }
                } else if old.is_same_type_as(&PType::Float, prog) {
                    if new.is_same_type_as(&PType::Float, prog) {
                        return Ok(x);
                    }
                    if new.is_same_type_as(&PType::Int, prog) {
                        return Ok(expr(ExprKind::Coerce(Box::new(x)), new, span));
                    }
                } else if old.is_kind(prog, TypeKind::Enum) {
                    if new.is_same_type_as(&old, prog) {
                        return Ok(x);
                    }
                    if new.is_same_type_as(&PType::Int, prog) {
                        return Ok(expr(ExprKind::Coerce(Box::new(x)), new, span));
                    }
                } else if old.is_same_type_as(&PType::Machine, prog) || matches!(old, PType::Permission(_)) {
                    if new.is_same_type_as(&old, prog) {
                        return Ok(x);
                    }
                    if new_is_perm {
                        if old.is_same_type_as(&PType::Machine, prog) {
                            return Ok(expr(ExprKind::Coerce(Box::new(x)), new, span));
                        }
                        if matches!(old, PType::Permission(_)) {
                            let oldp = old.allowed_permissions(prog).unwrap_or_default();
                            let newp = new.allowed_permissions(prog).unwrap_or_default();
                            if let Some(out) = newp.iter().find(|e| !oldp.contains(e)) {
                                let d = self.e.illegal_interface_coerce(
                                    span,
                                    &old.original_repr(prog),
                                    &new.original_repr(prog),
                                    &prog.events[out.idx()].name,
                                );
                                self.report(d)?;
                                return Ok(err_expr(span));
                            }
                            return Ok(expr(ExprKind::Coerce(Box::new(x)), new, span));
                        }
                    }
                }
                let d = self.e.illegal_to(span, &old, &new, prog);
                self.report(d)?;
                Ok(err_expr(span))
            }
        }
    }

    fn check_lvalue(&mut self, ctx: Ctx, e: &ast::Expr) -> Result<Expr, Fatal> {
        let span = e.span;
        match &e.kind {
            ast::ExprKind::Name(n) => match self.scopes.lookup_var(ctx.scope, &n.name) {
                Some(v) => {
                    let vd = &self.prog.var_decls[v.idx()];
                    if vd.role == VarRole::GlobalParam {
                        return Err(self.e.modify_global_param(span, &vd.name, vd.span));
                    }
                    let ty = vd.ty.clone();
                    Ok(expr(ExprKind::Var(v), ty, span))
                }
                None => {
                    let d = self.e.missing_declaration(span, "variable", &n.name);
                    self.report(d)?;
                    Ok(err_expr(span))
                }
            },
            ast::ExprKind::NamedTupleAccess { base, field } => {
                let b = self.check_lvalue(ctx, base)?;
                if b.ty.is_error() {
                    return Ok(err_expr(span));
                }
                let PType::NamedTuple(fs) = b.ty.canonicalize(&self.prog) else {
                    let d = self.e.type_mismatch_kinds(b.span, &b.ty, &[TypeKind::NamedTuple], &self.prog);
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                let Some(i) = fs.iter().position(|(n, _)| *n == field.name) else {
                    let d = self.e.missing_named_tuple_entry(field.span, &b.ty, &field.name, &self.prog);
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                let ty = fs[i].1.clone();
                Ok(expr(ExprKind::NamedTupleAccess { base: Box::new(b), index: i as u32 }, ty, span))
            }
            ast::ExprKind::TupleAccess { base, index, index_span } => {
                let b = self.check_lvalue(ctx, base)?;
                if b.ty.is_error() {
                    return Ok(err_expr(span));
                }
                let PType::Tuple(ts) = b.ty.canonicalize(&self.prog) else {
                    let d = self.e.type_mismatch_kinds(b.span, &b.ty, &[TypeKind::Tuple], &self.prog);
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                let Some(idx) = parse_int_literal(index) else {
                    let d = self.e.value_out_of_range(*index_span, "int");
                    self.report(d)?;
                    return Ok(err_expr(span));
                };
                if idx as usize >= ts.len() {
                    let d = self.e.out_of_bounds_tuple_access(*index_span, &b.ty, index, &self.prog);
                    self.report(d)?;
                    return Ok(err_expr(span));
                }
                let ty = ts[idx as usize].clone();
                Ok(expr(ExprKind::TupleAccess { base: Box::new(b), index: idx as u32 }, ty, span))
            }
            ast::ExprKind::Index { base, index } => {
                let b = self.check_lvalue(ctx, base)?;
                let i = self.check_expr(ctx, index)?;
                if b.ty.is_error() || i.ty.is_error() {
                    return Ok(err_expr(span));
                }
                match b.ty.canonicalize(&self.prog) {
                    PType::Map(k, v) => {
                        if !self.check_assignable(index.span, &k, &i.ty)? {
                            return Ok(err_expr(span));
                        }
                        Ok(expr(ExprKind::MapAccess { base: Box::new(b), key: Box::new(i) }, *v, span))
                    }
                    PType::Seq(el) => {
                        if !self.check_assignable(index.span, &PType::Int, &i.ty)? {
                            return Ok(err_expr(span));
                        }
                        Ok(expr(ExprKind::SeqAccess { base: Box::new(b), index: Box::new(i) }, *el, span))
                    }
                    _ => {
                        let d =
                            self.e.type_mismatch_kinds(b.span, &b.ty, &[TypeKind::Sequence, TypeKind::Map], &self.prog);
                        self.report(d)?;
                        Ok(err_expr(span))
                    }
                }
            }
            _ => Err(self.e.internal_error(span, "invalid lvalue")),
        }
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn is_definitely_null_event(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::NullLit => true,
            ExprKind::EventRef(ev) => self.prog.events[ev.idx()].is_null(),
            _ => false,
        }
    }

    /// Checks an event expression used by raise/send/announce, reporting null and non-event values.
    fn check_event_expr(&mut self, ctx: Ctx, e: &ast::Expr) -> Result<Expr, Fatal> {
        let x = self.check_expr(ctx, e)?;
        if x.ty.is_error() {
            return Ok(x);
        }
        if self.is_definitely_null_event(&x) {
            let d = self.e.emitted_null_event(x.span);
            self.report(d)?;
        } else if !PType::Event.is_assignable_from(&x.ty, &self.prog) {
            let d = self.e.type_mismatch(e.span, &x.ty, &[&PType::Event], &self.prog);
            self.report(d)?;
        }
        Ok(x)
    }

    fn event_payload_type(&self, ev: &Expr) -> Option<PType> {
        match &ev.kind {
            ExprKind::EventRef(id) => Some(self.prog.events[id.idx()].payload.clone()),
            _ => None,
        }
    }

    fn stmt(kind: StmtKind, span: Span) -> Stmt {
        Stmt { kind, span }
    }

    fn check_bool_cond(&mut self, ctx: Ctx, c: &ast::Expr) -> Result<Expr, Fatal> {
        let x = self.check_expr(ctx, c)?;
        if !x.ty.is_error() && !PType::Bool.is_same_type_as(&x.ty, &self.prog) {
            let d = self.e.type_mismatch(c.span, &x.ty, &[&PType::Bool], &self.prog);
            self.report(d)?;
        }
        Ok(x)
    }

    /// Checks a statement. Only the statements that contain statements are
    /// checked in this frame, one of which is on the stack per level of
    /// statement nesting; the others go to [`Self::check_simple_stmt`].
    pub fn check_stmt(&mut self, ctx: Ctx, s: &'a ast::Stmt) -> Result<Stmt, Fatal> {
        let span = s.span;
        match &s.kind {
            ast::StmtKind::Compound(stmts) => {
                let mut v = Vec::new();
                for st in stmts {
                    v.push(self.check_stmt(ctx, st)?);
                }
                Ok(Self::stmt(StmtKind::Block(Block { stmts: v, span }), span))
            }
            ast::StmtKind::While { cond, body } => {
                let c = self.check_bool_cond(ctx, cond)?;
                let b = self.check_stmt(ctx, body)?;
                Ok(Self::stmt(StmtKind::While { cond: c, body: Box::new(b) }, span))
            }
            ast::StmtKind::If { cond, then_branch, else_branch } => {
                let c = self.check_bool_cond(ctx, cond)?;
                let t = self.check_stmt(ctx, then_branch)?;
                let el = match else_branch {
                    None => Self::stmt(StmtKind::Nop, span),
                    Some(e) => self.check_stmt(ctx, e)?,
                };
                Ok(Self::stmt(StmtKind::If { cond: c, then_branch: Box::new(t), else_branch: Box::new(el) }, span))
            }
            ast::StmtKind::Foreach { item, collection, invariants, body } => {
                self.check_foreach(ctx, item, collection, invariants, body, span)
            }
            _ => self.check_simple_stmt(ctx, s),
        }
    }

    #[inline(never)]
    fn check_foreach(
        &mut self,
        ctx: Ctx,
        item: &'a ast::Ident,
        collection: &'a ast::Expr,
        invariants: &'a [ast::Expr],
        body: &'a ast::Stmt,
        span: Span,
    ) -> Result<Stmt, Fatal> {
        let Some(var) = self.scopes.lookup_var(ctx.scope, &item.name) else {
            let d = self.e.missing_declaration(item.span, "foreach iterator variable", &item.name);
            self.report(d)?;
            self.check_expr(ctx, collection)?;
            self.check_stmt(ctx, body)?;
            for inv in invariants {
                self.check_expr(ctx, inv)?;
            }
            return Ok(Self::stmt(StmtKind::Nop, span));
        };
        let coll = self.check_expr(ctx, collection)?;
        let expected = if coll.ty.is_error() {
            PType::Error
        } else {
            match coll.ty.canonicalize(&self.prog) {
                PType::Set(el) => *el,
                PType::Seq(el) => *el,
                _ => {
                    let d = self.e.type_mismatch_kinds(
                        coll.span,
                        &coll.ty,
                        &[TypeKind::Set, TypeKind::Sequence],
                        &self.prog,
                    );
                    self.report(d)?;
                    PType::Error
                }
            }
        };
        let item_ty = self.prog.var_decls[var.idx()].ty.clone();
        if !expected.is_error()
            && !item_ty.is_error()
            && (!expected.is_same_type_as(&item_ty, &self.prog) || !expected.is_assignable_from(&item_ty, &self.prog))
        {
            let d = self.e.type_mismatch(item.span, &item_ty, &[&expected], &self.prog);
            self.report(d)?;
        }
        let b = self.check_stmt(ctx, body)?;
        for inv in invariants {
            self.check_expr(ctx, inv)?;
        }
        Ok(Self::stmt(StmtKind::Foreach { item: var, collection: coll, body: Box::new(b) }, span))
    }

    #[inline(never)]
    fn check_simple_stmt(&mut self, ctx: Ctx, s: &'a ast::Stmt) -> Result<Stmt, Fatal> {
        let span = s.span;
        let ret_is_void = self.prog.funs[ctx.fun.idx()].ret.is_same_type_as(&PType::Null, &self.prog);
        match &s.kind {
            ast::StmtKind::Assert { cond, message } | ast::StmtKind::Assume { cond, message } => {
                let c = self.check_bool_cond(ctx, cond)?;
                let loc = self.sm.location(span).to_string().replace('\\', "\\\\");
                let mut msg = expr(ExprKind::Str { base: loc, args: Vec::new() }, PType::String, span);
                if let Some(m) = message {
                    let mx = self.check_expr(ctx, m)?;
                    if !mx.ty.is_error() && !mx.ty.is_same_type_as(&PType::String, &self.prog) {
                        let d = self.e.type_mismatch(m.span, &mx.ty, &[&PType::String], &self.prog);
                        self.report(d)?;
                    }
                    let msp = mx.span;
                    msg = expr(ExprKind::Str { base: "{0} {1}".into(), args: vec![msg, mx] }, PType::String, msp);
                }
                if matches!(s.kind, ast::StmtKind::Assert { .. }) {
                    Ok(Self::stmt(StmtKind::Assert { cond: c, message: msg }, span))
                } else {
                    Ok(Self::stmt(StmtKind::Assume { cond: c, message: msg }, span))
                }
            }
            ast::StmtKind::Print(m) => {
                let mx = self.check_expr(ctx, m)?;
                if !mx.ty.is_error() && !mx.ty.is_same_type_as(&PType::String, &self.prog) {
                    let d = self.e.type_mismatch(m.span, &mx.ty, &[&PType::String], &self.prog);
                    self.report(d)?;
                }
                Ok(Self::stmt(StmtKind::Print(mx), span))
            }
            ast::StmtKind::Return(v) => {
                let rv = match v {
                    None => None,
                    Some(e) => Some(self.check_expr(ctx, e)?),
                };
                let rt = rv.as_ref().map(|e| e.ty.clone()).unwrap_or(PType::Null);
                let expected = self.prog.funs[ctx.fun.idx()].ret.clone();
                self.check_assignable(span, &expected, &rt)?;
                Ok(Self::stmt(StmtKind::Return(rv), span))
            }
            ast::StmtKind::Break => Ok(Self::stmt(StmtKind::Break, span)),
            ast::StmtKind::Continue => Ok(Self::stmt(StmtKind::Continue, span)),
            ast::StmtKind::Assign { lhs, rhs } => {
                let l = self.check_lvalue(ctx, lhs)?;
                let r = self.check_expr(ctx, rhs)?;
                self.check_assignable(rhs.span, &l.ty, &r.ty)?;
                Ok(Self::stmt(StmtKind::Assign { lhs: l, rhs: r }, span))
            }
            ast::StmtKind::Add { lhs, value } => {
                let l = self.check_lvalue(ctx, lhs)?;
                let v = self.check_expr(ctx, value)?;
                if l.ty.is_error() {
                    return Ok(Self::stmt(StmtKind::Add { lhs: l, value: v }, span));
                }
                match l.ty.canonicalize(&self.prog) {
                    PType::Set(el) => {
                        self.check_assignable(value.span, &el, &v.ty)?;
                    }
                    _ => {
                        let d = self.e.type_mismatch_kinds(l.span, &l.ty, &[TypeKind::Set], &self.prog);
                        self.report(d)?;
                    }
                }
                Ok(Self::stmt(StmtKind::Add { lhs: l, value: v }, span))
            }
            ast::StmtKind::Insert { lhs, index, value } => {
                let l = self.check_lvalue(ctx, lhs)?;
                let i = self.check_expr(ctx, index)?;
                let v = self.check_expr(ctx, value)?;
                if l.ty.is_error() {
                    return Ok(Self::stmt(StmtKind::Insert { lhs: l, index: i, value: v }, span));
                }
                let (kt, vt) = match l.ty.canonicalize(&self.prog) {
                    PType::Seq(el) => (PType::Int, *el),
                    PType::Map(k, val) => (*k, *val),
                    _ => {
                        let d =
                            self.e.type_mismatch_kinds(l.span, &l.ty, &[TypeKind::Sequence, TypeKind::Map], &self.prog);
                        self.report(d)?;
                        return Ok(Self::stmt(StmtKind::Insert { lhs: l, index: i, value: v }, span));
                    }
                };
                self.check_assignable(index.span, &kt, &i.ty)?;
                self.check_assignable(value.span, &vt, &v.ty)?;
                Ok(Self::stmt(StmtKind::Insert { lhs: l, index: i, value: v }, span))
            }
            ast::StmtKind::Remove { lhs, value } => {
                let l = self.check_lvalue(ctx, lhs)?;
                let v = self.check_expr(ctx, value)?;
                if l.ty.is_error() {
                    return Ok(Self::stmt(StmtKind::Remove { lhs: l, value: v }, span));
                }
                match l.ty.canonicalize(&self.prog) {
                    PType::Seq(_) => {
                        self.check_assignable(value.span, &PType::Int, &v.ty)?;
                    }
                    PType::Map(k, _) => {
                        self.check_assignable(value.span, &k, &v.ty)?;
                    }
                    PType::Set(el) => {
                        self.check_assignable(value.span, &el, &v.ty)?;
                    }
                    _ => {
                        let d =
                            self.e.type_mismatch_kinds(l.span, &l.ty, &[TypeKind::Sequence, TypeKind::Map], &self.prog);
                        self.report(d)?;
                    }
                }
                Ok(Self::stmt(StmtKind::Remove { lhs: l, value: v }, span))
            }
            ast::StmtKind::Ctor { interface, args } => {
                let a = self.check_args(ctx, args)?;
                let Some(iface) = self.scopes.lookup_interface(ctx.scope, &interface.name) else {
                    let d = self.e.missing_declaration(interface.span, "interface", &interface.name);
                    self.report(d)?;
                    return Ok(Self::stmt(StmtKind::Nop, span));
                };
                let pt = self.prog.interfaces[iface.idx()].payload.clone();
                self.validate_payload_types(span, &pt, &a)?;
                self.prog.funs[ctx.fun.idx()].can_create = true;
                let payload = self.args_to_payload(a, span);
                Ok(Self::stmt(StmtKind::Ctor { interface: iface, payload }, span))
            }
            ast::StmtKind::FunCall { fun, args } => {
                let a = self.check_args(ctx, args)?;
                let Some(f) = self.scopes.lookup_fun(ctx.scope, &fun.name) else {
                    let d = self.e.missing_declaration(fun.span, "function", &fun.name);
                    self.report(d)?;
                    return Ok(Self::stmt(StmtKind::Nop, span));
                };
                let params = self.prog.funs[f.idx()].params.clone();
                if params.len() != a.len() {
                    let argspan = if args.is_empty() { span } else { args[0].span.to(args[args.len() - 1].span) };
                    let d = self.e.incorrect_argument_count(argspan, a.len(), params.len());
                    self.report(d)?;
                    return Ok(Self::stmt(StmtKind::FunCall { fun: f, args: a }, span));
                }
                for (p, arg) in params.iter().zip(a.iter()) {
                    let pt = self.prog.var_decls[p.idx()].ty.clone();
                    self.check_assignable(span, &pt, &arg.ty)?;
                }
                self.add_callee(ctx.fun, f);
                Ok(Self::stmt(StmtKind::FunCall { fun: f, args: a }, span))
            }
            ast::StmtKind::Raise { event, args } => {
                if !ret_is_void {
                    let d = self.e.raise_in_non_void(span);
                    self.report(d)?;
                    self.check_expr(ctx, event)?;
                    self.check_args(ctx, args)?;
                    return Ok(Self::stmt(StmtKind::Nop, span));
                }
                let ev = self.check_expr(ctx, event)?;
                if ev.ty.is_error() {
                    self.prog.funs[ctx.fun.idx()].can_raise = true;
                    self.check_args(ctx, args)?;
                    return Ok(Self::stmt(StmtKind::Raise { event: ev, payload: None }, span));
                }
                if self.is_definitely_null_event(&ev) {
                    let d = self.e.emitted_null_event(ev.span);
                    self.report(d)?;
                } else if !PType::Event.is_assignable_from(&ev.ty, &self.prog) {
                    let d = self.e.type_mismatch(event.span, &ev.ty, &[&PType::Event], &self.prog);
                    self.report(d)?;
                }
                self.prog.funs[ctx.fun.idx()].can_raise = true;
                let a = self.check_args(ctx, args)?;
                if let Some(pt) = self.event_payload_type(&ev) {
                    self.validate_payload_types(span, &pt, &a)?;
                }
                let payload = self.args_to_payload(a, span);
                Ok(Self::stmt(StmtKind::Raise { event: ev, payload }, span))
            }
            ast::StmtKind::Send { target, event, args } => {
                if self.is_spec_ctx(ctx) {
                    let d = self.e.illegal_monitor_operation(span, &self.mname(ctx));
                    self.report(d)?;
                    self.check_expr(ctx, target)?;
                    self.check_expr(ctx, event)?;
                    self.check_args(ctx, args)?;
                    return Ok(Self::stmt(StmtKind::Nop, span));
                }
                let t = self.check_expr(ctx, target)?;
                if !t.ty.is_error() && !PType::Machine.is_assignable_from(&t.ty, &self.prog) {
                    let d = self.e.type_mismatch(target.span, &t.ty, &[&PType::Machine], &self.prog);
                    self.report(d)?;
                }
                let ev = self.check_event_expr(ctx, event)?;
                let a = self.check_args(ctx, args)?;
                if let Some(pt) = self.event_payload_type(&ev) {
                    self.validate_payload_types(span, &pt, &a)?;
                }
                self.prog.funs[ctx.fun.idx()].can_send = true;
                let payload = self.args_to_payload(a, span);
                Ok(Self::stmt(StmtKind::Send { target: t, event: ev, payload }, span))
            }
            ast::StmtKind::Announce { event, args } => {
                if self.is_spec_ctx(ctx) {
                    let d = self.e.illegal_monitor_operation(span, &self.mname(ctx));
                    self.report(d)?;
                    self.check_expr(ctx, event)?;
                    self.check_args(ctx, args)?;
                    return Ok(Self::stmt(StmtKind::Nop, span));
                }
                let ev = self.check_event_expr(ctx, event)?;
                self.prog.funs[ctx.fun.idx()].can_send = true;
                let a = self.check_args(ctx, args)?;
                if let Some(pt) = self.event_payload_type(&ev) {
                    self.validate_payload_types(span, &pt, &a)?;
                }
                let payload = self.args_to_payload(a, span);
                Ok(Self::stmt(StmtKind::Announce { event: ev, payload }, span))
            }
            ast::StmtKind::Goto { state, args } => {
                if !ret_is_void {
                    let d = self.e.change_state_in_non_void(span);
                    self.report(d)?;
                    self.check_args(ctx, args)?;
                    return Ok(Self::stmt(StmtKind::Nop, span));
                }
                let st = ctx
                    .machine
                    .and_then(|m| self.scopes.get(self.machine_scopes[m.idx()]).states.get(&state.name).copied());
                let Some(st) = st else {
                    let d = self.e.missing_declaration(state.span, "state", &state.name);
                    self.report(d)?;
                    self.check_args(ctx, args)?;
                    return Ok(Self::stmt(StmtKind::Nop, span));
                };
                let entry = self.prog.states[st.idx()].entry;
                let entry_params: Vec<VarId> =
                    entry.map(|f| self.prog.funs[f.idx()].params.clone()).unwrap_or_default();
                let expected_ty =
                    entry_params.first().map(|v| self.prog.var_decls[v.idx()].ty.clone()).unwrap_or(PType::Null);
                let a = self.check_args(ctx, args)?;
                if a.len() != entry_params.len() {
                    let d = self.e.incorrect_argument_count(span, a.len(), entry_params.len());
                    self.report(d)?;
                    return Ok(Self::stmt(StmtKind::Goto { state: st, payload: None }, span));
                }
                let payload = self.args_to_payload(a, span);
                let pt = payload.as_ref().map(|p| p.ty.clone()).unwrap_or(PType::Null);
                self.check_assignable(span, &expected_ty, &pt)?;
                self.prog.funs[ctx.fun.idx()].can_change_state = true;
                Ok(Self::stmt(StmtKind::Goto { state: st, payload }, span))
            }
            ast::StmtKind::Receive { cases } => self.check_receive(ctx, cases, span),
            ast::StmtKind::NoStmt => Ok(Self::stmt(StmtKind::Nop, span)),
            ast::StmtKind::Compound(_)
            | ast::StmtKind::While { .. }
            | ast::StmtKind::If { .. }
            | ast::StmtKind::Foreach { .. } => unreachable!("checked by check_stmt"),
        }
    }

    /// Creates and checks the handler function of a receive case, nested in `ctx.fun`.
    fn receive_handler(&mut self, ctx: Ctx, case: &'a ast::RecvCase) -> Result<FunId, Fatal> {
        let hs = self.scopes.child(ctx.scope);
        let hid = self.new_fun("", ctx.machine, hs, FunSrc::Receive, case.handler.span);
        self.fun_root[hid.idx()] = self.fun_root[ctx.fun.idx()];
        self.prog.funs[hid.idx()].parent = Some(ctx.fun);
        self.prog.funs[hid.idx()].role.receive = true;
        if let Some(p) = &case.handler.param {
            self.check_conflict_var(hs, &p.name.name, p.name.span)?;
            let slot = self.alloc_slot(hid);
            let t = self.resolve_type(&p.ty, hs)?;
            let v = self.new_var(&p.name.name, VarRole::Param, t, slot, p.name.span);
            let s = self.scopes.get_mut(hs);
            s.variables.insert(p.name.name.clone(), v);
            s.var_list.push(v);
            self.prog.funs[hid.idx()].params.push(v);
        }
        self.populate_body(hid, &case.handler.body)?;
        Ok(hid)
    }

    fn check_receive(&mut self, ctx: Ctx, cases: &'a [ast::RecvCase], span: Span) -> Result<Stmt, Fatal> {
        let is_spec = self.is_spec_ctx(ctx);
        if is_spec {
            let d = self.e.illegal_monitor_operation(span, &self.mname(ctx));
            self.report(d)?;
            // Like upstream: still surface undeclared events and handler-body
            // errors, with one handler per case.
            for case in cases {
                for ev in &case.events {
                    if self.scopes.lookup_event(ctx.scope, &ev.name).is_none() {
                        let d = self.e.missing_declaration(ev.span, "event", &ev.name);
                        self.report(d)?;
                    }
                }
                self.receive_handler(ctx, case)?;
            }
            return Ok(Self::stmt(StmtKind::Nop, span));
        }
        let mut out: Vec<(EventId, FunId)> = Vec::new();
        for case in cases {
            for ev in &case.events {
                // one handler function per event id, as upstream
                let hid = self.receive_handler(ctx, case)?;
                let Some(eid) = self.scopes.lookup_event(ctx.scope, &ev.name) else {
                    let d = self.e.missing_declaration(ev.span, "event", &ev.name);
                    self.report(d)?;
                    continue;
                };
                if out.iter().any(|(e, _)| *e == eid) {
                    let d = self.e.duplicate_receive_case(ev.span, &ev.name);
                    self.report(d)?;
                    continue;
                }
                let expected = self.prog.funs[hid.idx()]
                    .params
                    .first()
                    .map(|v| self.prog.var_decls[v.idx()].ty.clone())
                    .unwrap_or(PType::Null);
                let payload = self.prog.events[eid.idx()].payload.clone();
                self.check_assignable(case.handler.span, &expected, &payload)?;
                let h = self.prog.funs[hid.idx()].clone();
                if h.can_change_state {
                    if !self.prog.funs[ctx.fun.idx()].ret.is_same_type_as(&PType::Null, &self.prog) {
                        let d = self.e.change_state_in_non_void(span);
                        self.report(d)?;
                    } else {
                        self.prog.funs[ctx.fun.idx()].can_change_state = true;
                    }
                }
                if h.can_raise {
                    if !self.prog.funs[ctx.fun.idx()].ret.is_same_type_as(&PType::Null, &self.prog) {
                        let d = self.e.raise_in_non_void(span);
                        self.report(d)?;
                    } else {
                        self.prog.funs[ctx.fun.idx()].can_raise = true;
                    }
                }
                for c in h.callees {
                    self.add_callee(ctx.fun, c);
                }
                out.push((eid, hid));
            }
        }
        self.prog.funs[ctx.fun.idx()].can_receive = true;
        Ok(Self::stmt(StmtKind::Receive { cases: out }, span))
    }

    fn check_conflict_var(&self, scope: ScopeId, name: &str, span: Span) -> Result<(), Fatal> {
        if let Some(existing) = self.scopes.get(scope).conflict(name, &[Namespace::Variables]) {
            let ename = self.prog.decl_name(existing);
            let espan = self.prog.decl_span(existing);
            return Err(self.e.duplicate_declaration(span, name, &ename, espan));
        }
        Ok(())
    }

    /// Checks that no local/field along the scope chain shadows a global parameter
    /// (`Scope.ValidateGlobalParamsUnique`).
    fn validate_global_params_unique(&self, scope: ScopeId) -> Result<(), Fatal> {
        let mut seen: Vec<VarId> = Vec::new();
        let mut cur = Some(scope);
        while let Some(s) = cur {
            let sc = self.scopes.get(s);
            for v in &sc.var_list {
                let vd = &self.prog.var_decls[v.idx()];
                if let Some(prev) = seen.iter().find(|p| self.prog.var_decls[p.idx()].name == vd.name) {
                    if vd.role == VarRole::GlobalParam {
                        let pd = &self.prog.var_decls[prev.idx()];
                        return Err(self.e.duplicate_declaration(pd.span, &pd.name, &vd.name, vd.span));
                    }
                }
                seen.push(*v);
            }
            cur = sc.parent;
        }
        Ok(())
    }

    /// `FunctionBodyVisitor.PopulateMethod` for a function with a body.
    pub fn populate_body(&mut self, fun: FunId, body: &'a ast::FunBody) -> Result<(), Fatal> {
        let fs = self.fun_scopes[fun.idx()];
        for v in &body.vars {
            let t = self.resolve_type(&v.ty, fs)?;
            for n in &v.names {
                self.check_conflict_var(fs, &n.name, n.span)?;
                let slot = self.alloc_slot(fun);
                let vid = self.new_var(&n.name, VarRole::Local, t.clone(), slot, n.span);
                let s = self.scopes.get_mut(fs);
                s.variables.insert(n.name.clone(), vid);
                s.var_list.push(vid);
                self.prog.funs[fun.idx()].locals.push(vid);
            }
        }
        self.validate_global_params_unique(fs)?;
        let ctx = Ctx { fun, machine: self.prog.funs[fun.idx()].owner, scope: fs };
        let mut stmts = Vec::new();
        for s in &body.stmts {
            stmts.push(self.check_stmt(ctx, s)?);
        }
        self.prog.funs[fun.idx()].body = Some(Block { stmts, span: body.span });
        Ok(())
    }

    /// Populates the body of a named or anonymous function from its AST.
    pub fn populate_fun(&mut self, fun: FunId) -> Result<(), Fatal> {
        match self.fun_src[fun.idx()] {
            FunSrc::Named(f) => match &f.body {
                Some(b) => self.populate_body(fun, b),
                None => Ok(()),
            },
            FunSrc::Anon(h) => self.populate_body(fun, &h.body),
            FunSrc::Receive | FunSrc::None => Ok(()),
        }
    }
}
