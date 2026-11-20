// Drivers: they create the coordinator and consumers and announce the work. The
// coordinator and consumers are the design under test.

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

// One partition: the second consumer gets nothing.
machine OnePartition {
  start state Run {
    entry { StartGroup(new Coordinator(1), 1, 2); }
  }
}

test tcJoinRebalance [main=JoinRebalance]:
  assert GroupSafety, GroupProgress in { JoinRebalance, Coordinator, Consumer };

test tcOnePartition [main=OnePartition]:
  assert GroupSafety, GroupProgress in { OnePartition, Coordinator, Consumer };
