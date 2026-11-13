//! Runtime values of P programs with the formatting and equality rules of the upstream
//! checker's `PChecker.Runtime.Values` classes.
//!
//! Values have copy semantics in P. Containers are reference counted and copied on write,
//! which makes assignment cheap while keeping the semantics.

use rustc_hash::{FxHashMap, FxHasher};
use std::fmt::Write;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::sema::{EventId, PType, Program};

/// A reference to a machine instance as seen by P code (`PMachineValue`).
#[derive(Debug, Clone)]
pub struct MachineRef {
    /// Operation id of the machine (`StateMachineId.Value`).
    pub id: u64,
    /// `Name(id)`, the display name of the machine.
    pub name: Rc<str>,
    /// Events this reference may be sent (interface permissions), sorted.
    pub permissions: Rc<[EventId]>,
}

#[derive(Debug, Clone)]
pub struct NamedTuple {
    pub names: Rc<[String]>,
    pub values: Vec<Value>,
}

#[derive(Debug, Clone, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    /// Integers and enum values (`PInt`).
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    Machine(Rc<MachineRef>),
    Event(EventId),
    Tuple(Rc<Vec<Value>>),
    NamedTuple(Rc<NamedTuple>),
    Seq(Rc<Vec<Value>>),
    Set(Rc<SlotSet>),
    Map(Rc<SlotMap>),
    /// A value of a foreign type.
    Foreign(Rc<dyn crate::checker::foreign::ForeignValue>),
}

/// `PValues.SafeEquals`.
impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Machine(a), Value::Machine(b)) => a.id == b.id,
            (Value::Event(a), Value::Event(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y),
            (Value::NamedTuple(a), Value::NamedTuple(b)) => {
                a.values.len() == b.values.len()
                    && a.names.iter().zip(b.names.iter()).all(|(x, y)| x == y)
                    && a.values.iter().zip(b.values.iter()).all(|(x, y)| x == y)
            }
            (Value::Seq(a), Value::Seq(b)) => a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y),
            (Value::Set(a), Value::Set(b)) => a.len() == b.len() && a.iter().all(|x| b.contains(x)),
            (Value::Map(a), Value::Map(b)) => {
                a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).map(|w| w == v).unwrap_or(false))
            }
            (Value::Foreign(a), Value::Foreign(b)) => a.eq_value(b.as_ref()),
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::Null => 0u8.hash(state),
            Value::Bool(b) => b.hash(state),
            Value::Int(i) => i.hash(state),
            Value::Float(f) => {
                let f = if *f == 0.0 { 0.0 } else { *f };
                f.to_bits().hash(state)
            }
            Value::Str(s) => s.hash(state),
            Value::Machine(m) => m.id.hash(state),
            Value::Event(e) => e.hash(state),
            Value::Tuple(t) => {
                for v in t.iter() {
                    v.hash(state);
                }
            }
            Value::NamedTuple(t) => {
                for v in t.values.iter() {
                    v.hash(state);
                }
            }
            Value::Seq(s) => {
                for v in s.iter() {
                    v.hash(state);
                }
            }
            // order-independent hashes for sets and maps
            Value::Set(s) => {
                let mut h: u64 = 0;
                for v in s.iter() {
                    let mut hh = FxHasher::default();
                    v.hash(&mut hh);
                    h = h.wrapping_add(hh.finish());
                }
                h.hash(state)
            }
            Value::Map(m) => {
                let mut h: u64 = 0;
                for (k, _) in m.iter() {
                    let mut hh = FxHasher::default();
                    k.hash(&mut hh);
                    h = h.wrapping_add(hh.finish());
                }
                h.hash(state)
            }
            Value::Foreign(f) => f.hash_value().hash(state),
        }
    }
}

/// A hash set that enumerates in the order .NET's `HashSet<T>` does: entries live in slots
/// in insertion order, and removed slots are reused (most recently freed first).
/// Sets and maps with at most this many slots (live or freed) look keys up by scanning the
/// slots, which is faster than hashing for the small collections models use.
const SCAN_SLOTS: usize = 8;

/// `a == b`, with the common key types compared inline.
#[inline(always)]
fn key_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Machine(x), Value::Machine(y)) => x.id == y.id,
        _ => a == b,
    }
}

