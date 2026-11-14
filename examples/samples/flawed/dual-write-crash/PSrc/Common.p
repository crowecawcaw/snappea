// The parts both designs share: the client-facing events, and the broker with its
// downstream consumer.

event Update: (client: machine, change: Change);
event UpdateResp: int;
// The connection dropped because the service crashed mid-request: the client cannot
// tell whether the change committed.
event Reset: int;
// From the fault injector (a test driver): the process restarts now, losing everything
// it held in memory. The database and the broker keep what they stored.
event Crash;

event Publish: (from: machine, epoch: int, change: Change);
event PublishAck: (epoch: int, id: int);

// The broker and its one downstream consumer, as one machine: the broker is durable and
// delivers every accepted message to the consumer in order, so the hop between them
// adds nothing to this model. The broker acknowledges once it has accepted a message.
// The consumer skips changes it already applied (by change id).
machine EventStream {
  var seen: set[int];

  start state Accepting {
    on Publish do (p: (from: machine, epoch: int, change: Change)) {
      announce Delivered, p.change;
      send p.from, PublishAck, (epoch = p.epoch, id = p.change.id);
      if (!(p.change.id in seen)) {
        seen += (p.change.id);
        announce Applied, p.change;
      }
    }
  }
}
