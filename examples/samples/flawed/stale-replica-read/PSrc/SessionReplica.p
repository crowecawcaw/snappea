// FIXED: session tokens. A session remembers the highest LSN it has seen: from its own
// write acknowledgements and from every read. Each read carries that token, and a replica
// answers only once it has applied at least that LSN; until then it holds the read (it
// could instead forward it to the primary). Reads still go to any replica.

event ReadAtLeast: (client: machine, session: int, minLsn: int);
event ReadAtResp: (value: int, lsn: int);

machine SessionReplica {
  var value: int;
  var applied: int;
  // Reads waiting for replication to reach their token.
  var waiting: seq[(client: machine, session: int, minLsn: int)];

  start state Serving {
    on Replicate do (r: (lsn: int, value: int)) {
      var i: int;
      var w: (client: machine, session: int, minLsn: int);
      value = r.value;
      applied = r.lsn;
      while (i < sizeof(waiting)) {
        if (waiting[i].minLsn <= applied) {
          w = waiting[i];
          waiting -= (i);
          Serve(w.client, w.session);
        } else {
          i = i + 1;
        }
      }
    }
    on ReadAtLeast do (r: (client: machine, session: int, minLsn: int)) {
      if (r.minLsn <= applied) {
        Serve(r.client, r.session);
      } else {
        waiting += (sizeof(waiting), r);
      }
    }
  }

  fun Serve(client: machine, session: int) {
    announce ReadServed, (session = session, value = value);
    send client, ReadAtResp, (value = value, lsn = applied);
  }
}

// Like Session, plus the token. `route` is the replica for each read, as before.
machine TokenSession {
  start state Run {
    entry (p: (session: int, primary: machine, route: seq[machine], ops: seq[Operation], firstValue: int)) {
      var op: Operation;
      var next: int;
      var reads: int;
      var token: int;
      next = p.firstValue;
      foreach (op in p.ops) {
        if (op == Write) {
          send p.primary, WriteRequest, (client = this, session = p.session, value = next);
          // A write's LSN is above every LSN committed before it, so above the token.
          receive { case WriteAck: (lsn: int) { token = lsn; } }
          // snappea-lint: allow mutation:swap-operator -- the values only need to be distinct; counting down works as well
          next = next + 1;
        } else {
          send p.route[reads], ReadAtLeast, (client = this, session = p.session, minLsn = token);
          // snappea-lint: allow mutation:remove-statement,swap-operator -- only steers which replica the next read uses; any routing is allowed, so the contract cannot tell
          reads = reads + 1;
          // The replica answered at or past the token, so this never moves it back.
          // SessionGuarantees checks the returned value at ReadServed; subsequent
          // routing depends only on its LSN, not application work on that value.
          receive { case ReadAtResp: (r: (value: int, lsn: int)) { token = r.lsn; } }
        }
      }
    }
  }
}
