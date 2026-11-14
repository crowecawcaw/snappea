// THE FIX. Every transfer locks the lower account id first. All workers then take
// locks in one global order, so no cycle of waiting workers can form: whoever holds
// the lower lock can always get the higher one eventually.

machine OrderedWorker {
  var db: Database;
  var t: Transfer;
  var first: int;
  var second: int;

  start state LockingFirst {
    entry (w: (db: Database, transfer: Transfer)) {
      db = w.db;
      t = w.transfer;
      first = t.src;
      second = t.dst;
      // snappea-lint: allow mutation:swap-operator -- a transfer's two accounts always differ, so < and <= agree
      if (t.dst < t.src) {
        first = t.dst;
        second = t.src;
      }
      send db, Lock, (worker = this, transfer = t.transfer, account = first);
    }
    on Locked goto LockingSecond;
  }

  state LockingSecond {
    entry {
      send db, Lock, (worker = this, transfer = t.transfer, account = second);
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
