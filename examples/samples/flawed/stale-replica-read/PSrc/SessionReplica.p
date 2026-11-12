// FIXED: session tokens. A session remembers the highest LSN it has seen: from its own
// write acknowledgements and from every read. Each read carries that token, and a replica
// answers only once it has applied at least that LSN; until then it holds the read (it
// could instead forward it to the primary). Reads still go to any replica.

event eReadAtLeast: (client: machine, session: int, minLsn: int);
event eReadAtResp: (value: int, lsn: int);

machine SessionReplica {
  var value: int;
  var applied: int;
  // Reads waiting for replication to reach their token.
  var waiting: seq[(client: machine, session: int, minLsn: int)];

  start state Serving {
    on eReplicate do (r: (lsn: int, value: int)) {
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
    on eReadAtLeast do (r: (client: machine, session: int, minLsn: int)) {
      if (r.minLsn <= applied) {
        Serve(r.client, r.session);
      } else {
        waiting += (sizeof(waiting), r);
      }
    }
  }

  fun Serve(client: machine, session: int) {
    announce eReadServed, (session = session, value = value);
    send client, eReadAtResp, (value = value, lsn = applied);
  }
}

// Like Session, plus the token. `route` is the replica for each read, as before.
machine TokenSession {
  start state Run {
    entry (p: (session: int, primary: machine, route: seq[machine], ops: seq[tOp], firstValue: int)) {
      var op: tOp;
      var next: int;
      var reads: int;
      var token: int;
      next = p.firstValue;
      foreach (op in p.ops) {
        if (op == Write) {
          send p.primary, eWrite, (client = this, session = p.session, value = next);
          // A write's LSN is above every LSN committed before it, so above the token.
          receive { case eWriteAck: (lsn: int) { token = lsn; } }
          next = next + 1;
        } else {
          send p.route[reads], eReadAtLeast, (client = this, session = p.session, minLsn = token);
          reads = reads + 1;
          // The replica answered at or past the token, so this never moves it back.
          receive { case eReadAtResp: (r: (value: int, lsn: int)) { token = r.lsn; } }
        }
      }
    }
  }
}
