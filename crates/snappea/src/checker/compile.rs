//! Lowering of the semantic IR to [`bytecode`](crate::checker::bytecode).

use std::rc::Rc;

use crate::sema::{BinOp, Expr, ExprKind, FunDecl, FunId, PType, Program, Stmt, StmtKind, UnOp, VarId, VarRole};

use crate::checker::bytecode::{CodeBlock, Op, Operand, PathElem, Probe, ProbeKind, ReceiveCase, Root};

fn is_comparison(op: BinOp) -> bool {
    matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Neq)
}

/// Compiled code for a whole program, indexed by [`FunId`].
#[derive(Debug, Clone, Default)]
pub struct Code {
    pub funs: Vec<Option<Rc<CodeBlock>>>,
    /// Receive set (permissions) of every interface, indexed by `InterfaceId`.
    pub permissions: Vec<Rc<[crate::sema::EventId]>>,
    /// Code coverage probes, indexed by [`Op::Probe`] (empty unless compiled with them).
    pub probes: Vec<Probe>,
}

impl Code {
    pub fn get(&self, f: FunId) -> Option<&Rc<CodeBlock>> {
        self.funs.get(f.idx()).and_then(|c| c.as_ref())
    }
}

/// Compiles every root function with a body.
pub fn compile_program(prog: &Program) -> Code {
    compile_program_with(prog, true, false)
}

/// Compiles every root function with a body, with code coverage probes (see [`Op::Probe`]).
pub fn compile_program_with_probes(prog: &Program) -> Code {
    compile_program_with(prog, true, true)
}

/// Compiles without the specialized and fused instructions, which must behave exactly
/// like the plain ones (see [`Op`]).
#[cfg(test)]
pub(crate) fn compile_program_plain(prog: &Program) -> Code {
    compile_program_with(prog, false, false)
}

fn compile_program_with(prog: &Program, fuse: bool, probe: bool) -> Code {
    let mut probes = Vec::new();
    let mut compile_all = |unread: &[Vec<bool>]| -> Vec<Option<Rc<CodeBlock>>> {
        probes.clear();
        let funs = prog.funs.iter().enumerate();
        funs.map(|(i, f)| {
            (f.parent.is_none() && f.body.is_some()).then(|| {
                let probes = probe.then_some(&mut probes);
                Rc::new(compile_fun(prog, FunId(i as u32), fuse, unread, probes))
            })
        })
        .collect()
    };
    let mut funs = compile_all(&[]);
    if fuse {
        // A second pass emits `UnreadFormat` for the parameters the first finds unread.
        // It changes no callee that has unread parameters, as those contain no calls.
        let unread: Vec<Vec<bool>> = funs.iter().map(|c| c.as_deref().map_or_else(Vec::new, unread_params)).collect();
        if unread.iter().flatten().any(|u| *u) {
            funs = compile_all(&unread);
        }
    }
    let permissions = (0..prog.interfaces.len())
        .map(|i| Rc::from(prog.interface_receives(crate::sema::InterfaceId(i as u32))))
        .collect();
    Code { funs, permissions, probes }
}

