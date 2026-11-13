//! Whole-program validation passes: `MachineChecker`, `FunctionValidator`,
//! `ControlFlowChecker`, purity propagation, capability checks and `InferMachineCreates`.

use crate::sema::analyzer::Analyzer;
use crate::sema::diag::Fatal;
use crate::sema::program::*;
use crate::sema::types::PType;
use crate::syntax::Span;

impl<'a> Analyzer<'a> {
    // ------------------------------------------------------------------
    // MachineChecker.Validate
    // ------------------------------------------------------------------

    pub fn validate_machine(&mut self, mid: MachineId) -> Result<(), Fatal> {
        let m = self.prog.machines[mid.idx()].clone();
        if !m.is_spec() {
            let start = self.find_start_state(mid)?;
            let payload = self.prog.states[start.idx()]
                .entry
                .and_then(|f| self.prog.funs[f.idx()].params.first().copied())
                .map(|v| self.prog.var_decls[v.idx()].ty.clone())
                .unwrap_or(PType::Null);
            self.prog.machines[mid.idx()].payload = payload.clone();
            if let Some(i) = m.interface {
                self.prog.interfaces[i.idx()].payload = payload;
            }
        }
        self.validate_handlers(mid)?;
        self.validate_transitions(mid)?;
        self.validate_spec_observes(mid);
        if m.is_scenario() {
            let all_states = self.all_states(mid);
            if all_states.iter().all(|s| self.prog.states[s.idx()].temperature != Temperature::Cold) {
                let line = self.sm.location(m.span).line;
                self.diags.warn(format!(
                    "[{}] scenario '{}' has no accepting (cold) state; it can never be marked satisfied and will always report 0 coverage. Mark its accepting state 'cold'.",
                    line, m.name
                ));
            }
            if let Some(s) = m.start_state {
                if self.prog.states[s.idx()].temperature == Temperature::Cold {
                    let line = self.sm.location(m.span).line;
                    self.diags.warn(format!(
                        "[{}] scenario '{}' has a 'cold' (accepting) start state; it accepts before any behavior is observed, so it is NOT counted as covered until observed events re-enter an accepting state. Mark the start state 'hot' (or leave it unmarked) and mark the accepting state 'cold'.",
                        line, m.name
                    ));
                }
            }
        }
        Ok(())
    }

    /// `Machine.AllStates()`: the start state first, then the others in declaration order.
    pub fn all_states(&self, mid: MachineId) -> Vec<StateId> {
        let m = &self.prog.machines[mid.idx()];
        let mut v = Vec::new();
        if let Some(s) = m.start_state {
            v.push(s);
        }
        for s in &m.states {
            if !self.prog.states[s.idx()].is_start {
                v.push(*s);
            }
        }
        v
    }

    fn find_start_state(&self, mid: MachineId) -> Result<StateId, Fatal> {
        let m = &self.prog.machines[mid.idx()];
        let mut found = false;
        for s in self.all_states(mid) {
            let st = &self.prog.states[s.idx()];
            if Some(s) == m.start_state || st.is_start {
                if !found {
                    found = true;
                } else {
                    let first = m.start_state.map(|x| self.prog.states[x.idx()].name.clone()).unwrap_or_default();
                    return Err(self.e.two_start_states(m.span, &m.name, &first, &st.name));
                }
            }
        }
        match m.start_state {
            Some(s) if found => Ok(s),
            _ => Err(self.e.missing_start_state(m.span, &m.name)),
        }
    }

