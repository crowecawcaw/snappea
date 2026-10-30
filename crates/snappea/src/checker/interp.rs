//! The bytecode interpreter: executes one action of a machine or monitor.
//!
//! Actions run to completion unless they reach a scheduling point (`new`, `send`,
//! `receive`), in which case the whole execution state is handed back to the runtime
//! and resumed later. Errors that the upstream runtime reports as .NET exceptions are
//! modeled as [`Exception`] values and turned into bug reports with the same text.

use std::rc::Rc;

use crate::sema::{BinOp, EventId, PType, Program};

use crate::checker::bytecode::{Op, Operand, PathElem, ReceiveCase, Root};
use crate::checker::runtime::{
    ActiveIter, Actor, Event, Exception, Exec, ExecOutcome, MachineStatus, OpStatus, Pending, Runtime, SchedKind, Stop,
    Transition,
};
use crate::checker::value::{default_value, event_cs_full_name, format_composite, format_exception, NamedTuple, Value};

enum Fault {
    Stop,
    Exc(Exception),
}

impl From<Stop> for Fault {
    fn from(_: Stop) -> Fault {
        Fault::Stop
    }
}

impl From<Exception> for Fault {
    fn from(e: Exception) -> Fault {
        Fault::Exc(e)
    }
}

enum Flow {
    Continue,
    Finished(Value),
    Yield(SchedKind),
    Unwound,
}

// ---- .NET exceptions ----------------------------------------------------------

fn nre() -> Exception {
    Exception::new("System.NullReferenceException", "Object reference not set to an instance of an object.")
}

fn index_oor() -> Exception {
    Exception::new(
        "System.ArgumentOutOfRangeException",
        "Index was out of range. Must be non-negative and less than the size of the collection. (Parameter 'index')",
    )
}

fn insert_oor() -> Exception {
    Exception::new(
        "System.ArgumentOutOfRangeException",
        "Index must be within the bounds of the List. (Parameter 'index')",
    )
}

fn element_at_oor() -> Exception {
    Exception::new(
        "System.ArgumentOutOfRangeException",
        "Specified argument was out of the range of valid values. (Parameter 'index')",
    )
}

fn key_not_found(key: &str) -> Exception {
    Exception::new(
        "System.Collections.Generic.KeyNotFoundException",
        format!("The given key '{}' was not present in the dictionary.", key),
    )
}

fn duplicate_key(key: &str) -> Exception {
    Exception::new(
        "System.ArgumentException",
        format!("An item with the same key has already been added. Key: {}", key),
    )
}

fn div_zero() -> Exception {
    Exception::new("System.DivideByZeroException", "Attempted to divide by zero.")
}

/// `long.MinValue / -1` (and `% -1`), which .NET does not wrap.
fn overflow() -> Exception {
    Exception::new("System.OverflowException", "Arithmetic operation resulted in an overflow.")
}

fn modified() -> Exception {
    Exception::new(
        "System.InvalidOperationException",
        "Collection was modified; enumeration operation may not execute.",
    )
    .from_source(crate::checker::runtime::CORE_LIB)
}

/// The C# class name of a value, for `InvalidCastException` messages.
fn value_class(v: &Value, prog: &Program) -> String {
    match v {
        Value::Event(e) => event_cs_full_name(prog, *e),
        other => format!("PChecker.Runtime.Values.{}", other.type_name()),
    }
}

/// The C# class name of a P type.
fn type_class(ty: &PType, prog: &Program) -> String {
    let name = match ty.canonicalize(prog) {
        PType::Bool => "PChecker.Runtime.Values.PBool",
        PType::Int | PType::Enum(_) => "PChecker.Runtime.Values.PInt",
        PType::Float => "PChecker.Runtime.Values.PFloat",
        PType::String => "PChecker.Runtime.Values.PString",
        PType::Event => "PChecker.Runtime.Events.Event",
        PType::Machine | PType::Permission(_) => "PChecker.Runtime.Values.PMachineValue",
        PType::Seq(_) => "PChecker.Runtime.Values.PSeq",
        PType::Set(_) => "PChecker.Runtime.Values.PSet",
        PType::Map(..) => "PChecker.Runtime.Values.PMap",
        PType::Tuple(_) => "PChecker.Runtime.Values.PTuple",
        PType::NamedTuple(_) => "PChecker.Runtime.Values.PNamedTuple",
        _ => "PChecker.Runtime.Values.IPValue",
    };
    name.to_string()
}

fn invalid_cast(v: &Value, to: &str, prog: &Program) -> Exception {
    Exception::new(
        "System.InvalidCastException",
        format!("Unable to cast object of type '{}' to type '{}'.", value_class(v, prog), to),
    )
}

/// Checks a C# cast `(T)value`.
fn check_cast(v: &Value, ty: &PType, prog: &Program) -> Result<(), Exception> {
    let canon = ty.canonicalize(prog);
    let ok = match (&canon, v) {
        (PType::Any | PType::Data | PType::Null | PType::Foreign(_) | PType::Error | PType::TypeDef(_), _) => true,
        (PType::Bool | PType::Int | PType::Float | PType::Enum(_), Value::Null) => {
            return Err(nre().from_source(crate::checker::runtime::CORE_LIB))
        }
        (_, Value::Null) => true,
        (PType::Bool, Value::Bool(_)) => true,
        (PType::Int | PType::Enum(_), Value::Int(_)) => true,
        (PType::Float, Value::Float(_)) => true,
        (PType::String, Value::Str(_)) => true,
        (PType::Event, Value::Event(_)) => true,
        (PType::Machine | PType::Permission(_), Value::Machine(_)) => true,
        (PType::Seq(_), Value::Seq(_)) => true,
        (PType::Set(_), Value::Set(_)) => true,
        (PType::Map(..), Value::Map(_)) => true,
        (PType::Tuple(_), Value::Tuple(_)) => true,
        (PType::NamedTuple(_), Value::NamedTuple(_)) => true,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(invalid_cast(v, &type_class(&canon, prog), prog))
    }
}

// ---- l-value navigation ----------------------------------------------------------

fn key_count(path: &[PathElem]) -> usize {
    path.iter().filter(|p| matches!(p, PathElem::SeqIndex | PathElem::MapKey)).count()
}

/// Walks an l-value path with getter semantics (`((PSeq)x)[i]`, `((PMap)m)[k]`, ...),
/// copying shared containers on write.
fn navigate<'a>(
    v: &'a mut Value,
    path: &[PathElem],
    keys: &[Value],
    prog: &Program,
) -> Result<&'a mut Value, Exception> {
    let mut cur = v;
    let mut ki = 0;
    for p in path {
        let next: &'a mut Value = match p {
            PathElem::TupleField(i) => match cur {
                Value::Tuple(t) => Rc::make_mut(t).get_mut(*i as usize).ok_or_else(index_oor)?,
                Value::NamedTuple(t) => Rc::make_mut(t).values.get_mut(*i as usize).ok_or_else(index_oor)?,
                Value::Null => return Err(nre()),
                other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PTuple", prog)),
            },
            PathElem::SeqIndex => {
                let k = &keys[ki];
                ki += 1;
                match cur {
                    Value::Seq(s) => {
                        let i = k.as_int();
                        if i < 0 || i as usize >= s.len() {
                            return Err(index_oor());
                        }
                        &mut Rc::make_mut(s)[i as usize]
                    }
                    Value::Null => return Err(nre()),
                    other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PSeq", prog)),
                }
            }
            PathElem::MapKey => {
                let k = &keys[ki];
                ki += 1;
                match cur {
                    Value::Map(m) => match Rc::make_mut(m).get_mut(k) {
                        Some(v) => v,
                        None => return Err(key_not_found(&k.to_display(prog))),
                    },
                    Value::Null => return Err(nre()),
                    other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PMap", prog)),
                }
            }
        };
        cur = next;
    }
    Ok(cur)
}

/// Performs the final store of an assignment through the last path element.
fn store_at(
    base: &mut Value,
    last: &PathElem,
    key: Option<&Value>,
    rhs: Value,
    prog: &Program,
) -> Result<(), Exception> {
    match last {
        PathElem::TupleField(i) => match base {
            Value::Tuple(t) => {
                *Rc::make_mut(t).get_mut(*i as usize).ok_or_else(index_oor)? = rhs;
            }
            Value::NamedTuple(t) => {
                *Rc::make_mut(t).values.get_mut(*i as usize).ok_or_else(index_oor)? = rhs;
            }
            Value::Null => return Err(nre()),
            other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PTuple", prog)),
        },
        PathElem::SeqIndex => match base {
            Value::Seq(s) => {
                let i = key.expect("sequence index").as_int();
                if i < 0 || i as usize >= s.len() {
                    return Err(index_oor());
                }
                Rc::make_mut(s)[i as usize] = rhs;
            }
            Value::Null => return Err(nre()),
            other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PSeq", prog)),
        },
        PathElem::MapKey => match base {
            Value::Map(m) => {
                Rc::make_mut(m).set(key.expect("map key").clone(), rhs);
            }
            Value::Null => return Err(nre()),
            other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PMap", prog)),
        },
    }
    Ok(())
}

