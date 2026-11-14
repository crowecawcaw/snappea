// FIXED: materialize the conflict. Every change to who is on call also writes one shared
// row, the roster (here: who changed it last):
//
//     UPDATE doctors SET on_call = false WHERE id = :me;
//     UPDATE roster SET changed_by = :me;                -- every such transaction writes it
//
// Two concurrent off-call transactions now write the same row, so first-committer-wins
// aborts one. It retries on a fresh snapshot, sees only one doctor on call, and refuses.
// (SELECT ... FOR UPDATE on the doctors' rows, or SERIALIZABLE with read-set validation,
// fix it the same way: the transactions must conflict on what they read, not only on
// what they write.)

// The roster row's id; doctor ids are 1 and up.
fun RosterRow() : int { return 0; }

// Tells whoever started the client (the test driver) that the request is over.
event DoctorDone;

machine RosterDoctor {
  start state Run {
    entry (p: (owner: machine, doctor: int, store: machine, doctors: set[int])) {
      var snap: Snapshot;
      var d: int;
      var count: int;
      var writes: map[int, int];
      var decided: bool;
      var granted: bool;
      while (!decided) {
        send p.store, SnapshotRead, this;
        receive { case Snapshot: (s: Snapshot) { snap = s; } }
        count = 0;
        foreach (d in p.doctors) { count = count + snap.rows[d]; }
        if (count < 2) {
          // The last one on call: refuse.
          decided = true;
        } else {
          writes[p.doctor] = 0;
          writes[RosterRow()] = p.doctor;  // the fix: every off-call transaction writes the roster
          send p.store, Commit, (client = this, ts = snap.ts, writes = writes);
          // On a conflict, start over with a fresh snapshot.
          receive { case CommitResult: (ok: bool) { decided = ok; granted = ok; } }
        }
      }
      announce OffCallAnswered, (doctor = p.doctor, granted = granted);
      send p.owner, DoctorDone;
    }
  }
}
