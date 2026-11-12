// The election contract, stated over events alone. The spec knows the cluster size
// from the driver and counts votes itself; it never reads a node's quorum setting.

// The driver announces the cluster size, then each node it asks to stand for election.
event eClusterStarted: int;
event eCandidateStarted: int;
// A candidate announces the term it stands in; a node announces each vote it grants
// (a candidate votes for itself); a candidate announces how its election ended.
event eCandidacy: (candidate: int, term: int);
event eVoteGranted: (term: int, voter: int, candidate: int);
event eElectionEnded: (term: int, candidate: int, won: bool);

// Safety: a node grants at most one vote per term, a vote goes only to a node standing
// in that term, and a term has at most one leader.
spec OneLeaderPerTerm observes eCandidacy, eVoteGranted, eElectionEnded {
  // term -> candidates standing in it
  var standing: map[int, set[int]];
  // term -> voter -> candidate it voted for
  var votes: map[int, map[int, int]];
  // term -> leader
  var leaders: map[int, int];

  start state Watching {
    on eCandidacy do (p: (candidate: int, term: int)) {
      if (!(p.term in standing)) {
        standing[p.term] = default(set[int]);
        votes[p.term] = default(map[int, int]);
      }
      standing[p.term] += (p.candidate);
    }
    on eVoteGranted do (p: (term: int, voter: int, candidate: int)) {
      assert p.term in standing && p.candidate in standing[p.term],
        format("node {0} voted for node {1} in term {2}, which it is not standing in", p.voter, p.candidate, p.term);
      assert !(p.voter in votes[p.term]),
        format("node {0} voted twice in term {1} (for {2} and {3})",
          p.voter, p.term, votes[p.term][p.voter], p.candidate);
      votes[p.term][p.voter] = p.candidate;
    }
    on eElectionEnded do (p: (term: int, candidate: int, won: bool)) {
      if (!p.won) { return; }
      assert !(p.term in leaders),
        format("split brain: term {0} has two leaders (node {1} and node {2})", p.term, leaders[p.term], p.candidate);
      leaders[p.term] = p.candidate;
    }
  }
}

// Progress: every node the driver asks to stand ends its election, and a candidate
// that a majority of the cluster voted for does not report a loss. Hot while any
// election is open.
spec ElectionsEnd observes eClusterStarted, eCandidateStarted, eCandidacy, eVoteGranted, eElectionEnded {
  var size: int;
  // candidate -> term it stands in, while its election is open
  var open: map[int, int];
  var started: set[int];
  // candidate -> votes it got in its term
  var grants: map[int, int];

  start state Idle {
    on eClusterStarted do (n: int) {
      assert n > 0, "empty cluster";
      size = n;
      goto ElectionOpen;
    }
    on eCandidateStarted do { assert false, "candidate before the cluster started"; }
    on eCandidacy do { assert false, "candidacy before the cluster started"; }
    on eVoteGranted do { assert false, "vote before the cluster started"; }
    on eElectionEnded do { assert false, "election ended before the cluster started"; }
  }

  hot state ElectionOpen {
    on eClusterStarted do { assert false, "cluster started twice"; }
    on eCandidateStarted do (c: int) {
      assert !(c in started), format("node {0} asked to stand twice", c);
      started += (c);
    }
    on eCandidacy do (p: (candidate: int, term: int)) {
      assert p.candidate in started, format("node {0} stood without being asked", p.candidate);
      open[p.candidate] = p.term;
      grants[p.candidate] = 0;
    }
    on eVoteGranted do (p: (term: int, voter: int, candidate: int)) {
      assert p.voter >= 1 && p.voter <= size, format("vote from node {0}, outside the cluster", p.voter);
      if (p.candidate in open && open[p.candidate] == p.term) {
        grants[p.candidate] = grants[p.candidate] + 1;
      }
    }
    on eElectionEnded do (p: (term: int, candidate: int, won: bool)) {
      assert p.candidate in open && open[p.candidate] == p.term,
        format("node {0} ended an election in term {1} it was not running", p.candidate, p.term);
      assert p.won || grants[p.candidate] * 2 <= size,
        format("node {0} gave up term {1} with {2} of {3} votes, a majority", p.candidate, p.term,
          grants[p.candidate], size);
      open -= (p.candidate);
      started -= (p.candidate);
      if (sizeof(started) == 0) { goto Idle; }
    }
  }
}