// The compiler balances the operand stack, so these never underflow.

fn pop_n(stack: &mut Vec<Value>, n: usize) -> Vec<Value> {
    let at = stack.len().checked_sub(n).expect("operand stack underflow");
    stack.split_off(at)
}

fn pop(stack: &mut Vec<Value>) -> Value {
    stack.pop().expect("operand stack underflow")
}

/// Invalidates the enumerators of `foreach` loops over the variable just modified.
fn note_mutation(exec: &mut Exec, frame_idx: usize, root: Root) {
    let hits: Vec<(usize, u32)> = exec
        .iters
        .iter()
        .filter(|it| it.root == root && (matches!(root, Root::Field(_)) || it.frame == frame_idx))
        .map(|it| (it.frame, it.slot))
        .collect();
    for (f, slot) in hits {
        if let Some(frame) = exec.frames.get(f) {
            exec.values[frame.base + slot as usize + 1] = Value::Int(-1);
        }
    }
}

/// `((PTuple)t)[i]`.
fn tuple_get(t: &Value, i: u32, prog: &Program) -> Result<Value, Exception> {
    match t {
        Value::Tuple(t) => t.get(i as usize).cloned().ok_or_else(index_oor),
        Value::NamedTuple(t) => t.values.get(i as usize).cloned().ok_or_else(index_oor),
        Value::Null => Err(nre()),
        other => Err(invalid_cast(other, "PChecker.Runtime.Values.PTuple", prog)),
    }
}

/// `((PSeq)s)[i]`.
fn seq_get(s: &Value, i: &Value, prog: &Program) -> Result<Value, Exception> {
    match s {
        Value::Seq(s) => {
            let i = i.as_int();
            if i < 0 || i as usize >= s.len() {
                return Err(index_oor());
            }
            Ok(s[i as usize].clone())
        }
        Value::Null => Err(nre()),
        other => Err(invalid_cast(other, "PChecker.Runtime.Values.PSeq", prog)),
    }
}

/// `((PMap)m)[key]`; with `java_null`, a missing key reads as null.
fn map_get(m: &Value, key: &Value, java_null: bool, prog: &Program) -> Result<Value, Exception> {
    match m {
        Value::Map(m) if java_null => Ok(m.get(key).cloned().unwrap_or(Value::Null)),
        Value::Map(m) => m.get(key).cloned().ok_or_else(|| key_not_found(&key.to_display(prog))),
        Value::Null => Err(nre()),
        other => Err(invalid_cast(other, "PChecker.Runtime.Values.PMap", prog)),
    }
}

/// The map of `keys(m)` and `values(m)`.
fn map_of<'a>(m: &'a Value, prog: &Program) -> Result<&'a crate::checker::value::SlotMap, Exception> {
    match m {
        Value::Map(m) => Ok(m),
        Value::Null => Err(nre()),
        other => Err(invalid_cast(other, "PChecker.Runtime.Values.PMap", prog)),
    }
}

/// `sizeof(c)`.
fn size_of(c: &Value) -> Result<Value, Exception> {
    if c.is_null() {
        return Err(nre());
    }
    Ok(Value::Int(c.count() as i64))
}

/// `Op::IntBin`: [`bin`] with the operations on two integers that cannot fail inlined.
#[inline(always)]
fn int_bin(op: BinOp, a: Value, b: Value, java: bool) -> Result<Value, Exception> {
    if let (Value::Int(x), Value::Int(y)) = (&a, &b) {
        let (x, y) = (*x, *y);
        match op {
            BinOp::Add => return Ok(Value::Int(x.wrapping_add(y))),
            BinOp::Sub => return Ok(Value::Int(x.wrapping_sub(y))),
            BinOp::Mul => return Ok(Value::Int(x.wrapping_mul(y))),
            BinOp::Lt => return Ok(Value::Bool(x < y)),
            BinOp::Le => return Ok(Value::Bool(x <= y)),
            BinOp::Gt => return Ok(Value::Bool(x > y)),
            BinOp::Ge => return Ok(Value::Bool(x >= y)),
            BinOp::Eq => return Ok(Value::Bool(x == y)),
            BinOp::Neq => return Ok(Value::Bool(x != y)),
            _ => {}
        }
    }
    bin(op, a, b, java)
}

/// The right operand of an `Op::IntBin`, taking it off the stack if it is there.
#[inline(always)]
fn operand(stack: &mut Vec<Value>, base: usize, rhs: Operand) -> Value {
    match rhs {
        Operand::Stack => pop(stack),
        Operand::Local(slot) => stack[base + slot as usize].clone(),
        Operand::Int(k) => Value::Int(k),
    }
}

/// A binary operation (`Op::Bin`). P floats are Java `float`s in the PObserve backend.
fn bin(op: BinOp, a: Value, b: Value, java: bool) -> Result<Value, Exception> {
    Ok(match arith(op, a, b)? {
        Value::Float(f) if java => Value::Float(f as f32 as f64),
        r => r,
    })
}

/// Leaves the innermost frame, handing `v` to its caller.
fn return_value(exec: &mut Exec, v: Value) -> Flow {
    let frame = exec.frames.pop().expect("a running frame");
    exec.values.truncate(frame.base);
    let depth = exec.frames.len();
    exec.iters.retain(|it| it.frame < depth);
    if depth == 0 {
        return Flow::Finished(v);
    }
    exec.values.push(v);
    Flow::Continue
}

fn arith(op: BinOp, a: Value, b: Value) -> Result<Value, Exception> {
    use Value::*;
    Ok(match (op, &a, &b) {
        (BinOp::Add, Int(x), Int(y)) => Int(x.wrapping_add(*y)),
        (BinOp::Sub, Int(x), Int(y)) => Int(x.wrapping_sub(*y)),
        (BinOp::Mul, Int(x), Int(y)) => Int(x.wrapping_mul(*y)),
        (BinOp::Div, Int(x), Int(y)) => {
            if *y == 0 {
                return Err(div_zero());
            }
            Int(x.checked_div(*y).ok_or_else(overflow)?)
        }
        (BinOp::Mod, Int(x), Int(y)) => {
            if *y == 0 {
                return Err(div_zero());
            }
            Int(x.checked_rem(*y).ok_or_else(overflow)?)
        }
        (BinOp::Add, Str(x), Str(y)) => Str(Rc::from(format!("{}{}", x, y))),
        (BinOp::Add, _, _) => Float(a.as_float() + b.as_float()),
        (BinOp::Sub, _, _) => Float(a.as_float() - b.as_float()),
        (BinOp::Mul, _, _) => Float(a.as_float() * b.as_float()),
        (BinOp::Div, _, _) => Float(a.as_float() / b.as_float()),
        (BinOp::Mod, _, _) => Float(a.as_float() % b.as_float()),
        (BinOp::Lt, Int(x), Int(y)) => Bool(x < y),
        (BinOp::Le, Int(x), Int(y)) => Bool(x <= y),
        (BinOp::Gt, Int(x), Int(y)) => Bool(x > y),
        (BinOp::Ge, Int(x), Int(y)) => Bool(x >= y),
        (BinOp::Lt, Str(x), Str(y)) => Bool(x < y),
        (BinOp::Le, Str(x), Str(y)) => Bool(x <= y),
        (BinOp::Gt, Str(x), Str(y)) => Bool(x > y),
        (BinOp::Ge, Str(x), Str(y)) => Bool(x >= y),
        (BinOp::Lt, _, _) => Bool(a.as_float() < b.as_float()),
        (BinOp::Le, _, _) => Bool(a.as_float() <= b.as_float()),
        (BinOp::Gt, _, _) => Bool(a.as_float() > b.as_float()),
        (BinOp::Ge, _, _) => Bool(a.as_float() >= b.as_float()),
        (BinOp::Eq, _, _) => Bool(a == b),
        (BinOp::Neq, _, _) => Bool(a != b),
        (BinOp::And, _, _) => Bool(a.as_bool() && b.as_bool()),
        (BinOp::Or, _, _) => Bool(a.as_bool() || b.as_bool()),
        (BinOp::Then, _, _) => Bool(!a.as_bool() || b.as_bool()),
        (BinOp::Iff, _, _) => Bool(a.as_bool() == b.as_bool()),
    })
}

