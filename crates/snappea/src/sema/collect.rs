//! Declaration collection: `DeclarationStubVisitor`, `DeclarationVisitor` and `TypeResolver`.

use crate::sema::analyzer::{Analyzer, FunSrc};
use crate::sema::diag::Fatal;
use crate::sema::program::*;
use crate::sema::scope::{Namespace, ScopeId};
use crate::sema::types::{PType, PermissionOrigin};
use crate::syntax::ast;
use crate::syntax::Span;

/// `int.TryParse(text, NumberStyles.None)`: digits only, must fit in a 32-bit int.
pub(crate) fn parse_int_literal(text: &str) -> Option<i64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i32>().ok().map(|v| v as i64)
}

impl<'a> Analyzer<'a> {
    fn check_conflict(&self, scope: ScopeId, name: &str, span: Span, namespaces: &[Namespace]) -> Result<(), Fatal> {
        if let Some(existing) = self.scopes.get(scope).conflict(name, namespaces) {
            // named modules are not in the program yet
            let (ename, espan) = match existing {
                DeclRef::Module(i) => (self.module_src[i as usize].0.clone(), self.module_src[i as usize].2),
                _ => (self.prog.decl_name(existing), self.prog.decl_span(existing)),
            };
            return Err(self.e.duplicate_declaration(span, name, &ename, espan));
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Stub pass
    // ------------------------------------------------------------------

    pub(crate) fn collect_stubs(&mut self, program: &'a ast::Program) -> Result<(), Fatal> {
        let g = crate::sema::scope::Scopes::GLOBAL;
        for d in &program.decls {
            match d {
                ast::TopDecl::GlobalParam { names, .. } => {
                    for n in names {
                        self.check_conflict(g, &n.name, n.span, &[Namespace::Variables])?;
                        let slot = self.prog.global_params.len() as u32;
                        let v = self.new_var(&n.name, VarRole::GlobalParam, PType::Error, slot, n.span);
                        self.prog.global_params.push(v);
                        let s = self.scopes.get_mut(g);
                        s.variables.insert(n.name.clone(), v);
                        s.var_list.push(v);
                    }
                }
                ast::TopDecl::Event { name, span, .. } => {
                    self.check_conflict(g, &name.name, *span, &[Namespace::Events, Namespace::EnumElems])?;
                    self.prog.events.push(EventDecl { name: name.name.clone(), payload: PType::Null, span: *span });
                    let id = EventId(self.prog.events.len() as u32 - 1);
                    self.scopes.get_mut(g).events.insert(name.name.clone(), id);
                }
                ast::TopDecl::EventSet { name, span, .. } => {
                    self.check_conflict(g, &name.name, *span, &[Namespace::EventSets, Namespace::Interfaces])?;
                    self.prog.event_sets.push(EventSetDecl {
                        name: name.name.clone(),
                        events: Vec::new(),
                        span: *span,
                    });
                    let id = EventSetId(self.prog.event_sets.len() as u32 - 1);
                    self.scopes.get_mut(g).event_sets.insert(name.name.clone(), id);
                }
                ast::TopDecl::Interface { name, span, .. } => {
                    self.check_conflict(
                        g,
                        &name.name,
                        *span,
                        &[Namespace::Interfaces, Namespace::Enums, Namespace::Typedefs, Namespace::Machines],
                    )?;
                    self.prog.interfaces.push(InterfaceDecl {
                        name: name.name.clone(),
                        payload: PType::Null,
                        receives: None,
                        implicit_for: None,
                        span: *span,
                    });
                    let id = InterfaceId(self.prog.interfaces.len() as u32 - 1);
                    self.scopes.get_mut(g).interfaces.insert(name.name.clone(), id);
                }
                ast::TopDecl::TypeDef { name, ty, span } => {
                    self.check_conflict(
                        g,
                        &name.name,
                        *span,
                        &[Namespace::Typedefs, Namespace::Enums, Namespace::Interfaces, Namespace::Machines],
                    )?;
                    self.prog.typedefs.push(TypeDefDecl { name: name.name.clone(), ty: PType::Error, span: *span });
                    let id = TypeDefId(self.prog.typedefs.len() as u32 - 1);
                    self.typedef_src.push(ty.as_ref());
                    self.typedef_resolved.push(false);
                    self.scopes.get_mut(g).typedefs.insert(name.name.clone(), id);
                }
                ast::TopDecl::Enum { name, elems, span } => {
                    self.check_conflict(
                        g,
                        &name.name,
                        *span,
                        &[Namespace::Enums, Namespace::Interfaces, Namespace::Typedefs, Namespace::Machines],
                    )?;
                    self.prog.enums.push(EnumDecl { name: name.name.clone(), elems: Vec::new(), span: *span });
                    let id = EnumId(self.prog.enums.len() as u32 - 1);
                    self.scopes.get_mut(g).enums.insert(name.name.clone(), id);
                    for (i, el) in elems.iter().enumerate() {
                        self.check_conflict(
                            g,
                            &el.name.name,
                            el.name.span,
                            &[Namespace::EnumElems, Namespace::Events],
                        )?;
                        self.prog.enums[id.idx()].elems.push(EnumElemDecl {
                            name: el.name.name.clone(),
                            value: 0,
                            span: el.name.span,
                        });
                        self.scopes.get_mut(g).enum_elems.insert(el.name.name.clone(), (id, i as u32));
                    }
                }
                ast::TopDecl::Machine(m) => {
                    self.check_conflict(
                        g,
                        &m.name.name,
                        m.span,
                        &[Namespace::Machines, Namespace::Interfaces, Namespace::Enums, Namespace::Typedefs],
                    )?;
                    let kind = match m.kind {
                        ast::MachineKind::Impl => MachineKind::Impl,
                        ast::MachineKind::Spec => MachineKind::Spec,
                        ast::MachineKind::Scenario => MachineKind::Scenario,
                    };
                    self.prog.machines.push(MachineDecl {
                        name: m.name.name.clone(),
                        kind,
                        span: m.span,
                        receives: None,
                        sends: None,
                        observes: Vec::new(),
                        fields: Vec::new(),
                        methods: Vec::new(),
                        states: Vec::new(),
                        start_state: None,
                        payload: PType::Null,
                        creates: Vec::new(),
                        interface: None,
                        cs_name: m.name.name.clone(),
                    });
                    let mid = MachineId(self.prog.machines.len() as u32 - 1);
                    self.scopes.get_mut(g).machines.insert(m.name.name.clone(), mid);
                    self.scopes.get_mut(g).machine_list.push(mid);
                    if kind == MachineKind::Impl {
                        self.prog.interfaces.push(InterfaceDecl {
                            name: m.name.name.clone(),
                            payload: PType::Null,
                            receives: None,
                            implicit_for: Some(mid),
                            span: m.span,
                        });
                        let iid = InterfaceId(self.prog.interfaces.len() as u32 - 1);
                        self.scopes.get_mut(g).interfaces.insert(m.name.name.clone(), iid);
                        self.prog.machines[mid.idx()].interface = Some(iid);
                    }
                    let ms = self.scopes.child(g);
                    self.machine_scopes.push(ms);
                    self.machine_src.push(m);
                    // body stubs
                    for v in &m.vars {
                        for n in &v.names {
                            self.check_conflict(ms, &n.name, n.span, &[Namespace::Variables])?;
                            let slot = self.prog.machines[mid.idx()].fields.len() as u32;
                            let vid = self.new_var(&n.name, VarRole::Field, PType::Error, slot, n.span);
                            self.prog.machines[mid.idx()].fields.push(vid);
                            let s = self.scopes.get_mut(ms);
                            s.variables.insert(n.name.clone(), vid);
                            s.var_list.push(vid);
                        }
                    }
                    for f in &m.funs {
                        self.stub_fun(ms, f, Some(mid))?;
                    }
                    for st in &m.states {
                        self.check_conflict(ms, &st.name.name, st.span, &[Namespace::States])?;
                        self.prog.states.push(StateDecl {
                            name: st.name.name.clone(),
                            machine: mid,
                            is_start: false,
                            temperature: Temperature::Warm,
                            entry: None,
                            exit: None,
                            handlers: Vec::new(),
                            span: st.span,
                        });
                        let sid = StateId(self.prog.states.len() as u32 - 1);
                        self.state_src.push(st);
                        self.scopes.get_mut(ms).states.insert(st.name.name.clone(), sid);
                        self.prog.machines[mid.idx()].states.push(sid);
                    }
                }
                ast::TopDecl::Fun(f) => {
                    self.stub_fun(g, f, None)?;
                }
                ast::TopDecl::Pure(p) => {
                    let idx = self.scopes.get(g).pures.len() as u32;
                    self.check_conflict(g, &p.name.name, p.name.span, &[Namespace::Pures])?;
                    self.scopes.get_mut(g).pures.insert(p.name.name.clone(), idx);
                }
                ast::TopDecl::Invariant(inv) => {
                    let idx = self.scopes.get(g).invariants.len() as u32;
                    self.check_conflict(g, &inv.name.name, inv.name.span, &[Namespace::Invariants])?;
                    self.scopes.get_mut(g).invariants.insert(inv.name.name.clone(), idx);
                }
                ast::TopDecl::InvariantGroup { name, invariants, .. } => {
                    let idx = self.scopes.get(g).invariant_groups.len() as u32;
                    self.check_conflict(g, &name.name, name.span, &[Namespace::InvariantGroups])?;
                    self.scopes.get_mut(g).invariant_groups.insert(name.name.clone(), idx);
                    for inv in invariants {
                        let n = format!("{}_{}", name.name, inv.name.name);
                        let i2 = self.scopes.get(g).invariants.len() as u32;
                        self.check_conflict(g, &n, inv.name.span, &[Namespace::Invariants])?;
                        self.scopes.get_mut(g).invariants.insert(n, i2);
                    }
                }
                ast::TopDecl::Axiom { .. } | ast::TopDecl::AssumeOnStart { .. } | ast::TopDecl::ProofBlock { .. } => {}
                ast::TopDecl::Module { name, expr, span } => {
                    self.check_conflict(g, &name.name, *span, &[Namespace::NamedModules])?;
                    let idx = self.module_src.len() as u32;
                    self.module_src.push((name.name.clone(), expr, *span));
                    self.scopes.get_mut(g).named_modules.insert(name.name.clone(), idx);
                }
                ast::TopDecl::SafetyTest(t) => {
                    self.check_conflict(
                        g,
                        &t.name.name,
                        t.span,
                        &[Namespace::Implementations, Namespace::SafetyTests, Namespace::RefinementTests],
                    )?;
                    let main = MachineId(0); // resolved by the module system
                    self.prog.tests.push(TestDecl {
                        name: t.name.name.clone(),
                        main,
                        module: ModuleInfo::default(),
                        assignments: Vec::new(),
                        span: t.span,
                    });
                    let id = TestId(self.prog.tests.len() as u32 - 1);
                    self.test_src.push(t);
                    self.scopes.get_mut(g).safety_tests.insert(t.name.name.clone(), id);
                }
                ast::TopDecl::RefinementTest { name, main, lhs, rhs, span } => {
                    self.check_conflict(
                        g,
                        &name.name,
                        *span,
                        &[Namespace::Implementations, Namespace::SafetyTests, Namespace::RefinementTests],
                    )?;
                    let idx = self.refinement_src.len() as u32;
                    self.refinement_src.push((name.name.clone(), lhs, rhs, *span, main.name.clone()));
                    self.scopes.get_mut(g).refinement_tests.insert(name.name.clone(), idx);
                }
                ast::TopDecl::Implementation { name, span, .. } => {
                    self.check_conflict(
                        g,
                        &name.name,
                        *span,
                        &[Namespace::Implementations, Namespace::SafetyTests, Namespace::RefinementTests],
                    )?;
                    self.prog.impls.push(ImplDecl {
                        name: name.name.clone(),
                        main: None,
                        module: ModuleInfo::default(),
                        span: *span,
                    });
                    let id = ImplId(self.prog.impls.len() as u32 - 1);
                    self.impl_src.push(d);
                    self.scopes.get_mut(g).implementations.insert(name.name.clone(), id);
                }
            }
        }
        Ok(())
    }

    fn stub_fun(&mut self, scope: ScopeId, f: &'a ast::FunDecl, owner: Option<MachineId>) -> Result<FunId, Fatal> {
        self.check_conflict(scope, &f.name.name, f.span, &[Namespace::Functions, Namespace::States])?;
        let fs = self.scopes.child(scope);
        let id = self.new_fun(&f.name.name, owner, fs, FunSrc::Named(f), f.span);
        self.scopes.get_mut(scope).functions.insert(f.name.name.clone(), id);
        if let Some(m) = owner {
            self.prog.machines[m.idx()].methods.push(id);
            self.prog.funs[id.idx()].role.method = true;
        }
        for p in &f.params {
            self.check_conflict(fs, &p.name.name, p.name.span, &[Namespace::Variables])?;
            let slot = self.alloc_slot(id);
            let v = self.new_var(&p.name.name, VarRole::Param, PType::Error, slot, p.name.span);
            let s = self.scopes.get_mut(fs);
            s.variables.insert(p.name.name.clone(), v);
            s.var_list.push(v);
            self.prog.funs[id.idx()].params.push(v);
        }
        Ok(id)
    }

    // ------------------------------------------------------------------
    // Type resolution
    // ------------------------------------------------------------------

    pub(crate) fn resolve_type(&mut self, t: &ast::Type, scope: ScopeId) -> Result<PType, Fatal> {
        Ok(match &t.kind {
            ast::TypeKind::Seq(e) => PType::seq(self.resolve_type(e, scope)?),
            ast::TypeKind::Set(e) => PType::set(self.resolve_type(e, scope)?),
            ast::TypeKind::Map(k, v) => PType::map(self.resolve_type(k, scope)?, self.resolve_type(v, scope)?),
            ast::TypeKind::Tuple(ts) => {
                let mut v = Vec::new();
                for t in ts {
                    v.push(self.resolve_type(t, scope)?);
                }
                PType::Tuple(v)
            }
            ast::TypeKind::NamedTuple(fs) => {
                let mut names: Vec<&str> = Vec::new();
                let mut v = Vec::new();
                for (n, t) in fs {
                    if names.contains(&n.name.as_str()) {
                        return Err(self.e.duplicate_named_tuple_entry(n.span, &n.name));
                    }
                    names.push(&n.name);
                    v.push((n.name.clone(), self.resolve_type(t, scope)?));
                }
                PType::NamedTuple(v)
            }
            ast::TypeKind::Bool => PType::Bool,
            ast::TypeKind::Int => PType::Int,
            ast::TypeKind::Float => PType::Float,
            ast::TypeKind::String => PType::String,
            ast::TypeKind::Event => PType::Event,
            ast::TypeKind::Machine => PType::Machine,
            ast::TypeKind::Data => PType::Data,
            ast::TypeKind::Any => PType::Any,
            ast::TypeKind::Named(n) => self.resolve_named_type(n, scope)?,
        })
    }

    fn resolve_named_type(&mut self, n: &ast::Ident, scope: ScopeId) -> Result<PType, Fatal> {
        if let Some(e) = self.scopes.lookup_enum(scope, &n.name) {
            return Ok(PType::Enum(e));
        }
        if let Some(td) = self.scopes.lookup_typedef(scope, &n.name) {
            if self.typedef_stack.contains(&td) {
                return Err(self.e.circular_typedef(n.span, &n.name));
            }
            if !self.typedef_resolved[td.idx()] {
                self.typedef_stack.push(td);
                let ty = match self.typedef_src[td.idx()] {
                    None => PType::Foreign(self.prog.typedefs[td.idx()].name.clone()),
                    Some(t) => {
                        let r = self.resolve_type(t, crate::sema::scope::Scopes::GLOBAL);
                        match r {
                            Ok(t) => t,
                            Err(e) => {
                                self.typedef_stack.pop();
                                return Err(e);
                            }
                        }
                    }
                };
                self.typedef_stack.pop();
                self.prog.typedefs[td.idx()].ty = ty;
                self.typedef_resolved[td.idx()] = true;
            }
            return Ok(PType::TypeDef(td));
        }
        if let Some(es) = self.scopes.lookup_event_set(scope, &n.name) {
            return Ok(PType::Permission(PermissionOrigin::EventSet(es)));
        }
        if let Some(i) = self.scopes.lookup_interface(scope, &n.name) {
            return Ok(PType::Permission(PermissionOrigin::Interface(i)));
        }
        if let Some(m) = self.scopes.lookup_machine(scope, &n.name) {
            return Ok(PType::Permission(PermissionOrigin::Machine(m)));
        }
        if let Some(ev) = self.scopes.lookup_event(scope, &n.name) {
            return Ok(PType::Permission(PermissionOrigin::Event(ev)));
        }
        Err(self.e.missing_declaration(n.span, "enum, typedef, event set, machine, or interface", &n.name))
    }

    pub(crate) fn resolve_opt_type(&mut self, t: &Option<ast::Type>, scope: ScopeId) -> Result<PType, Fatal> {
        match t {
            None => Ok(PType::Null),
            Some(t) => self.resolve_type(t, scope),
        }
    }

    fn lookup_event_or_err(&self, scope: ScopeId, n: &ast::Ident) -> Result<EventId, Fatal> {
        self.scopes.lookup_event(scope, &n.name).ok_or_else(|| self.e.missing_declaration(n.span, "event", &n.name))
    }

    /// Resolves an event list into a sorted, de-duplicated set (upstream `EventSet`).
    fn event_set(&self, scope: ScopeId, names: &[ast::Ident]) -> Result<Vec<EventId>, Fatal> {
        let mut v = Vec::new();
        for n in names {
            v.push(self.lookup_event_or_err(scope, n)?);
        }
        Ok(self.sorted_event_set(v))
    }

    pub(crate) fn sorted_event_set(&self, mut v: Vec<EventId>) -> Vec<EventId> {
        v.sort_by(|a, b| self.prog.events[a.idx()].name.cmp(&self.prog.events[b.idx()].name));
        v.dedup();
        v
    }

    // ------------------------------------------------------------------
    // Declaration pass
    // ------------------------------------------------------------------

    pub(crate) fn collect_declarations(
        &mut self,
        program: &'a ast::Program,
        machine_iter: &mut usize,
        fun_cursor: &mut FunCursor,
    ) -> Result<(), Fatal> {
        let g = crate::sema::scope::Scopes::GLOBAL;
        // machine/function ids are assigned in stub order; walk the AST again in the same order
        for d in &program.decls {
            match d {
                ast::TopDecl::GlobalParam { names, ty, .. } => {
                    let t = self.resolve_type(ty, g)?;
                    for n in names {
                        let v = self.scopes.get(g).variables[&n.name];
                        self.prog.var_decls[v.idx()].ty = t.clone();
                    }
                }
                ast::TopDecl::Event { name, payload, .. } => {
                    let id = self.scopes.get(g).events[&name.name];
                    let t = self.resolve_opt_type(payload, g)?;
                    self.prog.events[id.idx()].payload = t;
                }
                ast::TopDecl::Interface { name, payload, receives, .. } => {
                    let id = self.scopes.get(g).interfaces[&name.name];
                    let t = self.resolve_opt_type(payload, g)?;
                    self.prog.interfaces[id.idx()].payload = t;
                    if let Some(r) = receives {
                        let set = self.event_set(g, r)?;
                        self.prog.interfaces[id.idx()].receives = Some(set);
                    }
                }
                ast::TopDecl::TypeDef { name, .. } => {
                    let id = self.scopes.get(g).typedefs[&name.name];
                    // Like upstream's VisitPTypeDef: resolve the right-hand side directly,
                    // without pushing this typedef on the cycle stack, so a cycle is
                    // reported at the first typedef met twice along the way.
                    match self.typedef_src[id.idx()] {
                        Some(t) => {
                            let ty = self.resolve_type(t, g)?;
                            self.prog.typedefs[id.idx()].ty = ty;
                            self.typedef_resolved[id.idx()] = true;
                        }
                        None => {
                            self.resolve_named_type(name, g)?;
                        }
                    }
                }
                ast::TopDecl::Enum { name, elems, .. } => {
                    let id = self.scopes.get(g).enums[&name.name];
                    for (i, el) in elems.iter().enumerate() {
                        let value = match &el.value {
                            None => i as i64,
                            Some((text, span)) => {
                                parse_int_literal(text).ok_or_else(|| self.e.value_out_of_range(*span, "int"))?
                            }
                        };
                        self.prog.enums[id.idx()].elems[i].value = value;
                    }
                }
                ast::TopDecl::EventSet { name, events, .. } => {
                    let id = self.scopes.get(g).event_sets[&name.name];
                    let set = self.event_set(g, events)?;
                    self.prog.event_sets[id.idx()].events = set;
                }
                ast::TopDecl::Machine(m) => {
                    let mid = self.scopes.get(g).machine_list[*machine_iter];
                    *machine_iter += 1;
                    self.declare_machine(mid, m, fun_cursor)?;
                }
                ast::TopDecl::Fun(f) => {
                    let id = fun_cursor.next_named(&self.fun_src, f);
                    self.declare_fun(id, f)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn declare_fun(&mut self, id: FunId, f: &'a ast::FunDecl) -> Result<(), Fatal> {
        let fs = self.fun_scopes[id.idx()];
        let params = self.prog.funs[id.idx()].params.clone();
        for (p, v) in f.params.iter().zip(params.iter()) {
            let t = self.resolve_type(&p.ty, fs)?;
            self.prog.var_decls[v.idx()].ty = t;
        }
        let ret = self.resolve_opt_type(&f.ret, fs)?;
        self.prog.funs[id.idx()].ret = ret;
        if f.body.is_none() {
            self.prog.funs[id.idx()].role.foreign = true;
            for c in &f.creates {
                let i = self
                    .scopes
                    .lookup_interface(fs, &c.name)
                    .ok_or_else(|| self.e.missing_declaration(c.span, "interface", &c.name))?;
                self.prog.funs[id.idx()].creates.push(i);
            }
            if let Some(rp) = &f.return_param {
                let t = self.resolve_type(&rp.ty, fs)?;
                self.prog.funs[id.idx()].ret = t;
            }
        }
        Ok(())
    }

    fn declare_machine(
        &mut self,
        mid: MachineId,
        m: &'a ast::MachineDecl,
        cursor: &mut FunCursor,
    ) -> Result<(), Fatal> {
        let g = crate::sema::scope::Scopes::GLOBAL;
        let ms = self.machine_scopes[mid.idx()];
        match m.kind {
            ast::MachineKind::Impl => {
                let mut recv: Option<Vec<EventId>> = None;
                let mut send: Option<Vec<EventId>> = None;
                for (events, _) in &m.receives {
                    let set = self.event_set(g, events)?;
                    recv.get_or_insert_with(Vec::new).extend(set);
                }
                for (events, _) in &m.sends {
                    let set = self.event_set(g, events)?;
                    send.get_or_insert_with(Vec::new).extend(set);
                }
                self.prog.machines[mid.idx()].receives = recv.map(|v| self.sorted_event_set(v));
                self.prog.machines[mid.idx()].sends = send.map(|v| self.sorted_event_set(v));
            }
            _ => {
                let obs = self.event_set(g, &m.observes)?;
                self.prog.machines[mid.idx()].observes = obs;
                self.prog.machines[mid.idx()].receives = Some(Vec::new());
                self.prog.machines[mid.idx()].sends = Some(Vec::new());
            }
        }
        // fields
        let mut field_iter = 0usize;
        for v in &m.vars {
            let t = self.resolve_type(&v.ty, ms)?;
            for n in &v.names {
                let vid = self.prog.machines[mid.idx()].fields[field_iter];
                field_iter += 1;
                if let Some(gp) = self.scopes.get(g).variables.get(&n.name).copied() {
                    if self.prog.var_decls[gp.idx()].role == VarRole::GlobalParam {
                        let gspan = self.prog.var_decls[gp.idx()].span;
                        return Err(self.e.redeclare_global_param(n.span, &n.name, &n.name, gspan));
                    }
                }
                self.prog.var_decls[vid.idx()].ty = t.clone();
            }
        }
        // named functions
        for f in &m.funs {
            let id = cursor.next_named(&self.fun_src, f);
            self.declare_fun(id, f)?;
        }
        // states
        let states = self.prog.machines[mid.idx()].states.clone();
        for (sid, st) in states.iter().zip(m.states.iter()) {
            self.declare_state(mid, *sid, st)?;
        }
        if let Some(iid) = self.prog.machines[mid.idx()].interface {
            self.prog.interfaces[iid.idx()].receives = self.prog.machines[mid.idx()].receives.clone();
        }
        Ok(())
    }

    fn create_anon_fun(&mut self, mid: MachineId, h: &'a ast::AnonHandler) -> Result<FunId, Fatal> {
        let ms = self.machine_scopes[mid.idx()];
        let fs = self.scopes.child(ms);
        let id = self.new_fun("", Some(mid), fs, FunSrc::Anon(h), h.span);
        self.prog.machines[mid.idx()].methods.push(id);
        self.prog.funs[id.idx()].role.method = true;
        if let Some(p) = &h.param {
            self.check_conflict(fs, &p.name.name, p.name.span, &[Namespace::Variables])?;
            let slot = self.alloc_slot(id);
            let t = self.resolve_type(&p.ty, fs)?;
            let v = self.new_var(&p.name.name, VarRole::Param, t, slot, p.name.span);
            let s = self.scopes.get_mut(fs);
            s.variables.insert(p.name.name.clone(), v);
            s.var_list.push(v);
            self.prog.funs[id.idx()].params.push(v);
        }
        Ok(id)
    }

    fn resolve_handler(&mut self, mid: MachineId, h: &'a ast::Handler) -> Result<FunId, Fatal> {
        match h {
            ast::Handler::Anon(a) => self.create_anon_fun(mid, a),
            ast::Handler::Named(n) => {
                let ms = self.machine_scopes[mid.idx()];
                self.scopes
                    .lookup_fun(ms, &n.name)
                    .ok_or_else(|| self.e.missing_declaration(n.span, "function", &n.name))
            }
        }
    }

    fn declare_state(&mut self, mid: MachineId, sid: StateId, st: &'a ast::StateDecl) -> Result<(), Fatal> {
        let ms = self.machine_scopes[mid.idx()];
        let is_spec = self.prog.machines[mid.idx()].is_spec();
        let mname = self.prog.machines[mid.idx()].name.clone();
        self.prog.states[sid.idx()].is_start = st.is_start;
        self.prog.states[sid.idx()].temperature = match st.temperature {
            ast::Temperature::Warm => Temperature::Warm,
            ast::Temperature::Hot => Temperature::Hot,
            ast::Temperature::Cold => Temperature::Cold,
        };
        for item in &st.items {
            match &item.kind {
                ast::StateItemKind::Entry(h) => {
                    let f = self.resolve_handler(mid, h)?;
                    self.prog.funs[f.idx()].role.entry = true;
                    if let Some(existing) = self.prog.states[sid.idx()].entry {
                        let espan = self.prog.funs[existing.idx()].span;
                        let sname = self.prog.states[sid.idx()].name.clone();
                        return Err(self.e.duplicate_state_entry(item.span, espan, &sname));
                    }
                    self.prog.states[sid.idx()].entry = Some(f);
                }
                ast::StateItemKind::Exit(h) => {
                    let f = self.resolve_handler(mid, h)?;
                    self.prog.funs[f.idx()].role.exit = true;
                    if let Some(existing) = self.prog.states[sid.idx()].exit {
                        let espan = self.prog.funs[existing.idx()].span;
                        let sname = self.prog.states[sid.idx()].name.clone();
                        return Err(self.e.duplicate_state_exit(item.span, espan, &sname));
                    }
                    self.prog.states[sid.idx()].exit = Some(f);
                }
                ast::StateItemKind::Defer(events) => {
                    if is_spec {
                        return Err(self.e.deferred_event_in_monitor(item.span, &mname));
                    }
                    let mut actions = Vec::new();
                    for ev in events {
                        let e = self.lookup_event_or_err(ms, ev)?;
                        actions.push(EventHandler { event: e, action: HandlerAction::Defer, span: ev.span });
                    }
                    self.add_handlers(sid, actions, is_spec, &mname)?;
                }
                ast::StateItemKind::Ignore(events) => {
                    let mut actions = Vec::new();
                    for ev in events {
                        let e = self.lookup_event_or_err(ms, ev)?;
                        actions.push(EventHandler { event: e, action: HandlerAction::Ignore, span: ev.span });
                    }
                    self.add_handlers(sid, actions, is_spec, &mname)?;
                }
                ast::StateItemKind::OnDo { events, handler } => {
                    let f = self.resolve_handler(mid, handler)?;
                    self.prog.funs[f.idx()].role.event_handler = true;
                    let mut actions = Vec::new();
                    for ev in events {
                        let e = self.lookup_event_or_err(ms, ev)?;
                        actions.push(EventHandler { event: e, action: HandlerAction::Do(f), span: ev.span });
                    }
                    self.add_handlers(sid, actions, is_spec, &mname)?;
                }
                ast::StateItemKind::OnGoto { events, target, with } => {
                    let tf = match with {
                        Some(h) => {
                            let f = self.resolve_handler(mid, h)?;
                            self.prog.funs[f.idx()].role.transition = true;
                            Some(f)
                        }
                        None => None,
                    };
                    let target_state = self
                        .scopes
                        .get(ms)
                        .states
                        .get(&target.name)
                        .copied()
                        .ok_or_else(|| self.e.missing_declaration(target.span, "state", &target.name))?;
                    let mut actions = Vec::new();
                    for ev in events {
                        let e = self.lookup_event_or_err(ms, ev)?;
                        actions.push(EventHandler {
                            event: e,
                            action: HandlerAction::Goto { target: target_state, with: tf },
                            span: ev.span,
                        });
                    }
                    self.add_handlers(sid, actions, is_spec, &mname)?;
                }
            }
        }
        if st.is_start {
            if let Some(existing) = self.prog.machines[mid.idx()].start_state {
                let dup = self.prog.states[sid.idx()].name.clone();
                let orig = self.prog.states[existing.idx()].name.clone();
                return Err(self.e.duplicate_start_state(st.span, &dup, &orig, &mname));
            }
            self.prog.machines[mid.idx()].start_state = Some(sid);
        }
        Ok(())
    }

    fn add_handlers(
        &mut self,
        sid: StateId,
        actions: Vec<EventHandler>,
        is_spec: bool,
        mname: &str,
    ) -> Result<(), Fatal> {
        for a in actions {
            if let Some(existing) = self.prog.states[sid.idx()].handler_for(a.event) {
                let sname = self.prog.states[sid.idx()].name.clone();
                return Err(self.e.duplicate_event_action(a.span, existing.span, &sname));
            }
            if self.prog.events[a.event.idx()].is_null() && is_spec {
                return Err(self.e.null_transition_in_monitor(a.span, mname));
            }
            self.prog.states[sid.idx()].handlers.push(a);
        }
        Ok(())
    }
}

/// Walks named functions in stub order so the declaration pass can find their ids.
#[derive(Default)]
pub(crate) struct FunCursor {
    pos: usize,
}

impl FunCursor {
    /// The id of `decl`, the next named function in stub order.
    fn next_named(&mut self, src: &[FunSrc<'_>], decl: &ast::FunDecl) -> FunId {
        while !matches!(src[self.pos], FunSrc::Named(_)) {
            self.pos += 1;
        }
        debug_assert!(
            matches!(src[self.pos], FunSrc::Named(d) if std::ptr::eq(d, decl)),
            "function `{}` declared out of stub order",
            decl.name.name
        );
        let id = FunId(self.pos as u32);
        self.pos += 1;
        id
    }
}
