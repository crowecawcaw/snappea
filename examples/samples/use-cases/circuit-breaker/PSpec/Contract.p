// The circuit breaker contract, over events the breaker announces at its decisions,
// the dependency announces when it answers, the cooldown timer announces when it
// fires, and the callers (drivers) announce. The spec rebuilds the breaker's legal
// state from that history: it counts consecutive failures itself and decides which
// results are late, then checks every admit, reject and transition the breaker makes.

enum BreakerState { Closed, Open, HalfOpen }
enum CallOutcome { Succeeded, Failed, Rejected }

// Driver: the failure threshold the breaker is configured with.
event BreakerPlanned: (threshold: int);
// Breaker: sent `call` to the dependency.
event CallDispatched: (call: int);
// Breaker: answered `call` with Rejected without calling the dependency.
event CallRejected: (call: int);
// Dependency: its answer to `call`.
event DepAnswered: (call: int, ok: bool);
// Breaker: received the dependency's result for `call`. Any transition it causes is
// announced right after, in the same handler.
event CallResultObserved: (call: int, ok: bool);
// Breaker: changed state.
event BreakerChanged: (mode: BreakerState);
// Timer: the cooldown of the open period that started it has elapsed. `period` is the
// breaker's transition count when it opened.
event CooldownElapsed: (period: int);

// Safety: no call reaches the dependency while open; at most one trial while
// half-open; rejections only while open or while a trial is in flight; the breaker
// opens exactly when K consecutive results of the current closed period failed (a
// success resets the count, a late result from an earlier period counts for nothing);
// half-open only after the cooldown; the trial's result decides closed or open.
// Progress: the breaker does not stay open (Open is hot): after the cooldown it lets
// a trial through.
spec BreakerContract observes BreakerPlanned, CallDispatched, CallRejected, DepAnswered, CallResultObserved,
    BreakerChanged, CooldownElapsed {
  var threshold: int;
  var mode: BreakerState;
  // number of transitions so far; calls are tagged with the period they were sent in
  var period: int;
  var failures: int;
  var sentIn: map[int, int];
  var answers: map[int, bool];
  var trialSent: bool;
  var cooled: bool;
  // a transition the breaker must announce next
  var mustChange: bool;
  var expected: BreakerState;

  start state Init {
    on BreakerPlanned do (p: (threshold: int)) {
      assert p.threshold > 0, "threshold must be positive";
      threshold = p.threshold;
      mode = Closed;
      goto Watching;
    }
    on CallDispatched do { assert false, "call before the breaker was planned"; }
    on CallRejected do { assert false, "rejection before the breaker was planned"; }
    on DepAnswered do { assert false, "answer before the breaker was planned"; }
    on CallResultObserved do { assert false, "result before the breaker was planned"; }
    on BreakerChanged do { assert false, "transition before the breaker was planned"; }
    on CooldownElapsed do { assert false, "cooldown before the breaker was planned"; }
  }

  state Watching {
    on BreakerPlanned do { assert false, "breaker planned twice"; }
    on CallDispatched do (d: (call: int)) { Dispatched(d.call); }
    on CallRejected do (r: (call: int)) { ShortCircuited(r.call); }
    on DepAnswered do (a: (call: int, ok: bool)) { Answered(a.call, a.ok); }
    on CallResultObserved do (r: (call: int, ok: bool)) { ResultSeen(r.call, r.ok); }
    on BreakerChanged do (c: (mode: BreakerState)) {
      Changed(c.mode);
      if (mode == Open) { goto Opened; }
    }
    on CooldownElapsed do (t: (period: int)) {
      assert false, format("cooldown for period {0} fired while the breaker was {1}", t.period, mode);
    }
  }

  // Hot: an execution must not end with the breaker open.
  hot state Opened {
    on BreakerPlanned do { assert false, "breaker planned twice"; }
    on CallDispatched do (d: (call: int)) {
      assert false, format("call {0} reached the dependency while the breaker was open", d.call);
    }
    on CallRejected do (r: (call: int)) { ShortCircuited(r.call); }
    on DepAnswered do (a: (call: int, ok: bool)) { Answered(a.call, a.ok); }
    on CallResultObserved do (r: (call: int, ok: bool)) { ResultSeen(r.call, r.ok); }
    on BreakerChanged do (c: (mode: BreakerState)) {
      Changed(c.mode);
      goto Watching;
    }
    on CooldownElapsed do (t: (period: int)) { Cooled(t.period); }
  }

  fun NoPendingChange() {
    assert !mustChange, format("breaker did not change to {0} after the result that requires it", expected);
  }

  // A function, not `answers[a.call] = a.ok` in the handler: the unused-payload-field
  // lint does not count a map key on the left of `=` as a read.
  fun Answered(call: int, ok: bool) {
    answers[call] = ok;
  }

  fun Dispatched(call: int) {
    NoPendingChange();
    assert !(call in sentIn), format("call {0} sent twice", call);
    assert mode != Open, format("call {0} reached the dependency while the breaker was open", call);
    if (mode == HalfOpen) {
      assert !trialSent, format("call {0} sent while a half-open trial was in flight", call);
      trialSent = true;
    }
    sentIn[call] = period;
  }

  fun ShortCircuited(call: int) {
    NoPendingChange();
    assert mode == Open || (mode == HalfOpen && trialSent),
      format("call {0} rejected while the breaker was {1}", call, mode);
  }

  fun ResultSeen(call: int, ok: bool) {
    var late: bool;
    NoPendingChange();
    assert call in sentIn, format("result for call {0} that was never sent", call);
    assert call in answers && answers[call] == ok,
      format("breaker saw result {0} for call {1}, which the dependency did not give", ok, call);
    late = sentIn[call] != period;
    sentIn -= (call);
    answers -= (call);
    if (late) {
      // Sent before the last transition. It answers its caller and nothing else.
      return;
    }
    if (mode == Closed) {
      if (ok) {
        failures = 0;
      } else {
        failures = failures + 1;
        if (failures == threshold) { Expect(Open); }
      }
    } else {
      // The half-open trial decides.
      if (ok) { Expect(Closed); } else { Expect(Open); }
    }
  }

  fun Expect(target: BreakerState) {
    mustChange = true;
    expected = target;
  }

  fun Changed(target: BreakerState) {
    if (mode == Open) {
      assert !mustChange, "breaker changed while a transition was pending";
      assert target == HalfOpen, format("breaker went from Open to {0}", target);
      assert cooled, "breaker went half-open before its cooldown elapsed";
    } else {
      assert mustChange,
        format("breaker went from {0} to {1} without the results that require it ({2} failures of {3})",
               mode, target, failures, threshold);
      assert target == expected, format("breaker went to {0}, the results require {1}", target, expected);
    }
    mustChange = false;
    mode = target;
    period = period + 1;
    failures = 0;
    trialSent = false;
    cooled = false;
  }

  fun Cooled(p: int) {
    NoPendingChange();
    // Timers start only when the breaker opens, and only a cooldown ends an open period,
    // so a timer fires during the period that started it.
    assert mode == Open && p == period,
      format("cooldown for period {0} fired while the breaker was {1} in period {2}", p, mode, period);
    cooled = true;
  }
}