impl<'p> Runtime<'p> {
    /// Runs (or resumes) an action until it finishes, unwinds or reaches a scheduling point.
    pub fn run_exec(&mut self, actor: Actor, mut exec: Exec) -> Result<ExecOutcome, Stop> {
        let r = self.run_exec_inner(actor, &mut exec);
        match r {
            Ok(Flow::Finished(v)) => Ok(ExecOutcome::Finished(v)),
            Ok(Flow::Yield(kind)) => Ok(ExecOutcome::Yield(exec, kind)),
            Ok(Flow::Unwound) => {
                if let (Actor::Machine(m), true) = (actor, self.xml.is_some()) {
                    // `raise`/`goto` return through a `PNonStandardReturnException`
                    const TY: &str = "PChecker.Runtime.Exceptions.PNonStandardReturnException";
                    let text = format!("{}: Exception of type '{}' was thrown.", TY, TY);
                    let (id, state) = (self.machines[m].name.to_string(), self.machine_state_name(m));
                    let attrs = [
                        ("id", id.as_str()),
                        ("state", state.as_str()),
                        ("action", exec.action.as_str()),
                        ("type", TY),
                    ];
                    self.xlog("ExceptionThrown", &attrs, Some(&text));
                    let handled: Vec<_> =
                        attrs.iter().copied().filter(|(k, v)| *k != "state" || !v.is_empty()).collect();
                    self.xlog("ExceptionHandled", &handled, Some(&text));
                }
                Ok(ExecOutcome::Unwound)
            }
            Ok(Flow::Continue) => Ok(ExecOutcome::Finished(Value::Null)),
            Err(Fault::Stop) => Err(Stop),
            Err(Fault::Exc(e)) => {
                let action = exec.action.clone();
                Err(self.report_exception(actor, &e, &action))
            }
        }
    }

    fn run_exec_inner(&mut self, actor: Actor, exec: &mut Exec) -> Result<Flow, Fault> {
        if let Some(p) = exec.pending.take() {
            match self.resume(actor, exec, p)? {
                Flow::Continue => {}
                other => return Ok(other),
            }
        }
        loop {
            match self.run_frame(actor, exec)? {
                Flow::Continue => {}
                other => return Ok(other),
            }
        }
    }

    /// The bug reported when an action runs longer than `--max-work-per-step` allows.
    #[cold]
    #[inline(never)]
    fn work_bound_exceeded(&mut self, actor: Actor) -> Fault {
        let msg = format!(
            "{} in state {} exceeded the work bound of {} without reaching a scheduling point (possible infinite loop).",
            self.actor_name(actor),
            self.actor_state_name(actor),
            self.config.max_work_per_step
        );
        self.report_bug(msg).into()
    }

    /// Calls a foreign function through the embedder's [`ForeignFunctions`].
    fn call_foreign(&mut self, actor: Actor, fun: crate::sema::FunId, args: &[Value]) -> Result<Value, Fault> {
        let prog = self.prog;
        let decl = prog.fun(fun);
        let name = decl.name.clone();
        let owner = decl.owner.map(|m| prog.machine(m).name.clone());
        let machine = match actor {
            Actor::Machine(m) => Some(m),
            Actor::Monitor(_) => None,
        };
        if decl.owner.is_none() {
            if let Some(r) = self.call_time_function(actor, &name, args) {
                return r.map_err(Into::into);
            }
        }
        let Some(f) = self.foreign.clone() else {
            return Err(foreign_missing(&name).into());
        };
        let mut cx = crate::checker::foreign::ForeignCx { rt: self, machine };
        match f.call(&name, owner.as_deref(), args, &mut cx) {
            None => Err(foreign_missing(&name).into()),
            Some(Ok(v)) => Ok(v),
            Some(Err(crate::checker::foreign::ForeignError::Stopped(s))) => Err(s.into()),
            Some(Err(crate::checker::foreign::ForeignError::Exception(ty, msg))) => Err(Exception::new(ty, msg).into()),
        }
    }

    fn resume(&mut self, actor: Actor, exec: &mut Exec, p: Pending) -> Result<Flow, Fault> {
        let Actor::Machine(m) = actor else { unreachable!("monitors never reach a scheduling point") };
        match p {
            Pending::Create { machine, iface, payload } => {
                let v = self.finish_create(m, machine, iface, payload);
                if !exec.frames.is_empty() {
                    exec.values.push(v);
                }
                Ok(Flow::Continue)
            }
            Pending::Send { target, event } => {
                self.finish_send(m, &target, event);
                Ok(Flow::Continue)
            }
            Pending::ReceiveWait { cases } if self.timed() => self.do_receive(m, exec, cases, true),
            Pending::ReceiveWait { cases } => {
                // the operation is enabled again only by the event it waits for
                let e = self.machines[m].received.take().expect("a blocked receive resumes with its event");
                Ok(self.dispatch_receive(m, exec, &cases, e))
            }
            Pending::ReceiveFound { cases, event } => Ok(self.dispatch_receive(m, exec, &cases, event)),
        }
    }

    fn dispatch_receive(&mut self, m: usize, exec: &mut Exec, cases: &[ReceiveCase], e: Event) -> Flow {
        self.time_receive(m, &e);
        if let Some(case) = cases.iter().find(|c| c.event == e.id) {
            if let Some(frame) = exec.frames.last_mut() {
                if let Some(slot) = case.param_slot {
                    exec.values[frame.base + slot as usize] = e.payload;
                }
                frame.pc = case.target as usize;
            }
            Flow::Continue
        } else {
            // `halt` is received without a case: re-raise it (`case PHalt _hv: RaiseEvent(_hv)`).
            self.machines[m].pending_transition = Transition::Raise(e);
            Flow::Unwound
        }
    }

    fn actor_name(&self, actor: Actor) -> String {
        match actor {
            Actor::Machine(m) => self.machines[m].name.to_string(),
            Actor::Monitor(mon) => self.monitor_full_name(mon),
        }
    }

    fn actor_state_name(&self, actor: Actor) -> String {
        match actor {
            Actor::Machine(m) => self.machine_state_name(m),
            Actor::Monitor(mon) => self.state_name(self.monitors[mon].state),
        }
    }

    fn fields_mut(&mut self, actor: Actor) -> &mut Vec<Value> {
        match actor {
            Actor::Machine(m) => &mut self.machines[m].fields,
            Actor::Monitor(mon) => &mut self.monitors[mon].fields,
        }
    }

    fn make_event(&self, id: EventId, payload: Option<Value>) -> Event {
        let payload = match payload {
            Some(p) => p,
            None => default_value(&self.prog.event(id).payload, self.prog),
        };
        Event { id, payload, stamp: None, vt: None }
    }

    /// `Runtime.Assert(false, s)`: the message goes through `String.Format` with no
    /// arguments, so braces in it make upstream throw a `FormatException` instead.
    fn assertion_failure(&mut self, text: String) -> Fault {
        match format_composite(&text, &[]) {
            Ok(t) => self.report_bug(t).into(),
            Err(m) => format_exception(m).into(),
        }
    }

    fn choose_error(&self, actor: Actor, detail: &str) -> String {
        format!(
            "Incorrect use of choose by Machine {} in state {}. {}",
            self.actor_name(actor),
            self.actor_state_name(actor),
            detail
        )
    }

    fn pick(&mut self, actor: Actor, count: usize, what: &str) -> Result<usize, Fault> {
        if count == 0 {
            let msg = self.choose_error(actor, &format!("Trying to choose from an empty {}!", what));
            return Err(self.assertion_failure(msg));
        }
        if count > 10000 {
            let msg = self.choose_error(
                actor,
                &format!("choose expects a parameter with at most 10000 choices, got {} choices instead.", count),
            );
            return Err(self.assertion_failure(msg));
        }
        let name = self.actor_name(actor);
        Ok(self.choose_int(count as i32, &name)? as usize)
    }

    /// P's `format(base, vals...)`, where the arguments have static types `types`.
    fn format(&mut self, base: &str, vals: &[Value], types: &[PType]) -> Result<String, Exception> {
        if let Some(java) = self.java {
            return java.format(base, vals, types).map_err(|m| Exception::new("java.lang.IllegalArgumentException", m));
        }
        // render every argument into one reused buffer, then slice it
        let buf = &mut self.format_buf;
        buf.clear();
        let mut ends = Vec::with_capacity(vals.len());
        for v in vals {
            v.write_display(buf, self.prog);
            ends.push(buf.len());
        }
        let args: Vec<&str> = ends
            .iter()
            .scan(0, |start, &end| {
                let arg = &buf[*start..end];
                *start = end;
                Some(arg)
            })
            .collect();
        format_composite(base, &args).map_err(format_exception)
    }

    /// Runs the innermost frame until it calls or returns, or until the action finishes,
    /// unwinds, fails or reaches a scheduling point. Every instruction is one unit of work.
    fn run_frame(&mut self, actor: Actor, exec: &mut Exec) -> Result<Flow, Fault> {
        // `step_work` lives in a local while the frame runs, so that it can stay in a
        // register; it is written back around every call that may log or run monitors.
        let mut work = self.step_work;
        let flow = self.run_ops(actor, exec, &mut work);
        self.step_work = work;
        flow
    }

