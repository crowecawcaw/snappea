// Drivers start a cluster, then make some nodes' election timers fire
// (eStartElection, with the membership), as after a leader failure.

fun StartCandidates(nodes: map[int, machine], candidates: seq[int]) {
  var c: int;
  announce eClusterStarted, sizeof(nodes);
  foreach (c in candidates) {
    announce eCandidateStarted, c;
  }
  foreach (c in candidates) {
    send nodes[c], eStartElection, nodes;
  }
}

// Four nodes; nodes 1 and 3 time out together. Each voter votes for whichever request
// reaches it first.
machine SplitVoteHalfQuorum {
  start state Init {
    entry {
      var nodes: map[int, machine];
      var id: int;
      var candidates: seq[int];
      id = 1;
      while (id <= 4) {
        nodes[id] = new HalfQuorumNode(id);
        id = id + 1;
      }
      candidates += (0, 1);
      candidates += (1, 3);
      StartCandidates(nodes, candidates);
    }
  }
}

machine SplitVote {
  start state Init {
    entry {
      var nodes: map[int, machine];
      var id: int;
      var candidates: seq[int];
      id = 1;
      while (id <= 4) {
        nodes[id] = new MajorityNode(id);
        id = id + 1;
      }
      candidates += (0, 1);
      candidates += (1, 3);
      StartCandidates(nodes, candidates);
    }
  }
}

// The network across a partition: a request to a node on the other side fails, as an
// RPC to an unreachable host does after its timeout.
machine Unreachable {
  start state Cut {
    on eRequestVote do (r: tVoteReq) {
      announce eRequestFailed, (term = r.term, voter = r.voter, candidate = r.candidate);
      send r.from, eVoteResp, (granted = false,);
    }
  }
}

// Gives the candidate the cluster as its side of the partition sees it: nodes on the
// same side are real, requests to the other side go into the cut.
fun ViewFrom(nodes: map[int, machine], side: set[int], cut: Unreachable): map[int, machine] {
  var view: map[int, machine];
  var id: int;
  foreach (id in keys(nodes)) {
    if (id in side) {
      view[id] = nodes[id];
    } else {
      view[id] = cut;
    }
  }
  return view;
}

// A network partition splits four nodes into {1, 2} and {3, 4}. The leader was on
// one side; both sides' election timers fire.
fun Partition(nodes: map[int, machine]) {
  var left: set[int];
  var right: set[int];
  var cut: Unreachable;
  cut = new Unreachable();
  left += (1);
  left += (2);
  right += (3);
  right += (4);
  announce eClusterStarted, sizeof(nodes);
  announce eCandidateStarted, 1;
  announce eCandidateStarted, 3;
  send nodes[1], eStartElection, ViewFrom(nodes, left, cut);
  send nodes[3], eStartElection, ViewFrom(nodes, right, cut);
}

machine PartitionHalfQuorum {
  start state Init {
    entry {
      var nodes: map[int, machine];
      var id: int;
      id = 1;
      while (id <= 4) {
        nodes[id] = new HalfQuorumNode(id);
        id = id + 1;
      }
      Partition(nodes);
    }
  }
}

machine PartitionMajority {
  start state Init {
    entry {
      var nodes: map[int, machine];
      var id: int;
      id = 1;
      while (id <= 4) {
        nodes[id] = new MajorityNode(id);
        id = id + 1;
      }
      Partition(nodes);
    }
  }
}

// Boundary: one candidate in an odd cluster of three. The quorum is 2 of 3, and the
// candidate can win as soon as one other vote arrives.
machine OddCluster {
  start state Init {
    entry {
      var nodes: map[int, machine];
      var id: int;
      var candidates: seq[int];
      id = 1;
      while (id <= 3) {
        nodes[id] = new MajorityNode(id);
        id = id + 1;
      }
      candidates += (0, 2);
      StartCandidates(nodes, candidates);
    }
  }
}

// Covered: a split vote in which both candidates stood in the same term and neither
// won. The fix's answer to a 2-2 split is "no leader this term".
scenario SplitVoteNobodyWins observes eElectionEnded {
  start hot state WaitFirstLoss {
    on eElectionEnded do (p: (term: int, candidate: int, won: bool)) {
      if (!p.won) { goto WaitSecondLoss; }
    }
  }
  hot state WaitSecondLoss {
    on eElectionEnded do (p: (term: int, candidate: int, won: bool)) {
      if (!p.won) { goto Covered; }
    }
  }
  cold state Covered {
    ignore eElectionEnded;
  }
}

// expect-bug: split brain: term 1 has two leaders
test tcBugSplitVote [main=SplitVoteHalfQuorum]:
  assert OneLeaderPerTerm, ElectionsEnd in
  { SplitVoteHalfQuorum, HalfQuorumNode };

// expect-bug: split brain: term 1 has two leaders
test tcBugPartition [main=PartitionHalfQuorum]:
  assert OneLeaderPerTerm, ElectionsEnd in
  { PartitionHalfQuorum, HalfQuorumNode, Unreachable };

test tcFixedPartition [main=PartitionMajority]:
  assert OneLeaderPerTerm, ElectionsEnd in
  { PartitionMajority, MajorityNode, Unreachable };

test tcFixedSplitVote [main=SplitVote]:
  assert OneLeaderPerTerm, ElectionsEnd in
  { SplitVote, MajorityNode };

test tcFixedOddCluster [main=OddCluster]:
  assert OneLeaderPerTerm, ElectionsEnd in
  { OddCluster, MajorityNode };
