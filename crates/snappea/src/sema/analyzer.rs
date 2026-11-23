//! Analyzer state shared by all passes.

use crate::sema::diag::{Diagnostics, Fatal};
use crate::sema::errors::Errors;
use crate::sema::program::*;
use crate::sema::scope::{ScopeId, Scopes};
use crate::sema::types::PType;
use crate::syntax::ast;
use crate::syntax::{SourceMap, Span};

/// Where a function's body comes from.
#[derive(Clone, Copy)]
pub enum FunSrc<'a> {
    Named(&'a ast::FunDecl),
    Anon(&'a ast::AnonHandler),
    /// Receive-case handlers are populated when the enclosing statement is checked.
    Receive,
    None,
}

pub struct Analyzer<'a> {
    pub sm: &'a SourceMap,
    pub e: Errors<'a>,
    pub prog: Program,
    pub scopes: Scopes,
    pub diags: Diagnostics,
    pub machine_scopes: Vec<ScopeId>,
    pub machine_src: Vec<&'a ast::MachineDecl>,
    pub state_src: Vec<&'a ast::StateDecl>,
    pub fun_scopes: Vec<ScopeId>,
    pub fun_src: Vec<FunSrc<'a>>,
    /// Root function whose frame holds this function's variables.
    pub fun_root: Vec<FunId>,
    /// Next free frame slot per function (meaningful for roots).
    pub fun_next_slot: Vec<u32>,
    pub typedef_src: Vec<Option<&'a ast::Type>>,
    pub typedef_resolved: Vec<bool>,
    pub typedef_stack: Vec<TypeDefId>,
    pub module_src: Vec<(String, &'a ast::ModExpr, Span)>,
    pub test_src: Vec<&'a ast::SafetyTestDecl>,
    pub refinement_src: Vec<(String, &'a ast::ModExpr, &'a ast::ModExpr, Span, String)>,
    pub impl_src: Vec<&'a ast::TopDecl>,
}

impl<'a> Analyzer<'a> {
    pub fn new(sm: &'a SourceMap, continue_on_error: bool) -> Analyzer<'a> {
        let mut prog = Program::default();
        // built-in events
        prog.events.push(EventDecl { name: "null".into(), payload: PType::Null, span: Span::dummy() });
        prog.events.push(EventDecl { name: "halt".into(), payload: PType::Null, span: Span::dummy() });
        prog.null_event = EventId(0);
        prog.halt_event = EventId(1);
        let mut scopes = Scopes::new();
        let g = scopes.global();
        scopes.get_mut(g).events.insert("null".into(), EventId(0));
        scopes.get_mut(g).events.insert("halt".into(), EventId(1));
        Analyzer {
            sm,
            e: Errors::new(sm),
            prog,
            scopes,
            diags: Diagnostics::new(continue_on_error),
            machine_scopes: Vec::new(),
            machine_src: Vec::new(),
            state_src: Vec::new(),
            fun_scopes: Vec::new(),
            fun_src: Vec::new(),
            fun_root: Vec::new(),
            fun_next_slot: Vec::new(),
            typedef_src: Vec::new(),
            typedef_resolved: Vec::new(),
            typedef_stack: Vec::new(),
            module_src: Vec::new(),
            test_src: Vec::new(),
            refinement_src: Vec::new(),
            impl_src: Vec::new(),
        }
    }

    pub fn report(&mut self, d: Fatal) -> Result<(), Fatal> {
        self.diags.report(d)
    }

    pub fn new_var(&mut self, name: &str, role: VarRole, ty: PType, slot: u32, span: Span) -> VarId {
        self.prog.var_decls.push(VarDecl { name: name.to_string(), role, ty, slot, span });
        VarId(self.prog.var_decls.len() as u32 - 1)
    }

    pub fn new_fun(
        &mut self,
        name: &str,
        owner: Option<MachineId>,
        scope: ScopeId,
        src: FunSrc<'a>,
        span: Span,
    ) -> FunId {
        self.prog.funs.push(FunDecl {
            name: name.to_string(),
            owner,
            parent: None,
            cs_name: String::new(),
            params: Vec::new(),
            ret: PType::Null,
            locals: Vec::new(),
            frame_size: 0,
            body: None,
            role: FunRole::default(),
            span,
            can_change_state: false,
            can_raise: false,
            can_receive: false,
            can_send: false,
            can_create: false,
            is_nondet: false,
            callees: Vec::new(),
            creates: Vec::new(),
        });
        let id = FunId(self.prog.funs.len() as u32 - 1);
        self.fun_scopes.push(scope);
        self.fun_src.push(src);
        self.fun_root.push(id);
        self.fun_next_slot.push(0);
        id
    }

    /// Allocates a frame slot in the root frame of `fun`.
    pub fn alloc_slot(&mut self, fun: FunId) -> u32 {
        let root = self.fun_root[fun.idx()];
        let s = self.fun_next_slot[root.idx()];
        self.fun_next_slot[root.idx()] = s + 1;
        s
    }

    pub fn fun_name(&self, id: FunId) -> String {
        self.prog.fun_display_name(id, self.sm)
    }

    pub fn add_callee(&mut self, caller: FunId, callee: FunId) {
        let f = &mut self.prog.funs[caller.idx()];
        if !f.callees.contains(&callee) {
            f.callees.push(callee);
        }
    }
}
