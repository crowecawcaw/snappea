// The caller's contract: an answer within 1 s of the call, at most 3 attempts, and a
// retry or an error only when that attempt's timeout reached the client before its reply.

// Announced by the caller (the test driver) as it calls: a stopwatch handle from Mark().
event Called: int;
// Announced by the client.
event Attempt: int;
event Outcome: bool;
// Announced by the server and the timer right after they send (the message is queued).
event Received: int;
event Replied: int;
event TimerFired: (ev: event, tag: int);

spec AnswersWithinDeadline observes Called, Attempt, Outcome, Received, Replied, TimerFired {
  var call: int;
  var attempts: int;
  var received: int;
  var answered: bool;
  // attempt -> true if its reply was queued before its timeout
  var replyFirst: map[int, bool];

  start state Idle {
    on Called goto Calling with (m: int) { call = m; }
    on Attempt do { assert false, "attempt before the call"; }
    on Outcome do { assert false, "answer before the call"; }
    on Received do { assert false, "request before the call"; }
    on Replied do { assert false, "reply before the call"; }
    on TimerFired do { assert false, "timer before the call"; }
  }

  // Hot until the client answers and every attempt reached the server.
  hot state Calling {
    on Called do { assert false, "called twice"; }
    on Attempt do (n: int) {
      assert n == attempts + 1 && n <= 3, format("attempt {0} after {1} attempts; at most 3 allowed", n, attempts);
      if (n > 1) {
        assert n - 1 in replyFirst && !replyFirst[n - 1], format("attempt {0} retried before its timeout", n - 1);
      }
      attempts = n;
    }
    on Received do (n: int) {
      assert n <= attempts, format("server received attempt {0}, which was never made", n);
      received = received + 1;
      Settle();
    }
    on Replied do (n: int) { First(n, true); }
    on TimerFired do (t: (ev: event, tag: int)) {
      if (t.ev == Timeout) { First(t.tag, false); }
    }
    on Outcome do (ok: bool) {
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
    on Called do { assert false, "called after the answer"; }
    on Attempt do { assert false, "attempt after the answer"; }
    on Outcome do { assert false, "answered twice"; }
    on Received do { assert false, "request received after every attempt was"; }
    ignore Replied, TimerFired;
  }

  fun Settle() {
    if (answered && received == attempts) { goto Answered; }
  }

  fun First(attempt: int, isReply: bool) {
    if (!(attempt in replyFirst)) { replyFirst[attempt] = isReply; }
  }
}
