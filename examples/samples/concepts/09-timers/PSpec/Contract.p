// The contract of a call with a timeout: each request reaches the server and ends
// exactly once. A success is justified by a reply to that request, with the server's
// result for it; a failure is justified by that request's own timeout. The spec does
// not guess which message reached the client first: a real log cannot show that.

// Announced by the driver: what it asked for.
event Requested: (id: int, value: int);
// Announced by the server when the request reaches it.
event Received: int;
// Announced by the server just before it sends the reply.
event Replied: int;
// Announced by a timer just before it sends the timeout.
event TimerFired: int;
// Announced by the client when it decides a request.
event Outcome: (id: int, ok: bool, result: int);

spec OutcomesJustified observes Requested, Received, Replied, TimerFired, Outcome {
  // id -> value the driver sent
  var requested: map[int, int];
  var received: set[int];
  var replied: set[int];
  var timedOut: set[int];
  var decided: set[int];

  // Hot: a request has not reached the server or has no outcome yet. A client that
  // never sends, or waits forever, fails here at the end of the run.
  start hot state Waiting {
    on Requested do (r: (id: int, value: int)) {
      assert !(r.id in requested), format("request {0} planned twice", r.id);
      requested[r.id] = r.value;
    }
    on Received do (id: int) {
      assert id in requested, format("server received request {0}, which was never sent", id);
      assert !(id in received), format("server received request {0} twice", id);
      received += (id);
      Settle();
    }
    on Replied do (id: int) { Note(id, true); }
    on TimerFired do (id: int) { Note(id, false); }
    on Outcome do (o: (id: int, ok: bool, result: int)) {
      assert o.id in requested, format("outcome for request {0}, which was never sent", o.id);
      assert !(o.id in decided), format("request {0} decided twice", o.id);
      if (o.ok) {
        assert o.result == requested[o.id] * 2,
          format("request {0} returned {1}, but the server computes {2} for it", o.id, o.result, requested[o.id] * 2);
        assert o.id in replied, format("request {0} succeeded, but the server never replied to it", o.id);
      } else {
        assert o.id in timedOut, format("request {0} timed out, but its timer never fired", o.id);
      }
      decided += (o.id);
      Settle();
    }
  }

  // Late replies, and timers that fire after their cancel, still arrive here.
  state AllDone {
    on Replied do (id: int) { Note(id, true); }
    on TimerFired do (id: int) { Note(id, false); }
  }

  fun Note(id: int, isReply: bool) {
    assert id in requested, format("reply or timeout for request {0}, which was never sent", id);
    if (isReply) {
      replied += (id);
    } else {
      timedOut += (id);
    }
  }

  fun Settle() {
    if (sizeof(decided) == sizeof(requested) && sizeof(received) == sizeof(requested)) {
      goto AllDone;
    }
  }
}
