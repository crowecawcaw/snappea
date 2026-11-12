// The parts both designs share: the client-facing events, and the broker with its
// downstream consumer.

event eUpdate: (client: machine, change: tChange);
event eUpdateResp: int;
// The connection dropped because the service crashed mid-request: the client cannot
// tell whether the change committed.
event eReset: int;
// From the fault injector (a test driver): the process restarts now, losing everything
// it held in memory. The database and the broker keep what they stored.
event eCrash;

event ePublish: (from: machine, epoch: int, change: tChange);
event ePublishAck: (epoch: int, id: int);

// The broker and its one downstream consumer, as one machine: the broker is durable and
// delivers every accepted message to the consumer in order, so the hop between them
// adds nothing to this model. The broker acknowledges once it has accepted a message.
// The consumer skips changes it already applied (by change id).
machine EventStream {
  var seen: set[int];

  start state Accepting {
    on ePublish do (p: (from: machine, epoch: int, change: tChange)) {
      send p.from, ePublishAck, (epoch = p.epoch, id = p.change.id);
      if (p.change.id in seen) {
        announce eDuplicateIgnored, p.change.id;
      } else {
        seen += (p.change.id);
        announce eApplied, p.change;
      }
    }
  }
}