// Driver: how many callers will run.
event CallersPlanned: int;
// Caller (driver): made its last call and got its answer.
event CallerDone;
// Caller (driver): started `call`.
event CallStarted: (call: int);
// Caller (driver): got this answer for `call`.
event CallAnswered: (call: int, outcome: CallOutcome);

// Progress: every caller finishes, every call gets exactly one answer, and it is the right one: Rejected only
// if the breaker short-circuited it, otherwise the dependency's answer.
spec CallsAnswered observes CallersPlanned, CallStarted, CallAnswered, CallerDone, CallRejected,
    DepAnswered {
  var callers: int;
  var started: set[int];
  // call -> the answer its caller must get
  var due: map[int, CallOutcome];

  start state Idle {
    on CallersPlanned do (n: int) {
      assert n > 0, "planned no callers";
      callers = n;
      goto Busy;
    }
    on CallerDone do { assert false, "caller done without a plan"; }
    on CallStarted do { assert false, "call started without a plan"; }
    on CallAnswered do { assert false, "answer without a plan"; }
    on CallRejected do { assert false, "rejection without a plan"; }
    on DepAnswered do { assert false, "dependency answer without a plan"; }
  }

  hot state Busy {
    on CallersPlanned do { assert false, "callers planned twice"; }
    on CallStarted do (c: (call: int)) {
      assert !(c.call in started), format("call {0} started twice", c.call);
      started += (c.call);
    }
    on CallRejected do (r: (call: int)) {
      Decide(r.call, Rejected);
    }
    on DepAnswered do (a: (call: int, ok: bool)) {
      if (a.ok) { Decide(a.call, Succeeded); } else { Decide(a.call, Failed); }
    }
    on CallAnswered do (a: (call: int, outcome: CallOutcome)) {
      assert a.call in due, format("call {0} answered before anything decided it", a.call);
      assert a.outcome == due[a.call],
        format("call {0} answered {1}, but it {2}", a.call, a.outcome, due[a.call]);
      due -= (a.call);
      started -= (a.call);
    }
    on CallerDone do {
      callers = callers - 1;
      if (callers == 0) {
        assert sizeof(started) == 0, "every caller finished with a call unanswered";
        goto Done;
      }
    }
  }

  state Done {
    on CallersPlanned do { assert false, "callers planned twice"; }
    on CallerDone do { assert false, "caller done twice"; }
    on CallStarted do { assert false, "call started after every call was answered"; }
    on CallAnswered do { assert false, "answer after every call was answered"; }
    on CallRejected do { assert false, "rejection after every call was answered"; }
    on DepAnswered do { assert false, "dependency answer after every call was answered"; }
  }

  fun Decide(call: int, outcome: CallOutcome) {
    assert call in started, format("call {0} decided but never started", call);
    assert !(call in due), format("call {0} decided twice", call);
    due[call] = outcome;
  }
}
