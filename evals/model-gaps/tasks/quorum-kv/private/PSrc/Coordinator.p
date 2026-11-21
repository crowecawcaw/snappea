// The coordinator: the client library that runs puts and gets against the replicas.
// One operation at a time. Both kinds read a quorum, then write to a quorum: a put
// writes its value under a new version, a get writes back what it read.

machine Coordinator {
  var owner: machine;
  var id: int;
  var replicas: seq[machine];

  // Durable: survive a crash.
  // The number of the last operation started; never reused.
  var op: int;
  // The highest counter this coordinator ever put in a version.
  var used: int;

  // Volatile: the operation in flight.
  var isPut: bool;
  var value: int;
  // Read phase: the newest answer so far. Write phase: what is being written.
  var best: Stamped;
  // The replicas that answered this phase.
  var answered: set[int];
  var acked: set[int];

  start state Init {
    entry (input: CoordinatorInit) {
      owner = input.owner;
      id = input.id;
      replicas = input.replicas;
      goto Idle;
    }
  }

  state Idle {
    on Put do (v: int) {
      op = op + 1;
      isPut = true;
      value = v;
      announce WriteStarted, (client = id, value = v);
      goto Reading;
    }
    on Get do {
      op = op + 1;
      isPut = false;
      announce ReadStarted, (client = id,);
      goto Reading;
    }
    on Crash do { Recover(false); }
    // Late answers and timeouts of operations that are over.
    ignore ReplicaReadResp, ReplicaWriteResp, OpTimeout;
  }

  state Reading {
    entry {
      best = default(Stamped);
      answered = default(set[int]);
      SendReads();
    }
    on ReplicaReadResp do (r: (replica: int, op: int, held: Stamped)) {
      // An answer to an earlier operation, arriving late.
      if (r.op != op) { return; }
      answered += (r.replica);
      if (Newer(r.held.version, best.version)) { best = r.held; }
      if (sizeof(answered) < 2) { return; }
      if (isPut) {
        // Above everything the quorum holds and everything this coordinator wrote.
        if (best.version.counter > used) { used = best.version.counter; }
        used = used + 1;
        best = (version = (counter = used, writer = id), value = value);
      }
      goto Writing;
    }
    on OpTimeout do { SendReads(); }
    on Crash do {
      Recover(true);
      goto Idle;
    }
    ignore ReplicaWriteResp;
  }

  state Writing {
    entry {
      acked = default(set[int]);
      SendWrites();
    }
    on ReplicaWriteResp do (a: (replica: int, op: int)) {
      // An ack for an earlier operation, arriving late.
      if (a.op != op) { return; }
      acked += (a.replica);
      if (sizeof(acked) < 2) { return; }
      Finish();
      goto Idle;
    }
    // The same request again: same operation, same version.
    on OpTimeout do { SendWrites(); }
    on Crash do {
      Recover(true);
      goto Idle;
    }
    ignore ReplicaReadResp;
  }

  fun SendReads() {
    var rep: machine;
    foreach (rep in replicas) {
      send rep, ReplicaRead, (client = this, op = op);
    }
  }

  fun SendWrites() {
    var rep: machine;
    foreach (rep in replicas) {
      send rep, ReplicaWrite, (client = this, op = op, write = best);
    }
  }

  fun Finish() {
    if (isPut) {
      announce WriteDone, (client = id, value = value, version = best.version);
      send owner, PutDone, (client = id,);
    } else {
      announce ReadDone, (client = id, value = best.value, version = best.version);
      send owner, GetDone, (client = id, value = best.value);
    }
  }

  // Crash and restart: the operation in flight is gone.
  fun Recover(abandoned: bool) {
    // `op` and `used` are durable and keep their values.
    announce CrashRecovered, (client = id, abandoned = abandoned);
    send owner, Restarted, (client = id, abandoned = abandoned);
  }
}
