//! The controlled runtime: machines, inboxes, monitors and the cooperative scheduler.
//!
//! Execution is single threaded. Every machine is an *operation* that runs until it
//! reaches a scheduling point (create, send, receive, default-event check, join or
//! completion). At that point the scheduling strategy picks the next operation, exactly
//! as `OperationScheduler.ScheduleNextEnabledOperation` does upstream, and the chosen
//! machine is resumed from its saved continuation.

use std::collections::VecDeque;
use std::rc::Rc;

use crate::sema::{
    EventId, FunId, HandlerAction, InterfaceId, MachineId, ModuleInfo, ParamValue, Program, StateId, Temperature,
};

use crate::checker::bytecode::{CodeBlock, ReceiveCase, Root};
use crate::checker::compile::Code;
use crate::checker::coverage::Coverage;
use crate::checker::javafmt::JavaFormatter;
use crate::checker::jsonlog::{payload_key, to_dict, Details, JsonLog};
use crate::checker::strategy::{OpInfo, ScheduleStep, ScheduleTrace, Strategy, StrategyKind};
use crate::checker::timeline::{Timeline, TimelineConfig, VectorTime};
use crate::checker::value::{default_value, event_cs_full_name, event_cs_name, MachineRef, Value};

/// An event instance: its declaration plus payload (`Value::Null` when it has none).
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: EventId,
    pub payload: Value,
    /// When the event reaches its receiver; only set while time is tracked.
    pub stamp: Option<Rc<crate::checker::time::Stamp>>,
    /// The sender's vector clock at the send; only set for the causal timelines.
    pub vt: Option<Rc<VectorTime>>,
}

/// Marker returned when the current schedule must stop (bug found or execution canceled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop;

/// The kinds of scheduling points (`AsyncOperationType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedKind {
    Start,
    Create,
    Send,
    Receive,
    Default,
    Join,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpStatus {
    Enabled,
    BlockedOnReceive,
    Completed,
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub id: u64,
    pub name: String,
    pub status: OpStatus,
    pub skip_next_receive: bool,
    /// Index into `Runtime::machines`, `None` for the test task.
    pub machine: Option<usize>,
    /// Kind of the last scheduling point (`AsyncOperation.Type`).
    pub kind: SchedKind,
    /// Target of the last send (`AsyncOperation.MessageReceiver`).
    pub receiver: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineStatus {
    Active,
    Halting,
    Halted,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Transition {
    None,
    Raise(Event),
    Goto(StateId),
}

impl Transition {
    /// The transition's type in a state machine (`Transition.Type`).
    pub fn kind_name(&self) -> &'static str {
        match self {
            Transition::None => "None",
            Transition::Raise(_) => "RaiseEvent",
            Transition::Goto(_) => "GotoState",
        }
    }

    /// The transition's type in a monitor, which names them differently
    /// (`Monitor.Transition.Type`).
    pub fn monitor_kind_name(&self) -> &'static str {
        match self {
            Transition::None => "None",
            Transition::Raise(_) => "Raise",
            Transition::Goto(_) => "Goto",
        }
    }
}

/// The event inbox of a machine (`EventQueue`).
#[derive(Debug, Default)]
pub struct Inbox {
    pub queue: VecDeque<Event>,
    pub raised: Option<Event>,
    pub closed: bool,
    pub handler_running: bool,
    /// Event types a pending `receive` waits for.
    pub waiting: Option<Rc<[EventId]>>,
}

pub enum EnqueueStatus {
    HandlerRunning,
    HandlerNotRunning,
    NextEventUnavailable,
    Dropped,
}

/// What a machine does next once it is scheduled (its continuation).
#[derive(Debug, Clone)]
pub enum Stage {
    /// Fresh machine: run the start state's entry action with the constructor payload.
    Start(Value),
    /// Entry action of the start state finished; apply its transition and start the loop.
    AfterStartEntry,
    /// Dequeue the next event.
    Loop,
    /// Resumed after the `Default` scheduling point: try to dequeue.
    DequeueTry { has_default: bool },
    /// Resumed after the `Receive` scheduling point following a dequeue.
    Dequeued(Event),
    /// Dispatch an event in the current state (`HandleEventAsync`).
    Handle(Event),
    /// A `do` action finished: apply its transition.
    AfterAction(Event),
    /// The state's exit action finished (goto or pop path).
    AfterExit { e: Event, then: ExitThen },
    /// The `goto ... with` lambda finished.
    AfterWith { e: Event, target: StateId },
    /// Entry action of the new state finished.
    AfterEntry(Event),
    /// The machine halts.
    Halt,
    /// The event handler is not running (machine idle).
    Idle,
}

#[derive(Debug, Clone)]
pub enum ExitThen {
    Goto { target: StateId, with: Option<FunId> },
    Pop,
}

/// Where the interpreter stopped inside an action, waiting for a scheduling point.
#[derive(Debug, Clone)]
pub enum Pending {
    Create {
        machine: MachineId,
        iface: InterfaceId,
        payload: Value,
    },
    Send {
        target: Rc<MachineRef>,
        event: Event,
    },
    /// Blocked in `receive`; the matching event is delivered via `Machine::received`.
    ReceiveWait {
        cases: Rc<[ReceiveCase]>,
    },
    /// Found the event in the queue; continue after the `Receive` scheduling point.
    ReceiveFound {
        cases: Rc<[ReceiveCase]>,
        event: Event,
    },
}

/// One active call. Its locals and operand stack live in [`Exec::values`].
#[derive(Debug, Clone)]
pub struct Frame {
    pub code: Rc<CodeBlock>,
    pub fun: FunId,
    pub pc: usize,
    /// Index of the frame's first local in [`Exec::values`]. The `code.frame_size` locals
    /// are followed by the operand stack, which ends where the next frame begins.
    pub base: usize,
}

/// A `foreach` loop in progress, used to detect modification of the iterated variable.
#[derive(Debug, Clone)]
pub struct ActiveIter {
    pub frame: usize,
    pub slot: u32,
    pub root: Root,
}

/// The interpreter state of one running action.
#[derive(Debug, Clone, Default)]
pub struct Exec {
    pub frames: Vec<Frame>,
    /// Locals and operand stacks of all frames, outermost first.
    pub values: Vec<Value>,
    pub pending: Option<Pending>,
    /// Name of the action for diagnostics.
    pub action: String,
    pub iters: Vec<ActiveIter>,
}

#[derive(Debug)]
pub struct Machine {
    pub id: u64,
    pub name: Rc<str>,
    pub decl: MachineId,
    /// The interface the machine was created as (drives the link and monitor maps).
    pub iface: InterfaceId,
    pub fields: Vec<Value>,
    pub state: Option<StateId>,
    pub status: MachineStatus,
    pub inbox: Inbox,
    pub pending_transition: Transition,
    pub goto_payload: Option<Value>,
    /// Target of a pending `GotoStateEvent` in the raised slot.
    pub goto_target: Option<StateId>,
    /// The state popped because it could not handle the current event (for the
    /// `UnhandledEventException` log lines).
    pub popped_from: Option<StateId>,
    pub stage: Stage,
    pub exec: Option<Exec>,
    /// Event delivered to a blocked `receive`.
    pub received: Option<Event>,
    pub this: Rc<MachineRef>,
    /// Local clock (only advanced while time is tracked).
    pub clock: crate::checker::time::Time,
    /// Vector clock (only advanced for the causal timeline representations).
    pub vt: VectorTime,
}

#[derive(Debug)]
pub struct Monitor {
    pub decl: MachineId,
    pub fields: Vec<Value>,
    pub state: Option<StateId>,
    pub temperature: usize,
    pub pending_transition: Transition,
    pub goto_payload: Option<Value>,
}

/// Configuration of one checking run (`CheckerConfiguration`).
#[derive(Debug, Clone)]
pub struct Config {
    pub max_unfair_steps: usize,
    pub max_fair_steps: usize,
    pub liveness_enabled: bool,
    pub liveness_temperature_threshold: usize,
    pub fail_on_max_steps: bool,
    pub strategy: StrategyKind,
    /// Upper bound on the work (interpreted instructions, log lines weighted) an actor may do
    /// without reaching a scheduling point; 0 = unbounded. Upstream has no such bound and
    /// simply hangs on e.g. an endless raise/goto cycle.
    pub max_work_per_step: u64,
    /// Strategy description recorded in the JSON trace (overrides the running strategy's,
    /// used when a buggy schedule is replayed to produce its JSON trace).
    pub strategy_description: Option<String>,
    /// Produce the text log (`false` while exploring: buggy schedules are replayed with
    /// logging on).
    pub logging: bool,
    /// Name of the compiled project's assembly, reported as the source of exceptions
    /// raised by the model's own code.
    pub assembly_name: String,
    /// How schedules are abstracted into timelines.
    pub timeline: TimelineConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_unfair_steps: 10000,
            max_fair_steps: 100000,
            liveness_enabled: true,
            liveness_temperature_threshold: 0,
            fail_on_max_steps: false,
            strategy: StrategyKind::Random,
            max_work_per_step: DEFAULT_MAX_WORK_PER_STEP,
            strategy_description: None,
            logging: true,
            assembly_name: "PImplementation".into(),
            timeline: TimelineConfig::default(),
        }
    }
}

/// Default for [`Config::max_work_per_step`].
pub const DEFAULT_MAX_WORK_PER_STEP: u64 = 20_000_000;

/// Weight of one log line in the per-step work budget.
pub const LOG_LINE_WORK: u64 = 20;

/// Which actor the interpreter is executing for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    Machine(usize),
    Monitor(usize),
}

/// Why execution stopped, as classified for PObserve (set when
/// [`Setup::java_semantics`] is on, or by an exception).
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// A failed `assert`; the message is `Assertion failure: <message>`.
    Assertion(String),
    /// An exception thrown by P code (`ty`, `message`).
    Exception(String, String),
    /// A monitor received an observed event its state has no handler for (Java's
    /// `UnhandledEventException`): the state and event names.
    Unhandled { state: String, event: String },
    /// Any other error reported as a bug.
    Other(String),
}

/// A .NET-style exception raised by P code.
#[derive(Debug, Clone)]
pub struct Exception {
    pub ty: std::borrow::Cow<'static, str>,
    pub message: String,
    /// The .NET assembly upstream reports the exception from (`Exception.Source`); `None`
    /// for the compiled project itself (see [`Config::assembly_name`]).
    pub source: Option<&'static str>,
}

/// `Exception.Source` of exceptions thrown by the .NET base library.
pub const CORE_LIB: &str = "System.Private.CoreLib";
/// `Exception.Source` of exceptions thrown by the P runtime library.
pub const P_CHECKER_CORE: &str = "PCheckerCore";

impl Exception {
    /// An exception with the source upstream reports for its type: collection errors come
    /// from the base library, integer division errors from the P runtime (`PInt`), the rest
    /// (null dereferences and casts in generated code, foreign code) from the project.
    pub fn new(ty: impl Into<std::borrow::Cow<'static, str>>, message: impl Into<String>) -> Exception {
        let ty = ty.into();
        let source = match ty.as_ref() {
            "System.Collections.Generic.KeyNotFoundException"
            | "System.ArgumentOutOfRangeException"
            | "System.ArgumentException" => Some(CORE_LIB),
            "System.DivideByZeroException" | "System.OverflowException" => Some(P_CHECKER_CORE),
            _ => None,
        };
        Exception { ty, message: message.into(), source }
    }

    pub fn from_source(mut self, source: &'static str) -> Exception {
        self.source = Some(source);
        self
    }
}

