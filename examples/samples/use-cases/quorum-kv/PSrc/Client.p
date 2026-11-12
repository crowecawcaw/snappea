// The client library: quorum reads and versioned quorum writes. A client contacts a
// quorum of the replicas, chosen by the network (whichever replicas answer first), so
// the replicas it reaches differ from call to call.

// Tells the driver that a client has finished all its operations.
event eClientDone;

// Quorum sizes, from configuration: N = 3 replicas, `r` replicas per read, `w` per write.
type tQuorums = (r: int, w: int);

// The `size` replicas that answer this call: consecutive from a starting point the
// network picks, so every quorum of that size is possible.
fun PickQuorum(replicas: seq[machine], size: int): seq[machine] {
  var q: seq[machine];
  var first: int;
  first = choose(sizeof(replicas));
  while (sizeof(q) < size) {
    q += (sizeof(q), replicas[(first + sizeof(q)) % sizeof(replicas)]);
  }
  return q;
}

// Reads a quorum; returns the (version, value) with the highest version among them.
fun QuorumRead(caller: machine, replicas: seq[machine], r: int): (version: int, value: int) {
  var q: seq[machine];
  var rep: machine;
  var best: (version: int, value: int);
  q = PickQuorum(replicas, r);
  foreach (rep in q) {
    send rep, eReplicaRead, (client = caller,);
  }
  foreach (rep in q) {
    receive {
      case eReplicaReadResp: (resp: (version: int, value: int)) {
        if (resp.version > best.version) { best = resp; }
      }
    }
  }
  return best;
}

machine Writer {
  start state Writing {
    entry (input: (owner: machine, replicas: seq[machine], quorums: tQuorums, id: int, writes: seq[int])) {
      var v: int;
      var seen: (version: int, value: int);
      var version: int;
      var q: seq[machine];
      var rep: machine;
      foreach (v in input.writes) {
        announce eWriteStarted, (value = v,);
        // Read phase: the next counter is above every version in the read quorum.
        seen = QuorumRead(this, input.replicas, input.quorums.r);
        version = (seen.version / 10 + 1) * 10 + input.id;
        q = PickQuorum(input.replicas, input.quorums.w);
        foreach (rep in q) {
          send rep, eReplicaWrite, (client = this, version = version, value = v);
        }
        foreach (rep in q) {
          receive { case eReplicaWriteResp: { } }
        }
        announce eWriteDone, (value = v,);
      }
      send input.owner, eClientDone;
    }
  }
}

machine Reader {
  start state Reading {
    entry (input: (owner: machine, replicas: seq[machine], quorums: tQuorums, id: int, reads: int)) {
      var i: int;
      var got: (version: int, value: int);
      while (i < input.reads) {
        announce eReadStarted, (reader = input.id,);
        got = QuorumRead(this, input.replicas, input.quorums.r);
        announce eReadDone, (reader = input.id, value = got.value);
        i = i + 1;
      }
      send input.owner, eClientDone;
    }
  }
}
