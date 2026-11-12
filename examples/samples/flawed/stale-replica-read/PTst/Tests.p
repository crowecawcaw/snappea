// Drivers. Sessions, replicas, log shippers and the primary are the design under test;
// the drivers announce every session's plan, then start the replicas, the primary and
// the sessions. Session s writes the values 10*s + 1, 10*s + 2, ... The drivers also
// play the load balancer: with two replicas, a session's first read goes to replica 0
// and its second to replica 1 (the unlucky routing for monotonic reads); with one
// replica, every read goes to it.

fun Ops1(a: tOp) : seq[tOp] {
  var s: seq[tOp];
  s += (0, a);
  return s;
}

fun Ops2(a: tOp, b: tOp) : seq[tOp] {
  var s: seq[tOp];
  s += (0, a);
  s += (1, b);
  return s;
}

// Read-your-writes: one session writes twice, then reads: it must see its second write.
fun WriteThenRead() : map[int, seq[tOp]] {
  var plans: map[int, seq[tOp]];
  plans[1] = Ops2(Write, Write);
  plans[1] += (2, Read);
  return plans;
}

// Monotonic reads: session 1 writes once; session 2 reads twice, and may see the write
// on its first read.
fun ReadTwiceWhileWriting() : map[int, seq[tOp]] {
  var plans: map[int, seq[tOp]];
  plans[1] = Ops1(Write);
  plans[2] = Ops2(Read, Read);
  return plans;
}

// Both sessions write, then read: a read's token can be ahead of a replica by two log
// records, so the replica keeps holding it after applying the first.
fun TwoWritersReading() : map[int, seq[tOp]] {
  var plans: map[int, seq[tOp]];
  plans[1] = Ops2(Write, Read);
  plans[2] = Ops2(Write, Read);
  return plans;
}

fun Replicas(n: int) : seq[machine] {
  var r: seq[machine];
  while (sizeof(r) < n) { r += (sizeof(r), new Replica()); }
  return r;
}

fun SessionReplicas(n: int) : seq[machine] {
  var r: seq[machine];
  while (sizeof(r) < n) { r += (sizeof(r), new SessionReplica()); }
  return r;
}

// Read i goes to replica i, or to the last replica.
fun Route(replicas: seq[machine]) : seq[machine] {
  var route: seq[machine];
  while (sizeof(route) < 2) {
    if (sizeof(route) < sizeof(replicas)) {
      route += (sizeof(route), replicas[sizeof(route)]);
    } else {
      route += (sizeof(route), replicas[sizeof(replicas) - 1]);
    }
  }
  return route;
}

fun Shippers(replicas: seq[machine]) : seq[machine] {
  var s: seq[machine];
  var r: machine;
  foreach (r in replicas) { s += (sizeof(s), new LogShipper(r)); }
  return s;
}

machine LoadBalancedReads {
  start state Init {
    entry (p: (plans: map[int, seq[tOp]], replicas: int)) {
      var plans: map[int, seq[tOp]];
      var replicas: seq[machine];
      var primary: machine;
      var s: int;
      plans = p.plans;
      announce eSessionsPlanned, plans;
      replicas = Replicas(p.replicas);
      primary = new Primary(Shippers(replicas));
      foreach (s in keys(plans)) {
        new Session((session = s, primary = primary, route = Route(replicas), ops = plans[s], firstValue = 10 * s + 1));
      }
    }
  }
}

machine TokenReads {
  start state Init {
    entry (p: (plans: map[int, seq[tOp]], replicas: int)) {
      var plans: map[int, seq[tOp]];
      var replicas: seq[machine];
      var primary: machine;
      var s: int;
      plans = p.plans;
      announce eSessionsPlanned, plans;
      replicas = SessionReplicas(p.replicas);
      primary = new Primary(Shippers(replicas));
      foreach (s in keys(plans)) {
        new TokenSession((session = s, primary = primary, route = Route(replicas), ops = plans[s], firstValue = 10 * s + 1));
      }
    }
  }
}

// Test mains take no payload, so each test has a one-line main that picks the plan.
// Read-your-writes and two writers need only one lagging replica; monotonic reads need
// two replicas at different points.
machine BugReadYourWrites { start state Init { entry { new LoadBalancedReads((plans = WriteThenRead(), replicas = 1)); } } }
machine BugMonotonicReads { start state Init { entry { new LoadBalancedReads((plans = ReadTwiceWhileWriting(), replicas = 2)); } } }
machine FixedReadYourWrites { start state Init { entry { new TokenReads((plans = WriteThenRead(), replicas = 1)); } } }
machine FixedMonotonicReads { start state Init { entry { new TokenReads((plans = ReadTwiceWhileWriting(), replicas = 2)); } } }
machine FixedTwoWriters { start state Init { entry { new TokenReads((plans = TwoWritersReading(), replicas = 1)); } } }

test tcFixedReadYourWrites [main=FixedReadYourWrites]:
  assert SessionGuarantees in { FixedReadYourWrites, TokenReads, Primary, LogShipper, SessionReplica, TokenSession };

test tcFixedMonotonicReads [main=FixedMonotonicReads]:
  assert SessionGuarantees in { FixedMonotonicReads, TokenReads, Primary, LogShipper, SessionReplica, TokenSession };

test tcFixedTwoWriters [main=FixedTwoWriters]:
  assert SessionGuarantees in { FixedTwoWriters, TokenReads, Primary, LogShipper, SessionReplica, TokenSession };

// expect-bug: read-your-writes violated
test tcBugReadYourWrites [main=BugReadYourWrites]:
  assert SessionGuarantees in { BugReadYourWrites, LoadBalancedReads, Primary, LogShipper, Replica, Session };

// expect-bug: monotonic reads violated
test tcBugMonotonicReads [main=BugMonotonicReads]:
  assert SessionGuarantees in { BugMonotonicReads, LoadBalancedReads, Primary, LogShipper, Replica, Session };
