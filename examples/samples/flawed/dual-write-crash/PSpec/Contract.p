// The contract between the service and downstream, stated over events the database, the
// downstream consumer and the client announce. It does not mention how changes travel
// (direct publish, outbox, change data capture), so it fits either design.

type tChange = (id: int, value: int);

// From the database, inside the transaction that commits a change.
event eCommitted: tChange;
// From the downstream consumer, when it applies a change for the first time, or skips
// one it already applied.
event eApplied: tChange;
event eDuplicateIgnored: int;

// Every committed change reaches downstream: applied once, with the committed value,
// and only if it committed. While a committed change has not been applied the spec is
// hot, so a run that ends with one missing fails.
spec CommittedChangesPublished observes eCommitted, eApplied, eDuplicateIgnored {
  var committed: map[int, int];
  var applied: set[int];

  start state CaughtUp {
    on eCommitted do (c: tChange) { Commit(c); goto CommittedNotPublished; }
    on eApplied do (c: tChange) { Apply(c); }
    on eDuplicateIgnored do (id: int) { Duplicate(id); }
  }

  hot state CommittedNotPublished {
    on eCommitted do (c: tChange) { Commit(c); }
    on eApplied do (c: tChange) {
      Apply(c);
      if (sizeof(applied) == sizeof(committed)) { goto CaughtUp; }
    }
    on eDuplicateIgnored do (id: int) { Duplicate(id); }
  }

  fun Commit(c: tChange) {
    assert !(c.id in committed), format("change {0} committed twice", c.id);
    committed[c.id] = c.value;
  }

  fun Apply(c: tChange) {
    assert c.id in committed, format("downstream applied change {0}, which never committed", c.id);
    assert c.value == committed[c.id],
      format("downstream applied change {0} with value {1}, but {2} committed", c.id, c.value, committed[c.id]);
    assert !(c.id in applied), format("downstream applied change {0} twice", c.id);
    applied += (c.id);
  }

  fun Duplicate(id: int) {
    assert id in applied, format("downstream skipped change {0} as a duplicate before applying it", id);
  }
}

// From the client (a test driver): how many updates it will send, each update, and the
// answer it got. ok means the service confirmed the commit; a reset (the connection
// dropped when the service crashed) leaves the outcome unknown.
event eUpdatesPlanned: int;
event eUpdateRequested: tChange;
event eUpdateAnswered: (id: int, ok: bool);

// Every update gets an answer, a change commits only with the value its client asked
// for, and a confirmed update has committed.
spec UpdatesAnswered observes eUpdatesPlanned, eUpdateRequested, eUpdateAnswered, eCommitted {
  // id -> value asked for
  var requested: map[int, int];
  var answered: set[int];
  var committed: set[int];
  var unanswered: int;

  start state Idle {
    on eUpdatesPlanned do (n: int) {
      assert n > 0, "planned no updates";
      unanswered = n;
      goto Answering;
    }
    on eUpdateRequested do { assert false, "update requested without a plan"; }
    on eUpdateAnswered do { assert false, "update answered without a plan"; }
    on eCommitted do { assert false, "change committed without a plan"; }
  }

  hot state Answering {
    on eUpdatesPlanned do { assert false, "updates planned twice"; }
    on eUpdateRequested do (c: tChange) {
      assert !(c.id in requested), format("update {0} requested twice", c.id);
      requested[c.id] = c.value;
    }
    on eCommitted do (c: tChange) {
      assert c.id in requested, format("change {0} committed, but no client asked for it", c.id);
      assert c.value == requested[c.id],
        format("change {0} committed with value {1}, but its client asked for {2}", c.id, c.value, requested[c.id]);
      committed += (c.id);
    }
    on eUpdateAnswered do (a: (id: int, ok: bool)) {
      assert a.id in requested && !(a.id in answered), format("update {0} answered twice or never requested", a.id);
      assert !a.ok || a.id in committed, format("update {0} confirmed but never committed", a.id);
      answered += (a.id);
      unanswered = unanswered - 1;
      if (unanswered == 0) { goto Done; }
    }
  }

  state Done {
    on eUpdatesPlanned do { assert false, "updates planned twice"; }
    on eUpdateRequested do { assert false, "update requested beyond the plan"; }
    on eUpdateAnswered do { assert false, "update answered beyond the plan"; }
    ignore eCommitted;
  }
}
