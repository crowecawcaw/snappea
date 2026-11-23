//! The typed, resolved representation of a P program produced by [`crate::sema::compile`].
//!
//! Everything is stored in arenas indexed by small id newtypes so the structure is cheap to
//! clone, easy to serialize and friendly to an interpreter.

use crate::sema::types::{PType, PermissionOrigin};
use crate::syntax::Span;

macro_rules! id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        pub struct $name(pub u32);
        impl $name {
            pub fn idx(self) -> usize {
                self.0 as usize
            }
        }
    };
}

id!(EventId);
id!(EnumId);
id!(TypeDefId);
id!(EventSetId);
id!(InterfaceId);
id!(MachineId);
id!(StateId);
id!(FunId);
id!(VarId);
id!(TestId);
id!(ImplId);

/// Upstream treats every declaration kind uniformly through `IPDecl`; we mirror that where
/// error messages need "the other declaration".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclRef {
    Event(EventId),
    EnumElem(EnumId, u32),
    Enum(EnumId),
    TypeDef(TypeDefId),
    EventSet(EventSetId),
    Interface(InterfaceId),
    Machine(MachineId),
    State(StateId),
    Fun(FunId),
    Var(VarId),
    Test(TestId),
    Impl(ImplId),
    Module(u32),
    Pure(u32),
    Invariant(u32),
    Other,
}

#[derive(Debug, Clone)]
pub struct EventDecl {
    pub name: String,
    pub payload: PType,
    pub span: Span,
}

