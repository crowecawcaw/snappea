//! A small stack bytecode that the interpreter executes.
//!
//! Each root function of the program (methods, entry/exit actions, event handlers,
//! transition functions) is lowered to one [`CodeBlock`]. Receive-case handlers are
//! inlined into the enclosing root function and share its frame, exactly as the upstream
//! C# code generator inlines them into the enclosing method.

use std::rc::Rc;

use crate::sema::{BinOp, EventId, FunId, InterfaceId, PType, StateId};
use crate::syntax::Span;

/// Where an l-value lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    Local(u32),
    Field(u32),
}

/// One step of an l-value path below its root. Keys for `SeqIndex`/`MapKey` are taken
/// from the operand stack in path order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathElem {
    TupleField(u32),
    SeqIndex,
    MapKey,
}

#[derive(Debug, Clone)]
pub struct ReceiveCase {
    pub event: EventId,
    /// Slot that receives the payload (`None` when the case takes no parameter).
    pub param_slot: Option<u32>,
    /// Program counter of the case body.
    pub target: u32,
}

#[derive(Debug, Clone)]
pub enum Op {
    PushNull,
    PushBool(bool),
    PushInt(i64),
    PushFloat(f64),
    PushEvent(EventId),
    PushThis,
    LoadLocal(u32),
    StoreLocal(u32),
    LoadField(u32),
    LoadGlobal(u32),
    Pop,
    Dup,
    MakeTuple(u32),
    MakeNamedTuple(Rc<[String]>),
    TupleGet(u32),
    SeqGet,
    SetGet,
    /// `map[key]`. `java_null`: the value type is a Java reference type, so in Java semantics
    /// a missing key reads as null (`HashMap.get`) instead of failing.
    MapGet {
        java_null: bool,
    },
    Keys,
    Values,
    Sizeof,
    Contains,
    /// Pushes a precomputed default value (values are copy-on-write, so sharing is safe).
    Default(crate::checker::value::Value),
    /// `format(base, args)`; `types` are the static types of the arguments.
    Format {
        base: Rc<str>,
        nargs: u32,
        types: Rc<[PType]>,
    },
    Bin(BinOp),
    Neg,
    Not,
    Jump(u32),
    JumpIfFalse(u32),
    JumpIfTrue(u32),
    Cast(std::rc::Rc<PType>),
    CoerceFloat,
    CoerceInt,
    CoerceInterface(InterfaceId),
    ChooseBool,
    ChooseInt,
    ChooseSeq,
    ChooseSet,
    ChooseMap,
    Call {
        fun: FunId,
        nargs: u32,
    },
    Return,
    Store {
        root: Root,
        path: Rc<[PathElem]>,
    },
    Insert {
        root: Root,
        path: Rc<[PathElem]>,
    },
    Add {
        root: Root,
        path: Rc<[PathElem]>,
    },
    Remove {
        root: Root,
        path: Rc<[PathElem]>,
    },
    Assert,
    Print,
    Create {
        iface: InterfaceId,
        has_payload: bool,
    },
    /// Pops payload (if any), event and target.
    Send {
        has_payload: bool,
    },
    Raise {
        has_payload: bool,
    },
    Announce {
        has_payload: bool,
    },
    Goto {
        state: StateId,
        has_payload: bool,
    },
    Receive {
        cases: Rc<[ReceiveCase]>,
    },
    /// Pops the collection (a sequence or set), stores an iterator in `iter_slot`.
    ForeachInit {
        iter_slot: u32,
        root: Option<Root>,
    },
    /// Advances the iterator; jumps to `exit` when exhausted, else stores the item.
    ForeachNext {
        iter_slot: u32,
        item_slot: u32,
        exit: u32,
    },