/// For each parameter of `code`, whether the function never reads it, and cannot expose
/// it either. Locals are only ever observed by the state hash of a scheduling point
/// (`statehash`), never by logs, traces or bug reports; a function that neither calls
/// nor yields, raises or chooses returns before any, so its frame is never hashed. Such a
/// parameter is dead, so a caller need not compute its value, only the effects of
/// computing it (see [`Op::UnreadFormat`]). Conservative: an instruction that names the
/// slot counts as a read unless it only overwrites it. The match is exhaustive so that
/// new instructions get classified.
fn unread_params(code: &CodeBlock) -> Vec<bool> {
    let mut unread = vec![true; code.param_slots.len()];
    for op in &code.ops {
        // the local slots `op` reads, or `None` when it may leave the frame
        let reads: Option<[Option<u32>; 2]> = match op {
            // calls, scheduling points, leaving the action, and choices (which consult the
            // strategy)
            Op::Call { .. }
            | Op::Create { .. }
            | Op::Send { .. }
            | Op::Raise { .. }
            | Op::Announce { .. }
            | Op::Goto { .. }
            | Op::Receive { .. }
            | Op::ChooseBool
            | Op::ChooseInt
            | Op::ChooseSeq
            | Op::ChooseSet
            | Op::ChooseMap => None,
            Op::LoadLocal(s)
            | Op::LocalTupleGet { slot: s, .. }
            | Op::LocalSizeof(s)
            | Op::MapGetLocal { slot: s, .. }
            | Op::KeyAtLocal(s)
            | Op::IntBin { rhs: Operand::Local(s), .. }
            | Op::IntCmpJumpIfFalse { rhs: Operand::Local(s), .. }
            | Op::Store { root: Root::Local(s), .. }
            | Op::Insert { root: Root::Local(s), .. }
            | Op::Add { root: Root::Local(s), .. }
            | Op::Remove { root: Root::Local(s), .. }
            | Op::ForeachInit { root: Some(Root::Local(s)), .. } => Some([Some(*s), None]),
            Op::ForeachNext { iter_slot, item_slot, .. } => Some([Some(*iter_slot), Some(*item_slot)]),
            // only overwrite a local, or touch none
            Op::StoreLocal(_)
            | Op::StoreLocalConst { .. }
            | Op::ForeachInit { .. }
            | Op::IntBin { .. }
            | Op::IntCmpJumpIfFalse { .. }
            | Op::Store { .. }
            | Op::Insert { .. }
            | Op::Add { .. }
            | Op::Remove { .. }
            | Op::PushNull
            | Op::PushBool(_)
            | Op::PushInt(_)
            | Op::PushFloat(_)
            | Op::PushEvent(_)
            | Op::PushThis
            | Op::LoadField(_)
            | Op::LoadGlobal(_)
            | Op::StoreField(_)
            | Op::Pop
            | Op::Dup
            | Op::MakeTuple(_)
            | Op::MakeNamedTuple(_)
            | Op::TupleGet(_)
            | Op::SeqGet
            | Op::SetGet
            | Op::MapGet { .. }
            | Op::Keys
            | Op::Values
            | Op::Sizeof
            | Op::Contains
            | Op::Default(_)
            | Op::Format { .. }
            | Op::UnreadFormat { .. }
            | Op::Bin(_)
            | Op::Neg
            | Op::Not
            | Op::Jump(_)
            | Op::JumpIfFalse(_)
            | Op::JumpIfTrue(_)
            | Op::JumpIfFalseElsePop(_)
            | Op::JumpIfTrueElsePop(_)
            | Op::Cast(_)
            | Op::CoerceFloat
            | Op::CoerceInt
            | Op::CoerceInterface(_)
            | Op::KeysSizeof
            | Op::KeysContains
            | Op::Return
            | Op::ReturnNull
            | Op::Assert
            | Op::Print
            | Op::Probe(_)
            | Op::ConditionProbe { .. } => Some([None, None]),
        };
        let Some(reads) = reads else { return vec![false; unread.len()] };
        for slot in reads.into_iter().flatten() {
            if let Some(i) = code.param_slots.iter().position(|p| *p == slot) {
                unread[i] = false;
            }
        }
    }
    unread
}

/// Whether `format(base, ...)` with `nargs` arguments always succeeds in C# semantics. A
/// `FormatException` depends only on `base` and the number of arguments, never on their
/// values.
fn format_cannot_fail(base: &str, nargs: usize) -> bool {
    crate::checker::value::format_composite(base, &vec![""; nargs]).is_ok()
}

struct LoopCtx {
    break_patches: Vec<usize>,
    continue_target: usize,
}

struct Compiler<'a> {
    prog: &'a Program,
    ops: Vec<Op>,
    next_hidden: u32,
    loops: Vec<LoopCtx>,
    /// Emit the specialized and fused instructions.
    fuse: bool,
    /// [`unread_params`] of every function, by [`FunId`] (empty in the first pass).
    unread: &'a [Vec<bool>],
    /// The last position taken as a jump target: instructions from there on cannot be
    /// fused with earlier ones.
    barrier: usize,
    /// The program's code coverage probes, when compiling with them.
    probes: Option<&'a mut Vec<Probe>>,
    /// The function whose code is being compiled (a receive case's inside its body).
    fun: FunId,
    /// The probe enclosing the code being compiled (see [`Probe::parent`]).
    parent_probe: Option<u32>,
    /// Inside code that can never run: no probes, so coverage does not require it.
    unreachable: u32,
    /// The statement just compiled never finishes: a `while (true)` without `break`.
    diverged: bool,
}