    #[inline(always)]
    fn run_ops(&mut self, actor: Actor, exec: &mut Exec, work: &mut u64) -> Result<Flow, Fault> {
        let Some(frame_idx) = exec.frames.len().checked_sub(1) else {
            return Ok(Flow::Finished(Value::Null));
        };
        let frame = &exec.frames[frame_idx];
        let code = frame.code.clone();
        let fun = frame.fun;
        let base = frame.base;
        let mut pc = frame.pc;
        let limit = self.config.max_work_per_step;
        let prog = self.prog;
        let java = self.java_semantics();
        // Counts `n` units of work. A fused instruction counts its first unit with the
        // loop and the others before the part they stand for. The bound is checked as if
        // the units were counted one at a time, so it is reported with the counter just
        // over it.
        macro_rules! charge {
            ($n:expr) => {
                *work += $n;
                if limit > 0 && *work > limit {
                    *work = (*work + 1 - $n).max(limit + 1);
                    self.step_work = *work;
                    let bug = self.work_bound_exceeded(actor);
                    *work = self.step_work;
                    return Err(bug);
                }
            };
        }
        let mut next_poll = self.next_cancel_work;
        loop {
            charge!(1);
            if *work >= next_poll {
                self.check_cancelled()?;
                next_poll = work.saturating_add(4096);
                self.next_cancel_work = next_poll;
            }
            let stack = &mut exec.values;
            // every code block ends with a `Return`
            let op = &code.ops[pc];
            pc += 1;
            match op {
                Op::Probe(id) => {
                    // probes count no work (see `Op::Probe`); any bound the charge above
                    // crossed, the next instruction would cross too
                    *work -= 1;
                    if let Some(c) = self.coverage.as_mut() {
                        c.probe_hit(*id);
                    }
                }
                Op::ConditionProbe { yes, no } => {
                    *work -= 1;
                    if let Some(c) = self.coverage.as_mut() {
                        c.probe_hit(if matches!(stack.last(), Some(Value::Bool(true))) { *yes } else { *no });
                    }
                }
                Op::PushNull => stack.push(Value::Null),
                Op::PushBool(b) => stack.push(Value::Bool(*b)),
                Op::PushInt(i) => stack.push(Value::Int(*i)),
                Op::PushFloat(f) if java => stack.push(Value::Float(*f as f32 as f64)),
                Op::PushFloat(f) => stack.push(Value::Float(*f)),
                Op::PushEvent(e) => stack.push(Value::Event(*e)),
                Op::PushThis => {
                    let v = match actor {
                        Actor::Machine(m) => Value::Machine(self.machines[m].this.clone()),
                        Actor::Monitor(_) => Value::Null,
                    };
                    stack.push(v);
                }
                Op::LoadLocal(s) => {
                    let v = stack[base + *s as usize].clone();
                    stack.push(v);
                }
                Op::StoreLocal(s) => {
                    let v = pop(stack);
                    stack[base + *s as usize] = v;
                }
                Op::StoreLocalConst { slot, value } => {
                    charge!(1);
                    stack[base + *slot as usize] = value.clone();
                }
                Op::StoreField(s) => {
                    let v = pop(stack);
                    self.fields_mut(actor)[*s as usize] = v;
                }
                Op::LoadField(s) => {
                    let v = self.fields_mut(actor)[*s as usize].clone();
                    stack.push(v);
                }
                Op::LoadGlobal(s) => stack.push(self.globals[*s as usize].clone()),
                Op::Pop => {
                    pop(stack);
                }
                Op::Dup => {
                    let v = stack.last().expect("operand stack underflow").clone();
                    stack.push(v);
                }
                Op::MakeTuple(n) => {
                    let vs = pop_n(stack, *n as usize);
                    stack.push(Value::Tuple(Rc::new(vs)));
                }
                Op::MakeNamedTuple(names) => {
                    let vs = pop_n(stack, names.len());
                    stack.push(Value::NamedTuple(Rc::new(NamedTuple { names: names.clone(), values: vs })));
                }
                Op::TupleGet(i) => {
                    let v = tuple_get(&pop(stack), *i, prog)?;
                    stack.push(v);
                }
                Op::LocalTupleGet { slot, index } => {
                    charge!(1);
                    let v = tuple_get(&stack[base + *slot as usize], *index, prog)?;
                    stack.push(v);
                }
                Op::SeqGet => {
                    let idx = pop(stack);
                    let v = seq_get(&pop(stack), &idx, prog)?;
                    stack.push(v);
                }
                Op::SetGet => {
                    let idx = pop(stack);
                    let base = pop(stack);
                    let v = match &base {
                        Value::Set(s) => {
                            let i = idx.as_int();
                            if i < 0 {
                                return Err(element_at_oor().into());
                            }
                            s.element_at(i as usize).cloned().ok_or_else(element_at_oor)?
                        }
                        Value::Null => return Err(nre().into()),
                        other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PSet", prog).into()),
                    };
                    stack.push(v);
                }
                Op::MapGet { java_null } => {
                    let key = pop(stack);
                    let v = map_get(&pop(stack), &key, *java_null && java, prog)?;
                    stack.push(v);
                }
                Op::MapGetLocal { slot, java_null } => {
                    charge!(1);
                    let m = pop(stack);
                    let v = map_get(&m, &stack[base + *slot as usize], *java_null && java, prog)?;
                    stack.push(v);
                }
                Op::Sizeof => {
                    let v = size_of(&pop(stack))?;
                    stack.push(v);
                }
                Op::LocalSizeof(slot) => {
                    charge!(1);
                    let v = size_of(&stack[base + *slot as usize])?;
                    stack.push(v);
                }
                Op::KeysSizeof => {
                    let m = pop(stack);
                    let n = map_of(&m, prog)?.len();
                    charge!(1);
                    stack.push(Value::Int(n as i64));
                }
                Op::KeysContains => {
                    let m = pop(stack);
                    let m = map_of(&m, prog)?;
                    charge!(1);
                    let item = pop(stack);
                    stack.push(Value::Bool(m.contains_key(&item)));
                }
                Op::KeyAtLocal(slot) => {
                    let m = pop(stack);
                    let m = map_of(&m, prog)?;
                    charge!(2);
                    let i = stack[base + *slot as usize].as_int();
                    if i < 0 || i as usize >= m.len() {
                        return Err(index_oor().into());
                    }
                    let key = if java {
                        crate::checker::javafmt::map_keys_in_java_order(m).swap_remove(i as usize)
                    } else {
                        m.key_at(i as usize).expect("index in range").clone()
                    };
                    stack.push(key);
                }
                Op::Contains => {
                    let coll = pop(stack);
                    let item = pop(stack);
                    let b = match &coll {
                        Value::Seq(s) => s.contains(&item),
                        Value::Set(s) => s.contains(&item),
                        Value::Map(m) => m.contains_key(&item),
                        Value::Null => return Err(nre().into()),
                        _ => false,
                    };
                    stack.push(Value::Bool(b));
                }
                Op::Default(v) => stack.push(v.clone()),
                Op::Format { base, nargs, types } => {
                    let at = stack.len() - *nargs as usize;
                    let text = self.format(base, &stack[at..], types)?;
                    stack.truncate(at);
                    stack.push(Value::Str(Rc::from(text)));
                }
                Op::UnreadFormat { base, nargs, types } => {
                    let at = stack.len() - *nargs as usize;
                    let v = match java {
                        true => Value::Str(Rc::from(self.format(base, &stack[at..], types)?)),
                        false => Value::Null,
                    };
                    stack.truncate(at);
                    stack.push(v);
                }
                Op::Bin(op) => {
                    let b = pop(stack);
                    let a = pop(stack);
                    let r = bin(*op, a, b, java)?;
                    stack.push(r);
                }
                Op::IntBin { op, rhs } => {
                    charge!(rhs.work());
                    let b = operand(stack, base, *rhs);
                    let a = pop(stack);
                    let r = int_bin(*op, a, b, java)?;
                    stack.push(r);
                }
                Op::IntCmpJumpIfFalse { op, rhs, target } => {
                    charge!(rhs.work());
                    let b = operand(stack, base, *rhs);
                    let a = pop(stack);
                    let r = int_bin(*op, a, b, java)?;
                    charge!(1);
                    if !r.as_bool() {
                        pc = *target as usize;
                    }
                }
                Op::Neg => {
                    let v = match pop(stack) {
                        Value::Int(i) => Value::Int(i.wrapping_neg()),
                        Value::Float(f) => Value::Float(-f),
                        Value::Null => return Err(nre().into()),
                        other => other,
                    };
                    stack.push(v);
                }
                Op::Not => {
                    let a = pop(stack);
                    if a.is_null() {
                        return Err(nre().into());
                    }
                    stack.push(Value::Bool(!a.as_bool()));
                }
                Op::Jump(t) => pc = *t as usize,
                Op::JumpIfFalse(t) => {
                    if !pop(stack).as_bool() {
                        pc = *t as usize;
                    }
                }
                Op::JumpIfTrue(t) => {
                    if pop(stack).as_bool() {
                        pc = *t as usize;
                    }
                }
                Op::JumpIfFalseElsePop(t) => {
                    charge!(1);
                    let outcome = stack.last().expect("operand stack underflow").as_bool();
                    if let Some(c) = self.coverage.as_mut() {
                        c.condition(fun.0, pc - 1, outcome);
                    }
                    self.strategy.observe_condition(fun.0, pc - 1, outcome);
                    if !outcome {
                        pc = *t as usize;
                    } else {
                        charge!(1);
                        pop(stack);
                    }
                }
                Op::JumpIfTrueElsePop(t) => {
                    charge!(1);
                    let outcome = stack.last().expect("operand stack underflow").as_bool();
                    if let Some(c) = self.coverage.as_mut() {
                        c.condition(fun.0, pc - 1, outcome);
                    }
                    self.strategy.observe_condition(fun.0, pc - 1, outcome);
                    if outcome {
                        pc = *t as usize;
                    } else {
                        charge!(1);
                        pop(stack);
                    }
                }
                Op::Cast(ty) => check_cast(stack.last().expect("operand stack underflow"), ty, prog)?,
                Op::CoerceFloat => {
                    let v = match pop(stack) {
                        Value::Int(i) => Value::Float(i as f64),
                        Value::Null => return Err(nre().into()),
                        other => other,
                    };
                    stack.push(v);
                }
                Op::CoerceInt => {
                    let v = match pop(stack) {
                        Value::Float(f) => Value::Int(f as i64),
                        Value::Null => return Err(nre().into()),
                        other => other,
                    };
                    stack.push(v);
                }
                Op::Call { fun, nargs } => {
                    let nargs = *nargs as usize;
                    if let Some(callee) = self.code.get(*fun) {
                        exec.frames[frame_idx].pc = pc;
                        exec.push_frame(callee.clone(), *fun, nargs);
                        return Ok(Flow::Continue);
                    }
                    let at = stack.len() - nargs;
                    self.step_work = *work;
                    let v = self.call_foreign(actor, *fun, &stack[at..]);
                    *work = self.step_work;
                    let v = v?;
                    stack.truncate(at);
                    stack.push(v);
                }
                Op::Return => {
                    let v = pop(stack);
                    return Ok(return_value(exec, v));
                }
                Op::ReturnNull => {
                    charge!(1);
                    return Ok(return_value(exec, Value::Null));
                }
                Op::Store { root, path } => {
                    let rhs = pop(stack);
                    let keys = pop_n(stack, key_count(path));
                    if path.len() == 1 {
                        note_mutation(exec, frame_idx, *root);
                    }
                    let target: &mut Value = match *root {
                        Root::Local(s) => &mut exec.values[base + s as usize],
                        Root::Field(s) => &mut self.fields_mut(actor)[s as usize],
                    };
                    match path.split_last() {
                        None => *target = rhs,
                        Some((last, init)) => {
                            let nk = key_count(init);
                            let base = navigate(target, init, &keys[..nk], prog)?;
                            store_at(base, last, keys.get(nk), rhs, prog)?;
                        }
                    }
                }
                Op::ForeachInit { iter_slot, root } => {
                    let coll = pop(stack);
                    if coll.is_null() {
                        return Err(nre().into());
                    }
                    let slot = base + *iter_slot as usize;
                    stack[slot] = coll;
                    stack[slot + 1] = Value::Int(0);
                    exec.iters.retain(|it| !(it.frame == frame_idx && it.slot == *iter_slot));
                    if let Some(root) = root {
                        exec.iters.push(ActiveIter { frame: frame_idx, slot: *iter_slot, root: *root });
                    }
                }
                Op::ForeachNext { iter_slot, item_slot, exit } => {
                    let slot = base + *iter_slot as usize;
                    let idx = stack[slot + 1].as_int();
                    if idx < 0 {
                        return Err(modified().into());
                    }
                    let item = match &stack[slot] {
                        Value::Seq(s) => s.get(idx as usize).cloned(),
                        Value::Set(s) => s.element_at(idx as usize).cloned(),
                        _ => unreachable!("foreach over a sequence or set"),
                    };
                    match item {
                        Some(v) => {
                            stack[base + *item_slot as usize] = v;
                            stack[slot + 1] = Value::Int(idx + 1);
                        }
                        None => {
                            exec.iters.retain(|it| !(it.frame == frame_idx && it.slot == *iter_slot));
                            pc = *exit as usize;
                        }
                    }
                }
                _ => {
                    exec.frames[frame_idx].pc = pc;
                    self.step_work = *work;
                    let flow = self.other_op(actor, exec, frame_idx, op);
                    *work = self.step_work;
                    match flow? {
                        Flow::Continue => pc = exec.frames[frame_idx].pc,
                        flow => return Ok(flow),
                    }
                }
            }
        }
    }