#[derive(Debug, Clone, Default)]
pub struct SlotSet {
    slots: Vec<Option<Value>>,
    index: FxHashMap<Value, usize>,
    free: Vec<usize>,
    /// Insertion stamp of each slot (for Java's `LinkedHashSet` order).
    stamps: Vec<u64>,
    next_stamp: u64,
}

impl SlotSet {
    pub fn new() -> SlotSet {
        SlotSet::default()
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn contains(&self, v: &Value) -> bool {
        if self.slots.len() <= SCAN_SLOTS {
            return self.slots.iter().any(|s| s.as_ref().is_some_and(|x| key_eq(x, v)));
        }
        self.index.contains_key(v)
    }

    /// Adds `v`; returns false if it was already present.
    pub fn insert(&mut self, v: Value) -> bool {
        if self.contains(&v) {
            return false;
        }
        let slot = match self.free.pop() {
            Some(s) => {
                self.slots[s] = Some(v.clone());
                s
            }
            None => {
                self.slots.push(Some(v.clone()));
                self.stamps.push(0);
                self.slots.len() - 1
            }
        };
        self.stamps[slot] = self.next_stamp;
        self.next_stamp += 1;
        self.index.insert(v, slot);
        true
    }

    pub fn remove(&mut self, v: &Value) -> bool {
        match self.index.remove(v) {
            Some(slot) => {
                self.slots[slot] = None;
                self.free.push(slot);
                true
            }
            None => false,
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Value> {
        Occupied { slots: self.slots.iter(), remaining: self.len() }
    }

    /// The slots (insertion order, with holes) and the free list, which together decide
    /// the enumeration order of later inserts (used for program-state hashing).
    pub fn layout(&self) -> (&[Option<Value>], &[usize]) {
        (&self.slots, &self.free)
    }

    /// `Enumerable.ElementAt(index)` over the enumeration order.
    pub fn element_at(&self, index: usize) -> Option<&Value> {
        self.iter().nth(index)
    }

    /// The elements in insertion order (Java's `LinkedHashSet`).
    pub fn iter_insertion(&self) -> impl Iterator<Item = &Value> {
        let mut v: Vec<(u64, &Value)> =
            self.slots.iter().zip(self.stamps.iter()).filter_map(|(s, t)| s.as_ref().map(|v| (*t, v))).collect();
        v.sort_by_key(|(t, _)| *t);
        v.into_iter().map(|(_, v)| v)
    }
}

/// The occupied slots of a [`SlotSet`] or [`SlotMap`], with an exact length so that
/// collecting them allocates once.
struct Occupied<'a, T> {
    slots: std::slice::Iter<'a, Option<T>>,
    remaining: usize,
}

impl<'a, T> Iterator for Occupied<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        let item = self.slots.find_map(|s| s.as_ref())?;
        self.remaining -= 1;
        Some(item)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<T> ExactSizeIterator for Occupied<'_, T> {}

/// One [`SlotMap`] slot: a key-value pair, or `None` for a freed slot.
pub type MapSlot = Option<(Value, Value)>;

/// A map with .NET `Dictionary<K,V>` enumeration order (see [`SlotSet`]).
#[derive(Debug, Clone, Default)]
pub struct SlotMap {
    slots: Vec<MapSlot>,
    index: FxHashMap<Value, usize>,
    free: Vec<usize>,
    /// Insertion stamp of each slot (for Java's `HashMap` bucket order).
    stamps: Vec<u64>,
    next_stamp: u64,
}

impl SlotMap {
    pub fn new() -> SlotMap {
        SlotMap::default()
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The slot holding key `k`.
    #[inline]
    fn find(&self, k: &Value) -> Option<usize> {
        if self.slots.len() <= SCAN_SLOTS {
            return self.slots.iter().position(|s| s.as_ref().is_some_and(|(key, _)| key_eq(key, k)));
        }
        self.index.get(k).copied()
    }

    pub fn contains_key(&self, k: &Value) -> bool {
        self.find(k).is_some()
    }

    pub fn get(&self, k: &Value) -> Option<&Value> {
        self.find(k).and_then(|s| self.slots[s].as_ref()).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, k: &Value) -> Option<&mut Value> {
        match self.find(k) {
            Some(s) => self.slots[s].as_mut().map(|(_, v)| v),
            None => None,
        }
    }

    /// `dict[k] = v`: overwrites in place or adds a new entry.
    pub fn set(&mut self, k: Value, v: Value) {
        if let Some(s) = self.find(&k) {
            self.slots[s] = Some((k, v));
            return;
        }
        self.add_new(k, v);
    }

    /// `dict.Add(k, v)`: returns false (and does nothing) if the key exists.
    pub fn add(&mut self, k: Value, v: Value) -> bool {
        if self.contains_key(&k) {
            return false;
        }
        self.add_new(k, v);
        true
    }

    fn add_new(&mut self, k: Value, v: Value) {
        let slot = match self.free.pop() {
            Some(s) => {
                self.slots[s] = Some((k.clone(), v));
                s
            }
            None => {
                self.slots.push(Some((k.clone(), v)));
                self.stamps.push(0);
                self.slots.len() - 1
            }
        };
        self.stamps[slot] = self.next_stamp;
        self.next_stamp += 1;
        self.index.insert(k, slot);
    }

    pub fn remove(&mut self, k: &Value) -> bool {
        match self.index.remove(k) {
            Some(slot) => {
                self.slots[slot] = None;
                self.free.push(slot);
                true
            }
            None => false,
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&Value, &Value)> {
        Occupied { slots: self.slots.iter(), remaining: self.len() }.map(|(k, v)| (k, v))
    }

    pub fn keys(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.iter().map(|(_, v)| v)
    }

    /// See [`SlotSet::layout`].
    pub fn layout(&self) -> (&[MapSlot], &[usize]) {
        (&self.slots, &self.free)
    }

    pub fn key_at(&self, index: usize) -> Option<&Value> {
        self.keys().nth(index)
    }

    /// The entries in insertion order.
    pub fn iter_insertion(&self) -> impl Iterator<Item = (&Value, &Value)> {
        let mut v: Vec<(u64, &(Value, Value))> =
            self.slots.iter().zip(self.stamps.iter()).filter_map(|(s, t)| s.as_ref().map(|e| (*t, e))).collect();
        v.sort_by_key(|(t, _)| *t);
        v.into_iter().map(|(_, (k, v))| (k, v))
    }
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => false,
        }
    }

    pub fn as_int(&self) -> i64 {
        match self {
            Value::Int(i) => *i,
            Value::Float(f) => *f as i64,
            Value::Bool(b) => *b as i64,
            _ => 0,
        }
    }

    pub fn as_float(&self) -> f64 {
        match self {
            Value::Float(f) => *f,
            Value::Int(i) => *i as f64,
            _ => 0.0,
        }
    }

    /// The .NET type name of the value's class, for exception messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "PBool",
            Value::Int(_) => "PInt",
            Value::Float(_) => "PFloat",
            Value::Str(_) => "PString",
            Value::Machine(_) => "PMachineValue",
            Value::Event(_) => "Event",
            Value::Tuple(_) => "PTuple",
            Value::NamedTuple(_) => "PNamedTuple",
            Value::Seq(_) => "PSeq",
            Value::Set(_) => "PSet",
            Value::Map(_) => "PMap",
            Value::Foreign(_) => "IPValue",
        }
    }

