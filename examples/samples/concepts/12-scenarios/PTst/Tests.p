// Two clients increment the counter once each, concurrently.
machine TwoClients {
  start state Init {
    entry {
      var store: Store;
      store = new Store();
      announce eIncrementsPlanned, 2;
      new Client(store);
      new Client(store);
    }
  }
}

// Coverage, not correctness: a compare-and-swap was refused, and a write committed after
// it. With two clients the winner has already written, so that write is the loser's
// retry. A passing test means little unless this race happened in some schedule.
scenario ConflictThenRetry observes eCasResp, eWritten {
  start hot state WaitConflict {
    on eCasResp do (ok: bool) {
      if (!ok) { goto WaitRetry; }
    }
    // Writes before the conflict are the winner's: not what we are waiting for.
    ignore eWritten;
  }
  hot state WaitRetry {
    on eWritten goto Retried;
    // Further refusals and successful replies do not matter here.
    ignore eCasResp;
  }
  cold state Retried {
    ignore eCasResp, eWritten;
  }
}

test tcTwoClients [main=TwoClients]:
  assert NoLostUpdate in { TwoClients, Store, Client };