    /// Executes the instructions that `run_frame` leaves out: the rarer ones and those that
    /// reach scheduling points or leave the action. The frame's `pc` is up to date.
    #[inline(never)]
    fn other_op(&mut self, actor: Actor, exec: &mut Exec, frame_idx: usize, op: &Op) -> Result<Flow, Fault> {
        let prog = self.prog;
        let base = exec.frames[frame_idx].base;
        let stack = &mut exec.values;
        match op {
            Op::Keys => {
                let m = pop(stack);
                let m = map_of(&m, prog)?;
                let keys = if self.java_semantics() {
                    crate::checker::javafmt::map_keys_in_java_order(m)
                } else {
                    m.keys().cloned().collect()
                };
                stack.push(Value::Seq(Rc::new(keys)));
            }
            Op::Values => {
                let m = pop(stack);
                let m = map_of(&m, prog)?;
                let values = if self.java_semantics() {
                    crate::checker::javafmt::map_values_in_java_order(m)
                } else {
                    m.values().cloned().collect()
                };
                stack.push(Value::Seq(Rc::new(values)));
            }
            Op::CoerceInterface(iface) => {
                let v = match pop(stack) {
                    Value::Machine(r) => {
                        let perms = self.code.permissions[iface.idx()].clone();
                        if !perms.iter().all(|e| r.permissions.contains(e)) {
                            return Err(Exception::new(
                                "PChecker.Runtime.Exceptions.PIllegalCoercionException",
                                format!("value cannot be coerced to interface I_{}", prog.interface(*iface).name),
                            )
                            .into());
                        }
                        Value::Machine(Rc::new(crate::checker::value::MachineRef {
                            id: r.id,
                            name: r.name.clone(),
                            permissions: perms,
                        }))
                    }
                    other => other,
                };
                stack.push(v);
            }
            Op::ChooseBool => {
                let name = self.actor_name(actor);
                let b = self.choose_bool(2, &name)?;
                stack.push(Value::Bool(b));
            }
            Op::ChooseInt => {
                let v = pop(stack);
                if v.is_null() {
                    return Err(nre().into());
                }
                let max = v.as_int();
                if max > 10000 {
                    let msg = self.choose_error(
                        actor,
                        "choose expects a parameter with at most 10000 choices, got {maxValue} choices instead.",
                    );
                    return Err(self.assertion_failure(msg));
                }
                let name = self.actor_name(actor);
                let i = self.choose_int(max.max(0) as i32, &name)?;
                stack.push(Value::Int(i as i64));
            }
            Op::ChooseSeq => {
                let v = pop(stack);
                let Value::Seq(s) = &v else { return Err(nre().into()) };
                let i = self.pick(actor, s.len(), "sequence")?;
                stack.push(s[i].clone());
            }
            Op::ChooseSet => {
                let v = pop(stack);
                let Value::Set(s) = &v else { return Err(nre().into()) };
                let i = self.pick(actor, s.len(), "set")?;
                stack.push(s.element_at(i).expect("choice in range").clone());
            }
            Op::ChooseMap => {
                let v = pop(stack);
                let Value::Map(m) = &v else { return Err(nre().into()) };
                let i = self.pick(actor, m.len(), "map")?;
                stack.push(m.key_at(i).expect("choice in range").clone());
            }
            Op::Insert { root, path } => {
                let value = pop(stack);
                let index = pop(stack);
                let keys = pop_n(stack, key_count(path));
                if path.is_empty() {
                    note_mutation(exec, frame_idx, *root);
                }
                let target: &mut Value = match *root {
                    Root::Local(s) => &mut exec.values[base + s as usize],
                    Root::Field(s) => &mut self.fields_mut(actor)[s as usize],
                };
                match navigate(target, path, &keys, prog)? {
                    Value::Seq(s) => {
                        let i = index.as_int();
                        if i < 0 || i as usize > s.len() {
                            return Err(insert_oor().into());
                        }
                        Rc::make_mut(s).insert(i as usize, value);
                    }
                    Value::Map(m) => {
                        if !Rc::make_mut(m).add(index.clone(), value) {
                            return Err(duplicate_key(&index.to_display(prog)).into());
                        }
                    }
                    Value::Set(s) => {
                        Rc::make_mut(s).insert(value);
                    }
                    Value::Null => return Err(nre().into()),
                    other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PSeq", prog).into()),
                }
            }
            Op::Add { root, path } => {
                let value = pop(stack);
                let keys = pop_n(stack, key_count(path));
                if path.is_empty() {
                    note_mutation(exec, frame_idx, *root);
                }
                let target: &mut Value = match *root {
                    Root::Local(s) => &mut exec.values[base + s as usize],
                    Root::Field(s) => &mut self.fields_mut(actor)[s as usize],
                };
                match navigate(target, path, &keys, prog)? {
                    Value::Set(s) => {
                        Rc::make_mut(s).insert(value);
                    }
                    Value::Seq(s) => Rc::make_mut(s).push(value),
                    Value::Null => return Err(nre().into()),
                    other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PSet", prog).into()),
                }
            }
            Op::Remove { root, path } => {
                let value = pop(stack);
                let keys = pop_n(stack, key_count(path));
                if path.is_empty() {
                    note_mutation(exec, frame_idx, *root);
                }
                let target: &mut Value = match *root {
                    Root::Local(s) => &mut exec.values[base + s as usize],
                    Root::Field(s) => &mut self.fields_mut(actor)[s as usize],
                };
                match navigate(target, path, &keys, prog)? {
                    Value::Seq(s) => {
                        let i = value.as_int();
                        if i < 0 || i as usize >= s.len() {
                            return Err(index_oor().into());
                        }
                        Rc::make_mut(s).remove(i as usize);
                    }
                    Value::Map(m) => {
                        Rc::make_mut(m).remove(&value);
                    }
                    Value::Set(s) => {
                        Rc::make_mut(s).remove(&value);
                    }
                    Value::Null => return Err(nre().into()),
                    other => return Err(invalid_cast(other, "PChecker.Runtime.Values.PSeq", prog).into()),
                }
            }
            Op::Assert => {
                let msg = pop(stack);
                let cond = pop(stack);
                if !cond.as_bool() {
                    if self.java_semantics() {
                        // `Monitor.tryAssert`: a `PAssertionFailureException`
                        let text = format!("Assertion failure: {}", msg.to_display(prog));
                        self.failure.get_or_insert(crate::checker::runtime::Failure::Assertion(text.clone()));
                        return Err(self.report_bug(text).into());
                    }
                    let text = format!("Assertion Failed: {}", msg.to_display(prog));
                    return Err(self.assertion_failure(text));
                }
            }
            Op::Print if self.java_semantics() => {
                // the generated monitors log through a java.util.logging logger that is off
                pop(stack);
            }
            Op::Print => {
                let msg = pop(stack);
                let text = msg.to_display(prog);
                self.log(|_| format!("<PrintLog> {}", text));
                if self.json_enabled() {
                    self.jlog(
                        "Print",
                        crate::checker::jsonlog::Details { log: Some(text), ..Default::default() },
                        false,
                    );
                }
            }
            Op::Create { iface, has_payload } => {
                let payload = if *has_payload { pop(stack) } else { Value::Null };
                let Actor::Machine(m) = actor else {
                    return Err(Exception::new("System.InvalidOperationException", "monitors cannot create machines")
                        .from_source(crate::checker::runtime::P_CHECKER_CORE)
                        .into());
                };
                let creator_iface = self.machines[m].iface;
                let created = self.module.link(creator_iface, *iface).unwrap_or(*iface);
                let decl = self.machines[m].decl;
                if !prog.machine(decl).creates.contains(&created) {
                    let msg = format!(
                        "Machine {} cannot create interface I_{}, not in its creates set",
                        prog.machine(decl).cs_name,
                        prog.interface(created).name
                    );
                    return Err(self.report_bug(msg).into());
                }
                let Some(machine) = self.module.machine_for(created) else {
                    let msg = format!(
                        "interface I_{} is not bound to a machine in this module",
                        prog.interface(created).name
                    );
                    return Err(self.report_bug(msg).into());
                };
                exec.pending = Some(Pending::Create { machine, iface: created, payload });
                return Ok(Flow::Yield(SchedKind::Create));
            }
            Op::Send { has_payload } => {
                let payload = if *has_payload { Some(pop(stack)) } else { None };
                let event = pop(stack);
                let target = pop(stack);
                let Actor::Machine(m) = actor else {
                    return Err(Exception::new("System.InvalidOperationException", "monitors cannot send events")
                        .from_source(crate::checker::runtime::P_CHECKER_CORE)
                        .into());
                };
                let name = self.machines[m].name.clone();
                let state = self.machine_state_name(m);
                let ev_id = match &event {
                    Value::Event(e) => *e,
                    Value::Null => {
                        let msg = format!(
                            "Machine cannot send a null event. Machine {} trying to send null event in state {}.",
                            name, state
                        );
                        return Err(self.report_bug(msg).into());
                    }
                    other => return Err(invalid_cast(other, "PChecker.Runtime.Events.Event", prog).into()),
                };
                let ev_name = self.event_name(ev_id).to_string();
                let target = match target {
                    Value::Machine(r) => r,
                    Value::Null => {
                        let msg = format!(
                            "Target in send cannot be null. Machine {} trying to send event {} to null target in state {}.",
                            name, ev_name, state
                        );
                        return Err(self.report_bug(msg).into());
                    }
                    other => return Err(invalid_cast(&other, "PChecker.Runtime.Values.PMachineValue", prog).into()),
                };
                let decl = self.machines[m].decl;
                if let Some(sends) = &prog.machine(decl).sends {
                    if !sends.contains(&ev_id) {
                        let msg = format!(
                            "Event {} is not in the sends set of the Machine {}",
                            ev_name,
                            prog.machine(decl).cs_name
                        );
                        return Err(self.report_bug(msg).into());
                    }
                }
                if !target.permissions.contains(&ev_id) {
                    let msg = format!(
                        "Event {} is not in the permissions set of the target machine {}",
                        ev_name, target.name
                    );
                    return Err(self.report_bug(msg).into());
                }
                let e = self.make_event(ev_id, payload);
                self.announce_internal(m, &e)?;
                let cur = self.current;
                self.op_mut(cur).receiver = Some(target.id);
                exec.pending = Some(Pending::Send { target, event: e });
                return Ok(Flow::Yield(SchedKind::Send));
            }
            Op::Raise { has_payload } => {
                let payload = if *has_payload { Some(pop(stack)) } else { None };
                let event = pop(stack);
                let name = self.actor_name(actor);
                let state = self.actor_state_name(actor);
                if let Actor::Machine(m) = actor {
                    if self.machines[m].status != MachineStatus::Active {
                        let msg = format!("{} invoked RaiseEvent while halting.", name);
                        return Err(self.report_bug(msg).into());
                    }
                }
                let ev_id = match &event {
                    Value::Event(e) => *e,
                    Value::Null => {
                        let msg = format!("{} is raising a null event in state {}", name, state);
                        return Err(self.report_bug(msg).into());
                    }
                    other => return Err(invalid_cast(other, "PChecker.Runtime.Events.Event", prog).into()),
                };
                let e = self.make_event(ev_id, payload);
                match actor {
                    Actor::Machine(m) => self.machines[m].pending_transition = Transition::Raise(e),
                    Actor::Monitor(mon) => self.monitors[mon].pending_transition = Transition::Raise(e),
                }
                return Ok(Flow::Unwound);
            }
            Op::Announce { has_payload } => {
                let payload = if *has_payload { pop(stack) } else { Value::Null };
                let event = pop(stack);
                let Actor::Machine(m) = actor else {
                    return Err(Exception::new("System.InvalidOperationException", "monitors cannot announce events")
                        .from_source(crate::checker::runtime::P_CHECKER_CORE)
                        .into());
                };
                let ev_id = match &event {
                    Value::Event(e) => *e,
                    Value::Null => {
                        let msg = format!(
                            "Incorrect use of announce by Machine {} in state {}. Machine cannot announce a null event",
                            self.machines[m].name,
                            self.machine_state_name(m)
                        );
                        return Err(self.report_bug(msg).into());
                    }
                    other => return Err(invalid_cast(other, "PChecker.Runtime.Events.Event", prog).into()),
                };
                let ptext = if payload.is_null() || !(self.logging || self.json_enabled()) {
                    String::new()
                } else {
                    format!(" with payload {}", payload.to_escaped(prog))
                };
                self.log(|rt| {
                    format!(
                        "<AnnounceLog> '{}' announced event '{}'{}.",
                        rt.machines[m].name,
                        rt.event_name(ev_id),
                        ptext
                    )
                });
                if self.json_enabled() {
                    let name = self.machines[m].name.to_string();
                    let ename = self.event_name(ev_id).to_string();
                    let d = crate::checker::jsonlog::Details {
                        log: Some(format!("{} announced event {}{}.", name, ename, ptext)),
                        id: Some(name),
                        event: Some(ename),
                        payload: (!payload.is_null()).then(|| crate::checker::jsonlog::to_dict(&payload, prog)),
                        ..Default::default()
                    };
                    self.jlog("Announce", d, true);
                }
                let e = Event { id: ev_id, payload, stamp: None, vt: None };
                self.announce_internal(m, &e)?;
            }
            Op::Goto { state, has_payload } => {
                let payload = if *has_payload { Some(pop(stack)) } else { None };
                match actor {
                    Actor::Machine(m) => {
                        if self.machines[m].status != MachineStatus::Active {
                            let msg = format!("{} invoked GotoState while halting.", self.machines[m].name);
                            return Err(self.report_bug(msg).into());
                        }
                        self.machines[m].goto_payload = payload;
                        self.machines[m].pending_transition = Transition::Goto(*state);
                    }
                    Actor::Monitor(mon) => {
                        self.monitors[mon].goto_payload = payload;
                        self.monitors[mon].pending_transition = Transition::Goto(*state);
                    }
                }
                return Ok(Flow::Unwound);
            }
            Op::Receive { cases } => {
                let Actor::Machine(m) = actor else {
                    return Err(Exception::new("System.InvalidOperationException", "monitors cannot receive events")
                        .from_source(crate::checker::runtime::P_CHECKER_CORE)
                        .into());
                };
                return self.do_receive(m, exec, cases.clone(), false);
            }
            _ => unreachable!("run_frame executes {:?}", op),
        }
        Ok(Flow::Continue)
    }

