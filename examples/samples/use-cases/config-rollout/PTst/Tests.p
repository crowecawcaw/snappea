// The operator and the test cases. The service, the nodes and the couriers (the
// network) define delivery assumptions; the service and nodes are the design under test.

// Issues each (version, content) in order, one at a time, at moments the scheduler
// picks: a later issue can land while the previous rollout is anywhere.
machine Operator {
  start state Issuing {
    entry (o: (service: machine, issues: seq[(version: int, content: int)])) {
      var i: (version: int, content: int);
      foreach (i in o.issues) {
        send o.service, Issue, i;
      }
    }
  }
}

// Three nodes running version 1 (content 100). Builds the nodes, service and operator.
fun StartRollout(issues: seq[(version: int, content: int)]) {
  var nodes: seq[machine];
  var n: int;
  var service: ConfigService;
  announce RolloutPlanned, (nodes = 3, version = 1, content = 100, issues = sizeof(issues));
  while (n < 3) {
    nodes += (n, new Node((id = n, version = 1, content = 100)));
    n = n + 1;
  }
  service = new ConfigService(nodes);
  new Operator((service = service, issues = issues));
}

fun Issues1(v: int, c: int): seq[(version: int, content: int)] {
  var s: seq[(version: int, content: int)];
  s += (0, (version = v, content = c));
  return s;
}

// One new version, staged: canary first, then the other two.
machine StagedRollout {
  start state Init {
    entry { StartRollout(Issues1(2, 200)); }
  }
}

// Version 2 goes out; the operator rolls back by issuing version 3 with version 1's
// content, while version 2's canary push (or its later pushes) are still in flight.
// Pushes of 2 and 3 to the same node can arrive in either order.
machine RollbackDuringCanary {
  start state Init {
    entry {
      var issues: seq[(version: int, content: int)];
      issues = Issues1(2, 200);
      issues += (1, (version = 3, content = 100));
      StartRollout(issues);
    }
  }
}

machine BuggyLastArrival {
  start state Init {
    entry {
      var nodes: seq[machine];
      var n: int;
      var issues: seq[(version: int, content: int)];
      issues = Issues1(2, 200);
      issues += (1, (version = 3, content = 100));
      announce RolloutPlanned, (nodes = 3, version = 1, content = 100, issues = 2);
      while (n < 3) {
        nodes += (n, new LastArrivalNode((id = n, version = 1, content = 100)));
        n = n + 1;
      }
      new Operator((service = new ConfigService(nodes), issues = issues));
    }
  }
}

// Covered: a node received the rollback before the bad version's delayed push. Version 2
// was pushed to the node but not applied when version 3 was, so that push was still in
// flight: the rollback overtook it.
scenario RollbackOvertookPush observes PushSent, Applied {
  // nodes with a push of version 2 that has not arrived
  var pending2: set[int];
  start hot state Watching {
    on PushSent do (p: (node: int, version: int)) {
      if (p.version == 2) { pending2 += (p.node); }
    }
    on Applied do (a: (node: int, version: int, content: int)) {
      if (a.version == 2) { pending2 -= (a.node); }
      if (a.version == 3 && a.node in pending2) { goto Covered; }
    }
  }
  cold state Covered {
  }
}

test tcStagedRollout [main=StagedRollout]:
  assert RolloutSafety, RolloutCompletes in { StagedRollout, ConfigService, Node, BBDelivery, Operator };

test tcRollbackDuringCanary [main=RollbackDuringCanary]:
  assert RolloutSafety, RolloutCompletes in { RollbackDuringCanary, ConfigService, Node, BBDelivery, Operator };

// snappea: expect-bug "applied version 2 after version 3"
test tcBugLastArrival [main=BuggyLastArrival]:
  assert RolloutSafety, RolloutCompletes in { BuggyLastArrival, ConfigService, LastArrivalNode, BBDelivery, Operator };
