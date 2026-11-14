// The migration: writers dual-write (old, then new) for the whole migration, the
// migrator backfills every key, then cuts reads over to new. Reads and writes go
// through a router, which drains in-flight reads and writes before it flips: once
// the flip happens, no read can still be served from old and no write is half done.

event BeginWrite: machine;
event WriteGranted;
event EndWrite;
event Route: (client: machine, reader: int, key: int);
event ReadDone;
event CutoverReq: machine;
event CutoverDone;
// To whoever started the migration (the test driver).
event MigrationDone;

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
    on BeginWrite do (writer: machine) {
      writes = writes + 1;
      send writer, WriteGranted;
    }
    on EndWrite do { writes = writes - 1; }
    on Route do (r: (client: machine, reader: int, key: int)) {
      reads = reads + 1;
      if (readsFromNew) {
        send newStore, ReadRow, r;
      } else {
        send oldStore, ReadRow, r;
      }
    }
    on ReadDone do { reads = reads - 1; }
    on CutoverReq do (m: machine) {
      migrator = m;
      goto Draining;
    }
  }

  // New reads and writes wait; in-flight ones finish. Then flip, atomically.
  state Draining {
    entry { TryCutover(); }
    defer BeginWrite, Route;
    on EndWrite do {
      writes = writes - 1;
      TryCutover();
    }
    on ReadDone do {
      reads = reads - 1;
      TryCutover();
    }
  }

  fun TryCutover() {
    if (writes == 0 && reads == 0) {
      readsFromNew = true;
      announce Cutover;
      send migrator, CutoverDone;
      goto Live;
    }
  }
}

type Op = (req: int, key: int, value: int, deleted: bool);

// An application writer: each write goes to old (which assigns the version), then to
// new with that version, inside a write slot from the router.
machine Writer {
  var router: machine;
  var oldStore: OldStore;
  var newStore: machine;
  var ops: seq[Op];
  var op: Op;

  start state Starting {
    entry (input: (router: machine, oldStore: OldStore, newStore: machine, ops: seq[Op])) {
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
      send router, BeginWrite, this;
    }
    on WriteGranted do {
      send oldStore, OldWrite, (client = this, req = op.req, key = op.key, value = op.value, deleted = op.deleted);
    }
    on OldWritten do (w: (req: int, lsn: int)) {
      send newStore, NewWrite, (client = this, key = op.key,
                                 row = (value = op.value, deleted = op.deleted, lsn = w.lsn));
    }
    on NewWritten goto Writing with {
      // Both stores have it: the application call returns now.
      announce WriteAcked, (req = op.req,);
      send router, EndWrite;
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
    on RowRead do (r: (key: int, row: Row)) {
      announce ReadSeen, (reader = id, key = r.key, row = r.row);
      send router, ReadDone;
      Next();
    }
  }

  fun Next() {
    if (left > 0) {
      left = left - 1;
      announce ReadStarted, (reader = id, key = key);
      send router, Route, (client = this, reader = id, key = key);
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
    on RowRead do (r: (key: int, row: Row)) {
      send newStore, Copy, (client = this, key = r.key, row = r.row);
    }
    on NewWritten do { Next(); }
    on CutoverDone do {
      send owner, MigrationDone;
    }
  }

  fun Next() {
    if (sizeof(todo) == 0) {
      send router, CutoverReq, this;
      return;
    }
    send oldStore, ScanRow, (client = this, key = todo[0]);
    todo -= (0);
  }
}
