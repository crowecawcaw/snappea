// The caller's contract: an answer within 1 s of the call, at most 3 attempts, and a
// retry or an error only when that attempt's timeout reached the client before its reply.

// Announced by the caller (the test driver) as it calls: a stopwatch handle from Mark().
event eCalled: int;
// Announced by the client.
event eAttempt: int;
event eOutcome: bool;
// Announced by the server and the timer right after they send (the message is queued).
event eReceived: int;
event eReplied: int;
event eTimerFired: (ev: event, tag: int);

spec AnswersWithinDeadline observes eCalled, eAttempt, eOutcome, eReceived, eReplied, eTimerFired {
  var call: int;
  var attempts: int;
  var received: int;
  var answered: bool;
  // attempt -> true if its reply was queued before its timeout
  var replyFirst: map[int, bool];

  start state Idle {
    on eCalled goto Calling with (m: int) { call = m; }
    on eAttempt do { assert false, "attempt before the call"; }
    on eOutcome do { assert false, "answer before the call"; }
    on eReceived do { assert false, "request before the call"; }
    on eReplied do { assert false, "reply before the call"; }
    on eTimerFired do { assert false, "timer before the call"; }
  }

  // Hot until the client answers and every attempt reached the server.
  hot state Calling {
    on eCalled do { assert false, "called twice"; }
    on eAttempt do (n: int) {
      assert n == attempts + 1 && n <= 3, format("attempt {0} after {1} attempts; at most 3 allowed", n, attempts);
      if (n > 1) {
        assert n - 1 in replyFirst && !replyFirst[n - 1], format("attempt {0} retried before its timeout", n - 1);
      }
      attempts = n;
    }
    on eReceived do (n: int) {
      assert n <= attempts, format("server received attempt {0}, which was never made", n);
      received = received + 1;
      Settle();
    }
    on eReplied do (n: int) { First(n, true); }
    on eTimerFired do (t: (ev: event, tag: int)) {
      if (t.ev == eTimeout) { First(t.tag, false); }
    }
    on eOutcome do (ok: bool) {
      assert Elapsed(call) <= ms(1000),
        format("answered (ok = {0}) after up to {1} us; the deadline is 1000000 us", ok, Elapsed(call));
      assert attempts in replyFirst && ok == replyFirst[attempts],
        format("attempt {0} answered ok = {1}, but the other message arrived first", attempts, ok);
      assert ok || attempts == 3, format("gave up after {0} attempts", attempts);
      answered = true;
      Settle();
    }
  }

  // Late replies and timeouts still arrive after the answer; their order no longer matters.
  state Answered {
    on eCalled do { assert false, "called after the answer"; }
    on eAttempt do { assert false, "attempt after the answer"; }
    on eOutcome do { assert false, "answered twice"; }
    on eReceived do { assert false, "request received after every attempt was"; }
    ignore eReplied, eTimerFired;
  }

  fun Settle() {
    if (answered && received == attempts) { goto Answered; }
  }

  fun First(attempt: int, isReply: bool) {
    if (!(attempt in replyFirst)) { replyFirst[attempt] = isReply; }
  }
}
