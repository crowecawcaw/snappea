// Negative control: the same client without the reply's id check. A reply to request 1
// that arrives after request 1 timed out is taken as the answer to request 2.
// tcBugStaleReply shows the spec catching it.
machine StaleReplyClient {
  var server: Server;
  var inputs: seq[int];
  var current: int;
  var timer: Timer;

  start state Sending {
    entry (input: (server: Server, inputs: seq[int])) {
      server = input.server;
      inputs = input.inputs;
      goto Calling;
    }
  }

  state Calling {
    entry {
      current = current + 1;
      send server, Request, (client = this, id = current, value = inputs[current - 1]);
      timer = new Timer((holder = this, tag = current));
    }
    // The bug is ignoring the id, which is what the lint reports here.
    // snappea-lint: allow unused-design-payload-field -- negative control: its flaw is that it ignores r.id
    on Reply do (r: (id: int, result: int)) {
      // BUG: no check that r.id is the request in flight.
      send timer, Cancel;
      Decide(true, r.result);
    }
    on Timeout do (tag: int) {
      if (tag != current) { return; }
      Decide(false, 0);
    }
  }

  state Done {
    ignore Reply, Timeout;
  }

  fun Decide(ok: bool, result: int) {
    announce Outcome, (id = current, ok = ok, result = result);
    if (current < sizeof(inputs)) { goto Calling; }
    goto Done;
  }
}
