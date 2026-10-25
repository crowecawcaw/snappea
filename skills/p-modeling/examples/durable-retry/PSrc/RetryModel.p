// One service, one bounded operation, and two alternative storage designs.
// Only adjacent local updates before a communication scheduling point model
// one storage primitive. A handler can yield at send/receive/create; this does
// not make the whole handler atomic. The real primitive must co-commit both
// durable updates in the fixed variant.
event eInvoke: (client: RetryDriver, id: int);
event eCrash;
event eRestart;
event eRemember: (client: RetryDriver, id: int);
event eCommitted: int;
event eReply: (id: int, total: int);

machine DurableService {
  var durableTotal: int;
  var durableOutcomes: map[int, int];

  start state Serving {
    on eInvoke do (call: (client: RetryDriver, id: int)) {
      if (call.id in durableOutcomes) {
        send call.client, eReply,
          (id = call.id, total = durableOutcomes[call.id]);
      } else {
        // The operation and its replayable result share one durable commit.
        durableTotal = durableTotal + 1;
        durableOutcomes[call.id] = durableTotal;
        announce eCommit, call.id;
        send call.client, eCommitted, call.id;
        // eCommitted is internal test control, not a client reply. The driver
        // restarts before any eReply, modeling an ambiguous committed request.
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
    on eInvoke do (call: (client: RetryDriver, id: int)) {
      if (call.id in volatileMemo) {
        send call.client, eReply,
          (id = call.id, total = volatileMemo[call.id]);
      } else {
        durableTotal = durableTotal + 1;
        announce eCommit, call.id;
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
    on eRemember do (call: (client: RetryDriver, id: int)) {
      volatileMemo[call.id] = durableTotal;
      send call.client, eReply, (id = call.id, total = durableTotal);
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
