// Drivers start the lock service, storage and the nodes, announcing the plan and each
// node's plan first so the specs know what must happen even if a node misbehaves.

fun Plan(first: int, n: int): seq[int] {
  var plan: seq[int];
  var i: int;
  while (i < n) {
    plan += (i, first + i);
    i = i + 1;
  }
  return plan;
}

fun StartUnfenced(locks: LockService, storage: UnfencedStorage, id: int, plan: seq[int]) {
  announce NodeStarted, (node = id, plan = plan);
  new UnfencedNode((id = id, plan = plan, locks = locks, storage = storage));
}

fun StartFenced(locks: LockService, storage: FencedStorage, id: int, plan: seq[int]) {
  announce NodeStarted, (node = id, plan = plan);
  new FencedNode((id = id, plan = plan, locks = locks, storage = storage));
}

// Two nodes compete for leadership. Node 1 writes twice (so a leader's own later
// write under the same lease must be accepted), node 2 writes once.
machine TwoLeadersUnfenced {
  start state Init {
    entry {
      var locks: LockService;
      var storage: UnfencedStorage;
      locks = new LockService();
      storage = new UnfencedStorage();
      announce NodesPlanned, 2;
      StartUnfenced(locks, storage, 1, Plan(10, 2));
      StartUnfenced(locks, storage, 2, Plan(20, 1));
    }
  }
}

machine TwoLeaders {
  start state Init {
    entry {
      var locks: LockService;
      var storage: FencedStorage;
      locks = new LockService();
      storage = new FencedStorage();
      announce NodesPlanned, 2;
      StartFenced(locks, storage, 1, Plan(10, 2));
      StartFenced(locks, storage, 2, Plan(20, 1));
    }
  }
}

// A node that takes the lease and then dies without releasing it. Only the lease's
// expiry lets anyone else lead; this is why leases have timers at all.
machine CrashingNode {
  var id: int;
  start state Acquiring {
    entry (c: (id: int, locks: LockService)) {
      id = c.id;
      announce NodeStarted, (node = c.id, plan = default(seq[int]));
      send c.locks, Acquire, (node = this, id = c.id);
    }
    on Granted do {
      announce NodeDone, (node = id, fenced = false);
      goto Dead;
    }
  }
  state Dead { }
}

// Lifecycle boundary: the first leader crashes while holding the lease. The second
// node must still get the lease and write.
machine LeaderCrashes {
  start state Init {
    entry {
      var locks: LockService;
      var storage: FencedStorage;
      locks = new LockService();
      storage = new FencedStorage();
      announce NodesPlanned, 2;
      new CrashingNode((id = 1, locks = locks));
      StartFenced(locks, storage, 2, Plan(20, 1));
    }
  }
}

// Covered: an old leader's write was fenced off after a newer leader wrote.
scenario OldLeaderFenced observes WriteRejected {
  start hot state WaitRejection {
    on WriteRejected goto Covered;
  }
  cold state Covered {
  }
}

// snappea: expect-bug "stale write: storage accepted value"
test tcBugStaleLeaderWrite [main=TwoLeadersUnfenced]:
  assert NoStaleWrites, NodesFinish in
  { TwoLeadersUnfenced, LockService, LeaseTimer, UnfencedStorage, UnfencedNode };

test tcFixedTwoLeaders [main=TwoLeaders]:
  assert NoStaleWrites, NodesFinish in
  { TwoLeaders, LockService, LeaseTimer, FencedStorage, FencedNode };

test tcFixedLeaderCrashes [main=LeaderCrashes]:
  assert NoStaleWrites, NodesFinish in
  { LeaderCrashes, LockService, LeaseTimer, FencedStorage, FencedNode, CrashingNode };
