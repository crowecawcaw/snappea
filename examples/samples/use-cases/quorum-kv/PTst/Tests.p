// Drivers. Writers and readers run the client library, so they are design code.

fun ThreeReplicas(): seq[machine] {
  var rs: seq[machine];
  while (sizeof(rs) < 3) { rs += (sizeof(rs), new Replica()); }
  return rs;
}

fun Values(a: int, b: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  if (b > 0) { s += (1, b); }
  return s;
}

// Concurrent clients: writers (each writing its values) and one reader doing `reads`
// reads, all started at once.
type tRun = (quorums: tQuorums, writers: seq[seq[int]], reads: int);

machine Clients {
  start state Running {
    entry (run: tRun) {
      var replicas: seq[machine];
      var ops: int;
      var i: int;
      replicas = ThreeReplicas();
      ops = run.reads;
      while (i < sizeof(run.writers)) {
        ops = ops + sizeof(run.writers[i]);
        i = i + 1;
      }
      announce eOpsPlanned, ops;
      i = 0;
      while (i < sizeof(run.writers)) {
        new Writer((owner = this, replicas = replicas, quorums = run.quorums, id = i + 1, writes = run.writers[i]));
        i = i + 1;
      }
      new Reader((owner = this, replicas = replicas, quorums = run.quorums, id = 1, reads = run.reads));
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
      var w: seq[seq[int]];
      w += (0, Values(1, 0));
      new Clients((quorums = (r = 2, w = 2), writers = w, reads = 1));
    }
  }
}

// Two writers race on the same key while a reader reads. Both can read the same
// highest counter and pick the same counter; the writer id breaks the tie. Sampled.
machine TwoWriters {
  start state Init {
    entry {
      var w: seq[seq[int]];
      w += (0, Values(1, 0));
      w += (1, Values(2, 0));
      new Clients((quorums = (r = 2, w = 2), writers = w, reads = 2));
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
    entry (input: (quorums: tQuorums, steps: seq[tStep])) {
      quorums = input.quorums;
      steps = input.steps;
      replicas = ThreeReplicas();
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
        new Writer((owner = this, replicas = replicas, quorums = quorums, id = st.id, writes = Values(st.value, 0)));
      } else {
        new Reader((owner = this, replicas = replicas, quorums = quorums, id = st.id, reads = 1));
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

// Read after a completed write: the write reaches only W = 2 replicas, then the read
// touches 2 replicas, which may overlap the write in exactly one. That replica must
// decide the answer.
machine WriteThenRead {
  start state Init {
    entry { new Sequence((quorums = (r = 2, w = 2), steps = Steps3((id = 1, value = 1), (id = 1, value = 0), (id = 0, value = 0)))); }
  }
}

// Write after a completed write, by another writer, then a read. The second writer's
// read phase must see the first version (quorums overlap) and pick a higher one, or
// replicas holding both keep the first value and the read returns it.
machine WriteWriteRead {
  start state Init {
    entry { new Sequence((quorums = (r = 2, w = 2), steps = Steps3((id = 1, value = 1), (id = 2, value = 2), (id = 1, value = 0)))); }
  }
}

// Negative control: the same as WriteThenRead with R = 1, so R + W = N and a read
// quorum can miss the write entirely.
machine WriteThenReadR1 {
  start state Init {
    entry { new Sequence((quorums = (r = 1, w = 2), steps = Steps3((id = 1, value = 1), (id = 1, value = 0), (id = 0, value = 0)))); }
  }
}

test tcWriteAndReads [main=WriteAndReads]:
  assert ReadAfterWrite, OperationsReturn in { WriteAndReads, Clients, Replica, Writer, Reader };

test tcTwoWriters [main=TwoWriters]:
  assert ReadAfterWrite, OperationsReturn in { TwoWriters, Clients, Replica, Writer, Reader };

test tcWriteThenRead [main=WriteThenRead]:
  assert ReadAfterWrite, OperationsReturn in { WriteThenRead, Sequence, Replica, Writer, Reader };

test tcWriteWriteRead [main=WriteWriteRead]:
  assert ReadAfterWrite, OperationsReturn in { WriteWriteRead, Sequence, Replica, Writer, Reader };

// expect-bug: returned the initial value after write
test tcBugReadOne [main=WriteThenReadR1]:
  assert ReadAfterWrite, OperationsReturn in { WriteThenReadR1, Sequence, Replica, Writer, Reader };
