// The election contract, stated over events nodes announce at their commit points
// (a vote is announced when it is persisted; leadership when a node becomes leader).

event eVoted: (voter: int, term: int, candidate: int);
event eBecameLeader: (id: int, term: int);

// Safety:
//  - a node never votes for two different candidates in one term;
//  - a node becomes leader only with votes from a majority (two of three) in its term,
//    counted from the vote history, not from the candidate's own tally;
//  - at most one leader per term.
spec OneLeaderPerTerm observes eVoted, eBecameLeader {
  // term * 10 + voter -> candidate it voted for
  var voteOf: map[int, int];
  // term * 10 + candidate -> number of distinct voters for it
  var tally: map[int, int];
  // term -> leader
  var leaderOf: map[int, int];

  start state Watching {
    on eVoted do (v: (voter: int, term: int, candidate: int)) {
      var key: int;
      key = v.term * 10 + v.voter;
      if (key in voteOf) {
        assert voteOf[key] == v.candidate,
          format("node {0} voted twice in term {1}: for {2} and for {3}", v.voter, v.term, voteOf[key], v.candidate);
      } else {
        voteOf[key] = v.candidate;
        key = v.term * 10 + v.candidate;
        if (!(key in tally)) { tally[key] = 0; }
        tally[key] = tally[key] + 1;
      }
    }
    on eBecameLeader do (l: (id: int, term: int)) {
      assert !(l.term in leaderOf),
        format("two leaders in term {0}: {1} and {2}", l.term, leaderOf[l.term], l.id);
      assert (l.term * 10 + l.id) in tally && tally[l.term * 10 + l.id] >= 2,
        format("node {0} became leader of term {1} without a majority of votes", l.id, l.term);
      leaderOf[l.term] = l.id;
    }
  }
}

// Progress (bounded): once the driver starts the elections, some node becomes leader.
// This holds because the driver keeps its promise: at most one crash, the crashed node
// restarts, every request is eventually answered, and if the contested round elects
// nobody, one node's timer fires alone after every earlier message was handled.
event eElectionsStarted;

spec SomeLeaderElected observes eElectionsStarted, eBecameLeader {
  start state Idle {
    on eElectionsStarted goto Electing;
    on eBecameLeader do { assert false, "leader elected before elections started"; }
  }
  hot state Electing {
    on eElectionsStarted do { assert false, "elections started twice"; }
    on eBecameLeader goto Elected;
  }
  state Elected {
    on eElectionsStarted do { assert false, "elections started twice"; }
    ignore eBecameLeader;
  }
}
