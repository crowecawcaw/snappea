// A client that sends requests one at a time, each with a timeout. Requests always reach
// the server, but the server may never answer (it crashed, or the reply was lost), so the timeout is what guarantees an
// outcome. Replies and timeouts are tagged with the request id, so a late reply or a
// timer that fired after its cancel is recognised and ignored.

event Request: (client: machine, id: int, value: int);
event Reply: (id: int, result: int);

// The remote service stands in for the environment, so it is a driver: its faults are
// test input, not code under test, and it is not mutated.
// snappea: driver
machine Server {
  start state Serving {
    on Request do (r: (client: machine, id: int, value: int)) {
      announce Received, r.id;
      // The reply may never come.
      if ($) { return; }
      send r.client, Reply, (id = r.id, result = r.value * 2);
      // Announce after the send: the send is where the scheduler can switch, so this is
      // the moment the reply is in the client's inbox.
      announce Served, r.id;
    }
  }
}

machine Client {
  var server: Server;
  var inputs: seq[int];
  // id of the request in flight; ids are 1, 2, ...
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
    on Reply do (r: (id: int, result: int)) {
      // A late reply to a request that already timed out.
      if (r.id != current) { return; }
      // Best effort: the timer may already have fired. Correctness does not depend
      // on the cancel winning, so the checker cannot tell it is there.
      // snappea-lint: allow mutation:remove-statement -- cancel is best effort; a timeout that fires anyway is ignored by its tag
      send timer, Cancel;
      Decide(true, r.result);
    }
    on Timeout do (tag: int) {
      // A timer for an earlier request, which fired although we cancelled it.
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
