// Negative controls, kept apart so their code is not part of the passing tests.

// Dual write: commit the order, then publish it directly, with no outbox. A crash
// between the commit and the publish loses the event for good.
// tcBugDualWrite shows the progress spec catching the lost order.
event eDualWriteOrder: (id: int, crashesBeforePublish: bool);
machine DualWriteOrderService {
  var broker: machine;
  start state Serving {
    entry (b: machine) { broker = b; }
    on eDualWriteOrder do (o: (id: int, crashesBeforePublish: bool)) {
      announce eOrderCommitted, o.id;
      // BUG: nothing durable says this order still has to be published.
      if (o.crashesBeforePublish) { goto Crashed; }
      send broker, ePublish, (relay = this, id = o.id);
    }
    ignore eAck;
  }
  state Crashed {
    ignore eDualWriteOrder, eAck;
  }
}

// A consumer that trusts the broker to deliver exactly once.
// tcBugNoDedup shows the safety spec catching the double application.
machine NaiveConsumer {
  start state Consuming {
    on eDeliver do (id: int) {
      // BUG: no check whether this id was already applied.
      announce eApplied, id;
    }
  }
}
