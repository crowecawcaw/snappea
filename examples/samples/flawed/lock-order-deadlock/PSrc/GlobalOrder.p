// THE FIX. Every transfer locks the lower account id first. All workers then take
// locks in one global order, so no cycle of waiting workers can form: whoever holds
// the lower lock can always get the higher one eventually.

machine OrderedWorker {
  var db: Database;
  var t: tTransfer;
  var first: int;
  var second: int;

  start state LockingFirst {
    entry (w: (db: Database, transfer: tTransfer)) {
      db = w.db;
      t = w.transfer;
      first = t.src;
      second = t.dst;
      // snappea-lint: allow mutation:swap-operator -- a transfer's two accounts always differ, so < and <= agree
      if (t.dst < t.src) {
        first = t.dst;
        second = t.src;
      }
      send db, eLock, (worker = this, transfer = t.transfer, account = first);
    }
    on eLocked goto LockingSecond;
  }

  state LockingSecond {
    entry {
      send db, eLock, (worker = this, transfer = t.transfer, account = second);
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
