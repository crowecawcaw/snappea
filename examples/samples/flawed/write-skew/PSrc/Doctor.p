// FLAWED: a doctor's client goes off call in one transaction:
//
//     BEGIN;                                             -- snapshot isolation
//     SELECT count(*) FROM doctors WHERE on_call;        -- 2: someone else is on call
//     UPDATE doctors SET on_call = false WHERE id = :me; -- writes only my row
//     COMMIT;
//
// Two doctors doing this at once each see the other on call in their snapshot, each
// writes only their own row, so the write sets do not overlap and both commit. Nobody
// is on call. This is write skew: no single row was written twice, but the pair of
// commits breaks an invariant that spans rows.

// Tells whoever started the client (the test driver) that the request is over.
event DoctorDone;

machine Doctor {
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
