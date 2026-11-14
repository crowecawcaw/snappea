//! Foreign types and functions.
//!
//! P programs may declare `type T;` (foreign type) and body-less `fun f(..): R;` (foreign
//! function). Upstream implements them in C#; here the embedder supplies Rust
//! implementations through [`ForeignFunctions`]. Foreign values are opaque Rust objects
//! implementing [`ForeignValue`], with P value semantics: implementations never mutate a
//! value in place but return a new one.

use std::any::Any;
use std::fmt::Debug;
use std::rc::Rc;

use crate::checker::runtime::{Runtime, Stop};
use crate::checker::value::Value;

/// A value of a foreign type (upstream: a C# class implementing `IPValue`).
pub trait ForeignValue: Debug {
    /// Value equality (`IPValue.Equals`).
    fn eq_value(&self, other: &dyn ForeignValue) -> bool;
    /// Hash consistent with [`ForeignValue::eq_value`].
    fn hash_value(&self) -> u64 {
        0
    }
    /// Text used when printing the value (`ToString`).
    fn display(&self) -> String {
        format!("{:?}", self)
    }
    fn as_any(&self) -> &dyn Any;
}

/// Wraps a foreign value.
pub fn foreign(v: impl ForeignValue + 'static) -> Value {
    Value::Foreign(Rc::new(v))
}

/// Why a foreign function stopped without a result.
#[derive(Debug)]
pub enum ForeignError {
    /// A .NET-style exception (`type`, `message`), reported like any exception in an action.
    Exception(String, String),
    /// The schedule was stopped (e.g. by a nondeterministic choice hitting a bound); return it
    /// unchanged.
    Stopped(Stop),
}

impl From<Stop> for ForeignError {
    fn from(s: Stop) -> Self {
        ForeignError::Stopped(s)
    }
}

/// What a foreign function can do besides computing its result (upstream passes the
/// calling `StateMachine`).
pub struct ForeignCx<'a, 'p> {
    pub(crate) rt: &'a mut Runtime<'p>,
    pub(crate) machine: Option<usize>,
}

impl ForeignCx<'_, '_> {
    /// Name of the calling machine (`Id.Name`), if called from a machine.
    pub fn machine_name(&self) -> Option<String> {
        self.machine.map(|m| self.rt.machines[m].name.to_string())
    }

    /// `machine.LogLine`: a `<PrintLog>` line in the runtime log.
    pub fn log_line(&mut self, text: &str) {
        self.rt.log(|_| format!("<PrintLog> {}", text));
        if self.rt.json_enabled() {
            self.rt.jlog(
                "Print",
                crate::checker::jsonlog::Details { log: Some(text.to_string()), ..Default::default() },
                false,
            );
        }
    }

    /// `machine.TryRandom(max)`: a checker-controlled choice in `0..max` (0 when `max` is
    /// 0). A negative `max` throws, as `System.Random.Next` does.
    pub fn random_int(&mut self, max: i32) -> Result<i32, ForeignError> {
        if max < 0 {
            return Err(ForeignError::Exception(
                "System.ArgumentOutOfRangeException".into(),
                format!(
                    "maxValue ('{}') must be a non-negative value. (Parameter 'maxValue')\nActual value was {}.",
                    max, max
                ),
            ));
        }
        let caller = self.machine_name().unwrap_or_default();
        let i = self.rt.choose_int(max, &caller)?;
        self.rt.note_choice(self.choice_site(0), max as usize, i as usize);
        Ok(i)
    }

    /// `machine.TryRandomBool()`: a checker-controlled boolean choice.
    pub fn random_bool(&mut self) -> Result<bool, ForeignError> {
        let caller = self.machine_name().unwrap_or_default();
        let b = self.rt.choose_bool(2, &caller)?;
        self.rt.note_choice(self.choice_site(1), 2, usize::from(b));
        Ok(b)
    }

    /// Identifies a foreign choice of kind `kind` for fairness (see
    /// [`Runtime::note_choice`]): by the calling machine, as the code is not known.
    fn choice_site(&self, kind: u64) -> u64 {
        // far from the code sites of P choices, which mix a code address and an offset
        u64::MAX - 2 * self.machine.map_or(0, |m| m as u64 + 1) - kind
    }
}

/// Implementations of a program's foreign functions.
pub trait ForeignFunctions {
    /// Calls foreign function `name` (`owner` is the declaring machine for machine-local
    /// foreign functions). Returns `None` if the function is not implemented.
    fn call(
        &self,
        name: &str,
        owner: Option<&str>,
        args: &[Value],
        cx: &mut ForeignCx,
    ) -> Option<Result<Value, ForeignError>>;

    /// Whether exhaustive search can rely on these functions: they keep no state between
    /// calls (a result depends only on the arguments and the checker's random choices),
    /// and [`ForeignValue::hash_value`] tells unequal values apart. Off by default, since
    /// the search cannot check either: `snappea check` then samples schedules only, and an
    /// exhaustive search that finishes does not claim the test is correct.
    fn searchable(&self) -> bool {
        false
    }
}
