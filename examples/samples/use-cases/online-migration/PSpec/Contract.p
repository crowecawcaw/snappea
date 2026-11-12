// The migration contract, over events the stores announce when a write commits, the
// router announces when it flips reads, and readers announce for each value they get.
// The specs rebuild both tables from that history; they never read the stores.

event eOldCommitted: (key: int, row: tRow, req: int);
event eNewApplied: (key: int, row: tRow);
event eCutover;
event eReadSeen: (reader: int, key: int, row: tRow);

// What a client sees for a row: its value, or -1 when there is none.
fun Visible(row: tRow): int {
  if (row.deleted) { return -1; }
  return row.value;
}

// Safety.
// - New only ever holds versions that were committed to old for that key (nothing
//   invented, no row from another key).
// - At cutover, every key reads the same from new as from old: no lost write, no
//   resurrected delete. (This is the verify step; the model shows it never fails.)
// - After cutover new never moves a key back to an older version.
// - A reader never sees a key go back to an older version, across the cutover too.
spec MigrationSafety observes eOldCommitted, eNewApplied, eCutover, eReadSeen {
  // lsn -> the key and row old committed at that version
  var history: map[int, (key: int, row: tRow)];
  var lastLsn: int;
  var oldRows: map[int, tRow];
  var newRows: map[int, tRow];
  var cutover: bool;
  // reader * 100 + key -> newest lsn that reader has seen for that key
  var seen: map[int, int];

  start state Watching {
    // snappea-lint: allow unused-payload-field -- request ids are MigrationCompletes' concern
    on eOldCommitted do (p: (key: int, row: tRow, req: int)) {
      assert p.row.lsn > lastLsn, format("old store reused version {0}", p.row.lsn);
      lastLsn = p.row.lsn;
      history[p.row.lsn] = (key = p.key, row = p.row);
      oldRows[p.key] = p.row;
    }
    on eNewApplied do (p: (key: int, row: tRow)) {
      assert Committed(p.key, p.row),
        format("new store holds a version of key {0} that old never committed: {1}", p.key, p.row);
      if (cutover) {
        assert p.row.lsn > RowOf(newRows, p.key).lsn,
          format("after cutover key {0} went back from version {1} to {2}", p.key, RowOf(newRows, p.key).lsn, p.row.lsn);
      }
      newRows[p.key] = p.row;
    }
    on eCutover do {
      var key: int;
      assert !cutover, "cut over twice";
      cutover = true;
      foreach (key in keys(oldRows)) {
        assert Visible(RowOf(newRows, key)) == Visible(oldRows[key]),
          format("at cutover key {0} reads {1} from new but {2} from old", key,
                 Visible(RowOf(newRows, key)), Visible(oldRows[key]));
      }
    }
    on eReadSeen do (p: (reader: int, key: int, row: tRow)) {
      var slot: int;
      assert Committed(p.key, p.row),
        format("reader {0} got a version of key {1} that old never committed: {2}", p.reader, p.key, p.row);
      slot = p.reader * 100 + p.key;
      if (slot in seen) {
        assert p.row.lsn >= seen[slot],
          format("reader {0} saw key {1} go back from version {2} to {3}", p.reader, p.key, seen[slot], p.row.lsn);
      }
      seen[slot] = p.row.lsn;
    }
  }

  // The never-written row (lsn 0) is a deleted row; anything else must match history.
  fun Committed(key: int, row: tRow): bool {
    if (row.lsn == 0) { return row == Absent(); }
    return row.lsn in history && history[row.lsn].key == key && history[row.lsn].row == row;
  }
}

// Progress. The driver announces how many writes and reads it plans and each write it
// requests. The spec stays hot until every requested write has committed to old, every
// planned read was answered, reads were cut over to new, and new holds exactly old's
// latest version of every key. A backfill that stops, a drain that never ends or a dual-write
// that never lands fails here.
event eMigrationPlanned: (writes: int, reads: int);
event eWriteRequested: tOp;

spec MigrationCompletes observes eMigrationPlanned, eWriteRequested, eOldCommitted, eNewApplied,
    eCutover, eReadSeen {
  var requested: map[int, tOp];
  var writesLeft: int;
  var readsLeft: int;
  var cutover: bool;
  var oldRows: map[int, tRow];
  var newRows: map[int, tRow];

  start state Idle {
    on eMigrationPlanned do (p: (writes: int, reads: int)) {
      writesLeft = p.writes;
      readsLeft = p.reads;
      goto Busy;
    }
    on eOldCommitted do { assert false, "old store written while no migration runs"; }
    on eWriteRequested do { assert false, "write requested without a plan"; }
    on eNewApplied do { assert false, "new store written while no migration runs"; }
    on eCutover do { assert false, "cutover while no migration runs"; }
    on eReadSeen do { assert false, "read while no migration runs"; }
  }

  hot state Busy {
    on eMigrationPlanned do { assert false, "migration planned twice"; }
    on eWriteRequested do (op: tOp) {
      assert !(op.req in requested), format("write {0} requested twice", op.req);
      requested[op.req] = op;
    }
    on eOldCommitted do (p: (key: int, row: tRow, req: int)) {
      oldRows[p.key] = p.row;
      // Request 0 is the table's existing data, written as the old store starts.
      if (p.req == 0) { return; }
      assert p.req in requested, format("write {0} committed but never requested", p.req);
      assert requested[p.req].key == p.key && requested[p.req].value == p.row.value
        && requested[p.req].deleted == p.row.deleted,
        format("write {0} committed {1} to key {2}, but asked for {3}", p.req, p.row, p.key, requested[p.req]);
      requested -= (p.req);
      writesLeft = writesLeft - 1;
      Check();
    }
    // snappea-lint: allow unused-payload-field -- false positive: p.key is read as the map index on the left
    on eNewApplied do (p: (key: int, row: tRow)) {
      newRows[p.key] = p.row;
      Check();
    }
    on eCutover do {
      cutover = true;
      Check();
    }
    // snappea-lint: allow unused-payload-field -- MigrationSafety checks what was read; this spec counts reads
    on eReadSeen do (p: (reader: int, key: int, row: tRow)) {
      readsLeft = readsLeft - 1;
      Check();
    }
  }

  fun Check() {
    var key: int;
    if (!cutover || writesLeft > 0 || readsLeft > 0) { return; }
    foreach (key in keys(oldRows)) {
      if (RowOf(newRows, key) != oldRows[key]) { return; }
    }
    goto Idle;
  }
}