/// What a run is given and records beyond its [`Config`]; everything is off by default.
#[derive(Default)]
pub struct Setup<'p> {
    /// Interrupt exploration without reporting a model failure.
    pub cancelled: Option<&'p dyn Fn() -> bool>,
    /// Values of the test case's global parameters (`param`).
    pub assignment: Option<&'p crate::sema::ParamAssignment>,
    /// Implementations of foreign functions.
    pub foreign: Option<Rc<dyn crate::checker::foreign::ForeignFunctions>>,
    /// Time tracking (see `design/time/PROPOSAL.md`).
    pub time: Option<crate::checker::time::TimeOptions>,
    /// Record the JSON trace.
    pub json_trace: bool,
    /// Record the XML trace, which names the test task by this id (`Task.CurrentId`).
    pub xml_trace: Option<u64>,
    /// Accumulate activity coverage.
    pub coverage: Option<Coverage>,
    /// Record what the test's driver does (see [`crate::checker::driver_facts`]).
    pub driver_facts: Option<crate::checker::driver_facts::DriverFacts>,
    /// Run spec monitors with the semantics of upstream's Java backend, which PObserve uses:
    /// `MessageFormat` formatting and Java `toString()` (by this formatter), 32-bit floats,
    /// `with` functions before exit actions, and unhandled events reported as exceptions.
    pub java_semantics: Option<&'p JavaFormatter<'p>>,
}

pub struct Runtime<'p> {
    pub(crate) prog: &'p Program,
    pub(crate) code: &'p Code,
    pub(crate) module: &'p ModuleInfo,
    pub(crate) config: &'p Config,
    pub(crate) globals: Vec<Value>,
    pub(crate) machines: Vec<Machine>,
    pub(crate) monitors: Vec<Monitor>,
    pub(crate) ops: Vec<Operation>,
    pub(crate) current: u64,
    pub(crate) strategy: &'p mut dyn Strategy,
    pub(crate) trace: ScheduleTrace,
    pub(crate) log: Vec<String>,
    pub(crate) timeline: Timeline,
    pub(crate) bug: Option<String>,
    cancelled: Option<&'p dyn Fn() -> bool>,
    stopped: bool,
    fully_explored: bool,
    /// The strategy cut the schedule short at an already explored state.
    pruned: bool,
    scheduled_steps: usize,
    /// Work done since the last scheduling point (see [`Config::max_work_per_step`]).
    pub(crate) step_work: u64,
    /// Next cancellation checkpoint; preserved across bytecode frames.
    pub(crate) next_cancel_work: u64,
    /// The JSON trace, when enabled.
    pub(crate) json: Option<JsonLog>,
    /// The XML trace, when enabled.
    pub(crate) xml: Option<crate::checker::xmllog::XmlLog>,
    /// The strategy description at the time the bug was found.
    pub(crate) bug_strategy_description: Option<String>,
    /// Implementations of foreign functions.
    pub(crate) foreign: Option<Rc<dyn crate::checker::foreign::ForeignFunctions>>,
    /// The monitors of the test and the interfaces each observes: the module's monitor map
    /// plus every scenario monitor, which upstream attaches to all tests.
    monitor_map: Vec<(MachineId, Vec<InterfaceId>)>,
    /// Progress of each attached scenario monitor in this schedule.
    pub(crate) scenarios: Vec<ScenarioProgress>,
    /// Whether the text log is produced (see [`Config::logging`]).
    pub(crate) logging: bool,
    /// Trace position at which the step bound was hit, if it was.
    pub(crate) bound_hit_at: Option<usize>,
    /// Activity coverage being accumulated, when enabled.
    pub(crate) coverage: Option<Coverage>,
    /// What the test's driver does, when recorded.
    pub(crate) driver_facts: Option<crate::checker::driver_facts::DriverFacts>,
    /// Time tracking (see `design/time/PROPOSAL.md`); `None` when the test does not use it.
    time: Option<Box<crate::checker::time::TimeState>>,
    /// Machine event handlers started (each runs on a fresh .NET task upstream).
    pub(crate) handler_starts: u64,
    /// The first failure of this execution (see [`Failure`]).
    pub(crate) failure: Option<Failure>,
    /// See [`Setup::java_semantics`].
    pub(crate) java: Option<&'p JavaFormatter<'p>>,
    next_op_id: u64,
    action_names: Vec<String>,
    /// Scratch buffer for rendering `format` arguments, reused to avoid reallocating.
    pub(crate) format_buf: String,
}