    /// Number of elements of a collection (`Count`).
    pub fn count(&self) -> usize {
        match self {
            Value::Seq(s) => s.len(),
            Value::Set(s) => s.len(),
            Value::Map(m) => m.len(),
            _ => unreachable!("sizeof of a sequence, set or map"),
        }
    }

    /// `IPValue.ToString()` (null prints as the empty string, as in .NET string concatenation).
    pub fn to_display(&self, prog: &Program) -> String {
        let mut s = String::new();
        self.write_display(&mut s, prog);
        s
    }

    pub(crate) fn write_display(&self, out: &mut String, prog: &Program) {
        match self {
            Value::Null => {}
            Value::Bool(b) => out.push_str(if *b { "True" } else { "False" }),
            Value::Int(i) => write!(out, "{}", i).unwrap(),
            Value::Float(f) => out.push_str(&format_double(*f)),
            Value::Str(s) => out.push_str(s),
            Value::Foreign(f) => out.push_str(&f.display()),
            Value::Machine(m) => out.push_str(&m.name),
            Value::Event(e) => out.push_str(event_cs_name(prog, *e)),
            Value::Tuple(t) => {
                out.push('<');
                for v in t.iter() {
                    v.write_display(out, prog);
                    out.push(',');
                }
                out.push('>');
            }
            Value::NamedTuple(t) => {
                out.push('<');
                for (n, v) in t.names.iter().zip(t.values.iter()) {
                    out.push_str(n);
                    out.push(':');
                    v.write_display(out, prog);
                    out.push_str(", ");
                }
                out.push('>');
            }
            Value::Seq(s) => {
                out.push('(');
                for (i, v) in s.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    v.write_display(out, prog);
                }
                out.push(')');
            }
            Value::Set(s) => {
                let mut items: Vec<String> = s.iter().map(|v| v.to_display(prog)).collect();
                items.sort();
                out.push('(');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push('<');
                    out.push_str(v);
                    out.push('>');
                }
                out.push(')');
            }
            Value::Map(m) => {
                let mut items: Vec<(String, &Value)> = m.iter().map(|(k, v)| (k.to_display(prog), v)).collect();
                items.sort_by(|a, b| a.0.cmp(&b.0));
                out.push('(');
                for (i, (k, v)) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push('<');
                    out.push_str(k);
                    out.push_str("->");
                    v.write_display(out, prog);
                    out.push('>');
                }
                out.push(')');
            }
        }
    }

    /// [`Value::to_display`], or with `escaped` [`Value::to_escaped`], of a value of
    /// static type `ty`, with enum values by name rather than as integers. `snappea`
    /// formats values this way; upstream (and `snappea-compat`) prints the integers.
    pub fn to_named(&self, ty: &PType, prog: &Program, escaped: bool) -> String {
        let mut s = String::new();
        self.write_named(&mut s, ty, prog, escaped);
        s
    }

    pub(crate) fn write_named(&self, out: &mut String, ty: &PType, prog: &Program, escaped: bool) {
        let plain = |v: &Value, out: &mut String| match escaped {
            true => v.write_escaped(out, prog),
            false => v.write_display(out, prog),
        };
        if !mentions_enum(ty, prog) {
            return plain(self, out);
        }
        match (self, ty.canonicalize(prog)) {
            (Value::Int(i), PType::Enum(id)) => match prog.enums[id.idx()].elems.iter().find(|e| e.value == *i) {
                Some(e) => out.push_str(&e.name),
                None => write!(out, "{}", i).unwrap(),
            },
            (Value::Tuple(vs), PType::Tuple(ts)) if vs.len() == ts.len() => {
                out.push('<');
                for (v, t) in vs.iter().zip(ts.iter()) {
                    v.write_named(out, t, prog, escaped);
                    out.push(',');
                }
                out.push('>');
            }
            (Value::NamedTuple(t), PType::NamedTuple(fs)) => {
                out.push('<');
                for (n, v) in t.names.iter().zip(t.values.iter()) {
                    out.push_str(n);
                    out.push(':');
                    match fs.iter().find(|(f, _)| f == n) {
                        Some((_, ft)) => v.write_named(out, ft, prog, escaped),
                        None => plain(v, out),
                    }
                    out.push_str(", ");
                }
                out.push('>');
            }
            (Value::Seq(s), PType::Seq(t)) => {
                out.push('(');
                for (i, v) in s.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    v.write_named(out, &t, prog, escaped);
                }
                out.push(')');
            }
            (Value::Set(s), PType::Set(t)) => {
                let mut items: Vec<String> = s.iter().map(|v| v.to_named(&t, prog, escaped)).collect();
                if !escaped {
                    items.sort();
                }
                out.push('(');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push('<');
                    out.push_str(v);
                    out.push('>');
                }
                out.push(')');
            }
            (Value::Map(m), PType::Map(kt, vt)) => {
                let mut items: Vec<(String, String)> =
                    m.iter().map(|(k, v)| (k.to_named(&kt, prog, escaped), v.to_named(&vt, prog, escaped))).collect();
                if !escaped {
                    items.sort_by(|a, b| a.0.cmp(&b.0));
                }
                out.push('(');
                for (i, (k, v)) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push('<');
                    out.push_str(k);
                    out.push_str("->");
                    out.push_str(v);
                    out.push('>');
                }
                out.push(')');
            }
            _ => plain(self, out),
        }
    }

    /// `IPValue.ToEscapedString()`: strings quoted, nested nulls as `null`, containers in
    /// enumeration order.
    pub fn to_escaped(&self, prog: &Program) -> String {
        let mut s = String::new();
        self.write_escaped(&mut s, prog);
        s
    }

    fn write_escaped(&self, out: &mut String, prog: &Program) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Str(s) => {
                out.push('"');
                out.push_str(&s.replace('"', "\\\""));
                out.push('"');
            }
            Value::Tuple(t) => {
                out.push('<');
                for v in t.iter() {
                    v.write_escaped(out, prog);
                    out.push(',');
                }
                out.push('>');
            }
            Value::NamedTuple(t) => {
                out.push('<');
                for (n, v) in t.names.iter().zip(t.values.iter()) {
                    out.push_str(n);
                    out.push(':');
                    v.write_escaped(out, prog);
                    out.push_str(", ");
                }
                out.push('>');
            }
            Value::Seq(s) => {
                out.push('(');
                for (i, v) in s.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    v.write_escaped(out, prog);
                }
                out.push(')');
            }
            Value::Set(s) => {
                out.push('(');
                for (i, v) in s.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push('<');
                    v.write_escaped(out, prog);
                    out.push('>');
                }
                out.push(')');
            }
            Value::Map(m) => {
                out.push('(');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push('<');
                    k.write_escaped(out, prog);
                    out.push_str("->");
                    v.write_escaped(out, prog);
                    out.push('>');
                }
                out.push(')');
            }
            other => other.write_display(out, prog),
        }
    }
}