    /// `receive { ... }`: takes a matching event from the queue or blocks.
    /// `ReceiveEventAsync`. `at_point`: resumed at a scheduling point (`--timed`, where
    /// the event is picked only there, after earlier senders have run).
    fn do_receive(
        &mut self,
        m: usize,
        exec: &mut Exec,
        cases: Rc<[ReceiveCase]>,
        at_point: bool,
    ) -> Result<Flow, Fault> {
        if self.machines[m].status != MachineStatus::Active {
            let msg = format!("{} invoked ReceiveEventAsync while halting.", self.machines[m].name);
            return Err(self.report_bug(msg).into());
        }
        let wait = crate::checker::runtime::receive_wait_set(self.prog, &cases);
        let name = self.machines[m].name.clone();
        let state = self.log_state_name(m);
        let timed = self.timed();
        let found = if timed && !at_point {
            self.timed_candidates(m, Some(&wait)).first().copied()
        } else if timed {
            self.timed_pick(m, Some(&wait), false)?
        } else {
            self.machines[m].inbox.queue.iter().position(|e| wait.contains(&e.id))
        };
        if found.is_some() && timed && !at_point {
            exec.pending = Some(Pending::ReceiveWait { cases });
            return Ok(Flow::Yield(SchedKind::Receive));
        }
        if let Some(i) = found {
            let e = self.machines[m].inbox.queue.remove(i).unwrap();
            let ev = if self.logging || self.json_enabled() { self.event_with_payload(&e) } else { String::new() };
            self.log(|_| format!("<ReceiveLog> '{}' dequeued event '{}' in state '{}'.", name, ev, state));
            if self.json_enabled() {
                let (payload, key) = self.jpayload(&e);
                let d = crate::checker::jsonlog::Details {
                    log: Some(format!("'{}' dequeued event '{}' in state '{}'.", name, ev, state)),
                    id: Some(name.to_string()),
                    state: Some(self.short_state(self.machines[m].state)),
                    event: Some(self.event_name(e.id).to_string()),
                    payload,
                    was_blocked: Some(false),
                    payload_key: key,
                    ..Default::default()
                };
                self.jlog("ReceiveEvent", d, true);
            }
            if self.xml.is_some() {
                let ev = self.xml_event(e.id);
                self.xlog(
                    "Receive",
                    &[("id", &name), ("state", &state), ("event", &ev), ("wasBlocked", "False")],
                    None,
                );
            }
            self.cov_receive(m, e.id);
            if timed {
                return Ok(self.dispatch_receive(m, exec, &cases, e));
            }
            exec.pending = Some(Pending::ReceiveFound { cases, event: e });
            return Ok(Flow::Yield(SchedKind::Receive));
        }
        let names: Vec<String> = wait.iter().map(|e| self.event_name(*e).to_string()).collect();
        let event_names = if names.len() == 1 {
            // the single-type overload logs the full type name
            format!("'{}'", event_cs_full_name(self.prog, wait[0]))
        } else {
            wait_event_names(&names)
        };
        self.log(|_| {
            format!(
                "<ReceiveLog> {} is waiting to dequeue an event of type {} in state '{}'.",
                name, event_names, state
            )
        });
        if self.json_enabled() {
            let d = crate::checker::jsonlog::Details {
                log: Some(format!(
                    "{} is waiting to dequeue an event of type {} in state '{}'.",
                    name, event_names, state
                )),
                id: Some(name.to_string()),
                event_types: Some(names.clone()),
                ..Default::default()
            };
            self.jlog("WaitMultipleEvents", d, true);
        }
        if self.xml.is_some() {
            let evs: Vec<String> = wait.iter().map(|e| self.xml_event(*e)).collect();
            self.xlog("WaitEvent", &[("id", &name), ("event", &evs.join(", ")), ("state", &state)], None);
        }
        self.machines[m].inbox.waiting = Some(Rc::from(wait));
        let id = self.machines[m].id;
        self.op_mut(id).status = OpStatus::BlockedOnReceive;
        exec.pending = Some(Pending::ReceiveWait { cases });
        Ok(Flow::Yield(SchedKind::Join))
    }
}

