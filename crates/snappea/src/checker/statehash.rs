//! Hashing of the program state at a scheduling point.
//!
//! The hash covers everything that decides how the program can continue: globals, every
//! machine (fields, state, inbox, pending transition and the saved continuation of a
//! running action), every monitor, and the operation table. It leaves out what only
//! affects logs, traces and statistics (names, vector clocks, scenario progress, step
//! counts) and the liveness temperatures, which grow with every step and would make
//! every state unique. It also leaves out fields, locals and event payloads no check
//! depends on ([`Unhashed`]): a write-only counter cannot change how the program
//! continues, so it must not keep the search from seeing a state again.
//!
//! With time tracking on, it also covers every clock: each machine's, the stamps of
//! queued events, `Mark()` handles and the symbol counter. Clocks only grow, so a timed
//! model revisits few states; the search still finishes when the model terminates.
//!
//! Two scheduling points with the same hash therefore have the same futures (up to hash
//! collisions), which is what the exhaustive search relies on to skip repeated states;
//! the Q-learning strategy uses the same hash to recognise states it has seen before.
//!
//! Hashes are computed with FNV-1a, so they do not depend on the process or the platform:
//! a seed reproduces the same search everywhere. Strings, options and slices are fed in
//! through std's `Hash` impls, whose byte layout std does not promise to keep, so a Rust
//! upgrade could in principle change hashes (and so which states a search visits first).

use std::hash::{Hash, Hasher};

use crate::checker::compile::Unhashed;
use crate::checker::runtime::{Event, Exec, Machine, Monitor, Pending, Runtime, Stage, Transition};
use crate::checker::value::Value;

/// 64-bit FNV-1a.
pub struct StableHasher(u64);

impl StableHasher {
    pub fn new() -> StableHasher {
        StableHasher(0xcbf2_9ce4_8422_2325)
    }
}

impl Default for StableHasher {
    fn default() -> Self {
        StableHasher::new()
    }
}

