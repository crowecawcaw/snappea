// Negative controls, kept apart so their code is not part of the passing tests.

// Dual write: commit the order, then publish it directly, with no outbox. A crash
// between the commit and the publish loses the event for good.
// tcBugDualWrite shows the progress spec catching the lost order.
event DualWriteOrder: (id: int, crashesBeforePublish: bool);
machine DualWriteOrderService {
  var broker: machine;
  start state Serving {
    entry (b: machine) { broker = b; }
    on DualWriteOrder do (o: (id: int, crashesBeforePublish: bool)) {
      announce OrderCommitted, o.id;
      // BUG: nothing durable says this order still has to be published.
      if (o.crashesBeforePublish) { goto Crashed; }
      send broker, Publish, (relay = this, id = o.id);
    }
    ignore Ack;
  }
  state Crashed {
    ignore DualWriteOrder, Ack;
  }
}

// A consumer that trusts the broker to deliver exactly once.
// tcBugNoDedup shows the safety spec catching the double application.
machine NaiveConsumer {
  start state Consuming {
    on Deliver do (id: int) {
      // BUG: no check whether this id was already applied.
      announce Applied, id;
    }
  }
}