fn compile_fun(
    prog: &Program,
    id: FunId,
    fuse: bool,
    unread: &[Vec<bool>],
    probes: Option<&mut Vec<Probe>>,
) -> CodeBlock {
    let f = prog.fun(id);
    let mut c = Compiler {
        prog,
        ops: Vec::new(),
        next_hidden: f.frame_size,
        loops: Vec::new(),
        fuse,
        unread,
        barrier: 0,
        probes,
        fun: id,
        parent_probe: None,
        unreachable: 0,
        diverged: false,
    };
    c.init_locals(f);
    c.parent_probe = c.probe(ProbeKind::Entry, f.span);
    if let Some(body) = &f.body {
        c.stmts(&body.stmts);
    }
    c.emit(Op::PushNull);
    c.emit(Op::Return);
    let param_slots: Vec<u32> = f.params.iter().map(|v| prog.var(*v).slot).collect();
    let params_in_order = param_slots.iter().enumerate().all(|(i, s)| *s as usize == i);
    CodeBlock { ops: c.ops, frame_size: c.next_hidden, param_slots, params_in_order }
}

impl<'a> Compiler<'a> {
    /// Emits a code coverage probe when compiling with them; returns its id.
    fn probe(&mut self, kind: ProbeKind, span: crate::syntax::Span) -> Option<u32> {
        if self.unreachable > 0 {
            return None;
        }
        let probes = self.probes.as_mut()?;
        let id = probes.len() as u32;
        probes.push(Probe { kind, fun: self.fun, span, parent: self.parent_probe, asserts_false: false });
        self.emit(Op::Probe(id));
        Some(id)
    }

    /// Compiles `s` with `parent` as the enclosing probe.
    fn stmt_under(&mut self, s: &Stmt, parent: Option<u32>) {
        let inner = parent.or(self.parent_probe);
        let outer = std::mem::replace(&mut self.parent_probe, inner);
        self.stmt(s);
        self.parent_probe = outer;
    }

    /// Appends `op`, fused with the instructions before it where possible; returns its
    /// position.
    fn emit(&mut self, op: Op) -> usize {
        let op = self.fuse(op);
        self.ops.push(op);
        self.ops.len() - 1
    }

    /// Whether the last `n` instructions can be fused with the next one: none of them but
    /// the first is a jump target, and neither is the next one.
    fn can_fuse(&self, n: usize) -> bool {
        self.ops.len() >= n && self.barrier + n <= self.ops.len()
    }

    /// Combines `op` with the instructions before it into one of the fused instructions
    /// (see [`Op`]), popping those it absorbs.
    fn fuse(&mut self, op: Op) -> Op {
        if !self.fuse {
            return op;
        }
        let fused = match (self.ops.as_slice(), &op) {
            ([.., Op::Keys, Op::LoadLocal(slot)], Op::SeqGet) if self.can_fuse(2) => (2, Op::KeyAtLocal(*slot)),
            ([.., Op::Default(value)], Op::StoreLocal(slot)) => {
                (1, Op::StoreLocalConst { slot: *slot, value: value.clone() })
            }
            ([.., Op::LoadLocal(slot)], Op::TupleGet(index)) => (1, Op::LocalTupleGet { slot: *slot, index: *index }),
            ([.., Op::LoadLocal(slot)], Op::Sizeof) => (1, Op::LocalSizeof(*slot)),
            ([.., Op::LoadLocal(slot)], Op::MapGet { java_null }) => {
                (1, Op::MapGetLocal { slot: *slot, java_null: *java_null })
            }
            ([.., Op::Keys], Op::Sizeof) => (1, Op::KeysSizeof),
            ([.., Op::Keys], Op::Contains) => (1, Op::KeysContains),
            ([.., Op::LoadLocal(slot)], Op::IntBin { op, rhs: Operand::Stack }) => {
                (1, Op::IntBin { op: *op, rhs: Operand::Local(*slot) })
            }
            ([.., Op::PushInt(k)], Op::IntBin { op, rhs: Operand::Stack }) => {
                (1, Op::IntBin { op: *op, rhs: Operand::Int(*k) })
            }
            ([.., Op::IntBin { op, rhs }], Op::JumpIfFalse(target)) if is_comparison(*op) => {
                (1, Op::IntCmpJumpIfFalse { op: *op, rhs: *rhs, target: *target })
            }
            ([.., Op::PushNull], Op::Return) => (1, Op::ReturnNull),
            _ => return op,
        };
        match fused {
            (n, fused) if self.can_fuse(n) => {
                self.ops.truncate(self.ops.len() - n);
                fused
            }
            _ => op,
        }
    }

