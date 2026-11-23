//! The names upstream's C# backend (`PCheckerNameManager`) gives declarations.
//!
//! Upstream's checker reports machines by their generated class name and state actions by
//! their generated method name. Every declaration other than events, states and interfaces
//! draws its name from one shared pool, uniquified with `_1`, `_2`, ... in the order code
//! generation first asks for it. A machine therefore loses its plain name when an earlier
//! variable took it (a field `client` generated before machine `client` makes the class
//! `client_1`), and anonymous handlers are numbered `Anon`, `Anon_1`, ... program-wide.

use std::collections::{HashMap, HashSet};

use crate::sema::program::{Block, EnumId, Expr, ExprKind, FunId, MachineId, Program, Stmt, StmtKind, VarId};

/// C# keywords upstream prefixes with `P_`.
const RESERVED: &[&str] = &[
    "bool",
    "byte",
    "sbyte",
    "short",
    "ushort",
    "int",
    "uint",
    "long",
    "ulong",
    "double",
    "float",
    "decimal",
    "string",
    "char",
    "void",
    "object",
    "typeof",
    "sizeof",
    "null",
    "true",
    "false",
    "if",
    "else",
    "while",
    "for",
    "foreach",
    "do",
    "switch",
    "case",
    "default",
    "lock",
    "try",
    "throw",
    "catch",
    "finally",
    "goto",
    "break",
    "continue",
    "return",
    "public",
    "private",
    "internal",
    "protected",
    "static",
    "readonly",
    "sealed",
    "const",
    "fixed",
    "stackalloc",
    "volatile",
    "new",
    "override",
    "abstract",
    "virtual",
    "event",
    "extern",
    "ref",
    "out",
    "in",
    "is",
    "as",
    "params",
    "__arglist",
    "__makeref",
    "__reftype",
    "__refvalue",
    "this",
    "base",
    "namespace",
    "using",
    "class",
    "struct",
    "interface",
    "enum",
    "delegate",
    "checked",
    "unchecked",
    "unsafe",
    "operator",
    "implicit",
    "explicit",
];

/// Prefix of compiler temporaries (`GetTemporaryName`).
const TEMP_PREFIX: &str = "PGEN_";

/// Generated names, indexed by `MachineId` and `FunId`.
pub(crate) struct CsNames {
    pub machines: Vec<String>,
    /// Empty for functions that get no method (receive cases, foreign functions never called).
    pub funs: Vec<String>,
}

/// `NameManagerBase.UniquifyName`.
#[derive(Default)]
struct Pool {
    usages: HashMap<String, u32>,
}

impl Pool {
    fn uniquify(&mut self, base: &str) -> String {
        let mut name = base.to_string();
        while let Some(u) = self.usages.get_mut(&name) {
            let n = *u;
            *u += 1;
            name = format!("{base}_{n}");
        }
        self.usages.insert(name.clone(), 1);
        name
    }

    /// `PCheckerNameManager.ComputeNameForDecl` for a non-event, non-state declaration.
    fn decl(&mut self, name: &str) -> String {
        let name = if name.is_empty() { "Anon" } else { name };
        let name = if let Some(rest) = name.strip_prefix('$') {
            format!("TMP_{rest}")
        } else if RESERVED.contains(&name) {
            format!("P_{name}")
        } else {
            name.to_string()
        };
        self.uniquify(&name)
    }
}

struct Walker<'p> {
    prog: &'p Program,
    pool: Pool,
    machines: Vec<String>,
    funs: Vec<String>,
    vars: HashSet<VarId>,
    elems: HashSet<(EnumId, u32)>,
}