    // ---- specialized and fused instructions -------------------------------------
    //
    // Each stands for the sequence of the instructions above in its documentation, with the
    // same effects and errors, and counts as that many units of work. Instructions that
    // may fail come last in the sequence, or are checked in sequence order, so the work
    // bound is reached at the same point.
    /// `Store` of a whole field (an empty path).
    StoreField(u32),
    /// `Default(value); StoreLocal(slot)`.
    StoreLocalConst {
        slot: u32,
        value: crate::checker::value::Value,
    },
    /// A run of `StoreLocalConst`, as at the start of a function that declares locals.
    StoreLocalConsts(Rc<[(u32, crate::checker::value::Value)]>),
    /// `src; StoreLocal(dst)`.
    Move {
        src: Operand,
        dst: u32,
    },
    /// `LoadLocal(slot); TupleGet(index)`.
    LocalTupleGet {
        slot: u32,
        index: u32,
    },
    /// `c; Sizeof`.
    SizeofOf(Operand),
    /// `map; key; MapGet { java_null }`.
    MapGetOf {
        map: Operand,
        key: Operand,
        java_null: bool,
    },
    /// `map; Keys; Sizeof`.
    KeysSizeof(Operand),
    /// `Keys; Contains`.
    KeysContains,
    /// `map; Keys; LoadLocal(index); SeqGet`: `keys(map)[i]`.
    KeyAt {
        map: Operand,
        index: u32,
    },
    /// `lhs; rhs; Bin(op)` of two `int` operands, then `StoreLocal` when `dst` is a local.
    /// Falls back to `Bin` for other values.
    IntBin {
        op: BinOp,
        lhs: Operand,
        rhs: Operand,
        dst: Dest,
    },
    /// `IntBin { op, lhs, rhs, dst: Dest::Stack }; JumpIfFalse(target)` for a comparison `op`.
    IntCmpJumpIfFalse {
        op: BinOp,
        lhs: Operand,
        rhs: Operand,
        target: u32,
    },
    /// `Dup; JumpIfFalse(target); Pop` (`&&`): jumps keeping a false operand, else pops it.
    JumpIfFalseElsePop(u32),
    /// `Dup; JumpIfTrue(target); Pop` (`||`, `==>`).
    JumpIfTrueElsePop(u32),
    /// `PushNull; Return`.
    ReturnNull,
    /// `Format; Print` (`print format(...)`), emitted only when `base` cannot fail to
    /// format with `nargs` arguments. Builds the string only when a log shows it (or, in
    /// Java semantics, where formatting can fail on the values).
    PrintFormat {
        base: Rc<str>,
        nargs: u32,
        types: Rc<[PType]>,
    },
    /// `Format`, as an argument of a call whose callee never reads that parameter (see
    /// `compile::unread_params`): pops the arguments and pushes `null` instead of building
    /// the string. Emitted only when `base` cannot fail to format with `nargs` arguments,
    /// so the only effect the plain `Format` has is lost on a value nobody sees. In Java
    /// semantics, whose formatting can fail on the values, it is `Format`.
    UnreadFormat {
        base: Rc<str>,
        nargs: u32,
        types: Rc<[PType]>,
    },
    /// Records that code coverage probe `id` (an index into [`crate::checker::compile::Code::probes`])
    /// ran. Emitted only when compiling for code coverage; counts no work, so a run explores
    /// the same schedules with or without probes.
    Probe(u32),
    /// Records the evaluated Boolean on top of the stack without consuming it.
    /// Like `Probe`, this counts no work and is emitted only for coverage.
    ConditionProbe {
        yes: u32,
        no: u32,
    },
}

/// Where an instruction reads an operand: the operand stack, or a register (a local or
/// field slot, or a literal) that the instruction loads itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    /// On the operand stack. Of two operands, the second is above the first.
    Stack,
    /// `LoadLocal(slot)`.
    Local(u32),
    /// `LoadField(slot)`.
    Field(u32),
    /// `PushInt(value)`.
    Int(i32),
}

impl Operand {
    /// The units of work of the instruction that loads the operand.
    pub fn work(self) -> u64 {
        match self {
            Operand::Stack => 0,
            Operand::Local(_) | Operand::Field(_) | Operand::Int(_) => 1,
        }
    }
}

/// Where an instruction puts its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    /// Pushed on the operand stack.
    Stack,
    /// `StoreLocal(slot)`.
    Local(u32),
}

impl Dest {
    /// The units of work of the instruction that stores the result.
    pub fn work(self) -> u64 {
        match self {
            Dest::Stack => 0,
            Dest::Local(_) => 1,
        }
    }
}

/// The compiled form of one root function.
#[derive(Debug, Clone, Default)]
pub struct CodeBlock {
    pub ops: Vec<Op>,
    /// Slots in a frame: declared variables plus hidden iterator slots.
    pub frame_size: u32,
    /// Slots the parameters occupy, in order.
    pub param_slots: Vec<u32>,
    /// Whether parameter `i` occupies slot `i`, so arguments can stay where the caller
    /// pushed them.
    pub params_in_order: bool,
}

/// What a code coverage probe marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeKind {
    /// The start of a function body (or receive case).
    Entry,
    /// A statement.
    Stmt,
    /// The `then` branch of the `if` at `span`.
    Then,
    /// The `else` branch (possibly empty) of the `if` at `span`.
    Else,
    /// An atomic condition in an `if` or `while` evaluated to true / false.
    ConditionTrue,
    ConditionFalse,
}

/// A code coverage probe: a point in the source whose execution is recorded.
#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub kind: ProbeKind,
    /// The function the probe is in (a receive case's own function for its body).
    pub fun: FunId,
    /// The statement (or, for a branch, the `if` statement; for an entry, the function).
    pub span: Span,
    /// The probe of the enclosing `if` branch, or for top-level statements the function's
    /// entry probe.
    pub parent: Option<u32>,
    /// The statement is `assert false, ...`: code that must never run.
    pub asserts_false: bool,
}
