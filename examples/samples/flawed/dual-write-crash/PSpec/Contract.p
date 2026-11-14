// The contract between the service and downstream, stated over events the database, the
// downstream consumer and the client announce. It does not mention how changes travel
// (direct publish, outbox, change data capture), so it fits either design.

type Change = (id: int, value: int);

// From the database, inside the transaction that commits a change.
event Committed: Change;
// From the broker when it accepts a published change, and from the downstream consumer
// when it applies one. A change delivered again is not applied again.
event Delivered: Change;
event Applied: Change;

// Every committed change reaches downstream: delivered only if it committed, with the
// committed value, and applied exactly once. While a committed change has not been applied the spec is
// hot, so a run that ends with one missing fails.
spec CommittedChangesPublished observes Committed, Delivered, Applied {
  var committed: map[int, int];
  var delivered: set[int];
  var applied: set[int];

  start state CaughtUp {
    on Committed do (c: Change) { Commit(c); goto CommittedNotPublished; }
    on Applied do (c: Change) {
      assert false, format("downstream applied change {0} = {1}, but every committed change was already applied", c.id, c.value);
    }
    on Delivered do (c: Change) { Deliver(c); }
  }

  hot state CommittedNotPublished {
    on Committed do (c: Change) { Commit(c); }
    on Applied do (c: Change) {
      Apply(c);
      if (sizeof(applied) == sizeof(committed)) { goto CaughtUp; }
    }
    on Delivered do (c: Change) { Deliver(c); }
  }

  fun Commit(c: Change) {
    assert !(c.id in committed), format("change {0} committed twice", c.id);
    committed[c.id] = c.value;
  }

  fun Deliver(c: Change) {
    assert c.id in committed, format("change {0} was published, but it never committed", c.id);
    assert c.value == committed[c.id],
      format("change {0} was published with value {1}, but {2} committed", c.id, c.value, committed[c.id]);
    delivered += (c.id);
  }

  fun Apply(c: Change) {
    assert c.id in delivered, format("downstream applied change {0} before the broker delivered it", c.id);
    assert c.value == committed[c.id],
      format("downstream applied change {0} with value {1}, but {2} committed", c.id, c.value, committed[c.id]);
    assert !(c.id in applied), format("downstream applied change {0} twice", c.id);
    applied += (c.id);
  }
}

// From the client (a test driver): how many updates it will send, each update, and the
// answer it got. ok means the service confirmed the commit; a reset (the connection
// dropped when the service crashed) leaves the outcome unknown.
event UpdatesPlanned: int;
event UpdateRequested: Change;
event UpdateAnswered: (id: int, ok: bool);

// Every update gets an answer, a change commits only with the value its client asked
// for, and a confirmed update has committed.
spec UpdatesAnswered observes UpdatesPlanned, UpdateRequested, UpdateAnswered, Committed {
  // id -> value asked for
  var requested: map[int, int];
  var answered: set[int];
  var committed: set[int];
  var unanswered: int;

  start state Idle {
    on UpdatesPlanned do (n: int) {
      assert n > 0, "planned no updates";
      unanswered = n;
      goto Answering;
    }
    on UpdateRequested do { assert false, "update requested without a plan"; }
    on UpdateAnswered do { assert false, "update answered without a plan"; }
    on Committed do { assert false, "change committed without a plan"; }
  }

  hot state Answering {
    on UpdatesPlanned do { assert false, "updates planned twice"; }
    on UpdateRequested do (c: Change) {
      assert !(c.id in requested), format("update {0} requested twice", c.id);
      requested[c.id] = c.value;
    }
    on Committed do (c: Change) {
      assert c.id in requested, format("change {0} committed, but no client asked for it", c.id);
      assert c.value == requested[c.id],
        format("change {0} committed with value {1}, but its client asked for {2}", c.id, c.value, requested[c.id]);
      committed += (c.id);
    }
    on UpdateAnswered do (a: (id: int, ok: bool)) {
      assert a.id in requested && !(a.id in answered), format("update {0} answered twice or never requested", a.id);
      assert !a.ok || a.id in committed, format("update {0} confirmed but never committed", a.id);
      answered += (a.id);
      unanswered = unanswered - 1;
      if (unanswered == 0) { goto Done; }
    }
  }

  state Done {
    on UpdatesPlanned do { assert false, "updates planned twice"; }
    on UpdateRequested do { assert false, "update requested beyond the plan"; }
    on UpdateAnswered do { assert false, "update answered beyond the plan"; }
    ignore Committed;
  }
}

// Without a relay crash, the outbox delivers each change once: the relay marks what the
// broker accepted. Asserted only in tests where the relay cannot crash; at-least-once
// delivery after a crash is expected and handled by the consumer.
spec DeliveredOnceWithoutRelayCrash observes Delivered {
  var delivered: set[int];
  start state Watching {
    // snappea-lint: allow unused-payload-field -- CommittedChangesPublished checks values; this spec checks duplicate delivery by id
    on Delivered do (c: Change) {
      assert !(c.id in delivered), format("change {0} = {1} delivered twice although the relay never crashed", c.id, c.value);
      delivered += (c.id);
    }
  }
}
