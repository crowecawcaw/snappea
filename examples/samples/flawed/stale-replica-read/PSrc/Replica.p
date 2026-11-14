// FLAWED: reads go to whichever replica the load balancer picks, and the replica answers
// from whatever it has applied. A session that just wrote can read from a replica the
// write has not reached (its own write is missing), and two reads in a row can hit a
// caught-up replica and then a lagging one (time goes backwards).

event ReadRequest: (client: machine, session: int);
event ReadResp: int;

machine Replica {
  var value: int;

  start state Serving {
    // Deliberately flawed: this replica exposes its current value without a
    // session token. Replication arrives in primary order, but may lag reads.
    on Replicate do (r: (lsn: int, value: int)) { value = r.value; }
    on ReadRequest do (r: (client: machine, session: int)) {
      announce ReadServed, (session = r.session, value = value);
      send r.client, ReadResp, value;
    }
  }
}

// Runs its operations in order. `route` is the replica the load balancer sends each of
// its reads to (the test driver plays the load balancer).
machine Session {
  start state Run {
    entry (p: (session: int, primary: machine, route: seq[machine], ops: seq[Operation], firstValue: int)) {
      var op: Operation;
      var next: int;
      var reads: int;
      next = p.firstValue;
      foreach (op in p.ops) {
        if (op == Write) {
          send p.primary, WriteRequest, (client = this, session = p.session, value = next);
          receive { case WriteAck: (lsn: int) { } }
          next = next + 1;
        } else {
          send p.route[reads], ReadRequest, (client = this, session = p.session);
          reads = reads + 1;
          receive { case ReadResp: (v: int) { } }
        }
      }
    }
  }
}
