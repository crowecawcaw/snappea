// The driver announces each request before the client starts, so the spec knows what
// must be decided even if the client never acts.
fun StartCalls(inputs: seq[int]): seq[int] {
  var i: int;
  while (i < sizeof(inputs)) {
    announce Requested, (id = i + 1, value = inputs[i]);
    i = i + 1;
  }
  return inputs;
}

// Two requests in a row with distinct inputs (1 and 2), so a reply credited to the
// wrong request shows up as a wrong result.
fun TwoInputs(): seq[int] {
  var inputs: seq[int];
  inputs += (0, 1);
  inputs += (1, 2);
  return StartCalls(inputs);
}

// A server that may drop each reply, and timers that may fire before or after it.
// Every order of reply, timeout, cancel and the next request is explored, including the
// late reply and the timer that fires after its cancel.
machine TwoCalls {
  start state Init {
    entry {
      new Client((server = new Server(true), inputs = TwoInputs()));
    }
  }
}

// A server that always answers, and a timeout longer than any reply: every call must
// succeed. This is what rules out a client that ignores replies and waits for its timer.
machine TwoAnsweredCalls {
  start state Init {
    entry {
      new Client((server = new Server(false), inputs = TwoInputs()));
    }
  }
}

// A timeout longer than any reply: it never fires before the reply, and the client
// always cancels it, so in this model it never fires at all.
machine LongTimer {
  start state Armed {
    entry (t: (holder: machine, tag: int)) {}
    // The client's cancel: nothing left to stop.
    ignore Cancel;
  }
}

machine StaleReplyCalls {
  start state Init {
    entry {
      new StaleReplyClient((server = new Server(true), inputs = TwoInputs()));
    }
  }
}

test tcTimeoutRace [main=TwoCalls]:
  assert OutcomesJustified in { TwoCalls, Client, Server, Timer };

test tcServerAnswers [main=TwoAnsweredCalls]:
  assert OutcomesJustified in (union { TwoAnsweredCalls, Client, Server }, { LongTimer -> Timer });

// snappea: expect-bug "request 2 returned 2, but the server computes 4 for it"
test tcBugStaleReply [main=StaleReplyCalls]:
  assert OutcomesJustified in { StaleReplyCalls, StaleReplyClient, Server, Timer };
