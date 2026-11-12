// The contract between the service and downstream, stated over events the database, the
// downstream consumer and the client announce. It does not mention how changes travel
// (direct publish, outbox, change data capture), so it fits either design.

type tChange = (id: int, value: int);

// From the database, inside the transaction that commits a change.
event eCommitted: tChange;
// From the broker when it accepts a published change, and from the downstream consumer
// when it applies one. A change delivered again is not applied again.
event eDelivered: tChange;
event eApplied: tChange;

// Every committed change reaches downstream: delivered only if it committed, with the
// committed value, and applied exactly once. While a committed change has not been applied the spec is
// hot, so a run that ends with one missing fails.
spec CommittedChangesPublished observes eCommitted, eDelivered, eApplied {
  var committed: map[int, int];
  var delivered: set[int];
  var applied: set[int];

  start state CaughtUp {
    on eCommitted do (c: tChange) { Commit(c); goto CommittedNotPublished; }
    on eApplied do (c: tChange) {
      assert false, format("downstream applied change {0} = {1}, but every committed change was already applied", c.id, c.value);
    }
    on eDelivered do (c: tChange) { Deliver(c); }
  }

  hot state CommittedNotPublished {
    on eCommitted do (c: tChange) { Commit(c); }
    on eApplied do (c: tChange) {
      Apply(c);
      if (sizeof(applied) == sizeof(committed)) { goto CaughtUp; }
    }
    on eDelivered do (c: tChange) { Deliver(c); }
  }

  fun Commit(c: tChange) {
    assert !(c.id in committed), format("change {0} committed twice", c.id);
    committed[c.id] = c.value;
  }

  fun Deliver(c: tChange) {
    assert c.id in committed, format("change {0} was published, but it never committed", c.id);
    assert c.value == committed[c.id],
      format("change {0} was published with value {1}, but {2} committed", c.id, c.value, committed[c.id]);
    delivered += (c.id);
  }

  fun Apply(c: tChange) {
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

// Without a relay crash, the outbox delivers each change once: the relay marks what the
// broker accepted. Asserted only in tests where the relay cannot crash; at-least-once
// delivery after a crash is expected and handled by the consumer.
spec DeliveredOnceWithoutRelayCrash observes eDelivered {
  var delivered: set[int];
  start state Watching {
    on eDelivered do (c: tChange) {
      assert !(c.id in delivered), format("change {0} = {1} delivered twice although the relay never crashed", c.id, c.value);
      delivered += (c.id);
    }
  }
}
