// A sender and a receiver that make an unreliable network reliable: the sender sends one
// message at a time and retransmits it until it is acknowledged; the receiver deduplicates by message
// id and acknowledges every copy, so a lost acknowledgement is repaired too.

// A data message carries its return address; an acknowledgement needs only the id.
type Msg = (from: machine, id: int);
event Data: Msg;
event Ack: int;

machine Sender {
  var net: Network;
  var receiver: machine;
  // Stop and wait: send one message, wait for its acknowledgement, then the next.
  var queue: seq[int];

  start state Sending {
    entry (input: (net: Network, receiver: machine, ids: seq[int])) {
      net = input.net;
      receiver = input.receiver;
      queue = input.ids;
      // Ask the network for retransmission timeouts.
      send net, SetTimer, this;
      SendHead();
    }
    on Ack do (id: int) {
      // A duplicate or late acknowledgement for a message already acknowledged.
      if (id != queue[0]) { return; }
      announce Acked, id;
      queue -= (0);
      if (sizeof(queue) == 0) { goto Done; }
      SendHead();
    }
    // The retransmission timeout.
    on Timeout do { SendHead(); }
  }

  state Done {
    ignore Ack, Timeout;
  }

  fun SendHead() {
    send net, NetSend, (dest = receiver, ev = Data, msg = (from = this, id = queue[0]));
  }
}

machine Receiver {
  var net: Network;
  var delivered: set[int];

  start state Receiving {
    entry (n: Network) {
      net = n;
    }
    on Data do (d: Msg) {
      if (!(d.id in delivered)) {
        delivered += (d.id);
        announce Delivered, d.id;
      }
      // Acknowledge every copy: the first acknowledgement may have been lost.
      send net, NetSend, (dest = d.from, ev = Ack, msg = d.id);
    }
  }
}