impl<'p> Runtime<'p> {
    /// A runtime for one schedule of the test of `module`, scheduled by `strategy`.
    pub fn new(
        prog: &'p Program,
        code: &'p Code,
        module: &'p ModuleInfo,
        config: &'p Config,
        strategy: &'p mut dyn Strategy,
        setup: Setup<'p>,
    ) -> Runtime<'p> {
        let Setup {
            cancelled,
            assignment,
            foreign,
            time,
            json_trace,
            xml_trace,
            coverage,
            driver_facts,
            java_semantics,
        } = setup;
        let mut globals: Vec<Value> =
            prog.global_params.iter().map(|v| default_value(&prog.var(*v).ty, prog)).collect();
        if let Some(a) = assignment {
            for (v, val) in &a.values {
                let slot = prog.var(*v).slot as usize;
                if slot < globals.len() {
                    globals[slot] = match val {
                        ParamValue::Int(i) => Value::Int(*i),
                        ParamValue::Bool(b) => Value::Bool(*b),
                    };
                }
            }
        }
        // Actions are reported by their generated method name (`Anon_3`, `_Foo`).
        let action_names = prog.funs.iter().map(|f| f.action_name()).collect();
        Runtime {
            prog,
            code,
            module,
            config,
            globals,
            machines: Vec::new(),
            monitors: Vec::new(),
            ops: Vec::new(),
            current: 0,
            strategy,
            trace: ScheduleTrace::default(),
            log: Vec::new(),
            timeline: Timeline::new(config.timeline),
            bug: None,
            cancelled,
            stopped: false,
            fully_explored: false,
            pruned: false,
            scheduled_steps: 0,
            step_work: 0,
            next_cancel_work: 4096,
            json: json_trace.then(JsonLog::new),
            xml: xml_trace.map(crate::checker::xmllog::XmlLog::new),
            bug_strategy_description: None,
            foreign,
            monitor_map: monitor_map_with_scenarios(prog, module),
            scenarios: Vec::new(),
            logging: config.logging,
            bound_hit_at: None,
            coverage,
            driver_facts,
            time: time.map(|o| Box::new(crate::checker::time::TimeState::new(o))),
            handler_starts: 0,
            failure: None,
            java: java_semantics,
            next_op_id: 0,
            action_names,
            format_buf: String::new(),
        }
    }

    /// Whether the schedule ran until nothing could run: not stopped by a bug, the step
    /// bound, cancellation, or the search cutting it at an explored state.
    pub fn ran_to_end(&self) -> bool {
        self.fully_explored && self.bug.is_none()
    }

    /// Whether spec monitors run with the semantics of upstream's Java backend (see
    /// [`Setup::java_semantics`]).
    pub fn java_semantics(&self) -> bool {
        self.java.is_some()
    }

    pub fn fun_name(&self, f: FunId) -> &str {
        &self.action_names[f.idx()]
    }

    /// Adds a line to the text log; `line` runs only when the log is produced. The line
    /// counts toward the work budget either way, so a logged replay of a schedule does the
    /// same work as the original.
    pub fn log(&mut self, line: impl FnOnce(&Self) -> String) {
        self.step_work += LOG_LINE_WORK;
        if self.logging {
            let line = line(self);
            self.log.push(line);
        }
    }

    // ---- names ------------------------------------------------------------

    pub fn state_name(&self, s: Option<StateId>) -> String {
        match s {
            Some(s) => self.prog.state(s).name.clone(),
            None => String::new(),
        }
    }

    pub fn machine_state_name(&self, m: usize) -> String {
        self.state_name(self.machines[m].state)
    }

    /// The state name for log lines only (empty when neither log is produced).
    pub fn log_state_name(&self, m: usize) -> String {
        if self.logging || self.json_enabled() || self.xml.is_some() {
            self.machine_state_name(m)
        } else {
            String::new()
        }
    }

    pub fn monitor_full_name(&self, mon: usize) -> String {
        format!("PImplementation.{}", self.prog.machine(self.monitors[mon].decl).cs_name)
    }

    pub fn event_name(&self, e: EventId) -> &str {
        if e == GOTO_EVENT {
            return "GotoStateEvent";
        }
        event_cs_name(self.prog, e)
    }

    /// `GetEventNameWithPayload`.
    pub fn event_with_payload(&self, e: &Event) -> String {
        let name = self.event_name(e.id);
        if e.payload.is_null() {
            name.to_string()
        } else {
            format!("{} with payload ({})", name, e.payload.to_escaped(self.prog))
        }
    }

    // ---- JSON trace ------------------------------------------------------------

    /// Adds a JSON trace entry (no-op unless the JSON trace is enabled).
    pub fn jlog(&mut self, ty: &str, d: Details, update_vc: bool) {
        if let Some(j) = &mut self.json {
            j.add(ty, d, update_vc);
        }
    }

    pub fn json_enabled(&self) -> bool {
        self.json.is_some()
    }

    // ---- XML trace -------------------------------------------------------------

    /// Adds an XML trace element (no-op unless the XML trace is enabled).
    pub fn xlog(&mut self, name: &str, attrs: &[(&str, &str)], text: Option<&str>) {
        if let Some(x) = &mut self.xml {
            x.element(name, attrs, text);
        }
    }

    /// `e.GetType().FullName`.
    pub fn xml_event(&self, e: EventId) -> String {
        if e == GOTO_EVENT {
            return "PChecker.Runtime.Events.GotoStateEvent".to_string();
        }
        event_cs_full_name(self.prog, e)
    }

    /// `OnRandom`: the caller is a machine's or monitor's name.
    fn xml_random(&mut self, result: &str, caller: &str) {
        if self.xml.is_none() {
            return;
        }
        let (name, ty) = match self.machines.iter().position(|m| &*m.name == caller) {
            Some(m) => (caller.to_string(), self.machine_type(m)),
            None if caller.starts_with("PImplementation.") => (caller.to_string(), caller.to_string()),
            None => (self.xml.as_ref().unwrap().task_id.to_string(), "task".to_string()),
        };
        self.xlog("Random", &[("result", result), ("creatorName", &name), ("creatorType", &ty)], None);
    }

    /// The generated class of machine `m` (`PImplementation.Client`).
    pub fn machine_type(&self, m: usize) -> String {
        format!("PImplementation.{}", self.prog.machine(self.machines[m].decl).cs_name)
    }

    /// `(payload ToDict, payload identity)` of an event for the JSON trace.
    pub fn jpayload(&self, e: &Event) -> (Option<crate::checker::jsonlog::Json>, String) {
        if e.id == GOTO_EVENT || e.payload.is_null() {
            return (None, String::new());
        }
        (Some(to_dict(&e.payload, self.prog)), payload_key(Some(&e.payload), self.prog))
    }

    /// Short (unqualified) state name.
    pub fn short_state(&self, s: Option<StateId>) -> String {
        let n = self.state_name(s);
        n.rsplit('.').next().unwrap_or(&n).to_string()
    }

    // ---- assertions and bugs ------------------------------------------------

    /// `OperationScheduler.NotifyAssertionFailure`.
    pub fn report_bug(&mut self, text: String) -> Stop {
        if self.bug.is_none() {
            self.log(|_| format!("<ErrorLog> {}", text));
            self.log(|rt| format!("<StrategyLog> Found bug using '{}' strategy.", rt.config.strategy));
            let desc = self.config.strategy_description.clone().unwrap_or_else(|| self.strategy.description());
            self.bug_strategy_description = Some(desc.clone());
            if self.json_enabled() {
                self.jlog(
                    "AssertionFailure",
                    Details { log: Some(text.clone()), error: Some(text.clone()), ..Default::default() },
                    false,
                );
                let suffix = if desc.is_empty() { " Description: " } else { "" };
                self.jlog(
                    "StrategyDescription",
                    Details {
                        log: Some(format!("Found bug using '{}' strategy.{}", self.config.strategy, suffix)),
                        strategy: Some(self.config.strategy.to_string()),
                        strategy_description: Some(desc.clone()),
                        ..Default::default()
                    },
                    false,
                );
            }
            if self.xml.is_some() {
                self.xlog("AssertionFailure", &[], Some(&format!("<ErrorLog> {}", text)));
                let strategy = self.config.strategy.to_string();
                self.xlog("Strategy", &[("strategy", &strategy)], Some(&desc));
            }
            self.failure.get_or_insert(Failure::Other(text.clone()));
            self.bug = Some(text);
        }
        self.stopped = true;
        Stop
    }

    /// Reports an unhandled .NET exception thrown inside an action.
    pub fn report_exception(&mut self, actor: Actor, exc: &Exception, action: &str) -> Stop {
        self.failure.get_or_insert(Failure::Exception(exc.ty.to_string(), exc.message.clone()));
        if let Actor::Machine(m) = actor {
            let short = exc.ty.rsplit('.').next().unwrap_or(&exc.ty).to_string();
            let body = format!(
                "{} running action '{}' in state '{}' threw exception '{}'.",
                self.machines[m].name,
                action,
                self.machine_state_name(m),
                short
            );
            self.log(|_| format!("<ExceptionLog> {}", body));
            if self.json_enabled() {
                let d = Details {
                    log: Some(body),
                    id: Some(self.machines[m].name.to_string()),
                    state: Some(self.short_state(self.machines[m].state)),
                    action: Some(action.to_string()),
                    exception: Some(short.to_string()),
                    ..Default::default()
                };
                self.jlog("ExceptionThrown", d, true);
            }
            if self.xml.is_some() {
                let (id, state) = (self.machines[m].name.to_string(), self.machine_state_name(m));
                let text = format!("{}: {}", exc.ty, exc.message);
                self.xlog(
                    "ExceptionThrown",
                    &[("id", &id), ("state", &state), ("action", action), ("type", &exc.ty)],
                    Some(&text),
                );
            }
        }
        let ctx = match actor {
            Actor::Machine(m) => {
                let state = match self.machines[m].state {
                    Some(_) => self.machine_state_name(m),
                    None => "<unknown>".to_string(),
                };
                format!("{} (state '{}', action '{}')", self.machines[m].name, state, action)
            }
            Actor::Monitor(mon) => {
                let state = self.state_name(self.monitors[mon].state);
                format!("{} (state '{}', action '{}')", self.monitor_full_name(mon), state, action)
            }
        };
        let text = format!(
            "Exception '{}' was thrown in {}: {}\nfrom location '{}':\nThe stack trace is:\n",
            exc.ty,
            ctx,
            exc.message,
            exc.source.unwrap_or(&self.config.assembly_name)
        );
        self.report_bug(text)
    }

    // ---- operations ----------------------------------------------------------

    fn op_index(&self, id: u64) -> usize {
        self.ops.iter().position(|o| o.id == id).expect("unknown operation")
    }

    pub fn op_mut(&mut self, id: u64) -> &mut Operation {
        let i = self.op_index(id);
        &mut self.ops[i]
    }

    fn new_op(&mut self, name: String, machine: Option<usize>) -> u64 {
        let id = self.next_op_id;
        self.next_op_id += 1;
        self.ops.push(Operation {
            id,
            name,
            status: OpStatus::Enabled,
            skip_next_receive: false,
            machine,
            kind: SchedKind::Start,
            receiver: None,
        });
        id
    }

    // ---- scheduling ------------------------------------------------------------

    fn check_steps_bound(&mut self) -> Result<(), Stop> {
        if self.strategy.has_reached_max_steps() {
            self.bound_hit_at = Some(self.trace.steps.len());
            let bound = if self.strategy.is_fair() { self.config.max_fair_steps } else { self.config.max_unfair_steps };
            let msg = format!("Scheduling steps bound of {} reached.", bound);
            if self.config.fail_on_max_steps {
                return Err(self.report_bug(msg));
            }
            self.stopped = true;
            return Err(Stop);
        }
        Ok(())
    }

    fn check_liveness_temperatures(&mut self) -> Result<(), Stop> {
        if !self.config.liveness_enabled || !self.strategy.is_fair() {
            return Ok(());
        }
        let threshold = self.config.liveness_temperature_threshold;
        if threshold == 0 {
            return Ok(());
        }
        for i in 0..self.monitors.len() {
            if self.prog.machine(self.monitors[i].decl).is_scenario() {
                continue;
            }
            let hot =
                self.monitors[i].state.map(|s| self.prog.state(s).temperature == Temperature::Hot).unwrap_or(false);
            if hot {
                self.monitors[i].temperature += 1;
                if self.monitors[i].temperature > threshold {
                    let msg = format!(
                        "{} detected potential liveness bug in hot state '{}'.",
                        self.monitor_full_name(i),
                        self.state_name(self.monitors[i].state)
                    );
                    return Err(self.report_bug(msg));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn check_cancelled(&mut self) -> Result<(), Stop> {
        if self.cancelled.is_some_and(|f| f()) {
            self.stopped = true;
            Err(Stop)
        } else {
            Ok(())
        }
    }

    /// `ScheduleNextEnabledOperation`: picks the operation to run next.
    pub fn schedule_next(&mut self, kind: SchedKind) -> Result<(), Stop> {
        self.check_cancelled()?;
        let cur = self.current;
        self.op_mut(cur).kind = kind;
        self.check_steps_bound()?;
        self.check_liveness_temperatures()?;
        let infos: Vec<OpInfo> = self
            .ops
            .iter()
            .map(|o| OpInfo { id: o.id, enabled: o.status == OpStatus::Enabled, kind: o.kind, receiver: o.receiver })
            .collect();
        let mut infos = infos;
        if self.time.as_ref().is_some_and(|t| t.opts.timed) {
            self.filter_timed(&mut infos);
        }
        if self.strategy.wants_state_hash() {
            let hash = crate::checker::statehash::program_state(self);
            if !self.strategy.observe_state(hash) {
                self.pruned = true;
                self.stopped = true;
                return Err(Stop);
            }
        }
        let current = self.current;
        match self.strategy.next_operation(current, &infos) {
            Some(next) => {
                self.step_work = 0;
                self.next_cancel_work = 4096;
                self.scheduled_steps = self.strategy.scheduled_steps();
                self.trace.steps.push(ScheduleStep::Operation(next));
                self.current = next;
                Ok(())
            }
            // the strategy cannot go on (a replay diverged): neither a deadlock nor the end
            None if infos.iter().any(|o| o.enabled) => {
                self.stopped = true;
                Err(Stop)
            }
            None => {
                self.check_deadlock()?;
                self.fully_explored = true;
                self.stopped = true;
                Err(Stop)
            }
        }
    }

    fn check_deadlock(&mut self) -> Result<(), Stop> {
        let blocked: Vec<String> =
            self.ops.iter().filter(|o| o.status == OpStatus::BlockedOnReceive).map(|o| o.name.clone()).collect();
        if blocked.is_empty() {
            return Ok(());
        }
        let mut msg = String::from("Deadlock detected.");
        let n = blocked.len();
        for (i, name) in blocked.iter().enumerate() {
            msg.push(' ');
            msg.push_str(name);
            if i + 2 == n {
                msg.push_str(" and");
            } else if i + 1 < n {
                msg.push(',');
            }
        }
        msg.push_str(if n == 1 { " is " } else { " are " });
        msg.push_str("waiting to receive an event, but no other controlled tasks are enabled.");
        Err(self.report_bug(msg))
    }

    /// `GetNondeterministicBooleanChoice`.
    pub fn choose_bool(&mut self, max: i32, caller: &str) -> Result<bool, Stop> {
        self.check_steps_bound()?;
        self.check_liveness_temperatures()?;
        let b = self.strategy.next_bool(max);
        if self.strategy.diverged() {
            self.stopped = true;
            return Err(Stop);
        }
        self.scheduled_steps = self.strategy.scheduled_steps();
        self.trace.steps.push(ScheduleStep::Bool(b));
        self.log(|_| {
            format!("<RandomLog> {} nondeterministically chose '{}'.", caller, if b { "True" } else { "False" })
        });
        self.xml_random(if b { "True" } else { "False" }, caller);
        Ok(b)
    }

    /// `GetNondeterministicIntegerChoice`.
    pub fn choose_int(&mut self, max: i32, caller: &str) -> Result<i32, Stop> {
        self.check_steps_bound()?;
        self.check_liveness_temperatures()?;
        let i = self.strategy.next_int(max);
        if self.strategy.diverged() {
            self.stopped = true;
            return Err(Stop);
        }
        self.scheduled_steps = self.strategy.scheduled_steps();
        self.trace.steps.push(ScheduleStep::Int(i));
        self.log(|_| format!("<RandomLog> {} nondeterministically chose '{}'.", caller, i));
        self.xml_random(&i.to_string(), caller);
        Ok(i)
    }

    // ---- test driver ---------------------------------------------------------------

    /// Runs one schedule of the test whose main machine is `main`.
    pub fn run_test(&mut self, main: MachineId, test_name: &str) {
        self.log(|_| format!("<TestLog> Running test '{}'.", test_name));
        let task = self.new_op("Task(0)".into(), None);
        self.current = task;
        let r = (|| -> Result<(), Stop> {
            self.create_monitors()?;
            // The test task creates the main machine.
            self.schedule_next(SchedKind::Create)?;
            let iface = self.prog.machine(main).interface.unwrap_or(InterfaceId(0));
            let name = self.prog.machine(main).name.clone();
            let (midx, _) =
                self.instantiate_machine(main, iface, &name, Value::Null, crate::checker::time::Time::default());
            let creator = format!("task '{}'", 1);
            let mname = self.machines[midx].name.clone();
            self.log(|_| format!("<CreateLog> {} was created by {}.", mname, creator));
            if self.json_enabled() {
                let d = Details {
                    log: Some(format!("{} was created by {}.", mname, creator)),
                    id: Some(mname.to_string()),
                    ..Default::default()
                };
                self.jlog("CreateStateMachine", d, true);
            }
            if let Some(x) = &self.xml {
                let task = x.task_id.to_string();
                self.xlog(
                    "CreateStateMachine",
                    &[("id", &mname), ("creatorName", &task), ("creatorType", "task")],
                    None,
                );
            }
            self.start_handler(midx);
            self.op_mut(task).status = OpStatus::Completed;
            self.schedule_next(SchedKind::Stop)?;
            self.run_loop()
        })();
        let _ = r;
    }

    fn run_loop(&mut self) -> Result<(), Stop> {
        loop {
            if self.stopped {
                return Err(Stop);
            }
            let cur = self.current;
            let Some(m) = self.ops[self.op_index(cur)].machine else {
                // The test task is never rescheduled after completion.
                self.stopped = true;
                return Err(Stop);
            };
            match self.run_machine(m)? {
                SchedKind::Stop => {
                    let op_id = self.machines[m].id;
                    {
                        let op = self.op_mut(op_id);
                        op.status = OpStatus::Completed;
                        op.skip_next_receive = true;
                    }
                    self.schedule_next(SchedKind::Stop)?;
                }
                kind => self.schedule_next(kind)?,
            }
        }
    }

    /// The name of the hot state monitor `mon` is in, if it is in one.
    pub fn monitor_hot_state(&self, mon: usize) -> Option<&str> {
        let s = self.monitors.get(mon)?.state?;
        let st = self.prog.state(s);
        (st.temperature == Temperature::Hot).then_some(st.name.as_str())
    }

    /// Checks monitors for liveness violations once a schedule is fully explored.
    pub fn check_hot_monitors_at_termination(&mut self) {
        if !self.fully_explored || self.bug.is_some() {
            return;
        }
        for i in 0..self.monitors.len() {
            if self.prog.machine(self.monitors[i].decl).is_scenario() {
                continue;
            }
            if let Some(s) = self.monitors[i].state {
                if self.prog.state(s).temperature == Temperature::Hot {
                    let msg = format!(
                        "{} detected liveness bug in hot state '{}' at the end of program execution.",
                        self.monitor_full_name(i),
                        self.prog.state(s).name
                    );
                    let _ = self.report_bug(msg);
                    return;
                }
            }
        }
    }

    // ---- machine creation -------------------------------------------------------------

    fn instantiate_machine(
        &mut self,
        decl: MachineId,
        iface: InterfaceId,
        name: &str,
        payload: Value,
        clock: crate::checker::time::Time,
    ) -> (usize, u64) {
        self.cov_declare(decl, false);
        if let Some(d) = self.driver_facts.as_mut() {
            d.record_create(decl);
        }
        let idx = self.machines.len();
        let id = self.next_op_id;
        let display: Rc<str> = Rc::from(format!("{}({})", name, id));
        let permissions: Rc<[EventId]> = self.code.permissions[iface.idx()].clone();
        let this = Rc::new(MachineRef { id, name: display.clone(), permissions });
        let fields =
            self.prog.machine(decl).fields.iter().map(|v| default_value(&self.prog.var(*v).ty, self.prog)).collect();
        let start = self.prog.machine(decl).start_state;
        self.machines.push(Machine {
            id,
            name: display.clone(),
            decl,
            iface,
            fields,
            state: start,
            status: MachineStatus::Active,
            inbox: Inbox { handler_running: true, ..Default::default() },
            pending_transition: Transition::None,
            goto_payload: None,
            goto_target: None,
            popped_from: None,
            stage: Stage::Start(payload),
            exec: None,
            received: None,
            this,
            clock,
            vt: VectorTime::new(id),
        });
        let op = self.new_op(display.to_string(), Some(idx));
        debug_assert_eq!(op, id);
        (idx, id)
    }

    /// `RunStateMachineEventHandler`: enables the machine's operation.
    fn start_handler(&mut self, m: usize) {
        self.handler_starts += 1;
        let id = self.machines[m].id;
        self.op_mut(id).status = OpStatus::Enabled;
    }

    /// Completes a `new I(...)` once the `Create` scheduling point has passed.
    pub fn finish_create(&mut self, creator: usize, machine: MachineId, iface: InterfaceId, payload: Value) -> Value {
        let name = self.prog.interface(iface).name.clone();
        let clock = self.machines[creator].clock.clone();
        if let Some(d) = self.driver_facts.as_mut() {
            d.record_create_payload(self.prog, self.machines[creator].decl, machine, &payload);
        }
        let (idx, _) = self.instantiate_machine(machine, iface, &name, payload, clock);
        let created = self.machines[idx].name.clone();
        let creator_name = self.machines[creator].name.clone();
        self.log(|_| format!("<CreateLog> {} was created by {}.", created, creator_name));
        if self.json_enabled() {
            let d = Details {
                log: Some(format!("{} was created by {}.", created, creator_name)),
                id: Some(created.to_string()),
                creator_name: Some(creator_name.to_string()),
                creator_type: Some(self.prog.machine(self.machines[creator].decl).cs_name.clone()),
                ..Default::default()
            };
            self.jlog("CreateStateMachine", d, true);
        }
        if self.xml.is_some() {
            let ty = self.machine_type(creator);
            self.xlog(
                "CreateStateMachine",
                &[("id", &created), ("creatorName", &creator_name), ("creatorType", &ty)],
                None,
            );
        }
        self.start_handler(idx);
        Value::Machine(self.machines[idx].this.clone())
    }

    pub fn find_machine(&self, id: u64) -> Option<usize> {
        self.machines.iter().position(|m| m.id == id)
    }

    // ---- sending --------------------------------------------------------------------

    /// Completes a `send` once the `Send` scheduling point has passed.
    pub fn finish_send(&mut self, sender: usize, target: &MachineRef, event: Event) {
        if self.coverage.is_some() {
            if let Some(t) = self.find_machine(target.id) {
                let decl = self.machines[t].decl;
                self.cov_send(sender, decl, event.id);
            }
        }
        if self.driver_facts.is_some() {
            if let Some(t) = self.find_machine(target.id) {
                let (id, from, to) = (self.machines[sender].id, self.machines[sender].decl, self.machines[t].decl);
                if let Some(d) = self.driver_facts.as_mut() {
                    d.record_send(self.prog, id, from, to, event.id, &event.payload);
                }
            }
        }
        let sender_name = self.machines[sender].name.clone();
        let sender_state = self.log_state_name(sender);
        let ev = if self.logging || self.json_enabled() { self.event_with_payload(&event) } else { String::new() };
        let tidx = self.find_machine(target.id);
        let halted = tidx.map(|t| self.machines[t].status == MachineStatus::Halted).unwrap_or(true);
        if self.json_enabled() {
            let (payload, key) = self.jpayload(&event);
            let d = Details {
                log: Some(format!(
                    "'{}' in state '{}' sent event '{}' to '{}'{}.",
                    sender_name,
                    sender_state,
                    ev,
                    target.name,
                    if halted { " which has halted" } else { "" }
                )),
                state: Some(self.short_state(self.machines[sender].state)),
                event: Some(self.event_name(event.id).to_string()),
                payload,
                sender: Some(sender_name.to_string()),
                target: Some(target.name.to_string()),
                is_target_halted: Some(halted),
                payload_key: key,
                ..Default::default()
            };
            self.jlog("SendEvent", d, true);
        }
        if self.xml.is_some() {
            let (ty, ev) = (self.machine_type(sender), self.xml_event(event.id));
            let halted = if halted { "True" } else { "False" };
            self.xlog(
                "Send",
                &[
                    ("target", &target.name),
                    ("senderName", &sender_name),
                    ("senderType", &ty),
                    ("senderState", &sender_state),
                    ("event", &ev),
                    ("isTargetHalted", halted),
                ],
                None,
            );
        }
        let mut event = event;
        if self.timeline.config.repr.causal() {
            let id = self.machines[sender].id;
            self.machines[sender].vt.increment(id);
            event.vt = Some(Rc::new(self.machines[sender].vt.clone()));
        }
        if halted {
            self.log(|_| {
                format!(
                    "<SendLog> '{}' in state '{}' sent event '{}' to '{}' which has halted.",
                    sender_name, sender_state, ev, target.name
                )
            });
            return;
        }
        self.log(|_| {
            format!("<SendLog> '{}' in state '{}' sent event '{}' to '{}'.", sender_name, sender_state, ev, target.name)
        });
        let t = tidx.unwrap();
        if self.time.is_some() {
            event.stamp = Some(Rc::new(self.send_stamp(sender, t, event.id)));
        }
        if let EnqueueStatus::HandlerNotRunning = self.enqueue(t, event) {
            self.start_handler(t);
        }
    }

    /// `EventQueue.Enqueue`.
    pub fn enqueue(&mut self, t: usize, e: Event) -> EnqueueStatus {
        if self.machines[t].status == MachineStatus::Halted || self.machines[t].inbox.closed {
            return EnqueueStatus::Dropped;
        }
        let waiting = self.machines[t].inbox.waiting.clone();
        if waiting.is_some_and(|w| w.contains(&e.id)) {
            if self.timed() {
                // the receiver picks among its queued events when it resumes
                self.machines[t].inbox.waiting = None;
                self.machines[t].inbox.queue.push_back(e);
                let id = self.machines[t].id;
                self.op_mut(id).status = OpStatus::Enabled;
                return EnqueueStatus::HandlerRunning;
            }
            {
                self.machines[t].inbox.waiting = None;
                let name = self.machines[t].name.clone();
                let state = self.log_state_name(t);
                let ev = if self.logging || self.json_enabled() { self.event_with_payload(&e) } else { String::new() };
                self.log(|_| {
                    format!("<ReceiveLog> '{}' dequeued event '{}' and unblocked in state '{}'.", name, ev, state)
                });
                if self.json_enabled() {
                    let (payload, key) = self.jpayload(&e);
                    let d = Details {
                        log: Some(format!("'{}' dequeued event '{}' and unblocked in state '{}'.", name, ev, state)),
                        id: Some(name.to_string()),
                        state: Some(self.short_state(self.machines[t].state)),
                        event: Some(self.event_name(e.id).to_string()),
                        payload,
                        was_blocked: Some(true),
                        payload_key: key,
                        ..Default::default()
                    };
                    self.jlog("ReceiveEvent", d, true);
                }
                if self.xml.is_some() {
                    let ev = self.xml_event(e.id);
                    self.xlog(
                        "Receive",
                        &[("id", &name), ("state", &state), ("event", &ev), ("wasBlocked", "True")],
                        None,
                    );
                }
                self.cov_receive(t, e.id);
                self.machines[t].received = Some(e);
                let id = self.machines[t].id;
                self.op_mut(id).status = OpStatus::Enabled;
                return EnqueueStatus::HandlerRunning;
            }
        }
        if self.xml.is_some() {
            let (id, ev) = (self.machines[t].name.to_string(), self.xml_event(e.id));
            self.xlog("EnqueueEvent", &[("id", &id), ("event", &ev)], None);
        }
        self.machines[t].inbox.queue.push_back(e);
        if !self.machines[t].inbox.handler_running {
            if self.try_dequeue(t, true).is_none() {
                return EnqueueStatus::NextEventUnavailable;
            }
            self.machines[t].inbox.handler_running = true;
            return EnqueueStatus::HandlerNotRunning;
        }
        EnqueueStatus::HandlerRunning
    }

    fn handler_action(&self, m: usize, e: EventId) -> Option<&HandlerAction> {
        let state = self.machines[m].state?;
        self.prog.state(state).handler_for(e).map(|h| &h.action)
    }

    fn is_ignored(&self, m: usize, e: EventId) -> bool {
        matches!(self.handler_action(m, e), Some(HandlerAction::Ignore))
    }

    fn is_deferred(&self, m: usize, e: EventId) -> bool {
        matches!(self.handler_action(m, e), Some(HandlerAction::Defer))
    }

    fn has_default_handler(&self, m: usize) -> bool {
        self.handler_action(m, self.prog.null_event).is_some()
    }

    /// `EventQueue.TryDequeueEvent`.
    fn try_dequeue(&mut self, m: usize, check_only: bool) -> Option<Event> {
        let mut i = 0;
        while i < self.machines[m].inbox.queue.len() {
            let id = self.machines[m].inbox.queue[i].id;
            if self.is_ignored(m, id) {
                if check_only {
                    i += 1;
                } else {
                    self.machines[m].inbox.queue.remove(i);
                }
                continue;
            }
            if !self.is_deferred(m, id) {
                if check_only {
                    return self.machines[m].inbox.queue.get(i).cloned();
                }
                return self.machines[m].inbox.queue.remove(i);
            }
            i += 1;
        }
        None
    }

    // ---- announce -----------------------------------------------------------------

    /// `AnnounceInternal`: delivers an event to the monitors observing the sender's interface.
    pub fn announce_internal(&mut self, sender: usize, e: &Event) -> Result<(), Stop> {
        let iface = self.machines[sender].iface;
        let mut targets = Vec::new();
        for (mon, ifaces) in &self.monitor_map {
            if ifaces.contains(&iface) {
                if let Some(i) = self.monitors.iter().position(|m| m.decl == *mon) {
                    targets.push(i);
                }
            }
        }
        for i in targets {
            if self.prog.machine(self.monitors[i].decl).observes.contains(&e.id) {
                self.monitor_event(i, e.clone(), Some(sender))?;
            }
        }
        Ok(())
    }

    // ---- machine lifecycle -----------------------------------------------------------

    /// Runs machine `m` until it reaches a scheduling point. Returns the kind of point
    /// (`SchedKind::Stop` when the event handler completed).
    fn run_machine(&mut self, m: usize) -> Result<SchedKind, Stop> {
        // Still at the scheduling point this step started from (`--timed` picks events here).
        let mut at_point = true;
        loop {
            // Resume or run the current action, if any.
            if let Some(exec) = self.machines[m].exec.take() {
                at_point = false;
                match self.run_exec(Actor::Machine(m), exec)? {
                    ExecOutcome::Finished(_) | ExecOutcome::Unwound => {}
                    ExecOutcome::Yield(exec, kind) => {
                        self.machines[m].exec = Some(exec);
                        return Ok(kind);
                    }
                }
            }
            let stage = std::mem::replace(&mut self.machines[m].stage, Stage::Idle);
            if !matches!(stage, Stage::Idle | Stage::Loop | Stage::DequeueTry { .. }) {
                at_point = false;
            }
            match stage {
                Stage::Start(payload) => {
                    let e = Event { id: self.prog.null_event, payload, stamp: None, vt: None };
                    self.machines[m].stage = Stage::AfterStartEntry;
                    self.enter_state(m, e);
                }
                Stage::AfterStartEntry => {
                    self.apply_transition(m);
                    if self.machines[m].status == MachineStatus::Halting {
                        self.machines[m].stage = Stage::Halt;
                    } else {
                        self.machines[m].stage = Stage::Loop;
                    }
                }
                Stage::Loop => {
                    if self.machines[m].status == MachineStatus::Halting {
                        self.machines[m].stage = Stage::Halt;
                        continue;
                    }
                    if let Some(raised) = self.machines[m].inbox.raised.take() {
                        if raised.id == GOTO_EVENT || !self.is_ignored(m, raised.id) {
                            self.cov_handle_raised(m, raised.id);
                            self.machines[m].stage = Stage::Handle(raised);
                            continue;
                        }
                    }
                    let has_default = self.has_default_handler(m);
                    self.machines[m].stage = Stage::DequeueTry { has_default };
                    if self.timed() {
                        // pick the event at a scheduling point, once earlier senders have run
                        if !at_point && (has_default || !self.timed_candidates(m, None).is_empty()) {
                            return Ok(if has_default { SchedKind::Default } else { SchedKind::Receive });
                        }
                    } else if has_default {
                        return Ok(SchedKind::Default);
                    }
                }
                Stage::DequeueTry { has_default } => {
                    let taken =
                        if self.timed() { self.timed_take(m, has_default)? } else { self.try_dequeue(m, false) };
                    if let Some(e) = taken.as_ref().filter(|_| self.timed()) {
                        self.machines[m].stage = Stage::Dequeued(e.clone());
                        continue;
                    }
                    if let Some(e) = taken {
                        let id = self.machines[m].id;
                        let op = self.op_mut(id);
                        if op.skip_next_receive {
                            op.skip_next_receive = false;
                            self.machines[m].stage = Stage::Dequeued(e);
                        } else {
                            self.machines[m].stage = Stage::Dequeued(e);
                            return Ok(SchedKind::Receive);
                        }
                    } else if has_default {
                        let name = self.machines[m].name.clone();
                        let state = self.log_state_name(m);
                        let e = Event { id: self.prog.null_event, payload: Value::Null, stamp: None, vt: None };
                        self.machines[m].stage = Stage::Handle(e);
                        self.cov_default(m);
                        self.log(|_| {
                            format!("<DefaultLog> {} is executing the default handler in state '{}'.", name, state)
                        });
                        if self.json_enabled() {
                            let d = Details {
                                log: Some(format!("{} is executing the default handler in state '{}'.", name, state)),
                                id: Some(name.to_string()),
                                state: Some(self.short_state(self.machines[m].state)),
                                ..Default::default()
                            };
                            self.jlog("DefaultEventHandler", d, true);
                        }
                        self.xlog("DefaultEvent", &[("id", &name), ("state", &state)], None);
                        return Ok(SchedKind::Receive);
                    } else {
                        self.machines[m].inbox.handler_running = false;
                        self.machines[m].stage = Stage::Idle;
                        return Ok(SchedKind::Stop);
                    }
                }
                Stage::Dequeued(e) => {
                    self.cov_dequeue(m, e.id);
                    let name = self.machines[m].name.clone();
                    let state = self.log_state_name(m);
                    let ev =
                        if self.logging || self.json_enabled() { self.event_with_payload(&e) } else { String::new() };
                    self.log(|_| format!("<DequeueLog> '{}' dequeued event '{}' in state '{}'.", name, ev, state));
                    if self.json_enabled() {
                        let (payload, key) = self.jpayload(&e);
                        let d = Details {
                            log: Some(format!("'{}' dequeued event '{}' in state '{}'.", name, ev, state)),
                            id: Some(name.to_string()),
                            event: Some(self.event_name(e.id).to_string()),
                            state: Some(self.short_state(self.machines[m].state)),
                            payload,
                            payload_key: key,
                            ..Default::default()
                        };
                        self.jlog("DequeueEvent", d, true);
                    }
                    if self.xml.is_some() {
                        let ev = self.xml_event(e.id);
                        self.xlog("DequeueEvent", &[("id", &name), ("state", &state), ("event", &ev)], None);
                    }
                    let mut label = event_cs_name(self.prog, e.id).to_string();
                    if self.timeline.config.payload && e.payload != Value::Null {
                        label = format!(
                            "{}#{}",
                            label,
                            crate::checker::feedback::stable_hash(&e.payload.to_display(self.prog))
                        );
                    }
                    let time = if self.timeline.config.repr.causal() {
                        if let Some(vt) = &e.vt {
                            self.machines[m].vt.merge(vt);
                        }
                        Some(Rc::new(self.machines[m].vt.clone()))
                    } else {
                        None
                    };
                    self.timeline.record(&name, &label, time);
                    self.time_receive(m, &e);
                    self.machines[m].stage = Stage::Handle(e);
                }
                Stage::Handle(e) => {
                    if self.machines[m].status != MachineStatus::Active {
                        self.machines[m].stage = Stage::Halt;
                        continue;
                    }
                    if e.id == GOTO_EVENT {
                        let target = self.machines[m].goto_target.take().expect("goto target");
                        self.begin_goto(m, target, None, e);
                        continue;
                    }
                    let Some(state) = self.machines[m].state else {
                        // UnhandledEventException, logged by OnUnhandledEventExceptionHandler
                        let name = self.machines[m].name.clone();
                        let popped = self.machines[m].popped_from.take();
                        let from = self.state_name(popped);
                        self.log(|_| format!(
                            "<ExceptionLog> {} running action '' in state '{}' threw exception 'UnhandledEventException'.",
                            name, from
                        ));
                        if self.json_enabled() {
                            let d = Details {
                                log: Some(format!(
                                    "{} running action '' in state '{}' threw exception 'UnhandledEventException'.",
                                    name, from
                                )),
                                id: Some(name.to_string()),
                                state: Some(from.clone()),
                                action: Some(String::new()),
                                exception: Some("UnhandledEventException".into()),
                                ..Default::default()
                            };
                            self.jlog("ExceptionThrown", d, true);
                        }
                        const UNHANDLED: &str = "PChecker.Runtime.StateMachines.Exceptions.UnhandledEventException";
                        let unhandled_text = format!("{}: Unhandled Event", UNHANDLED);
                        self.xlog(
                            "ExceptionThrown",
                            &[("id", &name), ("state", &from), ("action", ""), ("type", UNHANDLED)],
                            Some(&unhandled_text),
                        );
                        if e.id == self.prog.halt_event {
                            self.log(|_| format!(
                                "<ExceptionLog> {} running action '' in state '{}' chose to handle exception 'UnhandledEventException'.",
                                name, from
                            ));
                            if self.json_enabled() {
                                let d = Details {
                                    log: Some(format!(
                                        "{} running action '' in state '{}' chose to handle exception 'UnhandledEventException'.",
                                        name, from
                                    )),
                                    id: Some(name.to_string()),
                                    state: Some(from.clone()),
                                    action: Some(String::new()),
                                    exception: Some("UnhandledEventException".into()),
                                    ..Default::default()
                                };
                                self.jlog("ExceptionHandled", d, true);
                            }
                            let mut attrs =
                                vec![("id", &*name), ("state", from.as_str()), ("action", ""), ("type", UNHANDLED)];
                            if from.is_empty() {
                                attrs.remove(1);
                            }
                            self.xlog("ExceptionHandled", &attrs, Some(&unhandled_text));
                            self.machines[m].status = MachineStatus::Halting;
                            self.machines[m].stage = Stage::Halt;
                            continue;
                        }
                        let msg = format!(
                            "{} received event '{}' that cannot be handled.",
                            self.machines[m].name,
                            event_cs_full_name(self.prog, e.id)
                        );
                        return Err(self.report_bug(msg));
                    };
                    let action = self.prog.state(state).handler_for(e.id).map(|h| h.action.clone());
                    if self.strategy.wants_frontiers()
                        && matches!(action, Some(HandlerAction::Do(_) | HandlerAction::Goto { .. }))
                    {
                        self.strategy.observe_frontier(self.machines[m].decl.0, state.0, e.id.0);
                    }
                    match action {
                        Some(HandlerAction::Do(f)) => {
                            self.machines[m].stage = Stage::AfterAction(e.clone());
                            self.start_action(m, f, &e);
                        }
                        Some(HandlerAction::Goto { target, with }) => {
                            self.begin_goto(m, target, with, e);
                        }
                        Some(HandlerAction::Ignore) | Some(HandlerAction::Defer) => {
                            self.machines[m].stage = Stage::Loop;
                        }
                        None => {
                            // Unhandled: run the exit action, then pop the state.
                            self.machines[m].stage = Stage::AfterExit { e: e.clone(), then: ExitThen::Pop };
                            self.exit_state(m, e);
                        }
                    }
                }
                Stage::AfterAction(_e) => {
                    self.apply_transition(m);
                    self.machines[m].stage = Stage::Loop;
                }
                Stage::AfterExit { e, then } => {
                    let t = std::mem::replace(&mut self.machines[m].pending_transition, Transition::None);
                    if t != Transition::None {
                        let msg = format!(
                            "{} has performed a '{}' transition from an OnExit action.",
                            self.machines[m].name,
                            t.kind_name()
                        );
                        return Err(self.report_bug(msg));
                    }
                    match then {
                        ExitThen::Goto { target, with } => {
                            if let (Some(w), MachineStatus::Active) = (with, self.machines[m].status) {
                                self.machines[m].stage = Stage::AfterWith { e: e.clone(), target };
                                self.start_action(m, w, &e);
                            } else {
                                self.machines[m].stage = Stage::AfterWith { e, target };
                            }
                        }
                        ExitThen::Pop => {
                            if self.machines[m].status == MachineStatus::Active {
                                let name = self.machines[m].name.clone();
                                let state = self.log_state_name(m);
                                self.log(|rt| {
                                    format!(
                                        "<PopLog> {} popped state {} due to unhandled event '{}'.",
                                        name,
                                        state,
                                        rt.event_name(e.id)
                                    )
                                });
                                if self.json_enabled() {
                                    let d = Details {
                                        log: Some(format!(
                                            "{} popped state {} due to unhandled event '{}'.",
                                            name,
                                            state,
                                            self.event_name(e.id)
                                        )),
                                        id: Some(name.to_string()),
                                        state: Some(state.clone()),
                                        event: Some(self.event_name(e.id).to_string()),
                                        ..Default::default()
                                    };
                                    self.jlog("PopStateUnhandledEvent", d, true);
                                }
                                if self.xml.is_some() {
                                    let ev = self.xml_event(e.id);
                                    self.xlog(
                                        "PopUnhandled",
                                        &[("id", &name), ("state", &state), ("event", &ev)],
                                        None,
                                    );
                                }
                                self.machines[m].popped_from = self.machines[m].state;
                                self.machines[m].state = None;
                                self.machines[m].stage = Stage::Handle(e);
                            } else {
                                self.machines[m].stage = Stage::Halt;
                            }
                        }
                    }
                }
                Stage::AfterWith { e, target } => {
                    let t = std::mem::replace(&mut self.machines[m].pending_transition, Transition::None);
                    if t != Transition::None {
                        let msg = format!(
                            "{} has performed a '{}' transition from an OnExit action.",
                            self.machines[m].name,
                            t.kind_name()
                        );
                        return Err(self.report_bug(msg));
                    }
                    if self.machines[m].status == MachineStatus::Active {
                        self.machines[m].state = Some(target);
                        self.machines[m].stage = Stage::AfterEntry(e.clone());
                        self.enter_state(m, e);
                    } else {
                        self.machines[m].stage = Stage::Halt;
                    }
                }
                Stage::AfterEntry(_e) => {
                    self.apply_transition(m);
                    self.machines[m].stage = Stage::Loop;
                }
                Stage::Halt => {
                    self.machines[m].status = MachineStatus::Halted;
                    self.machines[m].inbox.closed = true;
                    let n = self.machines[m].inbox.queue.len();
                    let name = self.machines[m].name.clone();
                    self.log(|_| format!("<HaltLog> {} halted with {} events in its inbox.", name, n));
                    if self.json_enabled() {
                        let d = Details {
                            log: Some(format!("{} halted with {} events in its inbox.", name, n)),
                            id: Some(name.to_string()),
                            halt_inbox_size: Some(n as i64),
                            ..Default::default()
                        };
                        self.jlog("Halt", d, true);
                    }
                    self.xlog("Halt", &[("id", &name), ("inboxSize", &n.to_string())], None);
                    self.machines[m].inbox.queue.clear();
                    self.machines[m].stage = Stage::Idle;
                    return Ok(SchedKind::Stop);
                }
                Stage::Idle => {
                    // Restarted by an enqueue: continue the event handler loop.
                    self.machines[m].stage = Stage::Loop;
                }
            }
        }
    }

    /// The payload passed to a handler with a parameter: a pending goto payload wins
    /// over the event's (`gotoPayload ?? e.Payload`, consumed only by parameterized
    /// actions, as in the generated code).
    fn take_payload(&mut self, m: usize, e: &Event) -> Value {
        match self.machines[m].goto_payload.take() {
            Some(p) if !p.is_null() => p,
            _ => e.payload.clone(),
        }
    }

    fn start_action(&mut self, m: usize, f: FunId, e: &Event) {
        self.cov_execute_action(m);
        if self.xml.is_some() {
            let (id, state) = (self.machines[m].name.to_string(), self.machine_state_name(m));
            let action = self.fun_name(f).to_string();
            let mut attrs = vec![("id", id.as_str()), ("state", state.as_str()), ("action", action.as_str())];
            if state.is_empty() {
                attrs.remove(1);
            }
            self.xlog("Action", &attrs, None);
        }
        let arg = if self.prog.fun(f).params.is_empty() { Value::Null } else { self.take_payload(m, e) };
        let exec = self.new_exec(f, vec![arg]);
        self.machines[m].exec = Some(exec);
    }

    pub fn new_exec(&self, f: FunId, args: Vec<Value>) -> Exec {
        let mut exec = Exec { action: self.fun_name(f).to_string(), ..Exec::default() };
        if let Some(code) = self.code.get(f) {
            // room for the locals and a typical operand stack and call depth
            exec.values = Vec::with_capacity(code.frame_size as usize + 32);
            let nargs = args.len();
            exec.values.extend(args);
            exec.push_frame(code.clone(), f, nargs);
        }
        exec
    }

    fn enter_state(&mut self, m: usize, e: Event) {
        self.cov_machine_enter(m);
        let name = self.machines[m].name.clone();
        let state = self.log_state_name(m);
        self.log(|_| format!("<StateLog> {} enters state '{}'.", name, state));
        self.json_state_transition(m, true);
        if let Some(s) = self.machines[m].state {
            if let Some(entry) = self.prog.state(s).entry {
                self.start_action(m, entry, &e);
            }
        }
    }

    fn json_state_transition(&mut self, m: usize, is_entry: bool) {
        if self.xml.is_some() {
            let (id, state) = (self.machines[m].name.to_string(), self.machine_state_name(m));
            self.xlog(
                "State",
                &[("id", &id), ("state", &state), ("isEntry", if is_entry { "True" } else { "False" })],
                None,
            );
        }
        if !self.json_enabled() {
            return;
        }
        let name = self.machines[m].name.to_string();
        let st = self.short_state(self.machines[m].state);
        let d = Details {
            log: Some(format!("{} {} state '{}'.", name, if is_entry { "enters" } else { "exits" }, st)),
            id: Some(name),
            state: Some(st),
            is_entry: Some(is_entry),
            ..Default::default()
        };
        self.jlog("StateTransition", d, true);
    }

    fn exit_state(&mut self, m: usize, e: Event) {
        let name = self.machines[m].name.clone();
        let state = self.log_state_name(m);
        self.log(|_| format!("<StateLog> {} exits state '{}'.", name, state));
        self.json_state_transition(m, false);
        if let Some(s) = self.machines[m].state {
            if let Some(exit) = self.prog.state(s).exit {
                self.start_action(m, exit, &e);
            }
        }
    }

    fn begin_goto(&mut self, m: usize, target: StateId, with: Option<FunId>, e: Event) {
        self.cov_goto(m, target);
        let name = self.machines[m].name.clone();
        let cur = self.log_state_name(m);
        let mname = self.prog.machine(self.machines[m].decl).cs_name.clone();
        let tname = self.prog.state(target).name.clone();
        self.log(|_| {
            format!(
                "<GotoLog> {} is transitioning from state '{}' to state 'PImplementation.{}.{}'.",
                name, cur, mname, tname
            )
        });
        if self.json_enabled() {
            let short_cur = self.short_state(self.machines[m].state);
            let d = Details {
                log: Some(format!("{} is transitioning from state '{}' to state '{}'.", name, short_cur, tname)),
                id: Some(name.to_string()),
                start_state: Some(short_cur),
                end_state: Some(tname.clone()),
                ..Default::default()
            };
            self.jlog("GotoState", d, true);
        }
        if self.xml.is_some() {
            let new_state = format!("PImplementation.{}.{}", mname, tname);
            self.xlog("Goto", &[("id", &name), ("currState", &cur), ("newState", &new_state)], None);
        }
        self.machines[m].stage = Stage::AfterExit { e: e.clone(), then: ExitThen::Goto { target, with } };
        self.exit_state(m, e)
    }

    /// `ApplyEventHandlerTransitionAsync` for the pending transition.
    fn apply_transition(&mut self, m: usize) {
        let t = std::mem::replace(&mut self.machines[m].pending_transition, Transition::None);
        match t {
            Transition::None => {}
            Transition::Raise(e) => {
                self.cov_raise(m, e.id);
                let name = self.machines[m].name.clone();
                let state = self.log_state_name(m);
                let ev = if self.logging || self.json_enabled() { self.event_with_payload(&e) } else { String::new() };
                self.log(|_| format!("<RaiseLog> '{}' raised event '{}' in state '{}'.", name, ev, state));
                if self.json_enabled() {
                    let (payload, key) = self.jpayload(&e);
                    let st = self.short_state(self.machines[m].state);
                    let d = Details {
                        log: Some(format!("'{}' raised event '{}' in state '{}'.", name, ev, st)),
                        id: Some(name.to_string()),
                        event: Some(self.event_name(e.id).to_string()),
                        state: Some(st),
                        payload,
                        payload_key: key,
                        ..Default::default()
                    };
                    self.jlog("RaiseEvent", d, true);
                }
                if self.xml.is_some() {
                    let ev = self.xml_event(e.id);
                    self.xlog("Raise", &[("id", &name), ("state", &state), ("event", &ev)], None);
                }
                self.machines[m].inbox.raised = Some(e);
            }
            Transition::Goto(target) => {
                self.cov_raise(m, GOTO_EVENT);
                if self.xml.is_some() {
                    let (id, state, ev) =
                        (self.machines[m].name.to_string(), self.machine_state_name(m), self.xml_event(GOTO_EVENT));
                    self.xlog("Raise", &[("id", &id), ("state", &state), ("event", &ev)], None);
                }
                // Modeled as a raised goto event, handled on the next dequeue.
                self.machines[m].inbox.raised =
                    Some(Event { id: GOTO_EVENT, payload: Value::Null, stamp: None, vt: None });
                self.machines[m].goto_target = Some(target);
            }
        }
    }

    // ---- monitors ----------------------------------------------------------------------

    fn create_monitors(&mut self) -> Result<(), Stop> {
        let decls: Vec<MachineId> = self.monitor_map.iter().map(|(m, _)| *m).collect();
        for decl in decls {
            if self.monitors.iter().any(|m| m.decl == decl) {
                continue;
            }
            self.cov_declare(decl, true);
            if self.prog.machine(decl).is_scenario() {
                self.scenarios.push(ScenarioProgress {
                    decl,
                    name: self.prog.machine(decl).cs_name.clone(),
                    total_states: self.prog.machine(decl).states.len(),
                    reached: std::collections::BTreeSet::new(),
                    saw_start_entry: false,
                    satisfied: false,
                });
            }
            let fields = self
                .prog
                .machine(decl)
                .fields
                .iter()
                .map(|v| default_value(&self.prog.var(*v).ty, self.prog))
                .collect();
            let idx = self.monitors.len();
            self.monitors.push(Monitor {
                decl,
                fields,
                state: self.prog.machine(decl).start_state,
                temperature: 0,
                pending_transition: Transition::None,
                goto_payload: None,
            });
            self.log(|rt| format!("<CreateLog> {} was created.", rt.monitor_full_name(idx)));
            if self.json_enabled() {
                let short = self.prog.machine(decl).cs_name.clone();
                let d = Details {
                    log: Some(format!("{} was created.", short)),
                    monitor: Some(short),
                    ..Default::default()
                };
                self.jlog("CreateMonitor", d, true);
            }
            if self.xml.is_some() {
                let ty = self.monitor_full_name(idx);
                self.xlog("CreateMonitor", &[("type", &ty)], None);
            }
            let e = Event { id: self.prog.null_event, payload: Value::Null, stamp: None, vt: None };
            self.monitor_enter_state(idx, &e)?;
        }
        Ok(())
    }

    /// Creates a monitor of type `decl` outside a test (as PObserve does) and runs its
    /// start state's entry action. Returns its index in `Runtime::monitors`.
    pub fn start_monitor(&mut self, decl: MachineId) -> Result<usize, Stop> {
        let fields =
            self.prog.machine(decl).fields.iter().map(|v| default_value(&self.prog.var(*v).ty, self.prog)).collect();
        let idx = self.monitors.len();
        self.monitors.push(Monitor {
            decl,
            fields,
            state: self.prog.machine(decl).start_state,
            temperature: 0,
            pending_transition: Transition::None,
            goto_payload: None,
        });
        let e = Event { id: self.prog.null_event, payload: Value::Null, stamp: None, vt: None };
        self.monitor_enter_state(idx, &e)?;
        Ok(idx)
    }

    fn monitor_state_log(&mut self, mon: usize, enters: bool) {
        if enters {
            self.cov_monitor_enter(mon);
        }
        let s = self.monitors[mon].state;
        let liveness = match s.map(|s| self.prog.state(s).temperature) {
            Some(Temperature::Hot) => "hot ",
            Some(Temperature::Cold) => "cold ",
            _ => "",
        };
        self.log(|rt| {
            format!(
                "<MonitorLog> {} {} {}state '{}'.",
                rt.monitor_full_name(mon),
                if enters { "enters" } else { "exits" },
                liveness,
                rt.state_name(s)
            )
        });
        let decl = self.monitors[mon].decl;
        if enters {
            if let (Some(st), Some(sc)) = (s, self.scenarios.iter_mut().find(|p| p.decl == decl)) {
                // ScenarioComplianceObserver.OnMonitorStateTransition
                sc.reached.insert(st);
                let is_start_entry = !sc.saw_start_entry;
                sc.saw_start_entry = true;
                if self.prog.state(st).temperature == Temperature::Cold && !is_start_entry {
                    sc.satisfied = true;
                }
            }
        }
        if self.json_enabled() {
            let short = self.prog.machine(self.monitors[mon].decl).cs_name.clone();
            let st = self.short_state(s);
            let hot = match s.map(|s| self.prog.state(s).temperature) {
                Some(Temperature::Hot) => Some(true),
                Some(Temperature::Cold) => Some(false),
                _ => None,
            };
            let d = Details {
                log: Some(format!("{} {} {}state '{}'.", short, if enters { "enters" } else { "exits" }, liveness, st)),
                state: Some(st),
                is_entry: Some(enters),
                is_in_hot_state: hot,
                monitor: Some(short),
                ..Default::default()
            };
            self.jlog("MonitorStateTransition", d, true);
        }
        if self.xml.is_some() {
            let (ty, st) = (self.monitor_full_name(mon), self.state_name(s));
            let hot = s.is_some_and(|s| self.prog.state(s).temperature == Temperature::Hot);
            self.xlog(
                "MonitorState",
                &[
                    ("monitorType", &ty),
                    ("state", &st),
                    ("isEntry", if enters { "True" } else { "False" }),
                    ("isInHotState", if hot { "True" } else { "False" }),
                ],
                None,
            );
        }
    }

    fn monitor_run_action(&mut self, mon: usize, f: FunId, e: &Event) -> Result<(), Stop> {
        let arg = if self.prog.fun(f).params.is_empty() {
            Value::Null
        } else {
            match self.monitors[mon].goto_payload.take() {
                Some(p) if !p.is_null() => p,
                _ => e.payload.clone(),
            }
        };
        let exec = self.new_exec(f, vec![arg]);
        match self.run_exec(Actor::Monitor(mon), exec)? {
            ExecOutcome::Finished(_) | ExecOutcome::Unwound => Ok(()),
            // creating machines, sending and receiving throw in a monitor
            ExecOutcome::Yield(..) => unreachable!("monitors never reach a scheduling point"),
        }
    }

    fn monitor_enter_state(&mut self, mon: usize, e: &Event) -> Result<(), Stop> {
        self.monitor_state_log(mon, true);
        if let Some(s) = self.monitors[mon].state {
            if let Some(entry) = self.prog.state(s).entry {
                self.monitor_run_action(mon, entry, e)?;
                self.monitor_apply_transition(mon)?;
            }
        }
        Ok(())
    }

    fn monitor_exit_state(&mut self, mon: usize, with: Option<FunId>, e: &Event) -> Result<(), Stop> {
        self.monitor_state_log(mon, false);
        if let Some(s) = self.monitors[mon].state {
            if let Some(exit) = self.prog.state(s).exit {
                self.monitor_run_action(mon, exit, e)?;
                self.monitor_check_exit_transition(mon)?;
            }
        }
        if let Some(w) = with {
            self.monitor_run_action(mon, w, e)?;
            self.monitor_check_exit_transition(mon)?;
        }
        Ok(())
    }

    fn monitor_check_exit_transition(&mut self, mon: usize) -> Result<(), Stop> {
        let t = std::mem::replace(&mut self.monitors[mon].pending_transition, Transition::None);
        if !matches!(t, Transition::None) {
            let msg = format!(
                "{} has performed a '{}' transition from an OnExit action.",
                self.monitor_full_name(mon),
                t.monitor_kind_name()
            );
            return Err(self.report_bug(msg));
        }
        Ok(())
    }

    fn monitor_apply_transition(&mut self, mon: usize) -> Result<(), Stop> {
        let t = std::mem::replace(&mut self.monitors[mon].pending_transition, Transition::None);
        match t {
            Transition::None => Ok(()),
            Transition::Raise(e) => {
                self.cov_monitor_raise(mon, e.id);
                self.log(|rt| {
                    format!(
                        "<MonitorLog> Monitor '{}' raised event '{}' in state '{}'.",
                        rt.prog.machine(rt.monitors[mon].decl).cs_name,
                        rt.event_with_payload(&e),
                        rt.state_name(rt.monitors[mon].state)
                    )
                });
                if self.json_enabled() {
                    let (payload, key) = self.jpayload(&e);
                    let short = self.prog.machine(self.monitors[mon].decl).cs_name.clone();
                    let st = self.short_state(self.monitors[mon].state);
                    let d = Details {
                        log: Some(format!(
                            "Monitor '{}' raised event '{}' in state '{}'.",
                            short,
                            self.event_with_payload(&e),
                            st
                        )),
                        event: Some(self.event_name(e.id).to_string()),
                        state: Some(st),
                        payload,
                        monitor: Some(short),
                        payload_key: key,
                        ..Default::default()
                    };
                    self.jlog("MonitorRaiseEvent", d, true);
                }
                self.xml_monitor_raise(mon, e.id);
                self.monitor_handle_event(mon, e)
            }
            Transition::Goto(target) => {
                self.cov_monitor_raise(mon, GOTO_EVENT);
                self.log(|rt| {
                    format!(
                        "<MonitorLog> Monitor '{}' raised event 'GotoStateEvent' in state '{}'.",
                        rt.prog.machine(rt.monitors[mon].decl).cs_name,
                        rt.state_name(rt.monitors[mon].state)
                    )
                });
                if self.json_enabled() {
                    let short = self.prog.machine(self.monitors[mon].decl).cs_name.clone();
                    let st = self.short_state(self.monitors[mon].state);
                    let d = Details {
                        log: Some(format!("Monitor '{}' raised event 'GotoStateEvent' in state '{}'.", short, st)),
                        event: Some("GotoStateEvent".into()),
                        state: Some(st),
                        monitor: Some(short),
                        ..Default::default()
                    };
                    self.jlog("MonitorRaiseEvent", d, true);
                }
                self.xml_monitor_raise(mon, GOTO_EVENT);
                let e = Event { id: self.prog.null_event, payload: Value::Null, stamp: None, vt: None };
                self.monitor_goto(mon, target, None, &e)
            }
        }
    }

    fn xml_monitor_raise(&mut self, mon: usize, e: EventId) {
        if self.xml.is_some() {
            let (ty, st, ev) =
                (self.monitor_full_name(mon), self.state_name(self.monitors[mon].state), self.xml_event(e));
            self.xlog("MonitorRaise", &[("monitorType", &ty), ("state", &st), ("event", &ev)], None);
        }
    }

    fn monitor_goto(&mut self, mon: usize, target: StateId, with: Option<FunId>, e: &Event) -> Result<(), Stop> {
        if self.java_semantics() {
            // the generated handler is `{ with(p); gotoState(target); }`: the transition
            // function runs before the exit action
            if let Some(w) = with {
                self.monitor_run_action(mon, w, e)?;
                self.monitor_check_exit_transition(mon)?;
            }
            self.monitor_exit_state(mon, None, e)?;
        } else {
            self.monitor_exit_state(mon, with, e)?;
        }
        self.monitors[mon].state = Some(target);
        if self.prog.state(target).temperature == Temperature::Cold {
            self.monitors[mon].temperature = 0;
        }
        self.monitor_enter_state(mon, e)
    }

    /// `Monitor.MonitorEvent`. With no sender, the monitor reads the event stamp
    /// as its clock (used for recorded observations).
    pub fn monitor_event(&mut self, mon: usize, e: Event, sender_idx: Option<usize>) -> Result<(), Stop> {
        let sender: Option<Rc<str>> = sender_idx.map(|i| self.machines[i].name.clone());
        self.cov_monitor_process(mon, e.id, sender_idx);
        self.log(|rt| {
            format!(
                "<MonitorLog> {} is processing event '{}' in state '{}'.",
                rt.prog.machine(rt.monitors[mon].decl).cs_name,
                rt.event_with_payload(&e),
                rt.state_name(rt.monitors[mon].state)
            )
        });
        if self.json_enabled() {
            let (payload, key) = self.jpayload(&e);
            let short = self.prog.machine(self.monitors[mon].decl).cs_name.clone();
            let d = Details {
                log: Some(format!(
                    "{} is processing event '{}' in state '{}'.",
                    short,
                    self.event_with_payload(&e),
                    self.state_name(self.monitors[mon].state)
                )),
                state: Some(self.state_name(self.monitors[mon].state)),
                event: Some(self.event_name(e.id).to_string()),
                payload,
                sender: sender.map(|s| s.to_string()),
                monitor: Some(short),
                payload_key: key,
                ..Default::default()
            };
            self.jlog("MonitorProcessEvent", d, true);
        }
        if self.xml.is_some() {
            let (ty, st) = (self.monitor_full_name(mon), self.state_name(self.monitors[mon].state));
            let (sname, stype, sstate) = match sender_idx {
                Some(i) => (self.machines[i].name.to_string(), self.machine_type(i), self.machine_state_name(i)),
                None => (self.xml.as_ref().unwrap().task_id.to_string(), "task".to_string(), String::new()),
            };
            let ev = event_cs_name(self.prog, e.id).to_string();
            self.xlog(
                "MonitorEvent",
                &[
                    ("monitorType", &ty),
                    ("state", &st),
                    ("senderName", &sname),
                    ("senderType", &stype),
                    ("senderState", &sstate),
                    ("event", &ev),
                ],
                None,
            );
        }
        let prev = self.time.as_mut().map(|t| {
            let clock =
                sender_idx.map(|i| self.machines[i].clock.clone()).or_else(|| e.stamp.as_ref().map(|s| s.at.clone()));
            std::mem::replace(&mut t.observer_clock, clock)
        });
        let r = self.monitor_handle_event(mon, e);
        if let (Some(t), Some(p)) = (self.time.as_mut(), prev) {
            t.observer_clock = p;
        }
        r
    }

    // ---- time ------------------------------------------------------------------------

    /// Stamp of an event `sender` sends to machine `target` now.
    fn send_stamp(&mut self, sender: usize, target: usize, event: EventId) -> crate::checker::time::Stamp {
        let clock = self.machines[sender].clock.clone();
        let sender_id = self.machines[sender].id;
        let ts = self.time.as_mut().expect("time tracking");
        let range = ts.opts.profile.as_ref().and_then(|p| {
            p.latency(
                &self.prog.machine(self.machines[sender].decl).name,
                &self.prog.machine(self.machines[target].decl).name,
                &self.prog.event(event).name,
                sender == target,
            )
        });
        let at = match range {
            Some(r) => ts.advance(&clock, r),
            None => clock,
        };
        crate::checker::time::Stamp { at, sender: sender_id }
    }

    /// Machine `m` takes event `e`: its clock catches up with the event's stamp.
    pub fn time_receive(&mut self, m: usize, e: &Event) {
        let (Some(ts), Some(stamp)) = (self.time.as_mut(), e.stamp.as_ref()) else { return };
        let clock = ts.join(&self.machines[m].clock, &stamp.at);
        self.machines[m].clock = clock;
        self.log(|rt| format!("<TimeLog> '{}' clock is {} us.", rt.machines[m].name, rt.machines[m].clock));
    }

    /// The window `[best, worst]` in which operation `op` would act if scheduled now
    /// (`--timed`).
    fn ready_time(&self, op: usize) -> (i64, i64) {
        let Some(m) = self.ops[op].machine else { return (0, 0) };
        let mach = &self.machines[m];
        let (cb, cw) = (mach.clock.best(), mach.clock.worst());
        let wait = match &mach.stage {
            Stage::Idle | Stage::Loop | Stage::DequeueTry { .. }
                if mach.inbox.raised.is_none() && mach.exec.is_none() =>
            {
                // a default handler runs at the clock when nothing has certainly arrived
                if self.has_default_handler(m) {
                    return (cb, cw);
                }
                None
            }
            Stage::Dequeued(e) => {
                let s = e.stamp.as_ref().map_or((i64::MIN, i64::MIN), |s| (s.at.best(), s.at.worst()));
                return (cb.max(s.0), cw.max(s.1));
            }
            _ => match mach.exec.as_ref().and_then(|x| x.pending.as_ref()) {
                Some(Pending::ReceiveWait { cases, .. }) => Some(receive_wait_set(self.prog, cases)),
                _ => return (cb, cw),
            },
        };
        let c = self.timed_candidates(m, wait.as_deref());
        let q = &self.machines[m].inbox.queue;
        let lo = c.iter().map(|&i| q[i].stamp.as_ref().map_or(i64::MIN, |s| s.at.best())).min();
        let hi = c.iter().map(|&i| q[i].stamp.as_ref().map_or(i64::MIN, |s| s.at.worst())).min();
        match (lo, hi) {
            (Some(lo), Some(hi)) => (cb.max(lo), cw.max(hi)),
            _ => (cb, cw),
        }
    }

    pub fn timed(&self) -> bool {
        self.time.as_ref().is_some_and(|t| t.opts.timed)
    }

    /// `--timed`: the queued events machine `m` may take next, among the events `wait`
    /// lists (a `receive`) or that the current state neither ignores nor defers. The
    /// first such event of each sender is a candidate (per-sender FIFO); a candidate
    /// that certainly arrives after another one is dropped. Queue order.
    pub fn timed_candidates(&self, m: usize, wait: Option<&[EventId]>) -> Vec<usize> {
        let mut senders: Vec<u64> = Vec::new();
        let mut cands: Vec<(i64, i64, usize)> = Vec::new();
        for (i, e) in self.machines[m].inbox.queue.iter().enumerate() {
            let eligible = match wait {
                Some(w) => w.contains(&e.id),
                None => !self.is_ignored(m, e.id) && !self.is_deferred(m, e.id),
            };
            if !eligible {
                continue;
            }
            if let Some(st) = &e.stamp {
                if senders.contains(&st.sender) {
                    continue;
                }
                senders.push(st.sender);
            }
            let (b, w) = e.stamp.as_ref().map_or((i64::MIN, i64::MIN), |st| (st.at.best(), st.at.worst()));
            cands.push((b, w, i));
        }
        let min_worst = cands.iter().map(|c| c.1).min().unwrap_or(i64::MAX);
        cands.into_iter().filter(|c| c.0 <= min_worst).map(|c| c.2).collect()
    }

    /// `--timed`: picks the event machine `m` takes next (`None`: none, or the default
    /// handler runs). When several events, or an event and the default handler, could
    /// come first, the choice is a scheduling decision recorded in the schedule.
    pub fn timed_pick(&mut self, m: usize, wait: Option<&[EventId]>, has_default: bool) -> Result<Option<usize>, Stop> {
        let mut c = self.timed_candidates(m, wait);
        let mut default = false;
        if has_default {
            let (cb, cw) = (self.machines[m].clock.best(), self.machines[m].clock.worst());
            let q = &self.machines[m].inbox.queue;
            let stamp = |i: usize| q[i].stamp.as_ref().map_or((i64::MIN, i64::MIN), |s| (s.at.best(), s.at.worst()));
            // the default handler runs at the clock: it may come first unless an event
            // certainly arrived before; an event certainly after the clock is not there yet
            default = c.iter().all(|&i| stamp(i).1 >= cb);
            c.retain(|&i| stamp(i).0 <= cw);
        }
        let n = c.len() + default as usize;
        if n == 0 {
            return Ok(None);
        }
        let k = if n == 1 {
            0
        } else {
            let caller = format!("'{}' (timed delivery)", self.machines[m].name);
            self.choose_int(n as i32, &caller)? as usize
        };
        Ok(c.get(k).copied())
    }

    /// `--timed` dequeue: removes the picked event, dropping ignored events queued
    /// ahead of it as `try_dequeue` does.
    fn timed_take(&mut self, m: usize, has_default: bool) -> Result<Option<Event>, Stop> {
        let Some(mut i) = self.timed_pick(m, None, has_default)? else { return Ok(None) };
        let mut j = 0;
        while j < i {
            let id = self.machines[m].inbox.queue[j].id;
            if self.is_ignored(m, id) {
                self.machines[m].inbox.queue.remove(j);
                i -= 1;
            } else {
                j += 1;
            }
        }
        Ok(self.machines[m].inbox.queue.remove(i))
    }

    /// `--timed`: disables every enabled operation that certainly acts later than
    /// another enabled one. The earliest one always stays enabled.
    fn filter_timed(&mut self, infos: &mut [OpInfo]) {
        let mut ready: Vec<Option<(i64, i64)>> = Vec::with_capacity(infos.len());
        for (i, info) in infos.iter().enumerate() {
            ready.push(info.enabled.then(|| self.ready_time(i)));
        }
        // The two smallest worst cases, so each operation can be compared with the others.
        let (mut w1, mut w2) = ((i64::MAX, usize::MAX), i64::MAX);
        for (i, r) in ready.iter().enumerate() {
            if let Some((_, w)) = *r {
                if w < w1.0 {
                    w2 = w1.0;
                    w1 = (w, i);
                } else if w < w2 {
                    w2 = w;
                }
            }
        }
        for (i, r) in ready.iter().enumerate() {
            if let Some((b, _)) = *r {
                let other_worst = if i == w1.1 { w2 } else { w1.0 };
                if other_worst < b {
                    infos[i].enabled = false;
                }
            }
        }
    }

    /// The clock an actor reads: its own, or for a monitor the observed machine's.
    pub fn actor_clock(&self, actor: Actor) -> crate::checker::time::Time {
        match actor {
            Actor::Machine(m) => self.machines[m].clock.clone(),
            Actor::Monitor(_) => self.time.as_ref().and_then(|t| t.observer_clock.clone()).unwrap_or_default(),
        }
    }

    /// The natively implemented time functions; `None` if `name` is not one of them.
    pub fn call_time_function(&mut self, actor: Actor, name: &str, args: &[Value]) -> Option<Result<Value, Exception>> {
        self.time.as_ref()?;
        crate::checker::time::time_signature(name)?;
        let int = |i: usize| match args.get(i) {
            Some(Value::Int(v)) => Ok(*v),
            _ => Err(Exception::new("System.ArgumentException", format!("{} expects int arguments", name))),
        };
        let clock = self.actor_clock(actor);
        let elapse = |rt: &mut Self, r: crate::checker::time::Range| -> Result<Value, Exception> {
            let Actor::Machine(m) = actor else {
                return Err(Exception::new(
                    "System.InvalidOperationException",
                    format!("{} cannot be called from a monitor", name),
                ));
            };
            let c = rt.time.as_mut().unwrap().advance(&rt.machines[m].clock, r);
            rt.machines[m].clock = c;
            Ok(Value::Null)
        };
        let mark = |rt: &Self, h: i64| -> Result<crate::checker::time::Time, Exception> {
            let ts = rt.time.as_ref().unwrap();
            usize::try_from(h).ok().and_then(|h| ts.marks.get(h)).cloned().ok_or_else(|| {
                Exception::new(
                    "System.ArgumentException",
                    format!("{}: {} is not a handle returned by Mark()", name, h),
                )
            })
        };
        Some((|| match name {
            "Now" => Ok(Value::Int(clock.worst())),
            "NowBest" => Ok(Value::Int(clock.best())),
            "Elapse" => {
                let (lo, hi) = (int(0)?, int(1)?);
                if lo < 0 || lo > hi || hi > crate::checker::time::MAX_RANGE {
                    return Err(Exception::new(
                        "System.ArgumentException",
                        format!(
                            "Elapse({}, {}): expected 0 <= lo <= hi <= {}",
                            lo,
                            hi,
                            crate::checker::time::MAX_RANGE
                        ),
                    ));
                }
                elapse(self, crate::checker::time::Range { lo, hi })
            }
            "ElapseCost" => {
                let Some(Value::Str(n)) = args.first() else {
                    return Err(Exception::new("System.ArgumentException", "ElapseCost expects a string"));
                };
                // without a profile everything costs 0; with one, an unpriced name is a typo
                let r = match &self.time.as_ref().unwrap().opts.profile {
                    None => crate::checker::time::Range { lo: 0, hi: 0 },
                    Some(p) => p.cost(n).ok_or_else(|| {
                        Exception::new(
                            "System.ArgumentException",
                            format!("ElapseCost(\"{}\"): latency profile '{}' has no entry \"cost.{}\"", n, p.name, n),
                        )
                    })?,
                };
                elapse(self, r)
            }
            "Mark" => {
                let ts = self.time.as_mut().unwrap();
                ts.marks.push(clock);
                Ok(Value::Int(ts.marks.len() as i64 - 1))
            }
            "Elapsed" => Ok(Value::Int(clock.sub(&mark(self, int(0)?)?).worst())),
            "ElapsedBest" => Ok(Value::Int(clock.sub(&mark(self, int(0)?)?).best())),
            _ => unreachable!(),
        })())
    }

    fn monitor_handle_event(&mut self, mon: usize, e: Event) -> Result<(), Stop> {
        loop {
            let Some(s) = self.monitors[mon].state else {
                let msg = format!(
                    "{} received event '{}' that cannot be handled.",
                    self.monitor_full_name(mon),
                    event_cs_full_name(self.prog, e.id)
                );
                return Err(self.report_bug(msg));
            };
            let action = self.prog.state(s).handler_for(e.id).map(|h| h.action.clone());
            match action {
                Some(HandlerAction::Do(f)) => {
                    self.cov_monitor_do(mon);
                    if self.xml.is_some() {
                        // `Monitor.Do` (entry and exit actions are not logged)
                        let s = self.monitors[mon].state;
                        let temp = match s.map(|s| self.prog.state(s).temperature) {
                            Some(Temperature::Hot) => "[hot]",
                            Some(Temperature::Cold) => "[cold]",
                            _ => "",
                        };
                        let (ty, st) = (self.monitor_full_name(mon), format!("{}{}", self.state_name(s), temp));
                        let action = self.fun_name(f).to_string();
                        self.xlog("MonitorAction", &[("monitorType", &ty), ("state", &st), ("action", &action)], None);
                    }
                    self.monitor_run_action(mon, f, &e)?;
                    return self.monitor_apply_transition(mon);
                }
                Some(HandlerAction::Goto { target, with }) => {
                    return self.monitor_goto(mon, target, with, &e);
                }
                // ignored (exact or wildcard) events are dropped
                Some(HandlerAction::Ignore) => return Ok(()),
                // deferral is not supported by the Java backend: no handler is generated
                Some(HandlerAction::Defer) | None if self.java_semantics() => {
                    let state = self.state_name(Some(s));
                    let state = state.rsplit('.').next().unwrap_or(&state).to_string();
                    let event = self.prog.event(e.id).name.clone();
                    self.failure.get_or_insert(Failure::Unhandled { state: state.clone(), event: event.clone() });
                    return Err(self.report_bug(format!("prt.State[{}] missing event handler for {}", state, event)));
                }
                Some(HandlerAction::Defer) => return Ok(()),
                None => {
                    self.monitor_state_log(mon, false);
                    self.monitors[mon].state = None;
                }
            }
        }
    }
}

impl Exec {
    /// Enters `code`, whose `nargs` arguments are on top of the operand stack.
    pub fn push_frame(&mut self, code: Rc<CodeBlock>, fun: FunId, nargs: usize) {
        let base = self.values.len() - nargs;
        let size = code.frame_size as usize;
        if code.params_in_order && nargs == code.param_slots.len() {
            // the arguments already sit in their slots
            self.values.resize(base + size, Value::Null);
        } else {
            let args = self.values.split_off(base);
            self.values.resize(base + size, Value::Null);
            for (slot, arg) in code.param_slots.iter().zip(args) {
                self.values[base + *slot as usize] = arg;
            }
        }
        self.frames.push(Frame { code, fun, pc: 0, base });
    }

    /// The locals and the operand stack of frame `i`.
    pub fn frame_values(&self, i: usize) -> (&[Value], &[Value]) {
        let f = &self.frames[i];
        let locals_end = f.base + f.code.frame_size as usize;
        let end = self.frames.get(i + 1).map_or(self.values.len(), |next| next.base);
        (&self.values[f.base..locals_end], &self.values[locals_end..end])
    }
}

/// Outcome of running an action.
pub enum ExecOutcome {
    Finished(Value),
    Yield(Exec, SchedKind),
    /// A raise or goto unwound the action.
    Unwound,
}

/// Pseudo event id used for the internal `GotoStateEvent`.
pub const GOTO_EVENT: EventId = EventId(u32::MAX);

/// Progress of a scenario (coverage) monitor within one schedule.
#[derive(Debug, Clone)]
pub struct ScenarioProgress {
    pub decl: MachineId,
    pub name: String,
    pub total_states: usize,
    /// Distinct states entered.
    pub reached: std::collections::BTreeSet<StateId>,
    pub saw_start_entry: bool,
    /// Entered a cold (accepting) state other than its start state.
    pub satisfied: bool,
}

/// `MonitorMapWithScenarios`: the module's monitors plus every scenario monitor observing
/// all interfaces of the module (only when the module defines interfaces).
pub fn monitor_map_with_scenarios(prog: &Program, module: &ModuleInfo) -> Vec<(MachineId, Vec<InterfaceId>)> {
    let mut map = module.monitor_map.clone();
    if !module.interface_def.is_empty() {
        let all: Vec<InterfaceId> = module.interface_def.iter().map(|(i, _)| *i).collect();
        for (i, m) in prog.machines.iter().enumerate() {
            let id = MachineId(i as u32);
            if m.is_scenario() && !map.iter().any(|(k, _)| *k == id) {
                map.push((id, all.clone()));
            }
        }
    }
    map
}

/// The events a `receive` with these cases waits for (`halt` always included).
pub fn receive_wait_set(prog: &Program, cases: &[ReceiveCase]) -> Vec<EventId> {
    let mut wait: Vec<EventId> = Vec::new();
    for c in cases {
        if !wait.contains(&c.event) {
            wait.push(c.event);
        }
    }
    if !wait.contains(&prog.halt_event) {
        wait.push(prog.halt_event);
    }
    wait
}