/// Whether values of type `ty` can hold enum values, which [`Value::to_named`] names.
fn mentions_enum(ty: &PType, prog: &Program) -> bool {
    match ty {
        PType::Enum(_) => true,
        PType::TypeDef(_) => match ty.canonicalize(prog) {
            PType::TypeDef(_) => false,
            t => mentions_enum(&t, prog),
        },
        PType::Seq(t) | PType::Set(t) => mentions_enum(t, prog),
        PType::Map(k, v) => mentions_enum(k, prog) || mentions_enum(v, prog),
        PType::Tuple(ts) => ts.iter().any(|t| mentions_enum(t, prog)),
        PType::NamedTuple(fs) => fs.iter().any(|(_, t)| mentions_enum(t, prog)),
        _ => false,
    }
}

/// The C# class name of an event, as it appears in logs (`PHalt`, `DefaultEvent`).
pub fn event_cs_name(prog: &Program, e: EventId) -> &str {
    let d = prog.event(e);
    if d.is_halt() {
        "PHalt"
    } else if d.is_null() {
        "DefaultEvent"
    } else {
        &d.name
    }
}

/// The fully qualified C# name of an event (`PImplementation.e`).
pub fn event_cs_full_name(prog: &Program, e: EventId) -> String {
    let d = prog.event(e);
    if d.is_halt() {
        "PChecker.Runtime.Events.PHalt".to_string()
    } else if d.is_null() {
        "PChecker.Runtime.Events.DefaultEvent".to_string()
    } else {
        format!("PImplementation.{}", d.name)
    }
}

