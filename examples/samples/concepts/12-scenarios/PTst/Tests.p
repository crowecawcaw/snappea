// Two clients increment the counter once each, concurrently.
machine TwoClients {
  start state Init {
    entry {
      var store: Store;
      store = new Store();
      announce IncrementsPlanned, 2;
      new Client(store);
      new Client(store);
    }
  }
}

// Coverage, not correctness: a compare-and-swap was refused, and a write committed after
// it. With two clients the winner has already written, so that write is the loser's
// retry. A passing test means little unless this race happened in some schedule.
scenario ConflictThenRetry observes CasResponse, Written {
  start hot state WaitConflict {
    on CasResponse do (ok: bool) {
      if (!ok) { goto WaitRetry; }
    }
  }
  hot state WaitRetry {
    on Written goto Retried;
  }
  cold state Retried {
  }
}

// Three clients increment the counter once each, concurrently: one of them can be
// refused twice before it gets through.
machine ThreeClients {
  start state Init {
    entry {
      var store: Store;
      store = new Store();
      announce IncrementsPlanned, 3;
      new Client(store);
      new Client(store);
      new Client(store);
    }
  }
}

// Coverage: two refusals in one run. Two clients can never produce this (only one of
// them can lose a round), so tcTwoClients alone reports it as a gap.
scenario RefusedTwice observes CasResponse {
  start hot state WaitFirst {
    on CasResponse do (ok: bool) {
      if (!ok) { goto WaitSecond; }
    }
  }
  hot state WaitSecond {
    on CasResponse do (ok: bool) {
      if (!ok) { goto Seen; }
    }
  }
  cold state Seen {
  }
}

test tcTwoClients [main=TwoClients]:
  assert NoLostUpdate in { TwoClients, Store, Client };

test tcThreeClients [main=ThreeClients]:
  assert NoLostUpdate in { ThreeClients, Store, Client };
