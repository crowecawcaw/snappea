// Exactly-once delivery over a network that loses, duplicates and reorders: every
// message the driver hands to the sender is delivered to the receiver's application
// exactly once, and the sender learns that each one arrived.

// Announced by the driver before the sender starts: the message ids to deliver.
event MessagesPlanned: set[int];
// Announced by the receiver when it hands a message to the application.
event Delivered: int;
// Announced by the sender when it accepts the acknowledgement for a message.
event Acked: int;

spec ExactlyOnce observes MessagesPlanned, Delivered, Acked {
  var planned: set[int];
  var delivered: set[int];
  var acked: set[int];

  start state Idle {
    on MessagesPlanned goto Delivering with (ids: set[int]) {
      assert sizeof(ids) > 0, "planned no messages";
      planned = ids;
    }
    on Delivered do { assert false, "delivery before any message was planned"; }
    on Acked do { assert false, "acknowledgement before any message was planned"; }
  }

  // Hot until every planned message is delivered and acknowledged.
  hot state Delivering {
    on MessagesPlanned do { assert false, "messages planned twice"; }
    on Delivered do (id: int) {
      assert id in planned, format("message {0} delivered but never sent", id);
      assert !(id in delivered), format("message {0} delivered twice", id);
      delivered += (id);
    }
    on Acked do (id: int) {
      assert id in delivered, format("message {0} acknowledged before it was delivered", id);
      assert !(id in acked), format("message {0} acknowledged twice", id);
      acked += (id);
      if (sizeof(acked) == sizeof(planned)) { goto AllDelivered; }
    }
  }

  state AllDelivered {
    on MessagesPlanned do { assert false, "messages planned after all were delivered"; }
    on Delivered do { assert false, "delivery after all messages were acknowledged"; }
    on Acked do { assert false, "acknowledgement after all messages were acknowledged"; }
  }
}
