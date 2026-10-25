// One service, a small bounded set of operations, and alternative storage designs.
// Only adjacent local updates before a communication scheduling point model
// one storage primitive. A handler can yield at send/receive/create; this does
// not make the whole handler atomic. The real primitive must co-commit both
// durable updates in the fixed variant.
event eInvoke: (client: machine, id: int, dropReply: bool);
event eCrash;
event eRestart;
event eRemember: (client: machine, id: int);
event eCommitted: int;
event eReply: (id: int, result: int, currentTotal: int);

machine DurableService {
  var durableTotal: int;
  var durableOutcomes: map[int, int];

  start state Serving {
    on eInvoke do (call: (client: machine, id: int, dropReply: bool)) {
      if (call.id in durableOutcomes) {
        send call.client, eReply,
          (id = call.id, result = durableOutcomes[call.id], currentTotal = durableTotal);
      } else {
        // This non-yielding update region is the modeled durable primitive.
        durableTotal = durableTotal + 1;
        durableOutcomes[call.id] = durableTotal;
        announce eCommit, (id = call.id, total = durableTotal);
        if (call.dropReply) {
          send call.client, eCommitted, call.id;
        } else {
          send call.client, eReply,
            (id = call.id, result = durableOutcomes[call.id], currentTotal = durableTotal);
        }
      }
    }

    on eCrash do { goto Stopped; }
  }

  state Stopped {
    on eRestart do { goto Serving; }
  }
}

// Deliberately broken: the effect survives in durableTotal, while the
// outcome memo lives in volatileMemo and is written in a later transition.
machine SplitCommitService {
  var durableTotal: int;
  var volatileMemo: map[int, int];

  start state Serving {
    on eInvoke do (call: (client: machine, id: int, dropReply: bool)) {
      if (call.id in volatileMemo) {
        send call.client, eReply,
          (id = call.id, result = volatileMemo[call.id], currentTotal = durableTotal);
      } else {
        durableTotal = durableTotal + 1;
        announce eCommit, (id = call.id, total = durableTotal);
        send call.client, eCommitted, call.id;
        goto MemoPending;
      }
    }

    on eCrash do {
      volatileMemo = default(map[int, int]);
      goto Stopped;
    }
  }

  state MemoPending {
    on eRemember do (call: (client: machine, id: int)) {
      volatileMemo[call.id] = durableTotal;
      send call.client, eReply,
        (id = call.id, result = durableTotal, currentTotal = durableTotal);
      goto Serving;
    }
    on eCrash do {
      volatileMemo = default(map[int, int]);
      goto Stopped;
    }
  }

  state Stopped {
    on eRestart do { goto Serving; }
  }
}

// Deliberately broken replay: it returns the saved result but applies that
// request's effect again after another ID has changed the current total.
machine ReapplyReplayService {
  var durableTotal: int;
  var durableOutcomes: map[int, int];

  start state Serving {
    on eInvoke do (call: (client: machine, id: int, dropReply: bool)) {
      if (call.id in durableOutcomes) {
        durableTotal = durableTotal + 1;
        announce eCommit, (id = call.id, total = durableTotal);
        if (call.dropReply) {
          send call.client, eCommitted, call.id;
        } else {
          send call.client, eReply,
            (id = call.id, result = durableOutcomes[call.id], currentTotal = durableTotal);
        }
      } else {
        durableTotal = durableTotal + 1;
        durableOutcomes[call.id] = durableTotal;
        announce eCommit, (id = call.id, total = durableTotal);
        if (call.dropReply) {
          send call.client, eCommitted, call.id;
        } else {
          send call.client, eReply,
            (id = call.id, result = durableOutcomes[call.id], currentTotal = durableTotal);
        }
      }
    }

    on eCrash do { goto Stopped; }
  }

  state Stopped {
    on eRestart do { goto Serving; }
  }
}
