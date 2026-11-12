// The two stores. A row carries the version (lsn) the old store assigned when the
// write committed there; deletes are soft (a tombstone row with its own lsn), so a
// delete has a version too.
//
// Real atomic primitives, one per handler:
//   old store: write a row (assigns the next lsn), read a row
//   new store: put a row only if its lsn is newer than the stored one
//              (UPDATE ... WHERE lsn < :lsn, or INSERT if absent), read a row
// The backfill reads a row from old and then writes it to new: two steps, with
// writers running in between. That gap is the race this sample is about.

type tRow = (value: int, deleted: bool, lsn: int);

event eOldWrite: (client: machine, req: int, key: int, value: int, deleted: bool);
event eOldWritten: (req: int, lsn: int);
event eReadRow: (client: machine, key: int);
event eRow: (key: int, row: tRow);
// A writer's second write, carrying the lsn the old store assigned.
event eNewWrite: (client: machine, key: int, row: tRow);
// The backfill's copy of a row it read from old.
event eCopy: (client: machine, key: int, row: tRow);
event eNewWritten: (key: int);

// What a key that was never written reads as.
fun Absent(): tRow {
  return (value = 0, deleted = true, lsn = 0);
}

fun RowOf(rows: map[int, tRow], key: int): tRow {
  if (key in rows) { return rows[key]; }
  return Absent();
}

machine OldStore {
  var rows: map[int, tRow];
  var lsn: int;

  start state Serving {
    // The table as it is when the migration starts: key -> value.
    entry (seed: map[int, int]) {
      var key: int;
      foreach (key in keys(seed)) {
        lsn = lsn + 1;
        rows[key] = (value = seed[key], deleted = false, lsn = lsn);
        announce eOldCommitted, (key = key, row = rows[key], req = 0);
      }
    }
    on eOldWrite do (w: (client: machine, req: int, key: int, value: int, deleted: bool)) {
      lsn = lsn + 1;
      rows[w.key] = (value = w.value, deleted = w.deleted, lsn = lsn);
      announce eOldCommitted, (key = w.key, row = rows[w.key], req = w.req);
      send w.client, eOldWritten, (req = w.req, lsn = lsn);
    }
    on eReadRow do (r: (client: machine, key: int)) {
      send r.client, eRow, (key = r.key, row = RowOf(rows, r.key));
    }
  }
}

// The fixed new store: both writers and the backfill use the same conditional put,
// so a stale copy can never overwrite a newer write or a tombstone.
machine NewStore {
  var rows: map[int, tRow];

  start state Serving {
    on eNewWrite do (w: (client: machine, key: int, row: tRow)) {
      PutIfNewer(w.key, w.row);
      send w.client, eNewWritten, (key = w.key,);
    }
    on eCopy do (w: (client: machine, key: int, row: tRow)) {
      PutIfNewer(w.key, w.row);
      send w.client, eNewWritten, (key = w.key,);
    }
    on eReadRow do (r: (client: machine, key: int)) {
      send r.client, eRow, (key = r.key, row = RowOf(rows, r.key));
    }
  }

  fun PutIfNewer(key: int, row: tRow) {
    if (row.lsn > RowOf(rows, key).lsn) {
      rows[key] = row;
      announce eNewApplied, (key = key, row = row);
    }
  }
}