/// The default value of a type (`GetDefaultValue` in the code generator).
pub fn default_value(ty: &PType, prog: &Program) -> Value {
    match ty.canonicalize(prog) {
        PType::Bool => Value::Bool(false),
        PType::Int => Value::Int(0),
        PType::Float => Value::Float(0.0),
        PType::String => Value::str(""),
        PType::Enum(id) => Value::Int(prog.enums[id.idx()].elems.iter().map(|e| e.value).min().unwrap_or(0)),
        PType::Seq(_) => Value::Seq(Rc::new(Vec::new())),
        PType::Set(_) => Value::Set(Rc::new(SlotSet::new())),
        PType::Map(..) => Value::Map(Rc::new(SlotMap::new())),
        PType::Tuple(ts) => Value::Tuple(Rc::new(ts.iter().map(|t| default_value(t, prog)).collect())),
        PType::NamedTuple(fs) => Value::NamedTuple(Rc::new(NamedTuple {
            names: fs.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>().into(),
            values: fs.iter().map(|(_, t)| default_value(t, prog)).collect(),
        })),
        _ => Value::Null,
    }
}

/// Formats a double the way .NET Core's `double.ToString()` does (shortest round-trip
/// digits, scientific notation outside `1e-5 ..= 1e15`).
pub fn format_double(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    // Shortest round-trip digits via Rust's `{:e}` formatting.
    let sci = format!("{:e}", f.abs());
    let (mant, exp) = sci.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let neg = if f < 0.0 { "-" } else { "" };
    if !(-4..15).contains(&exp) {
        let mut m = String::new();
        m.push_str(&digits[..1]);
        if digits.len() > 1 {
            m.push('.');
            m.push_str(&digits[1..]);
        }
        let sign = if exp < 0 { '-' } else { '+' };
        return format!("{}{}E{}{:02}", neg, m, sign, exp.abs());
    }
    let mut out = String::from(neg);
    if exp >= 0 {
        let int_len = exp as usize + 1;
        if digits.len() <= int_len {
            out.push_str(digits);
            for _ in digits.len()..int_len {
                out.push('0');
            }
        } else {
            out.push_str(&digits[..int_len]);
            out.push('.');
            out.push_str(&digits[int_len..]);
        }
    } else {
        out.push_str("0.");
        for _ in 0..(-exp - 1) {
            out.push('0');
        }
        out.push_str(digits);
    }
    out
}

