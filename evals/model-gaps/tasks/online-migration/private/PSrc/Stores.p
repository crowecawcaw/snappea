// The two stores. A row carries the version (lsn) the old store assigned when the
// write committed there; deletes are soft (a tombstone row with its own lsn), so a
// delete has a version too.

type Row = (value: int, deleted: bool, lsn: int);

event OldWrite: (client: machine, req: int, key: int, value: int, deleted: bool);
event OldWritten: (req: int, lsn: int);
// An application read, routed to one store.
event ReadRow: (client: machine, reader: int, key: int);
// The backfill's read of one row from old.
event ScanRow: (client: machine, key: int);
event RowRead: (key: int, row: Row);
// A writer's second write, carrying the lsn the old store assigned.
event NewWrite: (client: machine, key: int, row: Row);
// The backfill's copy of a row it read from old.
event Copy: (client: machine, key: int, row: Row);
event NewWritten: (key: int);

// What a key that was never written reads as.
fun Absent(): Row {
  return (value = 0, deleted = true, lsn = 0);
}

fun RowOf(rows: map[int, Row], key: int): Row {
  if (key in rows) { return rows[key]; }
  return Absent();
}

// The existing store. It is given: do not change it.
machine OldStore {
  var rows: map[int, Row];
  var lsn: int;

  start state Serving {
    // The table as it is when the migration starts: key -> value.
    entry (seed: map[int, int]) {
      var key: int;
      foreach (key in keys(seed)) {
        lsn = lsn + 1;
        rows[key] = (value = seed[key], deleted = false, lsn = lsn);
        announce OldCommitted, (key = key, row = rows[key], req = 0);
      }
    }
    on OldWrite do (w: (client: machine, req: int, key: int, value: int, deleted: bool)) {
      lsn = lsn + 1;
      rows[w.key] = (value = w.value, deleted = w.deleted, lsn = lsn);
      announce OldCommitted, (key = w.key, row = rows[w.key], req = w.req);
      send w.client, OldWritten, (req = w.req, lsn = lsn);
    }
    on ReadRow do (r: (client: machine, reader: int, key: int)) {
      // commit point: the read
      announce ReadServed, (table = OldTable, reader = r.reader, key = r.key);
      send r.client, RowRead, (key = r.key, row = RowOf(rows, r.key));
    }
    on ScanRow do (r: (client: machine, key: int)) {
      send r.client, RowRead, (key = r.key, row = RowOf(rows, r.key));
    }
  }
}

// The new store: writers and the backfill both put a row only if its version is newer
// than the stored one, and a delete stays as a tombstone with its version. So a stale
// copy or a late dual-write can never replace a newer write or bring back a delete.
machine NewStore {
  var rows: map[int, Row];

  start state Serving {
    on NewWrite do (w: (client: machine, key: int, row: Row)) {
      // Two writers' rows can arrive in the other order than old committed them.
      if (Newer(w.key, w.row)) {
        Put(w.key, w.row);
      }
      send w.client, NewWritten, (key = w.key,);
    }
    on Copy do (w: (client: machine, key: int, row: Row)) {
      // The backfill read this row a while ago; a writer may have been here since.
      if (Newer(w.key, w.row)) {
        Put(w.key, w.row);
      }
      send w.client, NewWritten, (key = w.key,);
    }
    on ReadRow do (r: (client: machine, reader: int, key: int)) {
      // commit point: the read
      announce ReadServed, (table = NewTable, reader = r.reader, key = r.key);
      send r.client, RowRead, (key = r.key, row = RowOf(rows, r.key));
    }
  }

  fun Newer(key: int, row: Row): bool {
    // snappea-lint: allow mutation:swap-operator -- versions are unique per write, so an equal version is the same row
    return row.lsn > RowOf(rows, key).lsn;
  }

  fun Put(key: int, row: Row) {
    // A deleted row is stored too: its version must outlive the delete.
    rows[key] = row;
    announce NewApplied, (key = key, row = row);
  }
}
