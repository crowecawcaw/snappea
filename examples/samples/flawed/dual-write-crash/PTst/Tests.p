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

// The outbox design with the same fault: the service may crash after its commit.
machine OutboxServiceCrash {
  start state Init {
    entry {
      var db: machine;
      var service: machine;
      announce eUpdatesPlanned, 2;
      db = new OutboxDb();
      new OutboxRelay((db = db, broker = new EventStream()));
      service = new OutboxService(db);
      new Client((service = service, changes = Changes(1, 2)));
      new Crasher(service);
    }
  }
}

// Two clients, and the relay may crash at any point: between reading and publishing,
// or after the broker accepted a row and before the row was marked, so a change is
// delivered twice and the consumer must skip the copy.
machine OutboxRelayCrash {
  start state Init {
    entry {
      var db: machine;
      var service: machine;
      var relay: machine;
      announce eUpdatesPlanned, 2;
      db = new OutboxDb();
      relay = new OutboxRelay((db = db, broker = new EventStream()));
      service = new OutboxService(db);
      new Client((service = service, changes = Changes(1, 1)));
      new Client((service = service, changes = Changes(2, 1)));
      new Crasher(relay);
    }
  }
}

// Covered: a change reached downstream twice and the copy was skipped.
scenario DuplicateSkipped observes eApplied, eDuplicateIgnored {
  start hot state WaitDuplicate {
    on eDuplicateIgnored goto Covered;
    ignore eApplied;
  }
  cold state Covered {
    ignore eApplied, eDuplicateIgnored;
  }
}

test tcFixedServiceCrash [main=OutboxServiceCrash]:
  assert CommittedChangesPublished, UpdatesAnswered in
  { OutboxServiceCrash, OutboxDb, OutboxService, OutboxRelay, EventStream, Client, Crasher };

test tcFixedRelayCrash [main=OutboxRelayCrash]:
  assert CommittedChangesPublished, UpdatesAnswered in
  { OutboxRelayCrash, OutboxDb, OutboxService, OutboxRelay, EventStream, Client, Crasher };

// expect-bug: CommittedNotPublished
test tcBugCrashBetweenWriteAndPublish [main=DualWriteCrash]:
  assert CommittedChangesPublished, UpdatesAnswered in
  { DualWriteCrash, OrdersDb, DualWriteService, EventStream, Client, Crasher };