    /// The current position, as the target of a jump.
    fn here(&mut self) -> u32 {
        self.barrier = self.ops.len();
        self.ops.len() as u32
    }

    fn patch(&mut self, at: usize, target: u32) {
        match &mut self.ops[at] {
            Op::Jump(t)
            | Op::JumpIfFalse(t)
            | Op::JumpIfTrue(t)
            | Op::IntCmpJumpIfFalse { target: t, .. }
            | Op::JumpIfFalseElsePop(t)
            | Op::JumpIfTrueElsePop(t) => *t = target,
            Op::ForeachNext { exit, .. } => *exit = target,
            _ => unreachable!("patching a non-jump"),
        }
    }

    /// The jump of `&&` (`if_true` false) or `||`: jumps to the end keeping the left
    /// operand when it decides the result, else pops it. Returns the jump to patch.
    fn short_circuit(&mut self, if_true: bool) -> usize {
        match (self.fuse, if_true) {
            (true, false) => self.emit(Op::JumpIfFalseElsePop(0)),
            (true, true) => self.emit(Op::JumpIfTrueElsePop(0)),
            (false, _) => {
                self.emit(Op::Dup);
                let j = self.emit(if if_true { Op::JumpIfTrue(0) } else { Op::JumpIfFalse(0) });
                self.emit(Op::Pop);
                j
            }
        }
    }

    fn is_int(&self, e: &Expr) -> bool {
        matches!(e.ty.canonicalize(self.prog), PType::Int)
    }

    fn init_locals(&mut self, f: &FunDecl) {
        for v in &f.locals {
            let d = self.prog.var(*v);
            self.emit(Op::Default(crate::checker::value::default_value(&d.ty, self.prog)));
            self.emit(Op::StoreLocal(d.slot));
        }
    }

    fn var_root(&self, v: VarId) -> Root {
        let d = self.prog.var(v);
        match d.role {
            VarRole::Field => Root::Field(d.slot),
            _ => Root::Local(d.slot),
        }
    }

    // ---- statements -------------------------------------------------------

    /// Compiles a statement list. Statements after a `while (true)` without `break` can
    /// never run (P still requires a `return` there), so they get no coverage probes.
    fn stmts(&mut self, list: &[Stmt]) {
        let mut dead = false;
        for s in list {
            self.unreachable += u32::from(dead);
            self.diverged = false;
            self.stmt(s);
            self.unreachable -= u32::from(dead);
            dead |= self.diverged && matches!(s.kind, StmtKind::While { .. } | StmtKind::Block(_));
        }
        self.diverged = dead;
    }