impl Hasher for StableHasher {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    // fixed-width integers hash in little endian on every platform
    fn write_u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }
    fn write_u32(&mut self, v: u32) {
        self.write(&v.to_le_bytes());
    }
    fn write_usize(&mut self, v: usize) {
        self.write_u64(v as u64);
    }
    fn write_i64(&mut self, v: i64) {
        self.write_u64(v as u64);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

/// Hash of the program state of `rt` (see the module documentation).
pub fn program_state(rt: &Runtime) -> u64 {
    let u = &rt.code.unhashed;
    let mut h = StableHasher::new();
    values(&rt.globals, &mut h);
    h.write_usize(rt.ops.len());
    for op in &rt.ops {
        h.write_u64(op.id);
        h.write_u8(op.status as u8);
        op.skip_next_receive.hash(&mut h);
        op.machine.hash(&mut h);
    }
    h.write_usize(rt.machines.len());
    for m in &rt.machines {
        machine(m, u, &mut h);
    }
    h.write_usize(rt.monitors.len());
    for m in &rt.monitors {
        monitor(m, u, &mut h);
    }
    if let Some(t) = &rt.time {
        // what decides a timed model's future beyond the untimed state
        t.syms.hash(&mut h);
        t.marks.hash(&mut h);
        t.observer_clock.hash(&mut h);
        // event stamps are hashed with their events
        for m in &rt.machines {
            m.clock.hash(&mut h);
        }
    }
    h.finish()
}

fn machine(m: &Machine, u: &Unhashed, h: &mut StableHasher) {
    h.write_u64(m.id);
    m.decl.hash(h);
    m.iface.hash(h);
    masked(&m.fields, u.fields.get(m.decl.idx()), h);
    m.state.hash(h);
    h.write_u8(m.status as u8);
    h.write_usize(m.inbox.queue.len());
    for e in &m.inbox.queue {
        event(e, u, h);
    }
    opt_event(m.inbox.raised.as_ref(), u, h);
    m.inbox.closed.hash(h);
    m.inbox.handler_running.hash(h);
    m.inbox.waiting.as_deref().hash(h);
    transition(&m.pending_transition, u, h);
    opt_value(m.goto_payload.as_ref(), h);
    m.goto_target.hash(h);
    m.popped_from.hash(h);
    stage(&m.stage, u, h);
    match &m.exec {
        Some(x) => {
            h.write_u8(1);
            exec(x, u, h);
        }
        None => h.write_u8(0),
    }
    opt_event(m.received.as_ref(), u, h);
}

fn monitor(m: &Monitor, u: &Unhashed, h: &mut StableHasher) {
    m.decl.hash(h);
    masked(&m.fields, u.fields.get(m.decl.idx()), h);
    m.state.hash(h);
    transition(&m.pending_transition, u, h);
    opt_value(m.goto_payload.as_ref(), h);
}

fn stage(s: &Stage, u: &Unhashed, h: &mut StableHasher) {
    match s {
        Stage::Start(v) => {
            h.write_u8(0);
            value(v, h);
        }
        Stage::AfterStartEntry => h.write_u8(1),
        Stage::Loop => h.write_u8(2),
        Stage::DequeueTry { has_default } => {
            h.write_u8(3);
            has_default.hash(h);
        }
        Stage::Dequeued(e) => {
            h.write_u8(4);
            event(e, u, h);
        }
        Stage::Handle(e) => {
            h.write_u8(5);
            event(e, u, h);
        }
        Stage::AfterAction(e) => {
            h.write_u8(6);
            event(e, u, h);
        }
        Stage::AfterExit { e, then } => {
            h.write_u8(7);
            event(e, u, h);
            match then {
                crate::checker::runtime::ExitThen::Goto { target, with } => {
                    h.write_u8(0);
                    target.hash(h);
                    with.hash(h);
                }
                crate::checker::runtime::ExitThen::Pop => h.write_u8(1),
            }
        }
        Stage::AfterWith { e, target } => {
            h.write_u8(8);
            event(e, u, h);
            target.hash(h);
        }
        Stage::AfterEntry(e) => {
            h.write_u8(9);
            event(e, u, h);
        }
        Stage::Halt => h.write_u8(10),
        Stage::Idle => h.write_u8(11),
    }
}

fn exec(x: &Exec, u: &Unhashed, h: &mut StableHasher) {
    h.write_usize(x.frames.len());
    for (i, f) in x.frames.iter().enumerate() {
        f.fun.hash(h);
        h.write_usize(f.pc);
        let (locals, stack) = x.frame_values(i);
        masked(locals, u.locals.get(f.fun.idx()), h);
        values(stack, h);
    }
    h.write_usize(x.iters.len());
    for it in &x.iters {
        h.write_usize(it.frame);
        h.write_u32(it.slot);
        match it.root {
            crate::checker::bytecode::Root::Local(s) => {
                h.write_u8(0);
                h.write_u32(s);
            }
            crate::checker::bytecode::Root::Field(s) => {
                h.write_u8(1);
                h.write_u32(s);
            }
        }
    }
    match &x.pending {
        None => h.write_u8(0),
        Some(Pending::Create { machine, iface, payload }) => {
            h.write_u8(1);
            machine.hash(h);
            iface.hash(h);
            value(payload, h);
        }
        Some(Pending::Send { target, event: e }) => {
            h.write_u8(2);
            h.write_u64(target.id);
            event(e, u, h);
        }
        Some(Pending::ReceiveWait { cases }) => {
            h.write_u8(3);
            receive_cases(cases, h);
        }
        Some(Pending::ReceiveFound { cases, event: e }) => {
            h.write_u8(4);
            receive_cases(cases, h);
            event(e, u, h);
        }
    }
}

fn receive_cases(cases: &[crate::checker::bytecode::ReceiveCase], h: &mut StableHasher) {
    h.write_usize(cases.len());
    for c in cases {
        c.event.hash(h);
        c.param_slot.hash(h);
        h.write_u32(c.target);
    }
}

fn transition(t: &Transition, u: &Unhashed, h: &mut StableHasher) {
    match t {
        Transition::None => h.write_u8(0),
        Transition::Raise(e) => {
            h.write_u8(1);
            event(e, u, h);
        }
        Transition::Goto(s) => {
            h.write_u8(2);
            s.hash(h);
        }
    }
}

fn event(e: &Event, u: &Unhashed, h: &mut StableHasher) {
    e.id.hash(h);
    if !u.payloads.get(e.id.idx()).copied().unwrap_or(false) {
        value(&e.payload, h);
    }
    // only set while time is tracked
    if let Some(stamp) = &e.stamp {
        stamp.hash(h);
    }
}

fn opt_event(e: Option<&Event>, u: &Unhashed, h: &mut StableHasher) {
    match e {
        Some(e) => {
            h.write_u8(1);
            event(e, u, h);
        }
        None => h.write_u8(0),
    }
}

fn opt_value(v: Option<&Value>, h: &mut StableHasher) {
    match v {
        Some(v) => {
            h.write_u8(1);
            value(v, h);
        }
        None => h.write_u8(0),
    }
}

/// [`values`], leaving out those `skip` marks (see [`Unhashed`]).
fn masked(vs: &[Value], skip: Option<&Vec<bool>>, h: &mut StableHasher) {
    match skip.filter(|s| !s.is_empty()) {
        None => values(vs, h),
        Some(skip) => {
            h.write_usize(vs.len());
            for (i, v) in vs.iter().enumerate() {
                if !skip.get(i).copied().unwrap_or(false) {
                    value(v, h);
                }
            }
        }
    }
}

fn values(vs: &[Value], h: &mut StableHasher) {
    h.write_usize(vs.len());
    for v in vs {
        value(v, h);
    }
}

/// Deep, order-sensitive hash of a value. Sets and maps hash their slot layout, since
/// it decides the order in which they are enumerated later.
pub fn value(v: &Value, h: &mut StableHasher) {
    match v {
        Value::Null => h.write_u8(0),
        Value::Bool(b) => {
            h.write_u8(1);
            b.hash(h);
        }
        Value::Int(i) => {
            h.write_u8(2);
            h.write_i64(*i);
        }
        Value::Float(f) => {
            h.write_u8(3);
            h.write_u64(f.to_bits());
        }
        Value::Str(s) => {
            h.write_u8(4);
            s.hash(h);
        }
        Value::Machine(m) => {
            h.write_u8(5);
            h.write_u64(m.id);
            m.permissions.hash(h);
        }
        Value::Event(e) => {
            h.write_u8(6);
            e.hash(h);
        }
        Value::Tuple(t) => {
            h.write_u8(7);
            values(t, h);
        }
        Value::NamedTuple(t) => {
            h.write_u8(8);
            t.names.hash(h);
            values(&t.values, h);
        }
        Value::Seq(s) => {
            h.write_u8(9);
            values(s, h);
        }
        Value::Set(s) => {
            h.write_u8(10);
            let (slots, free) = s.layout();
            h.write_usize(slots.len());
            for slot in slots {
                opt_value(slot.as_ref(), h);
            }
            free.hash(h);
        }
        Value::Map(m) => {
            h.write_u8(11);
            let (slots, free) = m.layout();
            h.write_usize(slots.len());
            for slot in slots {
                match slot {
                    Some((k, v)) => {
                        h.write_u8(1);
                        value(k, h);
                        value(v, h);
                    }
                    None => h.write_u8(0),
                }
            }
            free.hash(h);
        }
        Value::Foreign(f) => {
            h.write_u8(12);
            h.write_u64(f.hash_value());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_stable() {
        let mut h = StableHasher::new();
        h.write(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn values_hash_deeply() {
        let hash = |v: &Value| {
            let mut h = StableHasher::new();
            value(v, &mut h);
            h.finish()
        };
        let a = Value::Seq(std::rc::Rc::new(vec![Value::Int(1), Value::Int(2)]));
        let b = Value::Seq(std::rc::Rc::new(vec![Value::Int(2), Value::Int(1)]));
        assert_ne!(hash(&a), hash(&b));
        assert_eq!(hash(&a), hash(&a.clone()));
    }
}
