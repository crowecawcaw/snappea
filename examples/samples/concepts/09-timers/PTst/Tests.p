// The driver announces each request before the client starts, so the spec knows what
// must be decided even if the client never acts.
fun StartCalls(inputs: seq[int]): seq[int] {
  var i: int;
  while (i < sizeof(inputs)) {
    announce eRequested, (id = i + 1, value = inputs[i]);
    i = i + 1;
  }
  return inputs;
}

// Two requests in a row with distinct inputs, so a reply credited to the wrong request
// shows up as a wrong result. Every order of reply, timeout, cancel and the next request
// is explored, including the late reply and the timer that fires after its cancel.
machine TwoCalls {
  start state Init {
    entry {
      var inputs: seq[int];
      inputs += (0, 1);
      inputs += (1, 2);
      new Client((server = new Server(), inputs = StartCalls(inputs)));
    }
  }
}

machine StaleReplyCalls {
  start state Init {
    entry {
      var inputs: seq[int];
      inputs += (0, 1);
      inputs += (1, 2);
      new StaleReplyClient((server = new Server(), inputs = StartCalls(inputs)));
    }
  }
}

test tcTimeoutRace [main=TwoCalls]:
  assert FirstArrivalDecides in { TwoCalls, Client, Server, Timer };

// expect-bug: but the server computes 4 for it
test tcBugStaleReply [main=StaleReplyCalls]:
  assert FirstArrivalDecides in { StaleReplyCalls, StaleReplyClient, Server, Timer };