    fn stmt(&mut self, s: &Stmt) {
        let probe = match s.kind {
            StmtKind::Block(_) | StmtKind::Nop => None,
            _ => self.probe(ProbeKind::Stmt, s.span),
        };
        if let (Some(id), StmtKind::Assert { cond, .. }) = (probe, &s.kind) {
            if matches!(cond.kind, ExprKind::BoolLit(false)) {
                if let Some(p) = self.probes.as_mut() {
                    p[id as usize].asserts_false = true;
                }
            }
        }
        match &s.kind {
            StmtKind::Block(b) => self.stmts(&b.stmts),
            StmtKind::Assert { cond, message } | StmtKind::Assume { cond, message } => {
                // Upstream lowers `assert c, m` to `if (!c) { m-deps; assert c, m }` unless `c`
                // is a literal, so the message is only evaluated when the assertion fails.
                if matches!(cond.kind, ExprKind::BoolLit(_)) {
                    self.expr(cond);
                    self.expr(message);
                    self.emit(Op::Assert);
                } else {
                    self.expr(cond);
                    let skip = self.emit(Op::JumpIfTrue(0));
                    self.emit(Op::PushBool(false));
                    self.expr(message);
                    self.emit(Op::Assert);
                    let end = self.here();
                    self.patch(skip, end);
                }
            }
            StmtKind::Print(e) => {
                self.expr(e);
                self.emit(Op::Print);
            }
            StmtKind::Return(e) => {
                match e {
                    Some(e) => self.expr(e),
                    None => {
                        self.emit(Op::PushNull);
                    }
                }
                self.emit(Op::Return);
            }
            // the type checker rejects `break` and `continue` outside a loop
            StmtKind::Break => {
                let at = self.emit(Op::Jump(0));
                self.loops.last_mut().expect("break outside a loop").break_patches.push(at);
            }
            StmtKind::Continue => {
                let target = self.loops.last().expect("continue outside a loop").continue_target as u32;
                self.emit(Op::Jump(target));
            }
            StmtKind::Assign { lhs, rhs } => {
                let (root, path) = self.lvalue(lhs);
                self.expr(rhs);
                match (root, path.is_empty() && self.fuse) {
                    (Root::Local(slot), true) => self.emit(Op::StoreLocal(slot)),
                    (Root::Field(slot), true) => self.emit(Op::StoreField(slot)),
                    _ => self.emit(Op::Store { root, path }),
                };
            }
            StmtKind::Insert { lhs, index, value } => {
                let (root, path) = self.lvalue(lhs);
                self.expr(index);
                self.expr(value);
                self.emit(Op::Insert { root, path });
            }
            StmtKind::Add { lhs, value } => {
                let (root, path) = self.lvalue(lhs);
                self.expr(value);
                self.emit(Op::Add { root, path });
            }
            StmtKind::Remove { lhs, value } => {
                let (root, path) = self.lvalue(lhs);
                self.expr(value);
                self.emit(Op::Remove { root, path });
            }
            StmtKind::While { cond, body } => {
                let start = self.here();
                self.condition(cond);
                let jf = self.emit(Op::JumpIfFalse(0));
                self.loops.push(LoopCtx { break_patches: Vec::new(), continue_target: start as usize });
                self.stmt(body);
                self.emit(Op::Jump(start));
                let end = self.here();
                self.patch(jf, end);
                let l = self.loops.pop().unwrap();
                let forever = matches!(cond.kind, ExprKind::BoolLit(true)) && l.break_patches.is_empty();
                for p in l.break_patches {
                    self.patch(p, end);
                }
                self.diverged = forever;
            }
            StmtKind::Foreach { item, collection, body } => {
                let root = match &collection.kind {
                    ExprKind::Var(v) if self.prog.var(*v).role != VarRole::GlobalParam => Some(self.var_root(*v)),
                    _ => None,
                };
                let iter_slot = self.next_hidden;
                self.next_hidden += 2;
                let item_slot = self.prog.var(*item).slot;
                self.expr(collection);
                self.emit(Op::ForeachInit { iter_slot, root });
                let start = self.here();
                let next = self.emit(Op::ForeachNext { iter_slot, item_slot, exit: 0 });
                self.loops.push(LoopCtx { break_patches: Vec::new(), continue_target: start as usize });
                self.stmt(body);
                self.emit(Op::Jump(start));
                let end = self.here();
                self.patch(next, end);
                let l = self.loops.pop().unwrap();
                for p in l.break_patches {
                    self.patch(p, end);
                }
            }
            StmtKind::If { cond, then_branch, else_branch } => {
                self.condition(cond);
                let jf = self.emit(Op::JumpIfFalse(0));
                let outer = std::mem::replace(&mut self.parent_probe, probe);
                let then_probe = self.probe(ProbeKind::Then, s.span);
                self.parent_probe = outer;
                self.stmt_under(then_branch, then_probe);
                if self.probes.is_some() {
                    // an else probe, even for an empty else
                    let j = self.emit(Op::Jump(0));
                    let else_start = self.here();
                    self.patch(jf, else_start);
                    let outer = std::mem::replace(&mut self.parent_probe, probe);
                    let else_probe = self.probe(ProbeKind::Else, s.span);
                    self.parent_probe = outer;
                    self.stmt_under(else_branch, else_probe);
                    let end = self.here();
                    self.patch(j, end);
                } else if matches!(else_branch.kind, StmtKind::Nop) {
                    let end = self.here();
                    self.patch(jf, end);
                } else {
                    let j = self.emit(Op::Jump(0));
                    let else_start = self.here();
                    self.patch(jf, else_start);
                    self.stmt(else_branch);
                    let end = self.here();
                    self.patch(j, end);
                }
            }
            StmtKind::Ctor { interface, payload } => {
                let has_payload = self.opt_expr(payload.as_ref());
                self.emit(Op::Create { iface: *interface, has_payload });
                self.emit(Op::Pop);
            }
            StmtKind::FunCall { fun, args } => {
                self.call(*fun, args);
                self.emit(Op::Pop);
            }
            StmtKind::Raise { event, payload } => {
                self.expr(event);
                let has_payload = self.opt_expr(payload.as_ref());
                self.emit(Op::Raise { has_payload });
            }
            StmtKind::Send { target, event, payload } => {
                self.expr(target);
                self.expr(event);
                let has_payload = self.opt_expr(payload.as_ref());
                self.emit(Op::Send { has_payload });
            }
            StmtKind::Announce { event, payload } => {
                self.expr(event);
                let has_payload = self.opt_expr(payload.as_ref());
                self.emit(Op::Announce { has_payload });
            }
            StmtKind::Goto { state, payload } => {
                let has_payload = self.opt_expr(payload.as_ref());
                self.emit(Op::Goto { state: *state, has_payload });
            }
            StmtKind::Receive { cases } => {
                let recv = self.emit(Op::Receive { cases: Rc::from(Vec::new()) });
                let mut compiled = Vec::new();
                let mut end_jumps = Vec::new();
                for (ev, fun) in cases {
                    let f = self.prog.fun(*fun);
                    let param_slot = f.params.first().map(|v| self.prog.var(*v).slot);
                    let target = self.here();
                    self.init_locals(f);
                    let (outer_fun, outer_parent) = (self.fun, self.parent_probe);
                    self.parent_probe = probe;
                    self.fun = *fun;
                    self.parent_probe = self.probe(ProbeKind::Entry, f.span);
                    if let Some(body) = &f.body {
                        for s in &body.stmts {
                            self.stmt(s);
                        }
                    }
                    (self.fun, self.parent_probe) = (outer_fun, outer_parent);
                    end_jumps.push(self.emit(Op::Jump(0)));
                    compiled.push(ReceiveCase { event: *ev, param_slot, target });
                }
                let end = self.here();
                for j in end_jumps {
                    self.patch(j, end);
                }
                self.ops[recv] = Op::Receive { cases: Rc::from(compiled) };
            }
            StmtKind::Nop => {}
        }
    }

