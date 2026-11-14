// THE FLAW. A transfer locks its source account, then its destination: the order of
// its arguments. Two opposite transfers (A -> B and B -> A) can each take their first
// lock and then wait forever for the other's.

machine ArgumentOrderWorker {
  var db: Database;
  var t: Transfer;

  start state LockingFirst {
    entry (w: (db: Database, transfer: Transfer)) {
      db = w.db;
      t = w.transfer;
      // BUG: lock order depends on the call's arguments, not on the accounts.
      send db, Lock, (worker = this, transfer = t.transfer, account = t.src);
    }
    on Locked goto LockingSecond;
  }

  state LockingSecond {
    entry {
      send db, Lock, (worker = this, transfer = t.transfer, account = t.dst);
    }
    on Locked goto Committing;
  }

  state Committing {
    entry {
      send db, Commit, (worker = this, transfer = t);
    }
    on Committed do {
      announce TransferDone, t.transfer;
    }
  }
}
