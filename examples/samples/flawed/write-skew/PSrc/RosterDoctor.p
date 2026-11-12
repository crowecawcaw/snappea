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

machine RosterDoctor {
  start state Run {
    entry (p: (owner: machine, doctor: int, store: machine, doctors: set[int])) {
      var snap: tSnapshot;
      var d: int;
      var count: int;
      var writes: map[int, int];
      var committed: bool;
      while (!committed) {
        send p.store, eSnapshotRead, this;
        receive { case eSnapshot: (s: tSnapshot) { snap = s; } }
        count = 0;
        foreach (d in p.doctors) { count = count + snap.rows[d]; }
        if (count < 2) {
          announce eOffCallAnswered, (doctor = p.doctor, granted = false);
          send p.owner, eDoctorDone;
          return;
        }
        writes[p.doctor] = 0;
        writes[RosterRow()] = p.doctor;
        send p.store, eCommit, (client = this, ts = snap.ts, writes = writes);
        receive { case eCommitResult: (ok: bool) { committed = ok; } }
      }
      announce eOffCallAnswered, (doctor = p.doctor, granted = true);
      send p.owner, eDoctorDone;
    }
  }
}
