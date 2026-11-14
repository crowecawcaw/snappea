// A database with snapshot isolation (SI), as PostgreSQL's REPEATABLE READ, Oracle's
// SERIALIZABLE and most MVCC databases provide it. Shared by both designs: the flaw is
// in how the application uses it, not in the database.
//
// - A transaction reads a snapshot: the committed state as of its start.
// - Its writes are buffered and applied at commit, all at once.
// - First committer wins: a commit aborts if another transaction committed a write to
//   any row it writes after its snapshot was taken. Rows it only read are not checked.
//
// Real atomic primitives: taking a snapshot and reading from it (one handler), commit
// with the conflict check (one handler). Between them, other transactions commit.

type Snapshot = (ts: int, rows: map[int, int]);

event SnapshotRead: machine;
event Snapshot: Snapshot;
event Commit: (client: machine, ts: int, writes: map[int, int]);
event CommitResult: bool;

machine SnapshotStore {
  // Committed state, and the commit log: entry i is the set of rows commit i+1 wrote.
  var rows: map[int, int];
  var log: seq[set[int]];

  start state Serving {
    entry (initial: map[int, int]) { rows = initial; }
    // Begin a transaction and run its query at the snapshot (one round trip). Since
    // reads at a snapshot never change, splitting several reads across messages would
    // add no interleaving.
    on SnapshotRead do (client: machine) {
      send client, Snapshot, (ts = sizeof(log), rows = rows);
    }
    on Commit do (c: (client: machine, ts: int, writes: map[int, int])) {
      var i: int;
      var row: int;
      var written: set[int];
      i = c.ts;
      while (i < sizeof(log)) {
        foreach (row in keys(c.writes)) {
          // snappea-lint: allow mutation:condition-true -- aborting on any concurrent commit is stricter than SI and still correct (the client retries); the contract cannot tell
          if (row in log[i]) {
            send c.client, CommitResult, false;
            return;
          }
        }
        i = i + 1;
      }
      foreach (row in keys(c.writes)) {
        rows[row] = c.writes[row];
        written += (row);
      }
      log += (sizeof(log), written);
      announce Committed, c.writes;
      send c.client, CommitResult, true;
    }
  }
}
