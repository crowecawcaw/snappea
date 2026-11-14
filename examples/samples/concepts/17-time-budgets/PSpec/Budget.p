// The caller's contract: an answer within 1 s of the call, at most 3 attempts, a retry
// only after the previous attempt timed out, a success only for an attempt the server
// answered, and an error only after the last attempt timed out.

// Announced by the caller (the test driver) as it calls: a stopwatch handle from Mark().
event Called: int;
// Announced by the client.
event Attempt: int;
event Outcome: bool;
// Announced by the server and the timer just before they send.
event Received: int;
event Replied: int;
event TimerFired: (message: event, tag: int);

spec AnswersWithinDeadline observes Called, Attempt, Outcome, Received, Replied, TimerFired {
  var call: int;
  var attempts: int;
  var received: int;
  var answered: bool;
  // attempts the server replied to, and attempts whose timeout fired
  var replied: set[int];
  var timedOut: set[int];

  start state Idle {
    on Called goto Calling with (m: int) { call = m; }
  }

  // Hot until the client answers and every attempt reached the server.
  hot state Calling {
    on Attempt do (n: int) {
      assert n == attempts + 1 && n <= 3, format("attempt {0} after {1} attempts; at most 3 allowed", n, attempts);
      if (n > 1) {
        assert n - 1 in timedOut, format("attempt {0} started before attempt {1} timed out", n, n - 1);
      }
      attempts = n;
    }
    on Received do (n: int) {
      assert n <= attempts, format("server received attempt {0}, which was never made", n);
      received = received + 1;
      Settle();
    }
    on Replied do (n: int) { replied += (n); }
    on TimerFired do (t: (message: event, tag: int)) {
      if (t.message == Timeout) { timedOut += (t.tag); }
    }
    on Outcome do (ok: bool) {
      assert Elapsed(call) <= ms(1000),
        format("answered (ok = {0}) after up to {1} us; the deadline is 1000000 us", ok, Elapsed(call));
      if (ok) {
        assert attempts in replied, format("attempt {0} succeeded, but the server never replied to it", attempts);
      } else {
        assert attempts == 3, format("gave up after {0} attempts", attempts);
        assert attempts in timedOut, format("gave up before attempt {0} timed out", attempts);
      }
      answered = true;
      Settle();
    }
  }

  // Late replies and timeouts still arrive after the answer.
  state Answered {
    on Received do (n: int) {
      assert false, format("server received attempt {0} after every attempt had arrived", n);
    }
    ignore Replied, TimerFired;
  }

  fun Settle() {
    if (answered && received == attempts) { goto Answered; }
  }
}