impl Walker<'_> {
    fn var(&mut self, v: VarId) {
        if self.vars.insert(v) {
            self.pool.decl(&self.prog.var(v).name);
        }
    }

    fn fun_name(&mut self, f: FunId) {
        if self.funs[f.idx()].is_empty() {
            self.funs[f.idx()] = self.pool.decl(&self.prog.fun(f).name);
        }
    }

    fn machine(&mut self, m: MachineId) {
        if self.machines[m.idx()].is_empty() {
            self.machines[m.idx()] = self.pool.decl(&self.prog.machine(m).name);
        }
    }

    /// `WriteFunction` (with `WriteNamedFunctionWrapper` and `WriteFunctionBody`).
    fn function(&mut self, f: FunId) {
        let d = self.prog.fun(f);
        if d.role.foreign {
            return;
        }
        let anon = d.name.is_empty();
        self.fun_name(f);
        if !anon {
            for &p in &d.params {
                self.var(p);
            }
        }
        if let Some(m) = d.owner {
            self.machine(m);
        }
        if anon {
            for &p in &d.params {
                self.var(p);
            }
        }
        self.locals_and_body(f);
    }

    fn locals_and_body(&mut self, f: FunId) {
        let d = self.prog.fun(f);
        for &l in &d.locals {
            self.var(l);
        }
        if let Some(b) = &d.body {
            self.block(b);
        }
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Block(b) => self.block(b),
            StmtKind::Assert { cond, message } | StmtKind::Assume { cond, message } => {
                self.expr(cond);
                self.expr(message);
            }
            StmtKind::Print(e) => self.expr(e),
            StmtKind::Return(e) => self.opt(e.as_ref()),
            StmtKind::Break | StmtKind::Continue | StmtKind::Nop => {}
            StmtKind::Assign { lhs, rhs } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            StmtKind::Insert { lhs, index, value } => {
                self.expr(lhs);
                self.expr(index);
                self.expr(value);
            }
            StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
                self.expr(lhs);
                self.expr(value);
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                self.stmt(body);
            }
            StmtKind::Foreach { item, collection, body } => {
                self.var(*item);
                self.expr(collection);
                self.stmt(body);
            }
            StmtKind::If { cond, then_branch, else_branch } => {
                self.expr(cond);
                self.stmt(then_branch);
                self.stmt(else_branch);
            }
            StmtKind::Ctor { payload, .. } => self.opt(payload.as_ref()),
            StmtKind::FunCall { fun, args } => {
                self.fun_name(*fun);
                self.exprs(args);
            }
            StmtKind::Raise { event, payload } | StmtKind::Announce { event, payload } => {
                self.expr(event);
                self.opt(payload.as_ref());
            }
            StmtKind::Send { target, event, payload } => {
                self.expr(target);
                self.expr(event);
                self.opt(payload.as_ref());
            }
            StmtKind::Goto { payload, .. } => self.opt(payload.as_ref()),
            StmtKind::Receive { cases } => {
                self.pool.uniquify(&format!("{TEMP_PREFIX}recvEvent"));
                for &(_, h) in cases {
                    self.pool.uniquify(&format!("{TEMP_PREFIX}evt"));
                    for &p in &self.prog.fun(h).params {
                        self.var(p);
                    }
                    self.locals_and_body(h);
                }
            }
        }
    }

    fn opt(&mut self, e: Option<&Expr>) {
        if let Some(e) = e {
            self.expr(e);
        }
    }

    fn exprs(&mut self, es: &[Expr]) {
        for e in es {
            self.expr(e);
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Var(v) => self.var(*v),
            ExprKind::EnumElem(en, i) => {
                if self.elems.insert((*en, *i)) {
                    self.pool.decl(&self.prog.enums[en.idx()].elems[*i as usize].name);
                }
            }
            ExprKind::Str { args, .. } | ExprKind::Tuple(args) | ExprKind::NamedTuple(args) => self.exprs(args),
            ExprKind::FunCall { fun, args } => {
                self.fun_name(*fun);
                self.exprs(args);
            }
            ExprKind::TupleAccess { base, .. } | ExprKind::NamedTupleAccess { base, .. } => self.expr(base),
            ExprKind::SeqAccess { base, index } | ExprKind::SetAccess { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            ExprKind::MapAccess { base, key } => {
                self.expr(base);
                self.expr(key);
            }
            ExprKind::Keys(x)
            | ExprKind::Values(x)
            | ExprKind::Sizeof(x)
            | ExprKind::Cast(x)
            | ExprKind::Coerce(x)
            | ExprKind::Unary { operand: x, .. } => self.expr(x),
            ExprKind::Ctor { payload, .. } => self.opt(payload.as_deref()),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Contains { item, collection } => {
                self.expr(item);
                self.expr(collection);
            }
            ExprKind::Choose(x) => self.opt(x.as_deref()),
            _ => {}
        }
    }
}

/// Follows the generator's walk: global parameters, event sets, global functions, then each
/// machine in turn (class name, fields, methods in declaration order). Receive-case handlers
/// are written inline in their enclosing function and get no name of their own.
pub(crate) fn names(prog: &Program) -> CsNames {
    let mut w = Walker {
        prog,
        pool: Pool::default(),
        machines: vec![String::new(); prog.machines.len()],
        funs: vec![String::new(); prog.funs.len()],
        vars: HashSet::new(),
        elems: HashSet::new(),
    };
    for &v in &prog.global_params {
        w.var(v);
    }
    for es in &prog.event_sets {
        w.pool.decl(&es.name);
    }
    for (i, d) in prog.funs.iter().enumerate() {
        if d.owner.is_none() && d.parent.is_none() {
            w.function(FunId(i as u32));
        }
    }
    for (m, md) in prog.machines.iter().enumerate() {
        w.machine(MachineId(m as u32));
        for &f in &md.fields {
            w.var(f);
        }
        let mut methods: Vec<FunId> = (0..prog.funs.len() as u32)
            .map(FunId)
            .filter(|f| prog.fun(*f).owner == Some(MachineId(m as u32)) && prog.fun(*f).parent.is_none())
            .collect();
        methods.sort_by_key(|f| prog.fun(*f).span.start);
        for f in methods {
            w.function(f);
        }
    }
    CsNames { machines: w.machines, funs: w.funs }
}

#[cfg(test)]
mod tests {
    use super::Pool;

    #[test]
    fn uniquify() {
        let mut n = Pool::default();
        assert_eq!(n.decl("client"), "client");
        assert_eq!(n.decl("client"), "client_1");
        assert_eq!(n.decl("client_1"), "client_1_1");
        assert_eq!(n.decl("client"), "client_2");
        assert_eq!(n.decl(""), "Anon");
        assert_eq!(n.decl("int"), "P_int");
        assert_eq!(n.decl("$tmp0"), "TMP_tmp0");
    }
}
