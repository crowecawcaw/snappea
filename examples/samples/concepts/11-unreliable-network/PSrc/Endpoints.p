// A sender and a receiver that make an unreliable network reliable: the sender sends one
// message at a time and retransmits it until it is acknowledged; the receiver deduplicates by message
// id and acknowledges every copy, so a lost acknowledgement is repaired too.

// Both carry the sending endpoint, so the receiver knows where to acknowledge.
type tMsg = (from: machine, id: int);
event eData: tMsg;
event eAck: tMsg;

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
      send net, eSetTimer, this;
      SendHead();
    }
    on eAck do (a: tMsg) {
      // A duplicate or late acknowledgement for a message already acknowledged.
      if (a.id != queue[0]) { return; }
      announce eAcked, a.id;
      queue -= (0);
      if (sizeof(queue) == 0) { goto Done; }
      SendHead();
    }
    // The retransmission timeout.
    on eTimeout do { SendHead(); }
  }

  state Done {
    ignore eAck, eTimeout;
  }

  fun SendHead() {
    send net, eNetSend, (dest = receiver, ev = eData, msg = (from = this, id = queue[0]));
  }
}

machine Receiver {
  var net: Network;
  var delivered: set[int];

  start state Receiving {
    entry (n: Network) {
      net = n;
    }
    on eData do (d: tMsg) {
      if (!(d.id in delivered)) {
        delivered += (d.id);
        announce eDelivered, d.id;
      }
      // Acknowledge every copy: the first acknowledgement may have been lost.
      send net, eNetSend, (dest = d.from, ev = eAck, msg = (from = this, id = d.id));
    }
  }
}
