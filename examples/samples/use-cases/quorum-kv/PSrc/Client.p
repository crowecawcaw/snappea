// The client library: quorum reads and versioned quorum writes. Both contact all
// N replicas and use the first `r` (read) or `w` (write) answers; the others are still
// in flight and land whenever the network delivers them (Deliver, in PTst/Network.p,
// can hold a write back past later messages).

// Tells the driver that a client has finished its operation. Each Writer and Reader
// machine performs one operation.
event ClientDone;

// Quorum sizes, from configuration: N = 3 replicas, `r` replicas per read, `w` per write.
type Quorums = (r: int, w: int);

// Reads every replica and uses the first `r` answers; returns the (version, value)
// with the highest version among them. Later answers are ignored.
fun QuorumRead(caller: machine, replicas: seq[machine], r: int): (version: int, value: int) {
  var rep: machine;
  var best: (version: int, value: int);
  var answers: int;
  foreach (rep in replicas) {
    send rep, ReplicaRead, (client = caller,);
  }
  while (answers < r) {
    receive {
      case ReplicaReadResp: (resp: (version: int, value: int)) {
        // snappea-lint: allow mutation:swap-operator -- equal versions name the same write, so they carry the same value
        if (resp.version > best.version) { best = resp; }
        answers = answers + 1;
      }
    }
  }
  return best;
}

// Sends the write to every replica and returns after `w` acks.
fun QuorumWrite(caller: machine, replicas: seq[machine], w: int, version: int, value: int) {
  var rep: machine;
  var acks: int;
  foreach (rep in replicas) {
    Deliver(rep, (client = caller, version = version, value = value));
  }
  while (acks < w) {
    // A writer machine performs one write, so every ack is for this write. A client
    // that writes again must tag acks (and read answers) with an operation id.
    receive { case ReplicaWriteResp: { acks = acks + 1; } }
  }
}

machine Writer {
  start state Writing {
    entry (input: (owner: machine, replicas: seq[machine], quorums: Quorums, id: int, value: int)) {
      var seen: (version: int, value: int);
      var version: int;
      announce WriteStarted, (value = input.value,);
      // Read phase: the next counter is above every version in the read quorum.
      seen = QuorumRead(this, input.replicas, input.quorums.r);
      version = (seen.version / 10 + 1) * 10 + input.id;
      QuorumWrite(this, input.replicas, input.quorums.w, version, input.value);
      announce WriteDone, (value = input.value,);
      send input.owner, ClientDone;
    }
    // Answers from the replicas that answered after the quorum.
    ignore ReplicaReadResp, ReplicaWriteResp;
  }
}

machine Reader {
  start state Reading {
    entry (input: (owner: machine, replicas: seq[machine], quorums: Quorums, id: int)) {
      var got: (version: int, value: int);
      announce ReadStarted, (reader = input.id,);
      got = QuorumRead(this, input.replicas, input.quorums.r);
      announce ReadDone, (reader = input.id, value = got.value);
      send input.owner, ClientDone;
    }
    // Answers from the replicas that answered after the quorum.
    ignore ReplicaReadResp;
  }
}
