// The test cases: one scenario each. The environment is in Environment.p.

// One coordinator, reliable network: a put, then a get of it, then a get of the
// value just read.
machine PutThenGet {
  start state Init {
    entry {
      var lane: seq[Step];
      lane = AndStep(AndStep(AndStep(NoSteps(), PutStep(1, 1)), GetStep(1)), GetStep(1));
      new Driver((clients = 1, down = 0, faults = NoFaults(), lanes = AndLane(NoLanes(), lane)));
    }
  }
}

// Two coordinators write in turn. The later put's read phase must see the earlier
// version and go above it; every get sees the latest put.
machine WritersInTurn {
  start state Init {
    entry {
      var lane: seq[Step];
      lane = AndStep(AndStep(AndStep(NoSteps(), PutStep(1, 1)), PutStep(2, 2)), GetStep(1));
      lane = AndStep(AndStep(AndStep(lane, PutStep(1, 3)), GetStep(2)), GetStep(3));
      new Driver((clients = 3, down = 0, faults = NoFaults(), lanes = AndLane(NoLanes(), lane)));
    }
  }
}

// Two coordinators put at the same time and can pick the same counter; the writer id
// decides, on every replica. A third reads meanwhile.
machine ConcurrentWriters {
  start state Init {
    entry {
      var lane: seq[Step];
      var lanes: seq[seq[Step]];
      lanes = AndLane(NoLanes(), AndStep(AndStep(NoSteps(), PutStep(1, 1)), GetStep(1)));
      lanes = AndLane(lanes, AndStep(AndStep(NoSteps(), PutStep(2, 2)), GetStep(2)));
      lanes = AndLane(lanes, AndStep(AndStep(NoSteps(), GetStep(3)), GetStep(3)));
      new Driver((clients = 3, down = 0, faults = NoFaults(), lanes = lanes));
    }
  }
}

// Replica 3 is down for the whole run: every operation completes with the other two.
machine ReplicaDown {
  start state Init {
    entry {
      var lane: seq[Step];
      lane = AndStep(AndStep(AndStep(NoSteps(), GetStep(1)), PutStep(1, 1)), GetStep(2));
      lane = AndStep(AndStep(lane, PutStep(2, 2)), GetStep(1));
      new Driver((clients = 2, down = 3, faults = NoFaults(), lanes = AndLane(NoLanes(), lane)));
    }
  }
}

// The put's writes to replicas 2 and 3 are lost. Only the timeouts get it through, and
// replica 1 acks every resend: its acks are one replica, not a quorum.
machine LostWrites {
  start state Init {
    entry {
      var lane: seq[Step];
      var faults: seq[Fault];
      faults = AndFault(NoFaults(), (client = 1, replica = 2, leg = WriteRequest, action = Lose, times = 1));
      faults = AndFault(faults, (client = 1, replica = 3, leg = WriteRequest, action = Lose, times = 1));
      lane = AndStep(AndStep(AndStep(NoSteps(), PutStep(1, 1)), GetStep(2)), GetStep(1));
      new Driver((clients = 2, down = 0, faults = faults, lanes = AndLane(NoLanes(), lane)));
    }
  }
}

// Replica 3 is down and the first two reads to replica 2 are lost: the timeout is
// the only way forward, and replica 1 answers every resend.
machine LostReads {
  start state Init {
    entry {
      var lane: seq[Step];
      var faults: seq[Fault];
      faults = AndFault(NoFaults(), (client = 1, replica = 2, leg = ReadRequest, action = Lose, times = 2));
      lane = AndStep(AndStep(AndStep(NoSteps(), PutStep(1, 1)), GetStep(1)), GetStep(2));
      new Driver((clients = 2, down = 3, faults = faults, lanes = AndLane(NoLanes(), lane)));
    }
  }
}

// A put reaches replica 1 while its writes to replicas 2 and 3 are slow. Gets run
// meanwhile: once one of them returned the new value, none may return the old one.
machine SlowWriteManyReads {
  start state Init {
    entry {
      var lane: seq[Step];
      var faults: seq[Fault];
      var lanes: seq[seq[Step]];
      faults = AndFault(NoFaults(), (client = 1, replica = 2, leg = WriteRequest, action = Delay, times = 1));
      faults = AndFault(faults, (client = 1, replica = 3, leg = WriteRequest, action = Delay, times = 1));
      lanes = AndLane(NoLanes(), AndStep(NoSteps(), PutStep(1, 1)));
      lanes = AndLane(lanes, AndStep(AndStep(AndStep(NoSteps(), GetStep(2)), GetStep(3)), GetStep(2)));
      new Driver((clients = 3, down = 0, faults = faults, lanes = lanes));
    }
  }
}