/// .NET composite formatting (`String.Format(InvariantCulture, base, args)`), with its
/// exact `FormatException` messages. P values are not `IFormattable`, so format strings
/// (`{0:x}`) are parsed but ignored; alignments (`{0,5}`, `{0,-5}`) pad.
pub fn format_composite(base: &str, args: &[&str]) -> Result<String, String> {
    const INDEX_LIMIT: usize = 1_000_000;
    const WIDTH_LIMIT: usize = 1_000_000;
    let mut s: Vec<char> = Vec::with_capacity(base.len());
    s.extend(base.chars());
    let len = s.len();
    let bad = |pos: usize, reason: &str| {
        Err(format!("Input string was not in a correct format. Failure to parse near offset {}. {}", pos, reason))
    };
    const PREMATURE: &str = "Format item ends prematurely.";
    const DIGIT: &str = "Expected an ASCII digit.";
    let mut out = String::with_capacity(base.len() + args.iter().map(|a| a.len()).sum::<usize>());
    let mut pos = 0;
    while pos < len {
        let c = s[pos];
        if c != '{' && c != '}' {
            out.push(c);
            pos += 1;
            continue;
        }
        pos += 1;
        if pos >= len {
            return bad(pos, PREMATURE);
        }
        if s[pos] == c {
            out.push(c);
            pos += 1;
            continue;
        }
        if c == '}' {
            return bad(pos, "Unexpected closing brace without a corresponding opening brace.");
        }
        // format item: index
        let mut ch = s[pos];
        if !ch.is_ascii_digit() {
            return bad(pos, DIGIT);
        }
        let mut index = 0usize;
        while ch.is_ascii_digit() && index < INDEX_LIMIT {
            index = index * 10 + ch.to_digit(10).unwrap() as usize;
            pos += 1;
            if pos >= len {
                return bad(pos, PREMATURE);
            }
            ch = s[pos];
        }
        macro_rules! skip_spaces {
            () => {
                while ch == ' ' {
                    pos += 1;
                    if pos >= len {
                        return bad(pos, PREMATURE);
                    }
                    ch = s[pos];
                }
            };
        }
        skip_spaces!();
        // alignment
        let mut width = 0usize;
        let mut left = false;
        if ch == ',' {
            pos += 1;
            if pos >= len {
                return bad(pos, PREMATURE);
            }
            ch = s[pos];
            skip_spaces!();
            if ch == '-' {
                left = true;
                pos += 1;
                if pos >= len {
                    return bad(pos, PREMATURE);
                }
                ch = s[pos];
            }
            if !ch.is_ascii_digit() {
                return bad(pos, DIGIT);
            }
            while ch.is_ascii_digit() && width < WIDTH_LIMIT {
                width = width * 10 + ch.to_digit(10).unwrap() as usize;
                pos += 1;
                if pos >= len {
                    return bad(pos, PREMATURE);
                }
                ch = s[pos];
            }
            skip_spaces!();
        }
        // format string (ignored)
        if ch == ':' {
            loop {
                pos += 1;
                if pos >= len {
                    return bad(pos, PREMATURE);
                }
                ch = s[pos];
                if ch == '}' {
                    break;
                }
                if ch == '{' {
                    return bad(pos, PREMATURE);
                }
            }
        } else if ch != '}' {
            return bad(pos, PREMATURE);
        }
        pos += 1;
        let Some(arg) = args.get(index) else {
            return Err(
                "Index (zero based) must be greater than or equal to zero and less than the size of the argument list."
                    .to_string(),
            );
        };
        let n = arg.chars().count();
        let pad = width.saturating_sub(n);
        if !left {
            out.extend(std::iter::repeat_n(' ', pad));
        }
        out.push_str(arg);
        if left {
            out.extend(std::iter::repeat_n(' ', pad));
        }
    }
    Ok(out)
}

