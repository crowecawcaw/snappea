// Negative control: a receiver that trusts the network to deliver each message once.
// A duplicated packet, or a retransmission after a lost acknowledgement, is handed to
// the application twice. tcBugNoDedup shows the spec catching it.
machine NoDedupReceiver {
  var net: Network;

  start state Receiving {
    entry (n: Network) {
      net = n;
    }
    on Data do (d: DataMessage) {
      // BUG: no check whether d.id was already delivered.
      announce Delivered, d.id;
      send net, NetSend, (destination = d.from, message = Ack, payload = d.id);
    }
  }
}
