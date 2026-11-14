// The storage contract for a leased leader, stated over events alone. Leases are
// numbered by the order the lock service grants them, counted here, so the spec does
// not trust any token the design computes.

// The driver announces how many nodes it starts, then each node and the plan it
// will write, in order, once it is leader.
event NodesPlanned: int;
event NodeStarted: (node: int, plan: seq[int]);
// The lock service announces each grant and each lease that ends (expired or released).
event LeaseGranted: (node: int);
event LeaseEnded: (node: int);
// Storage announces each write it accepts or rejects, when it decides.
event WriteAccepted: (node: int, value: int);
event WriteRejected: (node: int, value: int);
// A node announces that it stopped, and whether it was told it lost leadership.
event NodeDone: (node: int, fenced: bool);

// Safety: at most one node holds the lease at a time, and storage never accepts a
// write from an older lease after it accepted one from a newer lease. A rejection is
// justified only by such a newer write.
spec NoStaleWrites observes LeaseGranted, LeaseEnded, WriteAccepted, WriteRejected {
  // node -> the number of the lease it was granted (1, 2, ...)
  var leaseOf: map[int, int];
  var grants: int;
  var holder: int;
  var held: bool;
  // newest lease that has had a write accepted
  var newestWritten: int;

  start state Watching {
    on LeaseGranted do (p: (node: int)) {
      assert !held, format("lease granted to node {0} while node {1} holds it", p.node, holder);
      assert !(p.node in leaseOf), format("node {0} granted the lease twice", p.node);
      grants = grants + 1;
      leaseOf[p.node] = grants;
      holder = p.node;
      held = true;
    }
    on LeaseEnded do (p: (node: int)) {
      assert held && holder == p.node, format("lease of node {0} ended, but it does not hold the lease", p.node);
      held = false;
    }
    on WriteAccepted do (p: (node: int, value: int)) {
      assert p.node in leaseOf, format("write {0} accepted from node {1}, which never held the lease", p.value, p.node);
      assert leaseOf[p.node] >= newestWritten,
        format("stale write: storage accepted value {0} from node {1} under lease {2} after a write under lease {3}",
          p.value, p.node, leaseOf[p.node], newestWritten);
      newestWritten = leaseOf[p.node];
    }
    on WriteRejected do (p: (node: int, value: int)) {
      assert p.node in leaseOf, format("write {0} from node {1}, which never held the lease", p.value, p.node);
      assert leaseOf[p.node] < newestWritten,
        format("storage rejected value {0} from node {1} under lease {2}, but no newer lease has written",
          p.value, p.node, leaseOf[p.node]);
    }
  }
}

// Progress: every node the driver starts gets the lease, has each value it planned
// written in order, until storage accepts them all or rejects one, and stops,
// reporting fenced exactly when storage rejected it. Hot while any node is unfinished.
spec NodesFinish observes NodesPlanned, NodeStarted, LeaseGranted, WriteAccepted, WriteRejected, NodeDone {
  // node -> plan still to be decided, in order
  var owed: map[int, seq[int]];
  var granted: set[int];
  var rejected: set[int];
  var unfinished: int;

  start state Idle {
    on NodesPlanned do (n: int) {
      assert n > 0, "planned no nodes";
      unfinished = n;
      goto NodesRunning;
    }
    on NodeStarted do { assert false, "node started without a plan"; }
    on LeaseGranted do { assert false, "lease granted with no node running"; }
    on WriteAccepted do { assert false, "write with no node running"; }
    on WriteRejected do { assert false, "write with no node running"; }
    on NodeDone do { assert false, "node stopped with none running"; }
  }

  hot state NodesRunning {
    on NodesPlanned do { assert false, "nodes planned twice"; }
    on NodeStarted do (p: (node: int, plan: seq[int])) {
      assert !(p.node in owed), format("node {0} started twice", p.node);
      owed[p.node] = p.plan;
    }
    on LeaseGranted do (p: (node: int)) {
      assert p.node in owed, format("lease granted to node {0}, which was never started", p.node);
      granted += (p.node);
    }
    on WriteAccepted do (p: (node: int, value: int)) { Decided(p.node, p.value); }
    on WriteRejected do (p: (node: int, value: int)) {
      Decided(p.node, p.value);
      rejected += (p.node);
    }
    on NodeDone do (p: (node: int, fenced: bool)) {
      assert p.node in granted, format("node {0} stopped without ever getting the lease", p.node);
      assert p.fenced == (p.node in rejected),
        format("node {0} reported fenced = {1}, but storage decided otherwise", p.node, p.fenced);
      assert p.fenced || sizeof(owed[p.node]) == 0,
        format("node {0} stopped with {1} writes undecided", p.node, sizeof(owed[p.node]));
      owed -= (p.node);
      unfinished = unfinished - 1;
      if (unfinished == 0) { goto Idle; }
    }
  }

  fun Decided(node: int, value: int) {
    assert node in owed && sizeof(owed[node]) > 0, format("storage decided a write node {0} never sent", node);
    assert value == owed[node][0],
      format("storage decided value {0} from node {1}, but its next planned value is {2}", value, node, owed[node][0]);
    owed[node] -= (0);
  }
}
