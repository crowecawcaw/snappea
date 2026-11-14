// Drivers. Clients run the design's acquire / write / release algorithm and are mutated;
// these machines only start them, and in the pause test decide when the paused client
// wakes up.

fun StartClient(owner: machine, lock: LockService, storage: machine, plan: ClientPlan): Client {
  announce ClientStarted, plan;
  return new Client((owner = owner, lock = lock, storage = storage, id = plan.id,
                     behavior = plan.behavior, writes = plan.writes,
                     maxAttempts = plan.maxAttempts));
}

// Two clients want the lock at once, each asking once. Every order of grants, busy answers and retries,
// lease expiry (at any point, including between grant and write), writes and releases
// is explored.
machine TwoClientsContend {
  start state Run {
    entry {
      var lock: LockService;
      var storage: Storage;
      lock = new LockService();
      storage = new Storage();
      announce ClientsPlanned, 2;
      StartClient(this, lock, storage, (id = 1, behavior = Normal, writes = 1, maxAttempts = 1));
      StartClient(this, lock, storage, (id = 2, behavior = Normal, writes = 1, maxAttempts = 1));
    }
    ignore ClientDone;
  }
}

// One client dies holding the lease while the other wants it. Only expiry frees the
// lock; the test is not over until it has. The survivor retries once, so it can be
// refused, then granted after the expiry, or give up.
machine HolderCrashes {
  start state Run {
    entry {
      var lock: LockService;
      var storage: Storage;
      lock = new LockService();
      storage = new Storage();
      announce ClientsPlanned, 2;
      StartClient(this, lock, storage, (id = 1, behavior = Crashes, writes = 1, maxAttempts = 1));
      StartClient(this, lock, storage, (id = 2, behavior = Normal, writes = 1, maxAttempts = 2));
    }
    ignore ClientDone;
  }
}

// The GC-pause story, forced: client 1 acquires and stalls; its lease runs out; client 2
// acquires, writes and releases; only then does client 1 wake and write with its old
// token. Storage must reject the first write; the client must abandon its second.
machine PausedHolder {
  var lock: LockService;
  var storage: machine;
  var first: Client;

  start state Run {
    entry (s: machine) {
      lock = new LockService();
      storage = s;
      announce ClientsPlanned, 2;
      first = StartClient(this, lock, storage, (id = 1, behavior = Pauses, writes = 2, maxAttempts = 1));
    }
    on Parked do {
      // The stall outlasts the lease: the lock's clock says its time to live has
      // passed. (The lease's own timer may fire too; the lock ignores the second one.)
      send lock, LeaseExpire, 1;
      // Two writes with the same token: storage must accept a token equal to the
      // highest it has seen.
      StartClient(this, lock, storage, (id = 2, behavior = Normal, writes = 2, maxAttempts = 1));
    }
    on ClientDone do (d: (id: int, outcome: Outcome)) {
      if (d.id == 2) { send first, Resume; }
    }
  }
}

machine MainPausedHolder {
  start state Init { entry { new PausedHolder(new Storage()); } }
}
machine MainBugStorageIgnoresTokens {
  start state Init { entry { new PausedHolder(new UnfencedStorage()); } }
}

// Covered: a holder's lease expired, a later holder wrote, and the first holder's late
// write was refused.
scenario LateWriteFenced observes Expired, WriteAccepted, WriteRejected {
  start hot state WaitExpiry {
    on Expired goto WaitSuccessorWrite;
  }
  hot state WaitSuccessorWrite {
    on WriteAccepted goto WaitLateWrite;
  }
  hot state WaitLateWrite {
    on WriteRejected goto Covered;
  }
  cold state Covered {
  }
}

module Locking = { LockService, LeaseTimer, Client };

test tcTwoClientsContend [main=TwoClientsContend]:
  assert FencedWrites, ClientsFinish in (union Locking, { TwoClientsContend, Storage });

test tcHolderCrashes [main=HolderCrashes]:
  assert FencedWrites, ClientsFinish in (union Locking, { HolderCrashes, Storage });

test tcPausedHolderWritesLate [main=MainPausedHolder]:
  assert FencedWrites, ClientsFinish in (union Locking, { MainPausedHolder, PausedHolder, Storage });

// snappea: expect-bug "write with token 1 accepted after token 2"
test tcBugStorageIgnoresTokens [main=MainBugStorageIgnoresTokens]:
  assert FencedWrites, ClientsFinish in
  (union Locking, { MainBugStorageIgnoresTokens, PausedHolder, UnfencedStorage });
