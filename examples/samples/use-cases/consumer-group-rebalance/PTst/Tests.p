// Drivers: they create the coordinator and consumers, announce the work, and make the
// membership changes (a consumer shutting down, a session expiring). The coordinator
// and consumers are the design under test.

// Two partitions with `records` records each, and two consumers.
fun StartGroup(coordinator: machine, records: int): seq[machine] {
  var started: seq[machine];
  var id: int;
  announce eGroupPlanned, (partitions = 2, records = records);
  while (id < 2) {
    started += (id, new Consumer((coordinator = coordinator, id = id + 1, records = records)));
    id = id + 1;
  }
  return started;
}

// Two consumers start together: the group goes from one member to two while the first
// is already processing. Its in-flight commits for the partition it loses are fenced.
machine JoinRebalance {
  start state Run {
    entry { StartGroup(new Coordinator(2), 2); }
  }
}

// Consumer 2 shuts down cleanly at a moment the scheduler picks: consumer 1 takes over
// its partition from the last committed offset.
machine LeaveRebalance {
  start state Run {
    entry {
      var consumers: seq[machine];
      consumers = StartGroup(new Coordinator(2), 1);
      send consumers[1], eShutdown;
    }
  }
}

// The session timer expires consumer 2 while it is still running: it becomes a zombie
// whose commits must be rejected, after which it rejoins (a third generation).
machine ZombieExpired {
  start state Run {
    entry {
      var coordinator: Coordinator;
      coordinator = new Coordinator(2);
      StartGroup(coordinator, 1);
      send coordinator, eExpire, (id = 2,);
    }
  }
}

// Member 2 joined and crashed: it never fetches or commits. The driver stands in for it
// (its join and then its session expiry, in that order). Consumer 1 must take over its
// partition once the session expires.
machine CrashedMember {
  start state Run {
    entry {
      var coordinator: Coordinator;
      coordinator = new Coordinator(2);
      announce eGroupPlanned, (partitions = 2, records = 1);
      new Consumer((coordinator = coordinator, id = 1, records = 1));
      send coordinator, eJoin, (consumer = this, id = 2);
      send coordinator, eExpire, (id = 2,);
    }
    ignore eAssignment;
  }
}

machine BugNoGenerationCheck {
  start state Run {
    entry {
      var coordinator: UnfencedCoordinator;
      coordinator = new UnfencedCoordinator(2);
      StartGroup(coordinator, 2);
      send coordinator, eExpire, (id = 2,);
    }
  }
}

// Covered: a consumer was fenced (its commit rejected) and rejoined.
scenario ZombieFenced observes eCommitRejected, eGeneration {
  start hot state Running {
    on eCommitRejected goto Fenced;
    ignore eGeneration;
  }
  hot state Fenced {
    on eGeneration goto Covered;
    ignore eCommitRejected;
  }
  cold state Covered {
    ignore eCommitRejected, eGeneration;
  }
}

test tcJoinRebalance [main=JoinRebalance]:
  assert GroupSafety, GroupProgress in { JoinRebalance, Coordinator, Consumer };

test tcLeaveRebalance [main=LeaveRebalance]:
  assert GroupSafety, GroupProgress in { LeaveRebalance, Coordinator, Consumer };

test tcCrashedMember [main=CrashedMember]:
  assert GroupSafety, GroupProgress in { CrashedMember, Coordinator, Consumer };

test tcZombieExpired [main=ZombieExpired]:
  assert GroupSafety, GroupProgress in { ZombieExpired, Coordinator, Consumer };

// expect-bug: commit from generation
test tcBugNoGenerationCheck [main=BugNoGenerationCheck]:
  assert GroupSafety, GroupProgress in { BugNoGenerationCheck, UnfencedCoordinator, Consumer };
