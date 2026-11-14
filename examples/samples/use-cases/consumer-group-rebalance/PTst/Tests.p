// Drivers: they create the coordinator and consumers, announce the work, and make the
// membership changes (a consumer shutting down, a session expiring). The coordinator
// and consumers are the design under test.

// `partitions` partitions with `records` records each, and two consumers.
fun StartGroup(coordinator: machine, partitions: int, records: int): seq[machine] {
  var started: seq[machine];
  var id: int;
  announce GroupPlanned, (partitions = partitions, records = records);
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
    entry { StartGroup(new Coordinator(2), 2, 2); }
  }
}

// Consumer 2 shuts down cleanly at a moment the scheduler picks: consumer 1 takes over
// its partition from the last committed offset.
machine LeaveRebalance {
  start state Run {
    entry {
      var consumers: seq[machine];
      consumers = StartGroup(new Coordinator(2), 2, 1);
      send consumers[1], Shutdown;
    }
  }
}

// Consumer 1's JoinGroup is retried (its first attempt timed out but was delivered):
// the coordinator sees the same join twice, in any order with consumer 2's join. A
// retry from a current member must not list it twice or start a new generation.
machine RetriedJoin {
  start state Run {
    entry {
      var coordinator: Coordinator;
      var consumers: seq[machine];
      coordinator = new Coordinator(2);
      consumers = StartGroup(coordinator, 2, 1);
      send coordinator, Join, (consumer = consumers[0], id = 1);
    }
  }
}

// The session timer expires consumer 2 while it is still running: it becomes a zombie
// whose commits must be rejected, after which it rejoins (a third generation).
machine ZombieExpired {
  start state Run {
    entry {
      var coordinator: Coordinator;
      coordinator = new Coordinator(1);
      StartGroup(coordinator, 1, 1);
      send coordinator, Expire, (id = 2,);
    }
  }
}

// Member 2 joined and crashed: it never fetches or commits. The driver stands in for it:
// it joins as member 2 and gets both partitions, then starts consumer 1 and expires
// member 2's session at a moment the scheduler picks. Consumer 1 must take over member
// 2's partitions once the session expires, whether it joined before or after that.
machine CrashedMember {
  var coordinator: Coordinator;

  start state Joining {
    entry {
      coordinator = new Coordinator(2);
      announce GroupPlanned, (partitions = 2, records = 1);
      send coordinator, Join, (consumer = this, id = 2);
    }
    on Assignment goto Crashed with {
      new Consumer((coordinator = coordinator, id = 1, records = 1));
      send coordinator, Expire, (id = 2,);
    }
  }

  state Crashed {
    ignore Assignment;
  }
}

// Generation alone is not authorization: reject a different member and a
// nonexistent partition even when their requests carry the current generation.
machine InvalidAssignmentRequests {
  var coordinator: Coordinator;
  start state Joining {
    entry {
      coordinator = new Coordinator(1);
      announce GroupPlanned, (partitions = 1, records = 1);
      send coordinator, Join, (consumer = this, id = 2);
    }
    on Assignment do (a: (gen: int, partitions: seq[int])) {
      send coordinator, Fetch, (consumer = this, id = 1, gen = a.gen, partition = 0);
      receive { case Fetched: (r: (member: int, gen: int, partition: int, ok: bool, offset: int)) {
        assert !r.ok, format("member {0}, not the owner, fetched partition {1}", r.member, r.partition);
      } }
      send coordinator, Fetch, (consumer = this, id = 2, gen = a.gen, partition = 1);
      receive { case Fetched: (r: (member: int, gen: int, partition: int, ok: bool, offset: int)) {
        assert !r.ok, format("member {0} fetched partition {1}, which does not exist", r.member, r.partition);
      } }
      send coordinator, LeaveGroup, (id = 2,);
      // The legitimate member joins and completes the planned record. Its join
      // also closes the progress obligation created by rejecting member 1.
      new Consumer((coordinator = coordinator, id = 1, records = 1));
      goto Finished;
    }
  }
  state Finished {
    // A rebalance assignment can already be queued when this test member leaves.
    ignore Assignment;
  }
}

machine BugNoGenerationCheck {
  start state Run {
    entry {
      var coordinator: UnfencedCoordinator;
      coordinator = new UnfencedCoordinator(2);
      StartGroup(coordinator, 2, 2);
      send coordinator, Expire, (id = 2,);
    }
  }
}

// Covered: an expelled consumer's commit was rejected, then that same consumer
// appeared in a later generation. An unrelated rebalance does not satisfy this.
scenario ZombieFenced observes CommitResponse, GenerationChanged {
  var members: seq[int];
  var fencedMember: int;
  start hot state Running {
    on GenerationChanged do (g: (gen: int, members: seq[int], owner: map[int, int])) {
      members = g.members;
    }
    on CommitResponse do (c: (member: int, gen: int, partition: int, offset: int, ok: bool)) {
      if (!c.ok && !(c.member in members)) {
        fencedMember = c.member;
        goto Fenced;
      }
    }
  }
  hot state Fenced {
    on GenerationChanged do (g: (gen: int, members: seq[int], owner: map[int, int])) {
      if (fencedMember in g.members) { goto Covered; }
    }
  }
  cold state Covered { }
}

test tcJoinRebalance [main=JoinRebalance]:
  assert GroupSafety, GroupProgress in { JoinRebalance, Coordinator, Consumer };

test tcRetriedJoin [main=RetriedJoin]:
  assert GroupSafety, GroupProgress in { RetriedJoin, Coordinator, Consumer };

test tcLeaveRebalance [main=LeaveRebalance]:
  assert GroupSafety, GroupProgress in { LeaveRebalance, Coordinator, Consumer };

test tcCrashedMember [main=CrashedMember]:
  assert GroupSafety, GroupProgress in { CrashedMember, Coordinator, Consumer };

test tcZombieExpired [main=ZombieExpired]:
  assert GroupSafety, GroupProgress in { ZombieExpired, Coordinator, Consumer };

// snappea: boundary -- current generation with an unauthorized member or nonexistent partition
test tcInvalidAssignmentRequests [main=InvalidAssignmentRequests]:
  assert GroupSafety, GroupProgress in { InvalidAssignmentRequests, Coordinator, Consumer };

// snappea: expect-bug "commit from generation"
test tcBugNoGenerationCheck [main=BugNoGenerationCheck]:
  assert GroupSafety, GroupProgress in { BugNoGenerationCheck, UnfencedCoordinator, Consumer };
