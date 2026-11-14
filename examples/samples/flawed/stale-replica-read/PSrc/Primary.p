// A primary with asynchronous replicas, for one register. The primary commits each
// write with the next log sequence number (LSN), acknowledges it, and ships it to every
// replica; replicas apply the log in order, each at its own pace. Clients read from a
// replica the load balancer picks.
//
// Real atomic primitives: the primary's commit; a replica applying one log record; a
// replica serving one read. Replication is asynchronous: each replica's log stream is a
// LogShipper, a link that delivers records in order but with any delay, so reads race
// with it.

event WriteRequest: (client: machine, session: int, value: int);
event WriteAck: int;
event Replicate: (lsn: int, value: int);

machine Primary {
  // One log shipper per replica.
  var replicas: seq[machine];
  var lsn: int;

  start state Serving {
    entry (r: seq[machine]) { replicas = r; }
    on WriteRequest do (w: (client: machine, session: int, value: int)) {
      var replica: machine;
      lsn = lsn + 1;
      announce Written, (session = w.session, value = w.value);
      send w.client, WriteAck, lsn;
      foreach (replica in replicas) {
        send replica, Replicate, (lsn = lsn, value = w.value);
      }
    }
  }
}

// The replication link to one replica: in order, any delay.
machine LogShipper {
  var replica: machine;

  start state Shipping {
    entry (r: machine) { replica = r; }
    on Replicate do (r: (lsn: int, value: int)) { send replica, Replicate, r; }
  }
}