impl EventDecl {
    pub fn is_halt(&self) -> bool {
        self.name == "halt"
    }
    pub fn is_null(&self) -> bool {
        self.name == "null"
    }
    pub fn is_builtin(&self) -> bool {
        self.is_halt() || self.is_null()
    }
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub elems: Vec<EnumElemDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumElemDecl {
    pub name: String,
    pub value: i64,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeDefDecl {
    pub name: String,
    /// The aliased type; `Foreign(name)` for `type T;`.
    pub ty: PType,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EventSetDecl {
    pub name: String,
    /// Sorted by event name, unique (upstream uses a `SortedSet` by name).
    pub events: Vec<EventId>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: String,
    pub payload: PType,
    /// `None` = universal event set (all declared events plus `halt`).
    pub receives: Option<Vec<EventId>>,
    /// The machine this interface was implicitly created for, if any.
    pub implicit_for: Option<MachineId>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineKind {
    Impl,
    Spec,
    Scenario,
}

#[derive(Debug, Clone)]
pub struct MachineDecl {
    pub name: String,
    pub kind: MachineKind,
    pub span: Span,
    /// `None` = universal set.
    pub receives: Option<Vec<EventId>>,
    pub sends: Option<Vec<EventId>>,
    pub observes: Vec<EventId>,
    pub fields: Vec<VarId>,
    pub methods: Vec<FunId>,
    pub states: Vec<StateId>,
    pub start_state: Option<StateId>,
    /// Type of the start state's entry parameter (`Null` when none).
    pub payload: PType,
    /// Interfaces this machine may create, inferred from its code (sorted by name).
    pub creates: Vec<InterfaceId>,
    /// The implicit interface with the machine's name (impl machines only).
    pub interface: Option<InterfaceId>,
    /// The generated class name upstream gives the machine (`PImplementation.<cs_name>`);
    /// differs from `name` when an earlier declaration claimed it (see `csharp_names`).
    pub cs_name: String,
}

impl FunDecl {
    /// The name upstream logs when the function runs as a state action: anonymous
    /// handlers run directly, named methods through their `_<name>` wrapper.
    pub fn action_name(&self) -> String {
        if self.name.is_empty() {
            self.cs_name.clone()
        } else {
            format!("_{}", self.cs_name)
        }
    }
}

impl MachineDecl {
    pub fn is_spec(&self) -> bool {
        matches!(self.kind, MachineKind::Spec | MachineKind::Scenario)
    }
    pub fn is_scenario(&self) -> bool {
        self.kind == MachineKind::Scenario
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Temperature {
    Warm,
    Hot,
    Cold,
}

#[derive(Debug, Clone)]
pub struct StateDecl {
    pub name: String,
    pub machine: MachineId,
    pub is_start: bool,
    pub temperature: Temperature,
    pub entry: Option<FunId>,
    pub exit: Option<FunId>,
    /// Handlers in declaration order.
    pub handlers: Vec<EventHandler>,
    pub span: Span,
}

impl StateDecl {
    pub fn handler_for(&self, ev: EventId) -> Option<&EventHandler> {
        self.handlers.iter().find(|h| h.event == ev)
    }
}

#[derive(Debug, Clone)]
pub struct EventHandler {
    pub event: EventId,
    pub action: HandlerAction,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum HandlerAction {
    Defer,
    Ignore,
    Do(FunId),
    Goto { target: StateId, with: Option<FunId> },
}

/// Flags describing where a function is used (`FunctionRole` upstream).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FunRole {
    pub method: bool,
    pub entry: bool,
    pub transition: bool,
    pub event_handler: bool,
    pub exit: bool,
    pub receive: bool,
    pub foreign: bool,
}

#[derive(Debug, Clone)]
pub struct FunDecl {
    /// Empty for anonymous handlers.
    pub name: String,
    pub owner: Option<MachineId>,
    /// Enclosing function for receive-case handlers.
    pub parent: Option<FunId>,
    /// The generated method name upstream gives the function (`Anon_3`, `Foo_1`, ...); empty
    /// for receive-case handlers and foreign functions, which get no method of their own.
    pub cs_name: String,
    pub params: Vec<VarId>,
    pub ret: PType,
    /// Local variables declared directly in this function (not nested receive handlers).
    pub locals: Vec<VarId>,
    /// Number of frame slots needed to run this function (params + locals + nested
    /// receive handler variables). Only meaningful for root functions.
    pub frame_size: u32,
    pub body: Option<Block>,
    pub role: FunRole,
    pub span: Span,
    /// Analysis results.
    pub can_change_state: bool,
    pub can_raise: bool,
    pub can_receive: bool,
    pub can_send: bool,
    pub can_create: bool,
    pub is_nondet: bool,
    pub callees: Vec<FunId>,
    /// Interfaces created by a foreign function (`creates` annotation).
    pub creates: Vec<InterfaceId>,
}

impl FunDecl {
    pub fn is_foreign(&self) -> bool {
        self.body.is_none()
    }
    pub fn is_anon(&self) -> bool {
        self.name.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarRole {
    Local,
    Param,
    Field,
    GlobalParam,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub role: VarRole,
    pub ty: PType,
    /// Slot in the owning frame (fields: index in the machine's field vector; locals and
    /// params: index in the function frame; global params: index in the global table).
    pub slot: u32,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Block(Block),
    /// `message` is the full assertion message including the source location prefix.
    Assert {
        cond: Expr,
        message: Expr,
    },
    Assume {
        cond: Expr,
        message: Expr,
    },
    Print(Expr),
    Return(Option<Expr>),
    Break,
    Continue,
    Assign {
        lhs: Expr,
        rhs: Expr,
    },
    Insert {
        lhs: Expr,
        index: Expr,
        value: Expr,
    },
    Add {
        lhs: Expr,
        value: Expr,
    },
    Remove {
        lhs: Expr,
        value: Expr,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    Foreach {
        item: VarId,
        collection: Expr,
        body: Box<Stmt>,
    },
    If {
        cond: Expr,
        then_branch: Box<Stmt>,
        else_branch: Box<Stmt>,
    },
    Ctor {
        interface: InterfaceId,
        payload: Option<Expr>,
    },
    FunCall {
        fun: FunId,
        args: Vec<Expr>,
    },
    Raise {
        event: Expr,
        payload: Option<Expr>,
    },
    Send {
        target: Expr,
        event: Expr,
        payload: Option<Expr>,
    },
    Announce {
        event: Expr,
        payload: Option<Expr>,
    },
    Goto {
        state: StateId,
        payload: Option<Expr>,
    },
    Receive {
        cases: Vec<(EventId, FunId)>,
    },
    Nop,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: PType,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Then,
    Iff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Var(VarId),
    EnumElem(EnumId, u32),
    EventRef(EventId),
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    NullLit,
    /// `base` is a .NET composite format string; `{0}`..`{n}` refer to `args`.
    Str {
        base: String,
        args: Vec<Expr>,
    },
    Nondet,
    FairNondet,
    This,
    Tuple(Vec<Expr>),
    NamedTuple(Vec<Expr>),
    TupleAccess {
        base: Box<Expr>,
        index: u32,
    },
    NamedTupleAccess {
        base: Box<Expr>,
        index: u32,
    },
    SeqAccess {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    SetAccess {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    MapAccess {
        base: Box<Expr>,
        key: Box<Expr>,
    },
    Keys(Box<Expr>),
    Values(Box<Expr>),
    Sizeof(Box<Expr>),
    Default,
    Ctor {
        interface: InterfaceId,
        payload: Option<Box<Expr>>,
    },
    FunCall {
        fun: FunId,
        args: Vec<Expr>,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `item in collection`
    Contains {
        item: Box<Expr>,
        collection: Box<Expr>,
    },
    /// `e as T` (checked at runtime)
    Cast(Box<Expr>),
    /// `e to T` (numeric or permission coercion)
    Coerce(Box<Expr>),
    Choose(Option<Box<Expr>>),
    Error,
}

/// Bindings of global parameters for one expansion of a parameterized test.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamAssignment {
    pub values: Vec<(VarId, ParamValue)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Int(i64),
    Bool(bool),
}

/// Static module information computed by the module system (`ModuleInfo` upstream).
#[derive(Debug, Clone, Default)]
pub struct ModuleInfo {
    pub creates: Vec<InterfaceId>,
    pub interface_def: Vec<(InterfaceId, MachineId)>,
    /// `link_map[creator][created] = bound interface`.
    pub link_map: Vec<(InterfaceId, Vec<(InterfaceId, InterfaceId)>)>,
    /// Monitors attached to the module and the interfaces they observe.
    pub monitor_map: Vec<(MachineId, Vec<InterfaceId>)>,
    pub private_events: Vec<EventId>,
    pub private_interfaces: Vec<InterfaceId>,
    pub receives: Vec<EventId>,
    pub sends: Vec<EventId>,
}

impl ModuleInfo {
    pub fn machine_for(&self, iface: InterfaceId) -> Option<MachineId> {
        self.interface_def.iter().find(|(i, _)| *i == iface).map(|(_, m)| *m)
    }

    pub fn link(&self, creator: InterfaceId, created: InterfaceId) -> Option<InterfaceId> {
        self.link_map
            .iter()
            .find(|(c, _)| *c == creator)
            .and_then(|(_, m)| m.iter().find(|(k, _)| *k == created).map(|(_, v)| *v))
    }
}

/// A `test` declaration, expanded to concrete parameter assignments.
#[derive(Debug, Clone)]
pub struct TestDecl {
    pub name: String,
    pub main: MachineId,
    pub module: ModuleInfo,
    /// Parameter assignments to run; a non-parametric test has exactly one empty assignment.
    pub assignments: Vec<ParamAssignment>,
    pub span: Span,
}

impl TestDecl {
    /// Name of the expanded test case for a given assignment (upstream
    /// `RenameSafetyTestByAssignment`).
    pub fn case_name(&self, assignment: &ParamAssignment, prog: &Program) -> String {
        if assignment.values.is_empty() {
            return self.name.clone();
        }
        let parts: Vec<String> = assignment
            .values
            .iter()
            .map(|(v, val)| {
                let name = &prog.var_decls[v.idx()].name;
                let mangled = match val {
                    ParamValue::Int(i) if *i < 0 => format!("neg{}", i.unsigned_abs()),
                    ParamValue::Int(i) => i.to_string(),
                    ParamValue::Bool(b) => {
                        if *b {
                            "true".into()
                        } else {
                            "false".into()
                        }
                    }
                };
                format!("{}_{}", name, mangled)
            })
            .collect();
        format!("{}___{}", self.name, parts.join("__"))
    }
}

#[derive(Debug, Clone)]
pub struct ImplDecl {
    pub name: String,
    pub main: Option<MachineId>,
    pub module: ModuleInfo,
    pub span: Span,
}

/// A fully analyzed program.
#[derive(Debug, Clone, Default)]
pub struct Program {
    /// Path-independent SHA-256 identity of the compilation source contents.
    pub source_fingerprint: Option<String>,
    /// The sources, to resolve spans to file names and lines.
    pub sources: std::sync::Arc<crate::syntax::SourceMap>,
    pub events: Vec<EventDecl>,
    pub enums: Vec<EnumDecl>,
    pub typedefs: Vec<TypeDefDecl>,
    pub event_sets: Vec<EventSetDecl>,
    pub interfaces: Vec<InterfaceDecl>,
    pub machines: Vec<MachineDecl>,
    pub states: Vec<StateDecl>,
    pub funs: Vec<FunDecl>,
    pub tests: Vec<TestDecl>,
    pub impls: Vec<ImplDecl>,
    /// Global parameters (`param x: int;`), indexed by `VarDecl::slot`.
    pub global_params: Vec<VarId>,
    pub var_decls: Vec<VarDecl>,
    /// Ids of the built-in `null` and `halt` events.
    pub null_event: EventId,
    pub halt_event: EventId,
    /// Every event except `null`, sorted by name (see [`Program::universal_events`]). Set once
    /// all events are declared.
    pub(crate) universal_events: Vec<EventId>,
}

impl Program {
    pub fn event(&self, id: EventId) -> &EventDecl {
        &self.events[id.idx()]
    }
    pub fn machine(&self, id: MachineId) -> &MachineDecl {
        &self.machines[id.idx()]
    }
    pub fn state(&self, id: StateId) -> &StateDecl {
        &self.states[id.idx()]
    }
    pub fn fun(&self, id: FunId) -> &FunDecl {
        &self.funs[id.idx()]
    }
    pub fn var(&self, id: VarId) -> &VarDecl {
        &self.var_decls[id.idx()]
    }
    pub fn interface(&self, id: InterfaceId) -> &InterfaceDecl {
        &self.interfaces[id.idx()]
    }

    pub fn find_event(&self, name: &str) -> Option<EventId> {
        self.events.iter().position(|e| e.name == name).map(|i| EventId(i as u32))
    }
    pub fn find_machine(&self, name: &str) -> Option<MachineId> {
        self.machines.iter().position(|e| e.name == name).map(|i| MachineId(i as u32))
    }
    pub fn find_interface(&self, name: &str) -> Option<InterfaceId> {
        self.interfaces.iter().position(|e| e.name == name).map(|i| InterfaceId(i as u32))
    }

    /// All declared events plus `halt` (the universal event set), sorted by name.
    pub fn universal_events(&self) -> Vec<EventId> {
        debug_assert_eq!(self.universal_events.len(), self.events.iter().filter(|e| !e.is_null()).count());
        self.universal_events.clone()
    }

    /// Computes [`Program::universal_events`]; called once every event is declared.
    pub(crate) fn sort_universal_events(&mut self) {
        let mut v: Vec<EventId> =
            (0..self.events.len() as u32).map(EventId).filter(|e| !self.events[e.idx()].is_null()).collect();
        v.sort_by(|a, b| self.events[a.idx()].name.cmp(&self.events[b.idx()].name));
        self.universal_events = v;
    }

    pub fn interface_receives(&self, id: InterfaceId) -> Vec<EventId> {
        match &self.interfaces[id.idx()].receives {
            Some(v) => v.clone(),
            None => self.universal_events(),
        }
    }

    pub fn machine_receives(&self, id: MachineId) -> Vec<EventId> {
        match &self.machines[id.idx()].receives {
            Some(v) => v.clone(),
            None => self.universal_events(),
        }
    }

    pub fn machine_sends(&self, id: MachineId) -> Vec<EventId> {
        match &self.machines[id.idx()].sends {
            Some(v) => v.clone(),
            None => self.universal_events(),
        }
    }

    pub fn permission_name(&self, o: PermissionOrigin) -> &str {
        match o {
            PermissionOrigin::Machine(m) => &self.machines[m.idx()].name,
            PermissionOrigin::Interface(i) => &self.interfaces[i.idx()].name,
            PermissionOrigin::EventSet(e) => &self.event_sets[e.idx()].name,
            PermissionOrigin::Event(e) => &self.events[e.idx()].name,
        }
    }

    pub fn permission_events(&self, o: PermissionOrigin) -> Vec<EventId> {
        match o {
            PermissionOrigin::Machine(m) => self.machine_receives(m),
            PermissionOrigin::Interface(i) => self.interface_receives(i),
            PermissionOrigin::EventSet(e) => self.event_sets[e.idx()].events.clone(),
            PermissionOrigin::Event(e) => vec![e],
        }
    }

    pub fn decl_name(&self, d: DeclRef) -> String {
        match d {
            DeclRef::Event(e) => self.events[e.idx()].name.clone(),
            DeclRef::EnumElem(e, i) => self.enums[e.idx()].elems[i as usize].name.clone(),
            DeclRef::Enum(e) => self.enums[e.idx()].name.clone(),
            DeclRef::TypeDef(t) => self.typedefs[t.idx()].name.clone(),
            DeclRef::EventSet(e) => self.event_sets[e.idx()].name.clone(),
            DeclRef::Interface(i) => self.interfaces[i.idx()].name.clone(),
            DeclRef::Machine(m) => self.machines[m.idx()].name.clone(),
            DeclRef::State(s) => self.states[s.idx()].name.clone(),
            DeclRef::Fun(f) => self.funs[f.idx()].name.clone(),
            DeclRef::Var(v) => self.var_decls[v.idx()].name.clone(),
            DeclRef::Test(t) => self.tests[t.idx()].name.clone(),
            DeclRef::Impl(i) => self.impls[i.idx()].name.clone(),
            _ => String::new(),
        }
    }

    pub fn decl_span(&self, d: DeclRef) -> Span {
        match d {
            DeclRef::Event(e) => self.events[e.idx()].span,
            DeclRef::EnumElem(e, i) => self.enums[e.idx()].elems[i as usize].span,
            DeclRef::Enum(e) => self.enums[e.idx()].span,
            DeclRef::TypeDef(t) => self.typedefs[t.idx()].span,
            DeclRef::EventSet(e) => self.event_sets[e.idx()].span,
            DeclRef::Interface(i) => self.interfaces[i.idx()].span,
            DeclRef::Machine(m) => self.machines[m.idx()].span,
            DeclRef::State(s) => self.states[s.idx()].span,
            DeclRef::Fun(f) => self.funs[f.idx()].span,
            DeclRef::Var(v) => self.var_decls[v.idx()].span,
            DeclRef::Test(t) => self.tests[t.idx()].span,
            DeclRef::Impl(i) => self.impls[i.idx()].span,
            _ => Span::dummy(),
        }
    }

    /// Display name of a function for diagnostics (`DeclarationName` upstream).
    pub fn fun_display_name(&self, id: FunId, sm: &crate::syntax::SourceMap) -> String {
        let f = &self.funs[id.idx()];
        if f.name.is_empty() {
            format!("at {}", sm.location(f.span))
        } else {
            f.name.clone()
        }
    }

    /// Enumerates all runnable test cases: implementations first, then every expanded
    /// `test` (one per parameter assignment) in declaration order.
    pub fn test_cases(&self) -> Vec<TestCase> {
        // upstream emits implementations before tests (`Scope.AllDecls` order)
        let mut v = Vec::new();
        for (ii, i) in self.impls.iter().enumerate() {
            v.push(TestCase { name: i.name.clone(), kind: TestCaseKind::Impl(ImplId(ii as u32)) });
        }
        for (ti, t) in self.tests.iter().enumerate() {
            for (ai, a) in t.assignments.iter().enumerate() {
                v.push(TestCase { name: t.case_name(a, self), kind: TestCaseKind::Test(TestId(ti as u32), ai) });
            }
        }
        v
    }

    pub fn test_case_module(&self, tc: &TestCase) -> (&ModuleInfo, Option<MachineId>) {
        match tc.kind {
            TestCaseKind::Test(t, _) => (&self.tests[t.idx()].module, Some(self.tests[t.idx()].main)),
            TestCaseKind::Impl(i) => (&self.impls[i.idx()].module, self.impls[i.idx()].main),
        }
    }

    pub fn test_case_assignment(&self, tc: &TestCase) -> Option<&ParamAssignment> {
        match tc.kind {
            TestCaseKind::Test(t, a) => Some(&self.tests[t.idx()].assignments[a]),
            TestCaseKind::Impl(_) => None,
        }
    }
}

/// One runnable test case (an expanded `test` or an `implementation`).
#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub kind: TestCaseKind,
}

#[derive(Debug, Clone, Copy)]
pub enum TestCaseKind {
    Test(TestId, usize),
    Impl(ImplId),
}
