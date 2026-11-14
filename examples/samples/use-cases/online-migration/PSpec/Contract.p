// The migration contract, over events the stores announce when a write commits, the
// router announces when it flips reads, and readers announce for each value they get.
// The specs rebuild both tables from that history; they never read the stores.

event OldCommitted: (key: int, row: Row, req: int);
event NewApplied: (key: int, row: Row);
event Cutover;
event ReadSeen: (reader: int, key: int, row: Row);
// A writer's call returned: the write is in both stores.
event WriteAcked: (req: int);
event ReadStarted: (reader: int, key: int);
// The old store served a read.
event OldRead;

// How a key that was never written reads, as the contract defines it.
fun NoRow(): Row {
  return (value = 0, deleted = true, lsn = 0);
}

// What a client sees for a row: its value, or -1 when there is none.
fun Visible(row: Row): int {
  if (row.deleted) { return -1; }
  return row.value;
}

// Safety.
// - New only ever holds versions that were committed to old for that key (nothing
//   invented, no row from another key).
// - At cutover, every key reads the same from new as from old: no lost write, no
//   resurrected delete. (This is the verify step; the model shows it never fails.)
// - New never moves a key back to an older version (a stale copy or a resurrected
//   delete fails here, at the moment it lands).
// - A read returns at least the newest write whose call returned before the read
//   started (no stale reads), and a reader never sees a key go back to an older
//   version, across the cutover too.
// - After cutover the old store serves no reads.
spec MigrationSafety observes OldCommitted, NewApplied, Cutover, ReadSeen, WriteAcked,
    ReadStarted, OldRead {
  // lsn -> the key and row old committed at that version
  var history: map[int, (key: int, row: Row)];
  var lastLsn: int;
  var oldRows: map[int, Row];
  var newRows: map[int, Row];
  var cutover: bool;
  // reader * 100 + key -> newest lsn that reader has seen for that key
  var seen: map[int, int];
  // req -> (key, lsn) it committed at; key -> newest lsn of a returned write
  var commits: map[int, (key: int, lsn: int)];
  var acked: map[int, int];
  // reader * 100 + key -> the lsn its current read must reach
  var floor: map[int, int];

  start state Watching {
    on OldCommitted do (p: (key: int, row: Row, req: int)) {
      assert p.row.lsn > lastLsn, format("old store reused version {0}", p.row.lsn);
      lastLsn = p.row.lsn;
      history[p.row.lsn] = (key = p.key, row = p.row);
      oldRows[p.key] = p.row;
      if (p.req == 0) {
        // Existing data counts as written before anything starts.
        acked[p.key] = p.row.lsn;
      } else {
        commits[p.req] = (key = p.key, lsn = p.row.lsn);
      }
    }
    on WriteAcked do (p: (req: int)) {
      var c: (key: int, lsn: int);
      assert p.req in commits, format("write {0} returned before it committed to old", p.req);
      c = commits[p.req];
      assert c.key in newRows && newRows[c.key].lsn >= c.lsn,
        format("new store lost write {0}: it returned, but new lacks version {1} of key {2}", p.req, c.lsn, c.key);
      if (!(c.key in acked) || acked[c.key] < c.lsn) {
        acked[c.key] = c.lsn;
      }
    }
    on ReadStarted do (p: (reader: int, key: int)) {
      var slot: int;
      slot = p.reader * 100 + p.key;
      floor[slot] = 0;
      if (p.key in acked) {
        floor[slot] = acked[p.key];
      }
    }
    on OldRead do {
      assert !cutover, "cutover broken: old store served a read after cutover";
    }
    on NewApplied do (p: (key: int, row: Row)) {
      assert Committed(p.key, p.row),
        format("new store holds a version of key {0} that old never committed: {1}", p.key, p.row);
      assert !(p.key in newRows) || p.row.lsn >= newRows[p.key].lsn,
        format("new store moved key {0} back from version {1} to {2}", p.key, newRows[p.key].lsn, p.row.lsn);
      newRows[p.key] = p.row;
    }
    on Cutover do {
      var key: int;
      assert !cutover, "cut over twice";
      cutover = true;
      foreach (key in keys(oldRows)) {
        assert key in newRows, format("at cutover key {0} is missing from new", key);
        assert Visible(newRows[key]) == Visible(oldRows[key]),
          format("cutover broken: new store lost a change: key {0} reads {1} from new but {2} from old", key,
                 Visible(newRows[key]), Visible(oldRows[key]));
      }
    }
    on ReadSeen do (p: (reader: int, key: int, row: Row)) {
      var slot: int;
      assert Committed(p.key, p.row),
        format("reader {0} got a version of key {1} that old never committed: {2}", p.reader, p.key, p.row);
      slot = p.reader * 100 + p.key;
      assert slot in floor, format("reader {0} got key {1} without asking for it", p.reader, p.key);
      assert p.row.lsn >= floor[slot],
        format("reader {0} read key {1} at version {2}, older than version {3} written before the read",
               p.reader, p.key, p.row.lsn, floor[slot]);
      floor -= (slot);
      if (slot in seen) {
        assert p.row.lsn >= seen[slot],
          format("reader {0} saw key {1} go back from version {2} to {3}", p.reader, p.key, seen[slot], p.row.lsn);
      }
      seen[slot] = p.row.lsn;
    }
  }

  // The never-written row (lsn 0) is a deleted row; anything else must match history.
  fun Committed(key: int, row: Row): bool {
    if (row.lsn == 0) { return row == NoRow(); }
    return row.lsn in history && history[row.lsn].key == key && history[row.lsn].row == row;
  }
}

