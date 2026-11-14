// Exactly-once delivery over a network that loses, duplicates and reorders: every
// message the driver hands to the sender is delivered to the receiver's application
// exactly once, and the sender learns that each one arrived.

// Announced by the driver before the sender starts: the message ids to deliver.
event MessagesPlanned: set[int];
// Announced by the receiver when it hands a message to the application.
event Delivered: int;
// Announced by the sender when it accepts the acknowledgement for a message.
event Acked: int;

// Safety stays active after completion: a delayed duplicate is still a defect.
spec ExactlyOnce observes MessagesPlanned, Delivered, Acked {
  var planned: set[int];
  var delivered: set[int];
  var acked: set[int];

  start state Watching {
    on MessagesPlanned do (ids: set[int]) {
      assert sizeof(planned) == 0, "messages planned twice";
      assert sizeof(ids) > 0, "planned no messages";
      planned = ids;
    }
    on Delivered do (id: int) {
      assert id in planned, format("message {0} delivered but never sent", id);
      assert !(id in delivered), format("message {0} delivered twice", id);
      delivered += (id);
    }
    on Acked do (id: int) {
      assert id in delivered, format("message {0} acknowledged before it was delivered", id);
      assert !(id in acked), format("message {0} acknowledged twice", id);
      acked += (id);
    }
  }
}

// ExactlyOnce checks that acknowledgements are valid and distinct. This monitor
// checks progress: every planned message must eventually be acknowledged.
spec MessagesAcknowledged observes MessagesPlanned, Acked {
  var remaining: int;

  start state Idle {
    on MessagesPlanned goto Awaiting with (ids: set[int]) {
      remaining = sizeof(ids);
    }
    ignore Acked;
  }

  hot state Awaiting {
    on Acked do {
      remaining = remaining - 1;
      if (remaining == 0) { goto Done; }
    }
    ignore MessagesPlanned;
  }

  state Done {
    ignore MessagesPlanned, Acked;
  }
}
