// The environment: the broker (at-least-once delivery) and the drivers, which place
// orders and play the process supervisor that restarts a crashed relay.

machine Broker {
  var consumer: machine;
  start state Serving {
    entry (c: machine) { consumer = c; }
    on Publish do (p: (relay: machine, id: int)) {
      announce BrokerReceived, p.id;
      send consumer, Deliver, p.id;
      send p.relay, Ack, p.id;
    }
  }
}

// Place orders 1..n; `commits[i]` says whether order i+1's transaction commits.
// `crashes` is how many relay crashes the test injects.
fun PlaceOrders(db: OrderDb, commits: seq[bool], crashes: int) {
  var i: int;
  announce OrdersPlanned, (orders = sizeof(commits), crashes = crashes);
  while (i < sizeof(commits)) {
    announce OrderPlaced, (id = i + 1, commits = commits[i]);
    send db, PlaceOrder, (id = i + 1, commits = commits[i]);
    i = i + 1;
  }
}

fun NewRelay(db: OrderDb, broker: machine, supervisor: machine, crashes: bool): Relay {
  return new Relay((db = db, broker = broker, supervisor = supervisor, crashAfterPublish = crashes));
}

// Two relays run at once (a rolling deploy, or a restart while the old process is
// still alive). Order 1 is committed first, so the search spends its budget on the
// relays: both can fetch the row and publish it, so the consumer sees a duplicate, and
// either relay can mark it while the other is still publishing. (Commits racing with a
// relay are covered by the other tests; a second row multiplies the schedules
// without a new race.)
machine TwoRelays {
  start state Run {
    entry (consumer: machine) {
      var db: OrderDb;
      var broker: Broker;
      var commits: seq[bool];
      db = new OrderDb();
      broker = new Broker(consumer);
      commits += (0, true);
      PlaceOrders(db, commits, 0);
      NewRelay(db, broker, this, false);
      NewRelay(db, broker, this, false);
    }
  }
}

// One relay crashes after the broker acked a message and before it marked the row.
// The supervisor starts a new relay, which publishes the row again from the table.
// Order 2 aborts and must never be published.
machine RelayCrashAfterPublish {
  var db: OrderDb;
  var broker: Broker;
  start state Run {
    entry {
      var commits: seq[bool];
      db = new OrderDb();
      broker = new Broker(new Consumer());
      NewRelay(db, broker, this, true);
      commits += (0, true);
      commits += (1, false);
      PlaceOrders(db, commits, 1);
    }
    on RelayCrashed do { NewRelay(db, broker, this, false); }
  }
}

// The relay may find the outbox empty and go to sleep; each commit must wake it. The
// second order can commit while the relay is still publishing the first, so after a
// batch the relay must fetch again rather than wait for a wake-up it will not get.
machine OrdersWhileRelayRuns {
  start state Run {
    entry {
      var db: OrderDb;
      var commits: seq[bool];
      db = new OrderDb();
      NewRelay(db, new Broker(new Consumer()), this, false);
      commits += (0, true);
      commits += (1, true);
      PlaceOrders(db, commits, 0);
    }
  }
}

machine MainTwoRelays { start state Init { entry { new TwoRelays(new Consumer()); } } }

machine BugDualWrite {
  start state Run {
    entry {
      var service: DualWriteOrderService;
      service = new DualWriteOrderService(new Broker(new Consumer()));
      announce OrdersPlanned, (orders = 1, crashes = 0);
      announce OrderPlaced, (id = 1, commits = true);
      send service, DualWriteOrder, (id = 1, crashesBeforePublish = $);
    }
  }
}

machine MainBugNoDedup { start state Init { entry { new TwoRelays(new NaiveConsumer()); } } }

module Outbox = { OrderDb, Relay, Consumer, Broker };

test tcTwoRelays [main=MainTwoRelays]:
  assert AppliedOnce, CommittedOrdersDelivered, OutboxDrains in (union Outbox, { MainTwoRelays, TwoRelays });

test tcRelayCrashAfterPublish [main=RelayCrashAfterPublish]:
  assert AppliedOnce, CommittedOrdersDelivered, OutboxDrains in (union Outbox, { RelayCrashAfterPublish });

test tcOrdersWhileRelayRuns [main=OrdersWhileRelayRuns]:
  assert AppliedOnce, CommittedOrdersDelivered, OutboxDrains in (union Outbox, { OrdersWhileRelayRuns });

// snappea: expect-bug "CommittedOrdersDelivered detected liveness bug in hot state 'Undelivered'"
test tcBugDualWrite [main=BugDualWrite]:
  assert AppliedOnce, CommittedOrdersDelivered in { BugDualWrite, DualWriteOrderService, Broker, Consumer };

// snappea: expect-bug "order 1 applied twice"
test tcBugNoDedup [main=MainBugNoDedup]:
  assert AppliedOnce, CommittedOrdersDelivered, OutboxDrains in
  { MainBugNoDedup, TwoRelays, OrderDb, Relay, NaiveConsumer, Broker };