// Progress. The driver announces the old table's contents, how many writes and reads
// it plans, and each write it requests. The spec stays hot until every requested write has returned, every
// planned read was answered, and reads were cut over to new. A backfill that stops, a
// drain that never ends or a dual-write that never lands fails here.
event MigrationPlanned: (writes: int, seed: map[int, int]);
event WriteRequested: Op;
event ReadsPlanned: (reader: int, key: int, reads: int);

spec MigrationCompletes observes MigrationPlanned, WriteRequested, ReadsPlanned, OldCommitted,
    WriteAcked, Cutover, ReadSeen {
  var requested: map[int, Op];
  var committed: set[int];
  // reader -> the key it was asked to read
  var readerKey: map[int, int];
  // reader -> reads it still has to get answered
  var readsLeft: map[int, int];
  var writesLeft: int;
  // key -> value the old table starts with, until the old store has it
  var seedLeft: map[int, int];
  var cutover: bool;

  start state Idle {
    on MigrationPlanned do (p: (writes: int, seed: map[int, int])) {
      writesLeft = p.writes;
      seedLeft = p.seed;
      goto Busy;
    }
    on OldCommitted do { assert false, "old store written while no migration runs"; }
    on WriteRequested do { assert false, "write requested without a plan"; }
    on ReadsPlanned do { assert false, "reads planned without a plan"; }
    on WriteAcked do { assert false, "write returned while no migration runs"; }
    on Cutover do { assert false, "cutover while no migration runs"; }
    on ReadSeen do { assert false, "read while no migration runs"; }
  }

  hot state Busy {
    on MigrationPlanned do { assert false, "migration planned twice"; }
    on WriteRequested do (op: Op) {
      assert !(op.req in requested), format("write {0} requested twice", op.req);
      requested[op.req] = op;
    }
    on OldCommitted do (p: (key: int, row: Row, req: int)) {
      // Request 0 is the table's existing data, written as the old store starts.
      if (p.req == 0) {
        assert p.key in seedLeft && seedLeft[p.key] == p.row.value && !p.row.deleted,
          format("old store started with {0} for key {1}, which the driver did not give it", p.row, p.key);
        seedLeft -= (p.key);
        return;
      }
      assert p.req in requested, format("write {0} committed but never requested", p.req);
      assert requested[p.req].key == p.key && requested[p.req].value == p.row.value
        && requested[p.req].deleted == p.row.deleted,
        format("write {0} committed {1} to key {2}, but asked for {3}", p.req, p.row, p.key, requested[p.req]);
      assert !(p.req in committed), format("write {0} committed twice", p.req);
      committed += (p.req);
    }
    on WriteAcked do (p: (req: int)) {
      assert p.req in committed, format("write {0} returned before it committed", p.req);
      committed -= (p.req);
      requested -= (p.req);
      writesLeft = writesLeft - 1;
      Check();
    }
    on ReadsPlanned do (p: (reader: int, key: int, reads: int)) {
      var r: int;
      r = p.reader;
      readerKey[r] = p.key;
      readsLeft[r] = p.reads;
    }
    on Cutover do {
      cutover = true;
      Check();
    }
    on ReadSeen do (p: (reader: int, key: int, row: Row)) {
      assert p.reader in readerKey && readerKey[p.reader] == p.key,
        format("reader {0} read key {1}, which it was not asked to read", p.reader, p.key);
      assert readsLeft[p.reader] > 0, format("reader {0} got more answers than it asked for", p.reader);
      readsLeft[p.reader] = readsLeft[p.reader] - 1;
      Check();
    }
  }

  fun Check() {
    var reader: int;
    if (!cutover || writesLeft > 0 || sizeof(seedLeft) > 0) { return; }
    foreach (reader in keys(readsLeft)) {
      if (readsLeft[reader] > 0) { return; }
    }
    goto Idle;
  }
}
