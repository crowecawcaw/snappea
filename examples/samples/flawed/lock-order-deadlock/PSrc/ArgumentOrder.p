// THE FLAW. A transfer locks its source account, then its destination: the order of
// its arguments. Two opposite transfers (A -> B and B -> A) can each take their first
// lock and then wait forever for the other's.

machine ArgumentOrderWorker {
  var db: Database;
  var t: tTransfer;

  start state LockingFirst {
    entry (w: (db: Database, transfer: tTransfer)) {
      db = w.db;
      t = w.transfer;
      // BUG: lock order depends on the call's arguments, not on the accounts.
      send db, eLock, (worker = this, transfer = t.transfer, account = t.src);
    }
    on eLocked goto LockingSecond;
  }

  state LockingSecond {
    entry {
      send db, eLock, (worker = this, transfer = t.transfer, account = t.dst);
    }
    on eLocked goto Committing;
  }

  state Committing {
    entry {
      send db, eCommit, (worker = this, transfer = t);
    }
    on eCommitted do {
      announce eTransferDone, t.transfer;
    }
  }
}