    fn validate_handlers(&self, mid: MachineId) -> Result<(), Fatal> {
        for s in self.all_states(mid) {
            let st = &self.prog.states[s.idx()];
            if let Some(f) = st.entry {
                let n = self.prog.funs[f.idx()].params.len();
                if n > 1 {
                    return Err(self.e.more_than_one_parameter_for_handlers(st.span, n));
                }
            }
            if let Some(f) = st.exit {
                let n = self.prog.funs[f.idx()].params.len();
                if n > 0 {
                    return Err(self.e.exit_function_cannot_take_parameters(st.span, n));
                }
            }
            for h in &st.handlers {
                match &h.action {
                    HandlerAction::Do(f) => {
                        let n = self.prog.funs[f.idx()].params.len();
                        if n > 1 {
                            return Err(self.e.more_than_one_parameter_for_handlers(h.span, n));
                        }
                    }
                    HandlerAction::Goto { with: Some(f), .. } => {
                        let n = self.prog.funs[f.idx()].params.len();
                        if n > 1 {
                            return Err(self.e.more_than_one_parameter_for_handlers(h.span, n));
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn validate_transitions(&self, mid: MachineId) -> Result<(), Fatal> {
        for s in self.all_states(mid) {
            let st = &self.prog.states[s.idx()];
            for h in &st.handlers {
                let payload = self.prog.events[h.event.idx()].payload.clone();
                match &h.action {
                    HandlerAction::Do(f) => self.validate_payload_to_target(h.span, &payload, *f)?,
                    HandlerAction::Goto { target, with } => {
                        if let Some(entry) = self.prog.states[target.idx()].entry {
                            self.validate_payload_to_target(h.span, &payload, entry)?;
                        }
                        if let Some(f) = with {
                            self.validate_payload_to_target(h.span, &payload, *f)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn validate_payload_to_target(&self, span: Span, payload: &PType, target: FunId) -> Result<(), Fatal> {
        let params: Vec<PType> =
            self.prog.funs[target.idx()].params.iter().map(|v| self.prog.var_decls[v.idx()].ty.clone()).collect();
        if params.is_empty() {
            return Ok(());
        }
        if params.len() > 1 {
            return Err(self.e.internal_error(
                span,
                "Target function cannot have multiple parameters (report this to the P developers)",
            ));
        }
        let p = &params[0];
        if p.is_assignable_from(payload, &self.prog) {
            return Ok(());
        }
        if let PType::Tuple(ts) = payload.canonicalize(&self.prog) {
            if ts.len() == 1 && p.is_assignable_from(&ts[0], &self.prog) {
                return Ok(());
            }
        }
        let ty = crate::sema::errors::format_type(payload, &self.prog);
        Err(self.e.type_mismatch(span, payload, &[p], &self.prog).help(format!(
            "the handler's parameter receives the event's payload, of type {ty}: declare it as `(v: {ty})`, \
             or change the event's payload type"
        )))
    }

    fn validate_spec_observes(&mut self, mid: MachineId) {
        let m = self.prog.machines[mid.idx()].clone();
        if !m.is_spec() {
            return;
        }
        for s in self.all_states(mid) {
            let handlers = self.prog.states[s.idx()].handlers.clone();
            for h in handlers {
                if !m.observes.contains(&h.event) {
                    let ev = self.prog.events[h.event.idx()].name.clone();
                    let w = self.e.spec_observes_incomplete_warning(h.span, &ev, &m.name);
                    self.diags.warn(w);
                }
            }
        }
    }

    /// `MachineChecker.ValidateNoStaticHandlers`.
    pub fn validate_no_static_handlers(&self, mid: MachineId) -> Result<(), Fatal> {
        let is_static = |f: FunId| {
            let fd = &self.prog.funs[f.idx()];
            fd.owner.is_none() || fd.is_foreign()
        };
        for s in self.all_states(mid) {
            let st = &self.prog.states[s.idx()];
            if let Some(f) = st.entry {
                if is_static(f) {
                    return Err(self.e.static_function_not_allowed_as_handler(st.span, &self.prog.funs[f.idx()].name));
                }
            }
            if let Some(f) = st.exit {
                if is_static(f) {
                    return Err(self.e.static_function_not_allowed_as_handler(st.span, &self.prog.funs[f.idx()].name));
                }
            }
            for h in &st.handlers {
                let f = match &h.action {
                    HandlerAction::Do(f) => Some(*f),
                    HandlerAction::Goto { with, .. } => *with,
                    _ => None,
                };
                if let Some(f) = f {
                    if is_static(f) {
                        return Err(self
                            .e
                            .static_function_not_allowed_as_handler(h.span, &self.prog.funs[f.idx()].name));
                    }
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // FunctionValidator
    // ------------------------------------------------------------------

    pub fn check_all_paths_return(&self, fun: FunId) -> Result<(), Fatal> {
        let f = &self.prog.funs[fun.idx()];
        let Some(body) = &f.body else { return Ok(()) };
        if !f.ret.is_same_type_as(&PType::Null, &self.prog) && !self.surely_returns_block(body) {
            return Err(self.e.not_all_paths_return(body.span, &self.fun_name(fun)));
        }
        Ok(())
    }

    fn surely_returns_block(&self, b: &Block) -> bool {
        b.stmts.iter().any(|s| self.surely_returns(s))
    }

    pub fn surely_returns(&self, s: &Stmt) -> bool {
        match &s.kind {
            StmtKind::Block(b) => self.surely_returns_block(b),
            StmtKind::If { then_branch, else_branch, .. } => {
                self.surely_returns(then_branch) && self.surely_returns(else_branch)
            }
            StmtKind::Return(_) | StmtKind::Goto { .. } | StmtKind::Raise { .. } => true,
            StmtKind::Assert { cond, .. } | StmtKind::Assume { cond, .. } => {
                matches!(cond.kind, ExprKind::BoolLit(false))
            }
            StmtKind::Receive { cases } => cases.iter().all(|(_, f)| {
                self.prog.funs[f.idx()].body.as_ref().map(|b| self.surely_returns_block(b)).unwrap_or(false)
            }),
            _ => false,
        }
    }

    // ------------------------------------------------------------------
    // ControlFlowChecker
    // ------------------------------------------------------------------

    pub fn check_control_flow(&self, fun: FunId) -> Result<(), Fatal> {
        if let Some(b) = &self.prog.funs[fun.idx()].body {
            for s in &b.stmts {
                self.check_cf_stmt(s)?;
            }
        }
        Ok(())
    }

    fn check_cf_stmt(&self, s: &Stmt) -> Result<(), Fatal> {
        match &s.kind {
            StmtKind::Break => Err(self.e.bare_loop_control_flow(s.span, "break")),
            StmtKind::Continue => Err(self.e.bare_loop_control_flow(s.span, "continue")),
            StmtKind::Block(b) => {
                for st in &b.stmts {
                    self.check_cf_stmt(st)?;
                }
                Ok(())
            }
            StmtKind::If { then_branch, else_branch, .. } => {
                self.check_cf_stmt(then_branch)?;
                self.check_cf_stmt(else_branch)
            }
            _ => Ok(()),
        }
    }

    // ------------------------------------------------------------------
    // Analyzer steps 4 and 5
    // ------------------------------------------------------------------

    /// Propagates purity flags from callees to callers (`ApplyPropagations`).
    pub fn propagate_flags(&mut self) {
        let n = self.prog.funs.len();
        // callers map
        let mut callers: Vec<Vec<FunId>> = vec![Vec::new(); n];
        for (i, f) in self.prog.funs.iter().enumerate() {
            for c in &f.callees {
                callers[c.idx()].push(FunId(i as u32));
            }
        }
        type Flag = (fn(&FunDecl) -> bool, fn(&mut FunDecl));
        let getters: [Flag; 6] = [
            (|f| f.can_raise, |f| f.can_raise = true),
            (|f| f.can_receive, |f| f.can_receive = true),
            (|f| f.can_change_state, |f| f.can_change_state = true),
            (|f| f.can_create, |f| f.can_create = true),
            (|f| f.can_send, |f| f.can_send = true),
            (|f| f.is_nondet, |f| f.is_nondet = true),
        ];
        for (get, set) in getters {
            let mut stack: Vec<FunId> =
                (0..n).map(|i| FunId(i as u32)).filter(|f| get(&self.prog.funs[f.idx()])).collect();
            while let Some(f) = stack.pop() {
                for c in callers[f.idx()].clone() {
                    if !get(&self.prog.funs[c.idx()]) {
                        set(&mut self.prog.funs[c.idx()]);
                        stack.push(c);
                    }
                }
            }
        }
    }

    pub fn check_capabilities(&self) -> Result<(), Fatal> {
        let n = self.prog.funs.len();
        for i in 0..n {
            let f = &self.prog.funs[i];
            let impure = f.is_nondet || f.can_create || f.can_send || f.can_receive;
            if let Some(o) = f.owner {
                if self.prog.machines[o.idx()].is_spec() && impure {
                    return Err(self.e.illegal_function_used_in_spec(
                        f.span,
                        &self.fun_name(FunId(i as u32)),
                        &self.prog.machines[o.idx()].name,
                    ));
                }
            }
            if f.owner.is_none() && impure {
                for c in &self.prog.funs {
                    if c.callees.contains(&FunId(i as u32)) {
                        if let Some(o) = c.owner {
                            if self.prog.machines[o.idx()].is_spec() {
                                return Err(self.e.illegal_function_used_in_spec(
                                    f.span,
                                    &self.fun_name(FunId(i as u32)),
                                    &self.prog.machines[o.idx()].name,
                                ));
                            }
                        }
                    }
                }
            }
            if (f.can_change_state || f.can_raise) && (f.role.transition || f.role.exit) {
                return Err(self.e.changed_state_mid_transition(f.span, &self.fun_name(FunId(i as u32))));
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // InferMachineCreates
    // ------------------------------------------------------------------

    pub fn infer_creates(&mut self, mid: MachineId) {
        let methods = self.prog.machines[mid.idx()].methods.clone();
        let mut out: Vec<InterfaceId> = Vec::new();
        for f in methods {
            let mut visited = Vec::new();
            self.creates_of_fun(f, &mut visited, &mut out);
        }
        out.sort_by(|a, b| self.prog.interfaces[a.idx()].name.cmp(&self.prog.interfaces[b.idx()].name));
        out.dedup();
        self.prog.machines[mid.idx()].creates = out;
    }

    fn creates_of_fun(&self, f: FunId, visited: &mut Vec<FunId>, out: &mut Vec<InterfaceId>) {
        let fd = &self.prog.funs[f.idx()];
        if fd.is_foreign() {
            out.extend(fd.creates.iter().copied());
            return;
        }
        if visited.contains(&f) {
            return;
        }
        visited.push(f);
        if let Some(b) = &fd.body {
            for s in &b.stmts {
                self.creates_of_stmt(s, visited, out);
            }
        }
    }

    fn creates_of_stmt(&self, s: &Stmt, visited: &mut Vec<FunId>, out: &mut Vec<InterfaceId>) {
        match &s.kind {
            StmtKind::Block(b) => {
                for st in &b.stmts {
                    self.creates_of_stmt(st, visited, out);
                }
            }
            StmtKind::Assert { cond, message } | StmtKind::Assume { cond, message } => {
                self.creates_of_expr(cond, visited, out);
                self.creates_of_expr(message, visited, out);
            }
            StmtKind::Print(x) => self.creates_of_expr(x, visited, out),
            StmtKind::Return(x) => {
                if let Some(x) = x {
                    self.creates_of_expr(x, visited, out)
                }
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Nop => {}
            StmtKind::Assign { lhs, rhs } => {
                self.creates_of_expr(lhs, visited, out);
                self.creates_of_expr(rhs, visited, out);
            }
            StmtKind::Insert { lhs, index, value } => {
                self.creates_of_expr(lhs, visited, out);
                self.creates_of_expr(index, visited, out);
                self.creates_of_expr(value, visited, out);
            }
            StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
                self.creates_of_expr(lhs, visited, out);
                self.creates_of_expr(value, visited, out);
            }
            StmtKind::While { cond, body } => {
                self.creates_of_expr(cond, visited, out);
                self.creates_of_stmt(body, visited, out);
            }
            StmtKind::Foreach { collection, body, .. } => {
                self.creates_of_expr(collection, visited, out);
                self.creates_of_stmt(body, visited, out);
            }
            StmtKind::If { cond, then_branch, else_branch } => {
                self.creates_of_expr(cond, visited, out);
                self.creates_of_stmt(then_branch, visited, out);
                self.creates_of_stmt(else_branch, visited, out);
            }
            StmtKind::Ctor { interface, payload } => {
                out.push(*interface);
                if let Some(p) = payload {
                    self.creates_of_expr(p, visited, out);
                }
            }
            StmtKind::FunCall { fun, args } => {
                self.creates_of_fun(*fun, visited, out);
                for a in args {
                    self.creates_of_expr(a, visited, out);
                }
            }
            StmtKind::Raise { event, payload } | StmtKind::Announce { event, payload } => {
                self.creates_of_expr(event, visited, out);
                if let Some(p) = payload {
                    self.creates_of_expr(p, visited, out);
                }
            }
            StmtKind::Send { target, event, payload } => {
                self.creates_of_expr(target, visited, out);
                self.creates_of_expr(event, visited, out);
                if let Some(p) = payload {
                    self.creates_of_expr(p, visited, out);
                }
            }
            StmtKind::Goto { payload, .. } => {
                if let Some(p) = payload {
                    self.creates_of_expr(p, visited, out);
                }
            }
            StmtKind::Receive { cases } => {
                for (_, f) in cases {
                    self.creates_of_fun(*f, visited, out);
                }
            }
        }
    }

    fn creates_of_expr(&self, x: &Expr, visited: &mut Vec<FunId>, out: &mut Vec<InterfaceId>) {
        match &x.kind {
            ExprKind::Ctor { interface, payload } => {
                out.push(*interface);
                if let Some(p) = payload {
                    self.creates_of_expr(p, visited, out);
                }
            }
            ExprKind::FunCall { fun, args } => {
                self.creates_of_fun(*fun, visited, out);
                for a in args {
                    self.creates_of_expr(a, visited, out);
                }
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.creates_of_expr(lhs, visited, out);
                self.creates_of_expr(rhs, visited, out);
            }
            ExprKind::Contains { item, collection } => {
                self.creates_of_expr(item, visited, out);
                self.creates_of_expr(collection, visited, out);
            }
            ExprKind::Cast(b) | ExprKind::Coerce(b) | ExprKind::Keys(b) | ExprKind::Values(b) | ExprKind::Sizeof(b) => {
                self.creates_of_expr(b, visited, out)
            }
            ExprKind::Unary { operand, .. } => self.creates_of_expr(operand, visited, out),
            ExprKind::Choose(Some(b)) => self.creates_of_expr(b, visited, out),
            ExprKind::TupleAccess { base, .. } | ExprKind::NamedTupleAccess { base, .. } => {
                self.creates_of_expr(base, visited, out)
            }
            ExprKind::SeqAccess { base, index } | ExprKind::SetAccess { base, index } => {
                self.creates_of_expr(base, visited, out);
                self.creates_of_expr(index, visited, out);
            }
            ExprKind::MapAccess { base, key } => {
                self.creates_of_expr(base, visited, out);
                self.creates_of_expr(key, visited, out);
            }
            ExprKind::Tuple(fs) | ExprKind::NamedTuple(fs) => {
                for f in fs {
                    self.creates_of_expr(f, visited, out);
                }
            }
            ExprKind::Str { args, .. } => {
                for a in args {
                    self.creates_of_expr(a, visited, out);
                }
            }
            _ => {}
        }
    }
}
