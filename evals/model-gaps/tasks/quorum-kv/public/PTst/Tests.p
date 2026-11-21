// The test cases: one scenario each. The environment is in Environment.p.

// One coordinator, reliable network: a put, then a get of it.
machine PutThenGet {
  start state Init {
    entry {
      var lane: seq[Step];
      lane = AndStep(AndStep(NoSteps(), PutStep(1, 1)), GetStep(1));
      new Driver((clients = 1, down = 0, faults = NoFaults(), lanes = AndLane(NoLanes(), lane)));
    }
  }
}

// Two coordinators write in turn, then a get. The second put's read phase must see
// the first version and go above it.
machine WritersInTurn {
  start state Init {
    entry {
      var lane: seq[Step];
      lane = AndStep(AndStep(AndStep(NoSteps(), PutStep(1, 1)), PutStep(2, 2)), GetStep(1));
      new Driver((clients = 2, down = 0, faults = NoFaults(), lanes = AndLane(NoLanes(), lane)));
    }
  }
}

test tcPutThenGet [main=PutThenGet]:
  assert ReadAfterWrite, ReplicasKeepNewest in
    { PutThenGet, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };

test tcWritersInTurn [main=WritersInTurn]:
  assert ReadAfterWrite, ReplicasKeepNewest in
    { WritersInTurn, Driver, Link, Courier, TimeoutTimer, Crasher, Unreachable, Replica, Coordinator };
