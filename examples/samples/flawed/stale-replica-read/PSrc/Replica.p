// FLAWED: reads go to whichever replica the load balancer picks, and the replica answers
// from whatever it has applied. A session that just wrote can read from a replica the
// write has not reached (its own write is missing), and two reads in a row can hit a
// caught-up replica and then a lagging one (time goes backwards).

event eRead: (client: machine, session: int);
event eReadResp: int;

machine Replica {
  var value: int;

  start state Serving {
    on eReplicate do (r: (lsn: int, value: int)) { value = r.value; }
    on eRead do (r: (client: machine, session: int)) {
      announce eReadServed, (session = r.session, value = value);
      send r.client, eReadResp, value;
    }
  }
}

// Runs its operations in order. `route` is the replica the load balancer sends each of
// its reads to (the test driver plays the load balancer).
machine Session {
  start state Run {
    entry (p: (session: int, primary: machine, route: seq[machine], ops: seq[tOp], firstValue: int)) {
      var op: tOp;
      var next: int;
      var reads: int;
      next = p.firstValue;
      foreach (op in p.ops) {
        if (op == Write) {
          send p.primary, eWrite, (client = this, session = p.session, value = next);
          receive { case eWriteAck: (lsn: int) { } }
          next = next + 1;
        } else {
          send p.route[reads], eRead, (client = this, session = p.session);
          reads = reads + 1;
          receive { case eReadResp: (v: int) { } }
        }
      }
    }
  }
}
