// The migration: the router, the application's writers and readers, and the
// migrator. The events and what each machine takes when created are fixed. The
// reader is given; the router, the writer and the migrator are yours. See DESIGN.md.

// A writer asks the router for a write slot, is granted one, and gives it back.
event BeginWrite: machine;
event WriteGranted;
event EndWrite;
event Route: (client: machine, reader: int, key: int);
event ReadDone;
event CutoverReq: machine;
event CutoverDone;
// To whoever started the migration (the test driver).
event MigrationDone;

type Op = (req: int, key: int, value: int, deleted: bool);

// Routes each read to one store, grants write slots, and flips reads to new when the
// migrator asks. It announces Cutover at the flip.
machine Router {
  start state Init {
    entry (input: (oldStore: OldStore, newStore: machine)) {
      // TODO
    }
  }
}

// An application writer: performs its ops in order, one at a time. Each goes to both
// stores; it announces WriteAcked when the application's call returns.
machine Writer {
  start state Starting {
    entry (input: (router: machine, oldStore: OldStore, newStore: machine, ops: seq[Op])) {
      // TODO
    }
  }
}

// An application reader (given: do not change it): reads a key several times through the router, reporting
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

// Backfills every key in `scan` from old to new, has the router cut reads over, then
// sends MigrationDone to `owner`.
machine Migrator {
  start state Backfilling {
    entry (input: (owner: machine, router: machine, oldStore: OldStore, newStore: machine, scan: seq[int])) {
      // TODO
    }
  }
}
