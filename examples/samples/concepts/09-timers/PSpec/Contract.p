// The contract of a call with a timeout: each request reaches the server and ends
// exactly once, and the outcome is decided by whichever reached the client first, the
// reply or that request's own timeout. A success carries the server's result for that
// request.

// Announced by the driver: what it asked for.
event eRequested: (id: int, value: int);
// Announced by the server when the request reaches it.
event eReceived: int;
// Announced by the server right after it sends the reply.
event eServed: int;
// Announced by a timer right after it sends the timeout.
event eTimerFired: int;
// Announced by the client when it decides a request.
event eOutcome: (id: int, ok: bool, result: int);

spec FirstArrivalDecides observes eRequested, eReceived, eServed, eTimerFired, eOutcome {
  // id -> value the driver sent
  var requested: map[int, int];
  var received: set[int];
  // id -> true if the reply was sent before the timeout, false if the timeout was first
  var replyFirst: map[int, bool];
  var decided: set[int];

  // Hot: a request has not reached the server or has no outcome yet. A client that
  // never sends, or waits forever, fails here at the end of the run.
  start hot state Waiting {
    on eRequested do (r: (id: int, value: int)) {
      assert !(r.id in requested), format("request {0} sent twice", r.id);
      requested[r.id] = r.value;
    }
    on eReceived do (id: int) {
      assert id in requested && !(id in received), format("request {0} received twice or never sent", id);
      received += (id);
      Settle();
    }
    on eServed do (id: int) { Arrived(id, true); }
    on eTimerFired do (id: int) { Arrived(id, false); }
    on eOutcome do (o: (id: int, ok: bool, result: int)) {
      assert o.id in requested && !(o.id in decided), format("request {0} decided twice or never sent", o.id);
      if (o.ok) {
        assert o.result == requested[o.id] * 2,
          format("request {0} returned {1}, but the server computes {2} for it", o.id, o.result, requested[o.id] * 2);
      }
      assert o.id in replyFirst, format("request {0} decided before its reply or timeout arrived", o.id);
      // A machine has one inbox: what was sent to it first is received first.
      assert o.ok == replyFirst[o.id],
        format("request {0} reported ok = {1}, but the other message arrived first", o.id, o.ok);
      decided += (o.id);
      Settle();
    }
  }

  // Late replies, and timers that fire after their cancel, still arrive here.
  state AllDone {
    on eRequested do { assert false, "request sent after every request was decided"; }
    on eReceived do { assert false, "request received after every request was decided"; }
    on eServed do (id: int) { Arrived(id, true); }
    on eTimerFired do (id: int) { Arrived(id, false); }
    on eOutcome do { assert false, "outcome after every request was decided"; }
  }

  fun Arrived(id: int, isReply: bool) {
    assert id in requested, format("reply or timeout for request {0}, which was never sent", id);
    if (!(id in replyFirst)) { replyFirst[id] = isReply; }
  }

  fun Settle() {
    if (sizeof(decided) == sizeof(requested) && sizeof(received) == sizeof(requested)) { goto AllDone; }
  }
}
