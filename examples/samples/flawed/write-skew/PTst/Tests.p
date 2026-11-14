// Drivers. Doctors' clients and the database are the design under test. Each driver
// announces who is on call, starts the database with one row per doctor (1 = on call)
// and the roster row, then announces each request and starts that doctor's client.

fun Roster(n: int) : set[int] {
  var s: set[int];
  while (sizeof(s) < n) { s += (sizeof(s) + 1); }
  return s;
}

fun InitialRows(doctors: set[int]) : map[int, int] {
  var rows: map[int, int];
  var d: int;
  rows[0] = 0;
  foreach (d in doctors) { rows[d] = 1; }
  return rows;
}

// Two doctors on call, both ask to go off call at once.
machine BothOffCall {
  start state Init {
    entry {
      var store: machine;
      var doctors: set[int];
      doctors = Roster(2);
      announce RosterStarted, (onCall = doctors, requests = 2);
      store = new SnapshotStore(InitialRows(doctors));
      announce OffCallRequested, 1;
      new Doctor((doctor = 1, store = store, doctors = doctors));
      announce OffCallRequested, 2;
      new Doctor((doctor = 2, store = store, doctors = doctors));
    }
  }
}

machine RosterBothOffCall {
  start state Init {
    entry {
      var store: machine;
      var doctors: set[int];
      doctors = Roster(2);
      announce RosterStarted, (onCall = doctors, requests = 2);
      store = new SnapshotStore(InitialRows(doctors));
      announce OffCallRequested, 1;
      new RosterDoctor((owner = this, doctor = 1, store = store, doctors = doctors));
      announce OffCallRequested, 2;
      new RosterDoctor((owner = this, doctor = 2, store = store, doctors = doctors));
    }
    // The spec checks each answer through OffCallAnswered; nothing waits here.
    ignore DoctorDone;
  }
}

// Three doctors on call, two ask at once: both must be granted, one after the other's
// conflict and retry. A fix that refuses whenever there is contention fails here.
machine RosterTwoOfThree {
  start state Init {
    entry {
      var store: machine;
      var doctors: set[int];
      doctors = Roster(3);
      announce RosterStarted, (onCall = doctors, requests = 2);
      store = new SnapshotStore(InitialRows(doctors));
      announce OffCallRequested, 1;
      new RosterDoctor((owner = this, doctor = 1, store = store, doctors = doctors));
      announce OffCallRequested, 2;
      new RosterDoctor((owner = this, doctor = 2, store = store, doctors = doctors));
    }
    // The spec checks each answer through OffCallAnswered; nothing waits here.
    ignore DoctorDone;
  }
}

// Lifecycle: doctor 1 goes off call and is done; only then does doctor 2 ask. Doctor 2
// is the last one on call and must be refused, on a fresh snapshot.
machine RosterOneAfterAnother {
  var store: machine;
  var doctors: set[int];

  start state First {
    entry {
      doctors = Roster(2);
      announce RosterStarted, (onCall = doctors, requests = 2);
      store = new SnapshotStore(InitialRows(doctors));
      announce OffCallRequested, 1;
      new RosterDoctor((owner = this, doctor = 1, store = store, doctors = doctors));
    }
    on DoctorDone goto Second;
  }
  state Second {
    entry {
      announce OffCallRequested, 2;
      new RosterDoctor((owner = this, doctor = 2, store = store, doctors = doctors));
    }
    // The spec checks each answer through OffCallAnswered; nothing waits here.
    ignore DoctorDone;
  }
}

// A transaction about something else (row 100: a doctor's phone number) commits while
// a doctor goes off call: it must not count as a conflict, since the rows differ. This
// is the snapshot-isolation rule the flawed design falls through.
machine RosterUnrelatedCommit {
  start state Init {
    entry {
      var store: machine;
      var doctors: set[int];
      doctors = Roster(2);
      announce RosterStarted, (onCall = doctors, requests = 1);
      store = new SnapshotStore(InitialRows(doctors));
      announce OffCallRequested, 1;
      new RosterDoctor((owner = this, doctor = 1, store = store, doctors = doctors));
      new PhoneUpdate(store);
    }
    // The spec checks each answer through OffCallAnswered; nothing waits here.
    ignore DoctorDone;
  }
}

// A one-row transaction written directly by the test: read a snapshot, write row 100.
machine PhoneUpdate {
  start state Run {
    entry (store: machine) {
      var writes: map[int, int];
      writes[100] = 5550100;
      send store, SnapshotRead, this;
      receive {
        case Snapshot: (s: Snapshot) {
          send store, Commit, (client = this, ts = s.ts, writes = writes);
        }
      }
      receive { case CommitResult: (ok: bool) { assert ok, "an unrelated transaction aborted"; } }
    }
  }
}

test tcFixedUnrelatedCommit [main=RosterUnrelatedCommit]:
  assert AlwaysSomeoneOnCall in { RosterUnrelatedCommit, SnapshotStore, RosterDoctor, PhoneUpdate };

test tcFixedOneAfterAnother [main=RosterOneAfterAnother]:
  assert AlwaysSomeoneOnCall in { RosterOneAfterAnother, SnapshotStore, RosterDoctor };

test tcFixedBothOffCall [main=RosterBothOffCall]:
  assert AlwaysSomeoneOnCall in { RosterBothOffCall, SnapshotStore, RosterDoctor };

test tcFixedTwoOfThree [main=RosterTwoOfThree]:
  assert AlwaysSomeoneOnCall in { RosterTwoOfThree, SnapshotStore, RosterDoctor };

// Doctor 1 or doctor 2 commits last, depending on the schedule.
// snappea: expect-bug "nobody is on call after doctor"
test tcBugWriteSkew [main=BothOffCall]:
  assert AlwaysSomeoneOnCall in { BothOffCall, SnapshotStore, Doctor };
