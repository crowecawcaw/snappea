// Exactly-once delivery over a network that loses, duplicates and reorders: every
// message the driver hands to the sender is delivered to the receiver's application
// exactly once, and the sender learns that each one arrived.

// Announced by the driver before the sender starts: the message ids to deliver.
event eMessagesPlanned: set[int];
// Announced by the receiver when it hands a message to the application.
event eDelivered: int;
// Announced by the sender when it accepts the acknowledgement for a message.
event eAcked: int;

spec ExactlyOnce observes eMessagesPlanned, eDelivered, eAcked {
  var planned: set[int];
  var delivered: set[int];
  var acked: set[int];

  start state Idle {
    on eMessagesPlanned goto Delivering with (ids: set[int]) {
      assert sizeof(ids) > 0, "planned no messages";
      planned = ids;
    }
    on eDelivered do { assert false, "delivery before any message was planned"; }
    on eAcked do { assert false, "acknowledgement before any message was planned"; }
  }

  // Hot until every planned message is delivered and acknowledged.
  hot state Delivering {
    on eMessagesPlanned do { assert false, "messages planned twice"; }
    on eDelivered do (id: int) {
      assert id in planned, format("message {0} delivered but never sent", id);
      assert !(id in delivered), format("message {0} delivered twice", id);
      delivered += (id);
    }
    on eAcked do (id: int) {
      assert id in delivered, format("message {0} acknowledged before it was delivered", id);
      assert !(id in acked), format("message {0} acknowledged twice", id);
      acked += (id);
      if (sizeof(acked) == sizeof(planned)) { goto AllDelivered; }
    }
  }

  state AllDelivered {
    on eMessagesPlanned do { assert false, "messages planned after all were delivered"; }
    on eDelivered do { assert false, "delivery after all messages were acknowledged"; }
    on eAcked do { assert false, "acknowledgement after all messages were acknowledged"; }
  }
}