/// The `System.FormatException` a failed [`format_composite`] throws.
pub fn format_exception(message: String) -> crate::checker::runtime::Exception {
    crate::checker::runtime::Exception::new("System.FormatException", message)
        .from_source(crate::checker::runtime::CORE_LIB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles() {
        assert_eq!(format_double(1.0), "1");
        assert_eq!(format_double(0.1), "0.1");
        assert_eq!(format_double(1.5), "1.5");
        assert_eq!(format_double(100.0), "100");
        assert_eq!(format_double(1e15), "1E+15");
        assert_eq!(format_double(1e14), "100000000000000");
        assert_eq!(format_double(0.0001), "0.0001");
        assert_eq!(format_double(0.00001), "1E-05");
        assert_eq!(format_double(-2.5), "-2.5");
        assert_eq!(format_double(123456.789), "123456.789");
    }

    #[test]
    fn composite() {
        let ok = |b: &str, a: &[&str]| format_composite(b, a);
        assert_eq!(ok("a {0} b {1}", &["x", "y"]).unwrap(), "a x b y");
        assert_eq!(ok("{{0}} {0}", &["x"]).unwrap(), "{0} x");
        assert_eq!(ok("x}}y{{", &[]).unwrap(), "x}y{");
        assert_eq!(ok("[{0,3}|{0,-3}|{0:x}]", &["a"]).unwrap(), "[  a|a  |a]");
    }

    #[test]
    fn composite_errors() {
        // messages and offsets observed from .NET 8 `String.Format` with no arguments
        let index =
            "Index (zero based) must be greater than or equal to zero and less than the size of the argument list.";
        let cases: &[(&str, &str)] = &[
            ("a{b", "2. Expected an ASCII digit."),
            ("a}b", "2. Unexpected closing brace without a corresponding opening brace."),
            ("{", "1. Format item ends prematurely."),
            ("}", "1. Format item ends prematurely."),
            ("x {maxValue} y", "3. Expected an ASCII digit."),
            ("{0,", "3. Format item ends prematurely."),
            ("{0", "2. Format item ends prematurely."),
            ("{ 0}", "1. Expected an ASCII digit."),
            ("ab{{c}", "6. Format item ends prematurely."),
            ("{0,x}", "3. Expected an ASCII digit."),
            ("{0:{{}", "3. Format item ends prematurely."),
            ("{0 x}", "3. Format item ends prematurely."),
            ("{0,5x}", "4. Format item ends prematurely."),
            ("{0:ab", "5. Format item ends prematurely."),
            ("{0:a{b}", "4. Format item ends prematurely."),
            ("{0,-}", "4. Expected an ASCII digit."),
            ("{0, ", "4. Format item ends prematurely."),
            ("{0x}", "2. Format item ends prematurely."),
            ("{12345678}", "8. Format item ends prematurely."),
            ("{0,99999999}", "10. Format item ends prematurely."),
        ];
        for (b, e) in cases {
            let msg = format_composite(b, &[]).unwrap_err();
            assert_eq!(
                msg,
                format!("Input string was not in a correct format. Failure to parse near offset {}", e),
                "{}",
                b
            );
        }
        for b in [
            "{0}",
            "{0 }",
            "{1000000}",
            "{0,-5}",
            "{0:x}}",
            "{0, 5}",
            "{0:a}b}",
            "{00}",
            "{0:}",
            "{0,5:x}",
            "{0,1000000}",
        ] {
            assert_eq!(format_composite(b, &[]).unwrap_err(), index, "{}", b);
        }
    }

    #[test]
    fn slot_order() {
        let mut s = SlotSet::new();
        for i in 0..4 {
            s.insert(Value::Int(i));
        }
        s.remove(&Value::Int(1));
        s.remove(&Value::Int(2));
        s.insert(Value::Int(9));
        let got: Vec<i64> = s.iter().map(|v| v.as_int()).collect();
        assert_eq!(got, vec![0, 9, 3]);
    }
}
