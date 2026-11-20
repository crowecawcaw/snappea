// The fixed interface of the circuit breaker: its states, the messages it exchanges
// with callers, the dependency and the timer service, and the events that are
// announced for specs to observe. See DESIGN.md. Do not change this file.

enum BreakerState { Closed, Open, HalfOpen }
enum CallOutcome { Succeeded, Failed, Rejected }

// ---- Messages ----

// Caller -> breaker: make a call. Call ids are unique.
event CallRequested: (caller: machine, call: int);
// Breaker -> caller: the one answer to `call`.
event CallResult: (call: int, outcome: CallOutcome);
// Breaker -> dependency: `call`, let through.
event DependencyCall: (breaker: machine, call: int);
// Dependency -> breaker: its answer to `call`.
event DependencyResult: (call: int, ok: bool);
// Breaker -> timer service, when it opens: fire Cooldown with this period once the
// cooldown is over. `period` is the number of state changes the breaker has made so
// far, counting the one that opened it (so the first time it opens, period is 1).
event StartCooldown: (breaker: machine, period: int);
// Timer service -> breaker: the cooldown that was started with `period` is over.
event Cooldown: (period: int);

// ---- Announced by the breaker (the design) ----

// The breaker let `call` through: announce it, then send DependencyCall.
event CallDispatched: (call: int);
// The breaker short-circuited `call`: announce it, then send CallResult with Rejected.
event CallRejected: (call: int);
// The breaker received the dependency's result for `call`. Announce it first thing in
// that step, for every result. If the result changes the breaker's state, announce
// BreakerChanged right after it, in the same step.
event CallResultObserved: (call: int, ok: bool);
// The breaker changed state, to `mode`. Announce it at every change (not for the
// initial Closed state), before any message the change causes is sent.
event BreakerChanged: (mode: BreakerState);

// ---- Announced by the environment (drivers) ----

// Test driver, before it creates the breaker: the failure threshold K it will get.
event BreakerPlanned: (threshold: int);
// Dependency: its answer to `call`, announced as it sends it.
event DepAnswered: (call: int, ok: bool);
// Timer service: the cooldown started with `period` is over; announced once per
// StartCooldown, as it first sends Cooldown.
event CooldownElapsed: (period: int);
// Test driver: how many callers will run.
event CallersPlanned: int;
// Caller: started `call` (it then sends CallRequested).
event CallStarted: (call: int);
// Caller: got this answer for `call`.
event CallAnswered: (call: int, outcome: CallOutcome);
// Caller: made its last call and got its answer.
event CallerDone;