    /// A call of `fun`. A `format` argument for a parameter `fun` never reads is evaluated
    /// for its effects only: its arguments as usual, then `UnreadFormat` in place of
    /// `Format`, which counts the same work and, if the format cannot fail, has no other
    /// effect.
    fn call(&mut self, fun: FunId, args: &[Expr]) {
        let unread = self.unread.get(fun.idx()).map_or(&[][..], Vec::as_slice);
        for (i, a) in args.iter().enumerate() {
            match &a.kind {
                ExprKind::Str { base, args }
                    if unread.get(i) == Some(&true) && format_cannot_fail(base, args.len()) =>
                {
                    self.format(base, args, true)
                }
                _ => self.expr(a),
            }
        }
        self.emit(Op::Call { fun, nargs: args.len() as u32 });
    }

    /// `format(base, args...)`, or its [`Op::UnreadFormat`] form when `unread`.
    fn format(&mut self, base: &str, args: &[Expr], unread: bool) {
        for a in args {
            self.expr(a);
        }
        let (base, nargs) = (Rc::from(base), args.len() as u32);
        let types: Rc<[PType]> = args.iter().map(|a| a.ty.clone()).collect();
        self.emit(match unread {
            true => Op::UnreadFormat { base, nargs, types },
            false => Op::Format { base, nargs, types },
        });
    }

    fn opt_expr(&mut self, e: Option<&Expr>) -> bool {
        match e {
            Some(e) => {
                self.expr(e);
                true
            }
            None => false,
        }
    }

    /// Emits the key expressions of an l-value and returns its root and path.
    fn lvalue(&mut self, e: &Expr) -> (Root, Rc<[PathElem]>) {
        let mut path = Vec::new();
        let root = self.lvalue_rec(e, &mut path);
        (root, Rc::from(path))
    }