/// Event list of the receive wait log (`OnWaitEvent`), including upstream's quirk of
/// leaving the second-to-last name empty for four or more types.
fn wait_event_names(names: &[String]) -> String {
    match names.len() {
        0 => "'<missing>'".to_string(),
        1 => format!("'{}'", names[0]),
        2 => format!("'{}' or '{}'", names[0], names[1]),
        3 => format!("'{}', '{}' or '{}'", names[0], names[1], names[2]),
        n => {
            let mut arr: Vec<&str> = names[..n - 2].iter().map(String::as_str).collect();
            arr.push("");
            format!("'{}' or '{}'", arr.join("', '"), names[n - 1])
        }
    }
}

fn foreign_missing(name: &str) -> Exception {
    Exception::new(
        "System.NotImplementedException",
        format!("foreign function '{}' has no implementation registered with the checker", name),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::checker::compile::{compile_program, compile_program_plain, compile_program_with_probes, Code};
    use crate::checker::runtime::{Config, Setup};

    fn analyze(src: &str) -> crate::sema::Analysis {
        let mut sm = crate::syntax::SourceMap::new();
        sm.add("model.p", src.to_string());
        crate::sema::compile(&sm, &crate::sema::Options::default()).unwrap_or_else(|_| panic!("does not compile"))
    }

    /// The bug, log and final work count of the program's test under a work bound.
    fn run(prog: &Program, code: &Code, max_work_per_step: u64) -> (Option<String>, Vec<String>, u64) {
        run_with(prog, code, max_work_per_step, false)
    }

    /// [`run`], in Java semantics if `java`.
    fn run_with(prog: &Program, code: &Code, max_work_per_step: u64, java: bool) -> (Option<String>, Vec<String>, u64) {
        run_full(prog, code, max_work_per_step, java).0
    }

    /// [`run_with`], collecting coverage; also returns the code coverage probes that ran.
    fn run_full(
        prog: &Program,
        code: &Code,
        max_work_per_step: u64,
        java: bool,
    ) -> ((Option<String>, Vec<String>, u64), crate::checker::coverage::ProbeSet) {
        let tc = prog.test_cases().into_iter().next().expect("a test case");
        let (module, main) = prog.test_case_module(&tc);
        let config = Config { max_work_per_step, ..Config::default() };
        let rng = Rc::new(std::cell::RefCell::new(crate::checker::random::RandomValueGenerator::new(0)));
        let mut strategy = crate::checker::strategy::RandomStrategy::new(1000, rng);
        let formatter = crate::checker::javafmt::JavaFormatter::new(prog);
        let tables = Rc::new(crate::checker::coverage::CoverageTables::new(prog));
        let coverage = Some(crate::checker::coverage::Coverage::new(tables, Default::default()));
        let setup = Setup { java_semantics: java.then_some(&formatter), coverage, ..Setup::default() };
        let mut rt = Runtime::new(prog, code, module, &config, &mut strategy, setup);
        rt.run_test(main.expect("a main machine"), &tc.name);
        let probes = rt.coverage.take().map(|c| c.into_info().probes).unwrap_or_default();
        ((rt.bug.clone(), rt.log.clone(), rt.step_work), probes)
    }

    /// Code compiled with coverage probes runs as without them, counting the same work,
    /// and records the statements and branches that ran.
    #[test]
    fn probes_change_nothing_but_coverage() {
        let src = "
            machine Main {
              var total: int;
              start state S {
                entry {
                  var i: int;
                  while (i < 50) {
                    if (i % 2 == 0) { total = total + i; } else { total = total - 1; }
                    if (i > 100) { print \"never\"; }
                    print format(\"{0}\", total);
                    i = i + 1;
                  }
                  assert total < 0, \"total\";
                }
              }
            }
            test tc [main=Main]: { Main };";
        let a = analyze(src);
        let (fused, probed) = (compile_program(&a.program), compile_program_with_probes(&a.program));
        assert!(fused.probes.is_empty());
        for bound in 1..1500 {
            assert_eq!(run(&a.program, &fused, bound), run(&a.program, &probed, bound), "{}", bound);
        }
        let (_, hit) = run_full(&a.program, &probed, 0, false);
        let missed: Vec<String> = (0..probed.probes.len() as u32)
            .filter(|i| !hit.contains(*i))
            .map(|i| format!("{:?}", probed.probes[i as usize].kind))
            .collect();
        // The second `if` is never true: its condition outcome, branch and print
        // never run. All work bounds above still behave exactly as without probes.
        assert_eq!(missed, ["ConditionTrue", "Then", "Stmt"]);
    }

    /// The specialized and fused instructions behave like the plain ones, and count the
    /// same work: whatever the work bound, a run logs the same lines and ends in the same
    /// way, be it the work bound or an exception.
    #[test]
    fn fused_instructions_behave_like_plain_ones() {
        const BODY: &str = "
            machine Main {
              var m: map[int, int];
              var total: int;
              start state S {
                entry {
                  var i: int; var j: int; var k: int; var d: int;
                  var t: (a: int, b: int);
                  var s: seq[int];
                  m[1] = 10; m[2] = 20; m[3] = 30;
                  s += (0, 5); s += (1, 6);
                  t = (a = 1, b = 2);
                  while (i < 100000) {
                    j = INDEX;
                    k = keys(m)[j];
                    if (k in keys(m) && sizeof(s) > 0 || t.a == 2) {
                      total = total + m[k] + t.b - 1;
                    }
                    d = DIVISOR;
                    total = total / d;
                    print format(\"{0} {1}\", total, sizeof(keys(m)));
                    helper();
                    log(format(\"{0} {1}\", m[k], total / d), i);
                    i = i + 1;
                  }
                }
              }
              fun helper() { var z: int; if (z >= 0) { z = 1; } }
              fun log(msg: string, n: int) { msg = \"\"; if (n > 1) { n = 1; } }
            }
            test tc [main=Main]: { Main };";
        // runs into the work bound; fails at `keys(m)[j]`; fails at `total / d`
        for (index, divisor, bounds) in [("i % sizeof(keys(m))", "1", 2000), ("i", "1", 600), ("i % 3", "4 - i", 900)] {
            let src = BODY.replace("INDEX", index).replace("DIVISOR", divisor);
            let a = analyze(&src);
            let (fused, plain) = (compile_program(&a.program), compile_program_plain(&a.program));
            assert_eq!(unread_formats(&fused), 1);
            for bound in 1..bounds {
                assert_eq!(run(&a.program, &fused, bound), run(&a.program, &plain, bound), "{} {}", index, bound);
            }
        }
    }

    /// The `UnreadFormat` instructions in `code`.
    fn unread_formats(code: &Code) -> usize {
        code.funs.iter().flatten().flat_map(|c| &c.ops).filter(|op| matches!(op, Op::UnreadFormat { .. })).count()
    }

    /// Compiles `calls` in the entry of a machine that also has `funs`, checks that it
    /// emits `expected` `UnreadFormat` instructions, and that it runs as without them,
    /// in C# and in Java semantics. Returns the bug and log of the run.
    fn unread_format_case(calls: &str, funs: &str, expected: usize) -> (Option<String>, Vec<String>) {
        let src = format!(
            "event E; machine Main {{ var m: map[string, int]; var r: int;
               start state S {{ entry {{ print \"a\"; {} print \"b\"; }} on E do {{}} }} {} }}
             test tc [main=Main]: {{ Main }};",
            calls, funs
        );
        let a = analyze(&src);
        let (fused, plain) = (compile_program(&a.program), compile_program_plain(&a.program));
        assert_eq!(unread_formats(&fused), expected, "{}", src);
        for java in [false, true] {
            assert_eq!(
                run_with(&a.program, &fused, 100_000, java),
                run_with(&a.program, &plain, 100_000, java),
                "{}",
                src
            );
        }
        let (bug, log, _) = run(&a.program, &fused, 100_000);
        (bug, log)
    }

    #[test]
    fn unread_format_arguments_are_not_built() {
        let empty = "fun log(msg: string) { }";
        // the arguments are still evaluated, and fail as usual
        let (bug, log) = unread_format_case("log(format(\"{0}\", m[\"x\"]));", empty, 1);
        assert!(bug.unwrap().contains("KeyNotFoundException"));
        assert!(log.iter().any(|l| l.ends_with(" a")) && !log.iter().any(|l| l.ends_with(" b")));
        // a parameter that is only written is unread
        let fun = "fun log(n: int, msg: string): int { msg = \"x\"; print \"in\"; return n; }";
        let (bug, log) = unread_format_case("r = log(1, format(\"{0}\", 2));", fun, 1);
        assert_eq!(bug, None);
        assert!(log.iter().any(|l| l.ends_with(" in")));
    }

    #[test]
    fn invalid_unread_formats_still_throw() {
        // an index that the type checker reads as 0 (it wraps around), but too large for C#:
        // a `FormatException` between `a` and `b`
        let (bug, log) = unread_format_case("log(format(\"{4294967296}\", 1));", "fun log(msg: string) { }", 0);
        let bug = bug.unwrap();
        assert!(bug.contains("System.FormatException"), "{}", bug);
        assert!(log.iter().any(|l| l.ends_with(" a")) && !log.iter().any(|l| l.ends_with(" b")));
        // valid in C#, but not in Java (`MessageFormat` reads `{{x}}` as argument `{x`), where
        // the string is still built
        let a = analyze(
            "machine Main { start state S { entry { log(format(\"{{x}} {0}\", 1)); } } fun log(msg: string) { } }
             test tc [main=Main]: { Main };",
        );
        let (fused, plain) = (compile_program(&a.program), compile_program_plain(&a.program));
        assert_eq!(unread_formats(&fused), 1);
        let java = run_with(&a.program, &fused, 0, true);
        assert!(java.0.as_ref().unwrap().contains("can't parse argument number"), "{:?}", java);
        assert_eq!(java, run_with(&a.program, &plain, 0, true));
    }

    #[test]
    fn read_or_exposed_parameters_are_formatted() {
        let f = "format(\"k{0}\", 1)";
        // read through a fused instruction (`m[s]` loads `s` in `MapGetLocal`)
        let fun = "fun get(t: map[string, int], s: string): int { return t[s]; }";
        let a = analyze(&format!(
            "machine Main {{ start state S {{ entry {{ }} }} {} }} test tc [main=Main]: {{ Main }};",
            fun
        ));
        let code = compile_program(&a.program);
        assert!(code.funs.iter().flatten().flat_map(|c| &c.ops).any(|op| matches!(op, Op::MapGetLocal { .. })));
        let (bug, _) = unread_format_case(&format!("m[\"k1\"] = 5; r = get(m, {}); assert r == 5;", f), fun, 0);
        assert_eq!(bug, None);
        // read by `print`, a `return`, a comparison, or a call
        for fun in [
            "fun log(msg: string) { print msg; }",
            "fun log(msg: string): string { return msg; }",
            "fun log(msg: string) { if (msg == \"\") { } }",
            "fun log(msg: string) { other(msg); } fun other(msg: string) { }",
        ] {
            unread_format_case(&format!("log({});", f), fun, 0);
        }
        // unread, but the callee yields, calls, raises or chooses
        for body in ["send this, E;", "receive { case E: { } }", "other();", "raise E;", "goto S;", "r = choose(3);"] {
            let fun = format!("fun log(msg: string) {{ {} }} fun other() {{ }}", body);
            unread_format_case(&format!("log({});", f), &fun, 0);
        }
    }

    #[test]
    fn integer_division_overflow_throws() {
        for op in [BinOp::Div, BinOp::Mod] {
            let e = arith(op, Value::Int(i64::MIN), Value::Int(-1)).unwrap_err();
            assert_eq!(e.ty, "System.OverflowException");
            assert_eq!(e.message, "Arithmetic operation resulted in an overflow.");
            assert_eq!(e.source, Some(crate::checker::runtime::P_CHECKER_CORE));
        }
        assert_eq!(arith(BinOp::Div, Value::Int(i64::MIN + 1), Value::Int(-1)).unwrap(), Value::Int(i64::MAX));
        assert_eq!(arith(BinOp::Mod, Value::Int(-7), Value::Int(-1)).unwrap(), Value::Int(0));
        // the other operations wrap, as unchecked C# arithmetic does
        assert_eq!(arith(BinOp::Mul, Value::Int(i64::MIN), Value::Int(-1)).unwrap(), Value::Int(i64::MIN));
    }
}