// Replica 3's answer to coordinator 1's first get arrives late, perhaps during its
// second get. Meanwhile coordinator 2's put may have missed replica 1. The late
// answer is not an answer to the second get.
machine LateReadAnswer {
  start state Init {
    entry {
      var lane: seq[Step];
      var faults: seq[Fault];
      faults = AndFault(NoFaults(), (client = 1, replica = 3, leg = ReadAnswer, action = Delay, times = 1));
      faults = AndFault(faults, (client = 2, replica = 1, leg = WriteRequest, action = Lose, times = 1));
      lane = AndStep(AndStep(AndStep(NoSteps(), GetStep(1)), PutStep(2, 1)), GetStep(1));
      new Driver((clients = 2, down = 0, faults = faults, lanes = AndLane(NoLanes(), lane)));
    }
  }
}

// Replica 3's ack of the first put arrives late, perhaps during the second put. It
// is not an ack of the second put.
machine LateAck {
  start state Init {
    entry {
      var lane: seq[Step];
      var faults: seq[Fault];
      faults = AndFault(NoFaults(), (client = 1, replica = 3, leg = WriteAck, action = Delay, times = 1));
      lane = AndStep(AndStep(AndStep(NoSteps(), PutStep(1, 1)), PutStep(1, 2)), GetStep(2));
      new Driver((clients = 2, down = 0, faults = faults, lanes = AndLane(NoLanes(), lane)));
    }
  }
}

// Coordinator 1 crashes around its first put, whose writes to replicas 2 and 3 are
// lost: the put may be on replica 1 only, and is abandoned. Its next put must not
// reuse the version. Coordinator 2 reads meanwhile.
machine CrashDuringPut {
  start state Init {
    entry {
      var lane: seq[Step];
      var faults: seq[Fault];
      var lanes: seq[seq[Step]];
      faults = AndFault(NoFaults(), (client = 1, replica = 2, leg = WriteRequest, action = Lose, times = 1));
      faults = AndFault(faults, (client = 1, replica = 3, leg = WriteRequest, action = Lose, times = 1));
      lanes = AndLane(NoLanes(), AndStep(AndStep(AndStep(NoSteps(), Crashing(PutStep(1, 1))), PutStep(1, 2)), GetStep(1)));
      lanes = AndLane(lanes, AndStep(AndStep(NoSteps(), GetStep(2)), GetStep(2)));
      new Driver((clients = 2, down = 0, faults = faults, lanes = lanes));
    }
  }
}

// Coordinator 1 crashes around its first get while two of the answers are late. They
// can arrive during its next get, after coordinator 2's put: they belong to an
// operation from before the crash.
machine CrashDuringGet {
  start state Init {
    entry {
      var lane: seq[Step];
      var faults: seq[Fault];
      faults = AndFault(NoFaults(), (client = 1, replica = 1, leg = ReadAnswer, action = Delay, times = 1));
      faults = AndFault(faults, (client = 1, replica = 2, leg = ReadAnswer, action = Delay, times = 1));
      lane = AndStep(AndStep(AndStep(NoSteps(), Crashing(GetStep(1))), PutStep(2, 1)), GetStep(1));
      new Driver((clients = 2, down = 0, faults = faults, lanes = AndLane(NoLanes(), lane)));
    }
  }
}

// Coordinator 1's write to replica 3 is slow and can arrive after coordinator 2's
// newer write, and after later gets: it must not move the replica back.
machine LateWrite {
  start state Init {
    entry {
      var lane: seq[Step];
      var faults: seq[Fault];
      faults = AndFault(NoFaults(), (client = 1, replica = 3, leg = WriteRequest, action = Delay, times = 1));
      lane = AndStep(AndStep(AndStep(NoSteps(), PutStep(1, 1)), PutStep(2, 2)), GetStep(1));
      lane = AndStep(lane, GetStep(2));
      new Driver((clients = 2, down = 0, faults = faults, lanes = AndLane(NoLanes(), lane)));
    }
  }
}

test tcPutThenGet [main=PutThenGet]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { PutThenGet, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcWritersInTurn [main=WritersInTurn]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { WritersInTurn, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcConcurrentWriters [main=ConcurrentWriters]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { ConcurrentWriters, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcReplicaDown [main=ReplicaDown]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { ReplicaDown, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcLostWrites [main=LostWrites]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { LostWrites, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcLostReads [main=LostReads]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { LostReads, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcSlowWriteManyReads [main=SlowWriteManyReads]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { SlowWriteManyReads, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcLateReadAnswer [main=LateReadAnswer]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { LateReadAnswer, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcLateAck [main=LateAck]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { LateAck, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcCrashDuringPut [main=CrashDuringPut]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { CrashDuringPut, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcCrashDuringGet [main=CrashDuringGet]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { CrashDuringGet, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcLateWrite [main=LateWrite]:
  assert AtomicRegister, ReplicasKeepNewest, QuorumBeforeReturn, OperationsReturn in
    { LateWrite, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };
