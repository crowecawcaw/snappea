// Drivers: clients and the fault injector. The service, relay, database, broker and
// downstream consumer are the design under test.

// A client sends its updates one at a time and announces each request and answer. It
// does not retry after a reset: the user saw an error, while the change may have
// committed.
machine Client {
  start state Run {
    entry (p: (service: machine, changes: seq[tChange])) {
      var c: tChange;
      foreach (c in p.changes) {
        announce eUpdateRequested, c;
        send p.service, eUpdate, (client = this, change = c);
        receive {
          case eUpdateResp: (id: int) { announce eUpdateAnswered, (id = id, ok = true); }
          case eReset: (id: int) { announce eUpdateAnswered, (id = id, ok = false); }
        }
      }
    }
  }
}

// Crashes the target once, at a moment the checker picks, or not at all.
machine Crasher {
  start state Run {
    entry (target: machine) {
      if ($) { send target, eCrash; }
    }
  }
}

fun Changes(first: int, n: int) : seq[tChange] {
  var s: seq[tChange];
  while (sizeof(s) < n) {
    s += (sizeof(s), (id = first + sizeof(s), value = 100 + first + sizeof(s)));
  }
  return s;
}

// One update; the service may crash at any point, including between the commit and the
// publish.
machine DualWriteCrash {
  start state Init {
    entry {
      var service: machine;
      announce eUpdatesPlanned, 1;
      service = new DualWriteService((db = new OrdersDb(), broker = new EventStream()));
      new Client((service = service, changes = Changes(1, 1)));
      new Crasher(service);
    }
  }
}

// The outbox design with the same fault as tcBugCrashBetweenWriteAndPublish: one
// update, and the service may crash at any point, including right after its commit.
machine OutboxServiceCrash {
  start state Init {
    entry {
      var db: machine;
      var service: machine;
      announce eUpdatesPlanned, 1;
      db = new OutboxDb();
      new OutboxRelay((db = db, broker = new EventStream()));
      service = new OutboxService(db);
      new Client((service = service, changes = Changes(1, 1)));
      new Crasher(service);
    }
  }
}

// A restarted service must ignore the result of a commit sent before the crash, or it
// confirms the next update before that one commits. CrashAfterCommit crashes the service
// right after each commit; the client sends two updates. Only the service's answers are
// checked here (no relay): publication is covered by the other tests.
machine StaleCommitResult {
  start state Init {
    entry {
      var service: machine;
      announce eUpdatesPlanned, 2;
      service = new OutboxService(new CrashAfterCommit(new OutboxDb()));
      new Client((service = service, changes = Changes(1, 2)));
    }
  }
}

// Forwards commits to the database. When the result comes back it first crashes the
// service, then passes the result on, which now reaches a restarted process.
machine CrashAfterCommit {
  var db: machine;
  var service: machine;

  start state Forwarding {
    entry (d: machine) { db = d; }
    on eCommitWithOutbox do (r: (from: machine, epoch: int, change: tChange)) {
      service = r.from;
      send db, eCommitWithOutbox, (from = this, epoch = r.epoch, change = r.change);
    }
    on eCommitResult do (e: int) {
      send service, eCrash;
      send service, eCommitResult, e;
    }
  }
}

// Two updates, no faults: commits, notifications and relay reads interleave, and the
// relay may find one or two unpublished rows per read.
machine OutboxTwoUpdates {
  start state Init {
    entry {
      var db: machine;
      var service: machine;
      announce eUpdatesPlanned, 2;
      db = new OutboxDb();
      new OutboxRelay((db = db, broker = new EventStream()));
      service = new OutboxService(db);
      new Client((service = service, changes = Changes(1, 2)));
    }
  }
}

// One update, and the relay may crash at any point: between reading and publishing,
// or after the broker accepted a row and before the row was marked, so a change is
// delivered twice and the consumer must skip the copy.
machine OutboxRelayCrash {
  start state Init {
    entry {
      var db: machine;
      var service: machine;
      var relay: machine;
      announce eUpdatesPlanned, 1;
      db = new OutboxDb();
      relay = new OutboxRelay((db = db, broker = new EventStream()));
      service = new OutboxService(db);
      new Client((service = service, changes = Changes(1, 1)));
      new Crasher(relay);
    }
  }
}

// Covered: the broker accepted the same change twice (the relay crashed after the
// broker accepted it and before marking it).
scenario DeliveredTwice observes eDelivered {
  var seen: set[int];
  start hot state WaitDuplicate {
    on eDelivered do (c: tChange) {
      if (c.id in seen) { goto Covered; }
      seen += (c.id);
    }
  }
  cold state Covered {
    ignore eDelivered;
  }
}

test tcFixedServiceCrash [main=OutboxServiceCrash]:
  assert CommittedChangesPublished, UpdatesAnswered, DeliveredOnceWithoutRelayCrash in
  { OutboxServiceCrash, OutboxDb, OutboxService, OutboxRelay, EventStream, Client, Crasher };

test tcFixedStaleCommitResult [main=StaleCommitResult]:
  assert UpdatesAnswered in { StaleCommitResult, CrashAfterCommit, OutboxDb, OutboxService, Client };

test tcFixedTwoUpdates [main=OutboxTwoUpdates]:
  assert CommittedChangesPublished, UpdatesAnswered, DeliveredOnceWithoutRelayCrash in
  { OutboxTwoUpdates, OutboxDb, OutboxService, OutboxRelay, EventStream, Client };

test tcFixedRelayCrash [main=OutboxRelayCrash]:
  assert CommittedChangesPublished, UpdatesAnswered in
  { OutboxRelayCrash, OutboxDb, OutboxService, OutboxRelay, EventStream, Client, Crasher };

// expect-bug: CommittedNotPublished
test tcBugCrashBetweenWriteAndPublish [main=DualWriteCrash]:
  assert CommittedChangesPublished, UpdatesAnswered in
  { DualWriteCrash, OrdersDb, DualWriteService, EventStream, Client, Crasher };
