// The two stores. A row carries the version (lsn) the old store assigned when the
// write committed there; deletes are soft (a tombstone row with its own lsn), so a
// delete has a version too.
//
// Real atomic primitives, one per handler:
//   old store: write a row (assigns the next lsn), read a row (a client read or a
//              backfill scan)
//   new store: put a row only if its lsn is newer than the stored one
//              (UPDATE ... WHERE lsn < :lsn, or INSERT if absent), read a row
// The backfill reads a row from old and then writes it to new: two steps, with
// writers running in between. That gap is the race this sample is about.

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

// The fixed new store: both writers and the backfill use the same conditional put,
// so a stale copy can never overwrite a newer write or a tombstone.
machine NewStore {
  var rows: map[int, Row];

  start state Serving {
    on NewWrite do (w: (client: machine, key: int, row: Row)) {
      PutIfNewer(w.key, w.row);
      send w.client, NewWritten, (key = w.key,);
    }
    on Copy do (w: (client: machine, key: int, row: Row)) {
      PutIfNewer(w.key, w.row);
      send w.client, NewWritten, (key = w.key,);
    }
    on ReadRow do (r: (client: machine, reader: int, key: int)) {
      // commit point: the read
      announce ReadServed, (table = NewTable, reader = r.reader, key = r.key);
      send r.client, RowRead, (key = r.key, row = RowOf(rows, r.key));
    }
  }

  fun PutIfNewer(key: int, row: Row) {
    // snappea-lint: allow mutation:swap-operator -- versions are unique per write, so an equal version is the same row
    if (row.lsn > RowOf(rows, key).lsn) {
      rows[key] = row;
      announce NewApplied, (key = key, row = row);
    }
  }
}
