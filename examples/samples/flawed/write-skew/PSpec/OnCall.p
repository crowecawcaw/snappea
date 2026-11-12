// The on-call rule, stated over committed writes and each doctor's request and answer.
// The spec rebuilds the committed on-call state from the writes alone; it does not know
// about snapshots, conflicts or retries.

// From the driver, before anything starts: which doctors are on call, and how many
// off-call requests it will make.
event eRosterStarted: (onCall: set[int], requests: int);
// From the driver: a doctor asks to go off call.
event eOffCallRequested: int;
// From the database, inside a commit: every row the transaction wrote (row -> value).
// Doctor rows hold 1 (on call) or 0 (off call); other rows are not doctors.
event eCommitted: map[int, int];
// From the doctor's client: the answer to its request.
event eOffCallAnswered: (doctor: int, granted: bool);

// Safety: at least one doctor is on call after every commit; a doctor goes off call only
// if they asked; "granted" means their row committed as off call; "refused" is only
// allowed when they are the last doctor on call.
// Progress: every request gets an answer (hot state).
spec AlwaysSomeoneOnCall observes eRosterStarted, eOffCallRequested, eCommitted, eOffCallAnswered {
  var onCall: set[int];
  var doctors: set[int];
  var asked: set[int];
  var unanswered: int;

  start state Idle {
    on eRosterStarted do (s: (onCall: set[int], requests: int)) {
      assert sizeof(s.onCall) > 0, "nobody on call at the start";
      assert s.requests > 0, "planned no requests";
      onCall = s.onCall;
      doctors = s.onCall;
      unanswered = s.requests;
      goto Pending;
    }
    on eOffCallRequested do { assert false, "request before the roster started"; }
    on eCommitted do { assert false, "commit before the roster started"; }
    on eOffCallAnswered do { assert false, "answer before the roster started"; }
  }

  hot state Pending {
    on eRosterStarted do { assert false, "roster started twice"; }
    on eOffCallRequested do (d: int) {
      assert d in doctors && !(d in asked), format("doctor {0} asked twice or is not on the roster", d);
      asked += (d);
    }
    on eCommitted do (w: map[int, int]) { Commit(w); }
    on eOffCallAnswered do (a: (doctor: int, granted: bool)) {
      assert a.doctor in asked, format("doctor {0} answered without an open request", a.doctor);
      if (a.granted) {
        assert !(a.doctor in onCall), format("doctor {0} was told they are off call, but their row says on call", a.doctor);
      } else {
        assert a.doctor in onCall && sizeof(onCall) == 1,
          format("doctor {0} was refused, but {1} doctors are on call", a.doctor, sizeof(onCall));
      }
      asked -= (a.doctor);
      unanswered = unanswered - 1;
      if (unanswered == 0) { goto AllAnswered; }
    }
  }

  // Other transactions (not about who is on call) may still commit.
  state AllAnswered {
    on eRosterStarted do { assert false, "roster started twice"; }
    on eOffCallRequested do { assert false, "request beyond the plan"; }
    on eCommitted do (w: map[int, int]) { Commit(w); }
    on eOffCallAnswered do { assert false, "answer beyond the plan"; }
  }

  fun Commit(w: map[int, int]) {
    var row: int;
    foreach (row in keys(w)) {
      if (row in doctors) {
        assert w[row] == 0 && row in asked, format("doctor {0} was taken off call without asking", row);
        onCall -= (row);
      }
    }
    assert sizeof(onCall) > 0, "nobody is on call: the last on-call doctors went off call together";
  }
}
