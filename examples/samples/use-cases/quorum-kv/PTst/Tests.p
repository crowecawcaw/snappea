// Drivers. Writers and readers run the client library, so they are design code.

// Three replicas; with `down`, the third never answers (crashed for good).
fun ThreeReplicas(down: bool): seq[machine] {
  var rs: seq[machine];
  rs += (0, new Replica());
  rs += (1, new Replica());
  if (down) { rs += (2, new Unreachable()); } else { rs += (2, new Replica()); }
  return rs;
}

machine Unreachable {
  start state Down {
    ignore eReplicaRead, eReplicaWrite;
  }
}

// Concurrent clients: one writer per value in `writes`, and one reader, all started
// at once.
type tRun = (quorums: tQuorums, writes: seq[int]);

machine Clients {
  start state Running {
    entry (run: tRun) {
      var replicas: seq[machine];
      var i: int;
      replicas = ThreeReplicas(false);
      announce eOpsPlanned, sizeof(run.writes) + 1;
      while (i < sizeof(run.writes)) {
        new Writer((owner = this, replicas = replicas, quorums = run.quorums, id = i + 1, value = run.writes[i]));
        i = i + 1;
      }
      new Reader((owner = this, replicas = replicas, quorums = run.quorums, id = 1));
    }
    ignore eClientDone;
  }
}

// The core race: one writer and one reader, concurrently. The read may start before,
// during or after the write, and touches any two replicas; if it starts after the
// write returned it must see it. Sampled: every order is several hundred thousand
// schedules; the sequential tests below explore their cases exhaustively.
machine WriteAndReads {
  start state Init {
    entry {
      var w: seq[int];
      w += (0, 1);
      new Clients((quorums = (r = 2, w = 2), writes = w));
    }
  }
}

// Two writers race on the same key while a reader reads. Both can read the same
// highest counter and pick the same counter; the writer id breaks the tie. Sampled.
machine TwoWriters {
  start state Init {
    entry {
      var w: seq[int];
      w += (0, 1);
      w += (1, 2);
      new Clients((quorums = (r = 2, w = 2), writes = w));
    }
  }
}

// One operation of a sequential run: a write of `value` by writer `id`, or a read
// (value 0) by reader `id`.
type tStep = (id: int, value: int);

// Runs operations one after another: each starts only after the previous returned.
// Every quorum choice is still explored, so a later operation can overlap an earlier
// write in exactly one replica.
machine Sequence {
  var replicas: seq[machine];
  var quorums: tQuorums;
  var steps: seq[tStep];
  var next: int;
  start state Init {
    entry (input: (quorums: tQuorums, steps: seq[tStep], down: bool)) {
      quorums = input.quorums;
      steps = input.steps;
      replicas = ThreeReplicas(input.down);
      announce eOpsPlanned, sizeof(steps);
      goto Run;
    }
  }
  state Run {
    entry {
      var st: tStep;
      st = steps[next];
      next = next + 1;
      if (st.value > 0) {
        new Writer((owner = this, replicas = replicas, quorums = quorums, id = st.id, value = st.value));
      } else {
        new Reader((owner = this, replicas = replicas, quorums = quorums, id = st.id));
      }
    }
    on eClientDone do {
      if (next < sizeof(steps)) { goto Run; }
    }
  }
}

fun Steps3(a: tStep, b: tStep, c: tStep): seq[tStep] {
  var s: seq[tStep];
  s += (0, a);
  s += (1, b);
  if (c.id > 0) { s += (2, c); }
  return s;
}

// Write, then read: the write returns after W = 2 acks with its third message
// possibly still in flight; the read uses the first 2 answers, which may overlap the
// acked replicas in exactly one. That replica must decide the answer.
machine WriteThenRead {
  start state Init {
    entry { new Sequence((quorums = (r = 2, w = 2), steps = Steps3((id = 1, value = 1), (id = 1, value = 0), (id = 0, value = 0)), down = false)); }
  }
}

// Write after a completed write, by another writer, then a read. The second writer's
// read phase must see the first version (quorums overlap) and pick a higher one, or
// replicas holding both keep the first value and the read returns it.
machine WriteWriteRead {
  start state Init {
    entry { new Sequence((quorums = (r = 2, w = 2), steps = Steps3((id = 1, value = 1), (id = 2, value = 2), (id = 1, value = 0)), down = false)); }
  }
}

// One replica is down for good: read, write, read must all complete with the two that
// answer; the first read sees the initial value and the last sees the write.
machine OneReplicaDown {
  start state Init {
    entry { new Sequence((quorums = (r = 2, w = 2), steps = Steps3((id = 1, value = 0), (id = 1, value = 1), (id = 1, value = 0)), down = true)); }
  }
}

// Negative control: write then read with R = 1, so R + W = N and the read can reach
// only the replica the write has not reached yet.
machine WriteThenReadR1 {
  start state Init {
    entry { new Sequence((quorums = (r = 1, w = 2), steps = Steps3((id = 1, value = 1), (id = 1, value = 0), (id = 0, value = 0)), down = false)); }
  }
}

test tcWriteAndReads [main=WriteAndReads]:
  assert ReadAfterWrite, OperationsReturn in { WriteAndReads, Clients, Replica, Unreachable, Writer, Reader, Courier };

test tcTwoWriters [main=TwoWriters]:
  assert ReadAfterWrite, OperationsReturn in { TwoWriters, Clients, Replica, Unreachable, Writer, Reader, Courier };

test tcWriteThenRead [main=WriteThenRead]:
  assert ReadAfterWrite, OperationsReturn in { WriteThenRead, Sequence, Replica, Unreachable, Writer, Reader, Courier };

test tcOneReplicaDown [main=OneReplicaDown]:
  assert ReadAfterWrite, OperationsReturn in { OneReplicaDown, Sequence, Replica, Unreachable, Writer, Reader, Courier };

test tcWriteWriteRead [main=WriteWriteRead]:
  assert ReadAfterWrite, OperationsReturn in { WriteWriteRead, Sequence, Replica, Unreachable, Writer, Reader, Courier };

// expect-bug: returned the initial value after write
test tcBugReadOne [main=WriteThenReadR1]:
  assert ReadAfterWrite, OperationsReturn in { WriteThenReadR1, Sequence, Replica, Unreachable, Writer, Reader, Courier };
