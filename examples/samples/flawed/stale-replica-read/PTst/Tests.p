// Drivers. Sessions, replicas and the primary are the design under test; the drivers
// announce every session's plan, then start two replicas, the primary and the sessions.
// Session s writes the values 10*s + 1, 10*s + 2, ...

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

// Read-your-writes: one session writes, then reads.
fun WriteThenRead() : map[int, seq[tOp]] {
  var plans: map[int, seq[tOp]];
  plans[1] = Ops2(Write, Read);
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

fun TwoReplicas() : seq[machine] {
  var r: seq[machine];
  r += (0, new Replica());
  r += (1, new Replica());
  return r;
}

fun TwoSessionReplicas() : seq[machine] {
  var r: seq[machine];
  r += (0, new SessionReplica());
  r += (1, new SessionReplica());
  return r;
}

machine LoadBalancedReads {
  start state Init {
    entry (plans: map[int, seq[tOp]]) {
      var replicas: seq[machine];
      var primary: machine;
      var s: int;
      announce eSessionsPlanned, plans;
      replicas = TwoReplicas();
      primary = new Primary(replicas);
      foreach (s in keys(plans)) {
        new Session((session = s, primary = primary, replicas = replicas, ops = plans[s], firstValue = 10 * s + 1));
      }
    }
  }
}

machine TokenReads {
  start state Init {
    entry (plans: map[int, seq[tOp]]) {
      var replicas: seq[machine];
      var primary: machine;
      var s: int;
      announce eSessionsPlanned, plans;
      replicas = TwoSessionReplicas();
      primary = new Primary(replicas);
      foreach (s in keys(plans)) {
        new TokenSession((session = s, primary = primary, replicas = replicas, ops = plans[s], firstValue = 10 * s + 1));
      }
    }
  }
}

// Test mains take no payload, so each test has a one-line main that picks the plan.
machine BugReadYourWrites { start state Init { entry { new LoadBalancedReads(WriteThenRead()); } } }
machine BugMonotonicReads { start state Init { entry { new LoadBalancedReads(ReadTwiceWhileWriting()); } } }
machine FixedReadYourWrites { start state Init { entry { new TokenReads(WriteThenRead()); } } }
machine FixedMonotonicReads { start state Init { entry { new TokenReads(ReadTwiceWhileWriting()); } } }
machine FixedTwoWriters { start state Init { entry { new TokenReads(TwoWritersReading()); } } }

test tcFixedReadYourWrites [main=FixedReadYourWrites]:
  assert SessionGuarantees in { FixedReadYourWrites, TokenReads, Primary, SessionReplica, TokenSession };

test tcFixedMonotonicReads [main=FixedMonotonicReads]:
  assert SessionGuarantees in { FixedMonotonicReads, TokenReads, Primary, SessionReplica, TokenSession };

test tcFixedTwoWriters [main=FixedTwoWriters]:
  assert SessionGuarantees in { FixedTwoWriters, TokenReads, Primary, SessionReplica, TokenSession };

// expect-bug: read-your-writes violated
test tcBugReadYourWrites [main=BugReadYourWrites]:
  assert SessionGuarantees in { BugReadYourWrites, LoadBalancedReads, Primary, Replica, Session };

// expect-bug: monotonic reads violated
test tcBugMonotonicReads [main=BugMonotonicReads]:
  assert SessionGuarantees in { BugMonotonicReads, LoadBalancedReads, Primary, Replica, Session };
