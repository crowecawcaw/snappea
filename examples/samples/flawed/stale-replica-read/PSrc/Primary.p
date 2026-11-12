// A primary with asynchronous replicas, for one register. The primary commits each
// write with the next log sequence number (LSN), acknowledges it, and ships it to every
// replica; replicas apply the log in order, each at its own pace. Clients read from a
// replica the load balancer picks.
//
// Real atomic primitives: the primary's commit; a replica applying one log record; a
// replica serving one read. Replication is asynchronous: shipping races with reads.

event eWrite: (client: machine, session: int, value: int);
event eWriteAck: int;
event eReplicate: (lsn: int, value: int);

machine Primary {
  var replicas: seq[machine];
  var lsn: int;

  start state Serving {
    entry (r: seq[machine]) { replicas = r; }
    on eWrite do (w: (client: machine, session: int, value: int)) {
      var replica: machine;
      lsn = lsn + 1;
      announce eWritten, (session = w.session, value = w.value);
      send w.client, eWriteAck, lsn;
      foreach (replica in replicas) {
        send replica, eReplicate, (lsn = lsn, value = w.value);
      }
    }
  }
}
