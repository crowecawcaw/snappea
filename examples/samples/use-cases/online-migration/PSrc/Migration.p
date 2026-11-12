// The migration: writers dual-write (old, then new) for the whole migration, the
// migrator backfills every key, then cuts reads over to new. Reads and writes go
// through a router, which drains in-flight reads and writes before it flips: once
// the flip happens, no read can still be served from old and no write is half done.

event eBeginWrite: machine;
event eWriteGranted;
event eEndWrite;
event eRoute: (client: machine, key: int);
event eReadDone;
event eCutoverReq: machine;
event eCutoverDone;
// To whoever started the migration (the test driver).
event eMigrationDone;

machine Router {
  var oldStore: OldStore;
  var newStore: machine;
  var readsFromNew: bool;
  var writes: int;
  var reads: int;
  var migrator: machine;

  start state Init {
    entry (input: (oldStore: OldStore, newStore: machine)) {
      oldStore = input.oldStore;
      newStore = input.newStore;
      goto Live;
    }
  }

  state Live {
    on eBeginWrite do (writer: machine) {
      writes = writes + 1;
      send writer, eWriteGranted;
    }
    on eEndWrite do { writes = writes - 1; }
    on eRoute do (r: (client: machine, key: int)) {
      reads = reads + 1;
      if (readsFromNew) {
        send newStore, eReadRow, r;
      } else {
        send oldStore, eReadRow, r;
      }
    }
    on eReadDone do { reads = reads - 1; }
    on eCutoverReq do (m: machine) {
      migrator = m;
      goto Draining;
    }
  }

  // New reads and writes wait; in-flight ones finish. Then flip, atomically.
  state Draining {
    entry { TryCutover(); }
    defer eBeginWrite, eRoute;
    on eEndWrite do {
      writes = writes - 1;
      TryCutover();
    }
    on eReadDone do {
      reads = reads - 1;
      TryCutover();
    }
  }

  fun TryCutover() {
    if (writes == 0 && reads == 0) {
      readsFromNew = true;
      announce eCutover;
      send migrator, eCutoverDone;
      goto Live;
    }
  }
}

type tOp = (req: int, key: int, value: int, deleted: bool);

// An application writer: each write goes to old (which assigns the version), then to
// new with that version, inside a write slot from the router.
machine Writer {
  var router: machine;
  var oldStore: OldStore;
  var newStore: machine;
  var ops: seq[tOp];
  var op: tOp;

  start state Next {
    entry (input: (router: machine, oldStore: OldStore, newStore: machine, ops: seq[tOp])) {
      router = input.router;
      oldStore = input.oldStore;
      newStore = input.newStore;
      ops = input.ops;
      goto Writing;
    }
  }

  state Writing {
    entry {
      if (sizeof(ops) == 0) { goto Finished; }
      op = ops[0];
      ops -= (0);
      send router, eBeginWrite, this;
    }
    on eWriteGranted do {
      send oldStore, eOldWrite, (client = this, req = op.req, key = op.key, value = op.value, deleted = op.deleted);
    }
    on eOldWritten do (w: (req: int, lsn: int)) {
      send newStore, eNewWrite, (client = this, key = op.key,
                                 row = (value = op.value, deleted = op.deleted, lsn = w.lsn));
    }
    on eNewWritten goto Writing with {
      send router, eEndWrite;
    }
  }

  state Finished { }
}

// An application reader: reads a key several times through the router, reporting
// each value it gets, and tells the router when each read is done.
machine Reader {
  var router: machine;
  var id: int;
  var key: int;
  var left: int;

  start state Reading {
    entry (input: (router: machine, id: int, key: int, reads: int)) {
      router = input.router;
      id = input.id;
      key = input.key;
      left = input.reads;
      Next();
    }
    on eRow do (r: (key: int, row: tRow)) {
      announce eReadSeen, (reader = id, key = r.key, row = r.row);
      send router, eReadDone;
      Next();
    }
  }

  fun Next() {
    if (left > 0) {
      left = left - 1;
      send router, eRoute, (client = this, key = key);
    }
  }
}

// Backfill every key (read from old, copy to new), then ask the router to cut over.
machine Migrator {
  var router: machine;
  var oldStore: OldStore;
  var newStore: machine;
  var todo: seq[int];
  var owner: machine;

  start state Backfilling {
    entry (input: (owner: machine, router: machine, oldStore: OldStore, newStore: machine, scan: seq[int])) {
      owner = input.owner;
      router = input.router;
      oldStore = input.oldStore;
      newStore = input.newStore;
      todo = input.scan;
      Next();
    }
    on eRow do (r: (key: int, row: tRow)) {
      send newStore, eCopy, (client = this, key = r.key, row = r.row);
    }
    on eNewWritten do { Next(); }
    on eCutoverDone do {
      send owner, eMigrationDone;
    }
  }

  fun Next() {
    if (sizeof(todo) == 0) {
      send router, eCutoverReq, this;
      return;
    }
    send oldStore, eReadRow, (client = this, key = todo[0]);
    todo -= (0);
  }
}
