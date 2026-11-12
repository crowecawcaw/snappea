// The election contract, stated over events alone. The spec knows the cluster size
// from the driver and counts votes itself; it never reads a node's quorum setting.

// The driver announces the cluster size, then each node whose election timer it fires.
event eClusterStarted: int;
event eCandidateStarted: int;
// A candidate announces the term it stands in. Every node announces each vote it
// grants or denies, when it decides (a candidate votes for itself). A candidate
// announces how its election ended.
event eCandidacy: (candidate: int, term: int);
event eVoteGranted: (term: int, voter: int, candidate: int);
event eVoteDenied: (term: int, voter: int, candidate: int);
event eElectionEnded: (term: int, candidate: int, won: bool);
// The network announces a vote request that failed because the node is unreachable.
event eRequestFailed: (term: int, voter: int, candidate: int);

// Safety: the voting rule and its consequence. A node votes only in terms newer than
// any it voted in before (so at most once per term), and only for a node standing in
// that term; it may deny a vote only if it already voted in that term or a newer one.
// A candidate stands only in a term newer than any it voted in. A term has at most
// one leader.
spec OneLeaderPerTerm observes eCandidacy, eVoteGranted, eVoteDenied, eElectionEnded {
  // term -> candidates standing in it
  var standing: map[int, set[int]];
  // node -> newest term it voted in (absent: never voted)
  var lastVoted: map[int, int];
  // term -> leader
  var leaders: map[int, int];

  start state Watching {
    on eCandidacy do (p: (candidate: int, term: int)) {
      assert !(p.candidate in lastVoted) || p.term > lastVoted[p.candidate],
        format("node {0} stood in term {1} after voting in term {2}", p.candidate, p.term, lastVoted[p.candidate]);
      if (!(p.term in standing)) { standing[p.term] = default(set[int]); }
      standing[p.term] += (p.candidate);
    }
    on eVoteGranted do (p: (term: int, voter: int, candidate: int)) {
      assert p.term in standing && p.candidate in standing[p.term],
        format("node {0} voted for node {1} in term {2}, which it is not standing in", p.voter, p.candidate, p.term);
      assert !(p.voter in lastVoted) || p.term > lastVoted[p.voter],
        format("node {0} voted in term {1} after voting in term {2}", p.voter, p.term, lastVoted[p.voter]);
      lastVoted[p.voter] = p.term;
    }
    on eVoteDenied do (p: (term: int, voter: int, candidate: int)) {
      assert p.voter in lastVoted && p.term <= lastVoted[p.voter],
        format("node {0} denied node {1} in term {2}, although it had not voted in that term",
          p.voter, p.candidate, p.term);
    }
    on eElectionEnded do (p: (term: int, candidate: int, won: bool)) {
      if (!p.won) { return; }
      assert !(p.term in leaders),
        format("split brain: term {0} has two leaders (node {1} and node {2})", p.term, leaders[p.term], p.candidate);
      leaders[p.term] = p.candidate;
    }
  }
}

// Progress: every node whose timer fired ends its election, after voting for itself.
// Each node answers a candidate once (an unreachable node "answers" with a failed
// request). A candidate gives up only once every node has answered, and never gives up an election that a majority of the cluster voted for.
// Hot while any election is open.
spec ElectionsEnd observes eClusterStarted, eCandidateStarted, eCandidacy, eVoteGranted, eVoteDenied,
    eRequestFailed, eElectionEnded {
  var size: int;
  var unfinished: set[int];
  // candidate -> term it stands in
  var termOf: map[int, int];
  // candidate -> votes it got in that term
  var grants: map[int, int];
  // candidate -> nodes that answered it, itself included
  var answered: map[int, set[int]];
  var selfVoted: set[int];

  start state Idle {
    on eClusterStarted do (n: int) {
      assert n > 0, "empty cluster";
      size = n;
      goto ElectionOpen;
    }
    on eCandidateStarted do { assert false, "candidate before the cluster started"; }
    on eCandidacy do { assert false, "candidacy before the cluster started"; }
    on eVoteGranted do { assert false, "vote before the cluster started"; }
    on eVoteDenied do { assert false, "vote before the cluster started"; }
    on eRequestFailed do { assert false, "request before the cluster started"; }
    on eElectionEnded do { assert false, "election ended before the cluster started"; }
  }

  hot state ElectionOpen {
    on eClusterStarted do { assert false, "cluster started twice"; }
    on eCandidateStarted do (c: int) {
      assert !(c in unfinished), format("node {0} asked to stand twice", c);
      unfinished += (c);
    }
    on eCandidacy do (p: (candidate: int, term: int)) {
      assert p.candidate in unfinished, format("node {0} stood without its timer firing", p.candidate);
      termOf[p.candidate] = p.term;
      grants[p.candidate] = 0;
      answered[p.candidate] = default(set[int]);
    }
    on eVoteGranted do (p: (term: int, voter: int, candidate: int)) { Count(p.term, p.voter, p.candidate); }
    on eVoteDenied do (p: (term: int, voter: int, candidate: int)) { Answered(p.term, p.voter, p.candidate); }
    // An unreachable node counts as an answer that is not a vote: the RPC failed.
    on eRequestFailed do (p: (term: int, voter: int, candidate: int)) { Answered(p.term, p.voter, p.candidate); }
    on eElectionEnded do (p: (term: int, candidate: int, won: bool)) {
      assert p.candidate in unfinished && termOf[p.candidate] == p.term,
        format("node {0} ended an election in term {1} it was not running", p.candidate, p.term);
      assert p.candidate in selfVoted, format("node {0} ended its election without voting for itself", p.candidate);
      if (!p.won) {
        assert sizeof(answered[p.candidate]) == size,
          format("node {0} gave up term {1} after only {2} of {3} nodes answered",
            p.candidate, p.term, sizeof(answered[p.candidate]), size);
        assert grants[p.candidate] * 2 <= size,
          format("node {0} gave up term {1}, but {2} of {3} nodes voted for it, a majority",
            p.candidate, p.term, grants[p.candidate], size);
      }
      unfinished -= (p.candidate);
      if (sizeof(unfinished) == 0) { goto AllEnded; }
    }
  }

  state AllEnded {
    on eVoteGranted do { assert false, "vote after every election ended"; }
    on eVoteDenied do { assert false, "vote after every election ended"; }
    on eRequestFailed do { assert false, "request after every election ended"; }
    on eClusterStarted do { assert false, "cluster started twice"; }
    on eCandidateStarted do { assert false, "candidate after every election ended"; }
    on eCandidacy do { assert false, "candidacy after every election ended"; }
    on eElectionEnded do { assert false, "election ended twice"; }
  }

  fun Count(term: int, voter: int, candidate: int) {
    Answered(term, voter, candidate);
    if (voter == candidate) { selfVoted += (candidate); }
    grants[candidate] = grants[candidate] + 1;
  }

  fun Answered(term: int, voter: int, candidate: int) {
    assert voter >= 1 && voter <= size, format("vote from node {0}, outside the cluster", voter);
    assert candidate in termOf && termOf[candidate] == term,
      format("node {0} answered node {1} for term {2}, which it is not standing in", voter, candidate, term);
    assert !(voter in answered[candidate]),
      format("node {0} answered node {1} twice in term {2}", voter, candidate, term);
    answered[candidate] += (voter);
  }
}