    fn lvalue_rec(&mut self, e: &Expr, path: &mut Vec<PathElem>) -> Root {
        match &e.kind {
            ExprKind::Var(v) => self.var_root(*v),
            ExprKind::TupleAccess { base, index } | ExprKind::NamedTupleAccess { base, index } => {
                let r = self.lvalue_rec(base, path);
                path.push(PathElem::TupleField(*index));
                r
            }
            ExprKind::SeqAccess { base, index } => {
                let r = self.lvalue_rec(base, path);
                self.expr(index);
                path.push(PathElem::SeqIndex);
                r
            }
            ExprKind::MapAccess { base, key } => {
                let r = self.lvalue_rec(base, path);
                self.expr(key);
                path.push(PathElem::MapKey);
                r
            }
            // Not assignable (sets, globals, arbitrary expressions): evaluate into a
            // scratch slot so the store is harmless.
            _ => {
                self.expr(e);
                let slot = self.next_hidden;
                self.next_hidden += 1;
                self.emit(Op::StoreLocal(slot));
                Root::Local(slot)
            }
        }
    }

    // ---- expressions ------------------------------------------------------

    /// Records actual atomic condition outcomes, retaining short-circuit evaluation.
    /// Assertions are deliberately excluded: their false outcome is a violation, not
    /// an obligation to exercise. Constants likewise have no two-outcome target.
    fn condition(&mut self, e: &Expr) {
        if self.probes.is_none() {
            self.expr(e);
            return;
        }
        match &e.kind {
            ExprKind::Binary { op: op @ (BinOp::And | BinOp::Or | BinOp::Then), lhs, rhs } => {
                self.condition(lhs);
                if *op == BinOp::Then {
                    self.emit(Op::Not);
                }
                let j = self.short_circuit(*op != BinOp::And);
                self.condition(rhs);
                let end = self.here();
                self.patch(j, end);
            }
            ExprKind::Binary { op: BinOp::Iff, lhs, rhs } => {
                self.condition(lhs);
                self.condition(rhs);
                self.emit(Op::Bin(BinOp::Eq));
            }
            ExprKind::Unary { op: UnOp::Not, operand } => {
                self.condition(operand);
                self.emit(Op::Not);
            }
            ExprKind::BoolLit(_) => self.expr(e),
            _ => {
                self.expr(e);
                // Allocate metadata without emitting unconditional hit instructions.
                let probes = self.probes.as_mut().unwrap();
                let yes = probes.len() as u32;
                for kind in [ProbeKind::ConditionTrue, ProbeKind::ConditionFalse] {
                    probes.push(Probe {
                        kind,
                        fun: self.fun,
                        span: e.span,
                        parent: self.parent_probe,
                        asserts_false: false,
                    });
                }
                self.emit(Op::ConditionProbe { yes, no: yes + 1 });
            }
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Var(v) => {
                let d = self.prog.var(*v);
                match d.role {
                    VarRole::Field => self.emit(Op::LoadField(d.slot)),
                    VarRole::GlobalParam => self.emit(Op::LoadGlobal(d.slot)),
                    _ => self.emit(Op::LoadLocal(d.slot)),
                };
            }
            ExprKind::EnumElem(en, i) => {
                let v = self.prog.enums[en.idx()].elems[*i as usize].value;
                self.emit(Op::PushInt(v));
            }
            ExprKind::EventRef(ev) => {
                self.emit(Op::PushEvent(*ev));
            }
            ExprKind::IntLit(i) => {
                self.emit(Op::PushInt(*i));
            }
            ExprKind::FloatLit(f) => {
                self.emit(Op::PushFloat(*f));
            }
            ExprKind::BoolLit(b) => {
                self.emit(Op::PushBool(*b));
            }
            ExprKind::NullLit | ExprKind::Error => {
                self.emit(Op::PushNull);
            }
            ExprKind::Str { base, args } => self.format(base, args, false),
            ExprKind::Nondet | ExprKind::FairNondet => {
                self.emit(Op::ChooseBool);
            }
            ExprKind::This => {
                self.emit(Op::PushThis);
            }
            ExprKind::Tuple(es) => {
                for x in es {
                    self.expr(x);
                }
                self.emit(Op::MakeTuple(es.len() as u32));
            }
            ExprKind::NamedTuple(es) => {
                for x in es {
                    self.expr(x);
                }
                let names: Vec<String> = match e.ty.canonicalize(self.prog) {
                    PType::NamedTuple(fs) => fs.into_iter().map(|(n, _)| n).collect(),
                    _ => (0..es.len()).map(|i| i.to_string()).collect(),
                };
                self.emit(Op::MakeNamedTuple(Rc::from(names)));
            }
            ExprKind::TupleAccess { base, index } | ExprKind::NamedTupleAccess { base, index } => {
                self.expr(base);
                self.emit(Op::TupleGet(*index));
            }
            ExprKind::SeqAccess { base, index } => {
                self.expr(base);
                self.expr(index);
                self.emit(Op::SeqGet);
            }
            ExprKind::SetAccess { base, index } => {
                self.expr(base);
                self.expr(index);
                self.emit(Op::SetGet);
            }
            ExprKind::MapAccess { base, key } => {
                self.expr(base);
                self.expr(key);
                let mut ty = &e.ty;
                while let PType::TypeDef(id) = ty {
                    ty = &self.prog.typedefs[id.idx()].ty;
                }
                // Java keeps int, bool, float and machines as primitives (unboxing null throws)
                let java_null =
                    !matches!(ty, PType::Int | PType::Bool | PType::Float | PType::Machine | PType::Permission(_));
                self.emit(Op::MapGet { java_null });
            }
            ExprKind::Keys(b) => {
                self.expr(b);
                self.emit(Op::Keys);
            }
            ExprKind::Values(b) => {
                self.expr(b);
                self.emit(Op::Values);
            }
            ExprKind::Sizeof(b) => {
                self.expr(b);
                self.emit(Op::Sizeof);
            }
            ExprKind::Default => {
                self.emit(Op::Default(crate::checker::value::default_value(&e.ty, self.prog)));
            }
            ExprKind::Ctor { interface, payload } => {
                let has_payload = self.opt_expr(payload.as_deref());
                self.emit(Op::Create { iface: *interface, has_payload });
            }
            ExprKind::FunCall { fun, args } => self.call(*fun, args),
            ExprKind::Unary { op, operand } => {
                self.expr(operand);
                match op {
                    UnOp::Neg => self.emit(Op::Neg),
                    UnOp::Not => self.emit(Op::Not),
                };
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::And | BinOp::Or | BinOp::Then => {
                    self.expr(lhs);
                    if *op == BinOp::Then {
                        self.emit(Op::Not);
                    }
                    let j = self.short_circuit(*op != BinOp::And);
                    self.expr(rhs);
                    let end = self.here();
                    self.patch(j, end);
                }
                BinOp::Iff => {
                    self.expr(lhs);
                    self.expr(rhs);
                    self.emit(Op::Bin(BinOp::Eq));
                }
                other => {
                    self.expr(lhs);
                    self.expr(rhs);
                    if self.fuse && self.is_int(lhs) && self.is_int(rhs) {
                        self.emit(Op::IntBin { op: *other, rhs: Operand::Stack });
                    } else {
                        self.emit(Op::Bin(*other));
                    }
                }
            },
            ExprKind::Contains { item, collection } => {
                self.expr(item);
                self.expr(collection);
                self.emit(Op::Contains);
            }
            ExprKind::Cast(inner) => {
                self.expr(inner);
                self.emit(Op::Cast(std::rc::Rc::new(e.ty.clone())));
            }
            ExprKind::Coerce(inner) => {
                self.expr(inner);
                match e.ty.canonicalize(self.prog) {
                    PType::Float => {
                        self.emit(Op::CoerceFloat);
                    }
                    PType::Int => {
                        self.emit(Op::CoerceInt);
                    }
                    PType::Permission(crate::sema::PermissionOrigin::Interface(i)) => {
                        self.emit(Op::CoerceInterface(i));
                    }
                    _ => {}
                }
            }
            ExprKind::Choose(None) => {
                self.emit(Op::ChooseBool);
            }
            ExprKind::Choose(Some(inner)) => {
                self.expr(inner);
                let op = match inner.ty.canonicalize(self.prog) {
                    PType::Seq(_) => Op::ChooseSeq,
                    PType::Set(_) => Op::ChooseSet,
                    PType::Map(..) => Op::ChooseMap,
                    _ => Op::ChooseInt,
                };
                self.emit(op);
            }
        }
    }
}
