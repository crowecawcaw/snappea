// The election contract, stated over events nodes announce at their commit points
// (a candidacy and every RequestVote answer are announced once persisted; leadership
// when a node becomes leader) and events the driver announces about what it set up
// and what it was told.

// Driver: how many nodes the cluster has (reachable or not), before any node starts.
event ClusterSize: int;
// Driver: a node starts from this durable state.
event Disk: (id: int, term: int, votedTerm: int);
// A node stood for election in `term`, voting for itself.
event ElectionStarted: (id: int, term: int);
// A node answered a RequestVote for `reqTerm`; `term` is its term afterwards.
event VoteAnswered: (voter: int, candidate: int, reqTerm: int, term: int, granted: bool);
event BecameLeader: (id: int, term: int);
// A candidate has heard a reply from every peer for its candidacy in `term`.
event HeardAll: (id: int, term: int);
// Driver: a candidate reported that it lost its candidacy in `term`.
event LossReported: (id: int, term: int);
// Driver: it sent a client request ...
event ProbeSent;
// ... and node `id` accepted or refused it, in `term`.
event ClientAnswered: (id: int, term: int, accepted: bool);

// Safety:
//  - a node never votes twice in one term (its own candidacy counts as a vote);
//  - a node's term never goes back, and it never grants a vote for an older term;
//  - a node becomes leader only with votes from a majority of the cluster in its
//    term, counted from the answers, not from the candidate's own tally;
//  - at most one leader per term;
//  - a node does not report losing a term it won, or one where a majority voted for
//    it and it saw no higher term;
//  - after losing, a node stands next in a term above every term it was told of;
//  - a node accepts a client request only as leader of its current term (a deposed
//    leader must stop), and refuses it otherwise.
spec ElectionSafety observes ClusterSize, Disk, ElectionStarted, VoteAnswered, BecameLeader, LossReported,
    ClientAnswered {
  // Votes needed to win: more than half the cluster.
  var majority: int;
  // term * 10 + voter -> candidate it voted for
  var voteOf: map[int, int];
  // term * 10 + candidate -> votes for it
  var tally: map[int, int];
  // term -> leader
  var leaderOf: map[int, int];
  // node -> highest term it has announced
  var termOf: map[int, int];
  // term * 10 + candidate -> its candidacy saw a higher term
  var disturbed: set[int];
  // term * 10 + candidate -> highest term in the answers to that candidacy
  var answeredTerm: map[int, int];
  // node -> its next candidacy must be above this (set when it reports a loss)
  var floor: map[int, int];
  // node -> term of its latest candidacy
  var stoodTerm: map[int, int];

  start state Watching {
    on ClusterSize do (n: int) {
      assert majority == 0, "cluster size announced twice";
      majority = n / 2 + 1;
    }
    on Disk do (d: (id: int, term: int, votedTerm: int)) {
      termOf[d.id] = d.term;
      if (d.votedTerm > 0) { Vote(d.id, d.votedTerm, d.id); }
    }
    on ElectionStarted do (s: (id: int, term: int)) {
      if (s.id in floor) {
        assert s.term > floor[s.id],
          format("node {0} stood in term {1} although it had been told of term {2}", s.id, s.term, floor[s.id]);
      }
      assert s.term > termOf[s.id],
        format("node {0} stood in term {1}, not above its term {2}", s.id, s.term, termOf[s.id]);
      Advance(s.id, s.term);
      Vote(s.id, s.term, s.id);
      stoodTerm[s.id] = s.term;
    }
    on VoteAnswered do (a: (voter: int, candidate: int, reqTerm: int, term: int, granted: bool)) {
      if (a.granted) {
        assert a.reqTerm == a.term,
          format("node {0} granted a vote for term {1} while in term {2}", a.voter, a.reqTerm, a.term);
        Vote(a.voter, a.term, a.candidate);
      }
      Advance(a.voter, a.term);
      // A voter that stood in an older term has now seen a higher one; so has a
      // candidate whose voter answers with a higher term.
      if (a.voter in stoodTerm && a.reqTerm > stoodTerm[a.voter]) {
        disturbed += (stoodTerm[a.voter] * 10 + a.voter);
      }
      if (a.term > a.reqTerm) { disturbed += (a.reqTerm * 10 + a.candidate); }
      if (!((a.reqTerm * 10 + a.candidate) in answeredTerm) || answeredTerm[a.reqTerm * 10 + a.candidate] < a.term) {
        answeredTerm[a.reqTerm * 10 + a.candidate] = a.term;
      }
    }
    on BecameLeader do (l: (id: int, term: int)) {
      assert !(l.term in leaderOf),
        format("two leaders in term {0}: {1} and {2}", l.term, leaderOf[l.term], l.id);
      assert majority > 0, "a leader was elected before the cluster size was announced";
      assert (l.term * 10 + l.id) in tally && tally[l.term * 10 + l.id] >= majority,
        format("node {0} became leader of term {1} without a majority of votes", l.id, l.term);
      leaderOf[l.term] = l.id;
    }
    on LossReported do (r: (id: int, term: int)) {
      var key: int;
      key = r.term * 10 + r.id;
      assert !(r.term in leaderOf && leaderOf[r.term] == r.id),
        format("node {0} reported losing term {1}, which it won", r.id, r.term);
      assert (key in disturbed) || !(key in tally) || tally[key] < majority,
        format("node {0} reported losing term {1} although a majority voted for it and it saw no higher term", r.id, r.term);
      floor[r.id] = termOf[r.id];
      if (answeredTerm[key] > floor[r.id]) { floor[r.id] = answeredTerm[key]; }
    }
    on ClientAnswered do (a: (id: int, term: int, accepted: bool)) {
      // A node that adopted a higher term and kept acting as leader would accept in a
      // term it did not win.
      if (a.accepted) {
        assert a.term in leaderOf && leaderOf[a.term] == a.id,
          format("node {0} accepted a client request as leader of term {1}, which it did not win", a.id, a.term);
      } else {
        assert !(a.term in leaderOf && leaderOf[a.term] == a.id),
          format("node {0} refused a client request while leader of term {1}", a.id, a.term);
      }
      Advance(a.id, a.term);
    }
  }

  fun Vote(voter: int, term: int, candidate: int) {
    var key: int;
    key = term * 10 + voter;
    assert !(key in voteOf),
      format("node {0} voted twice in term {1}: for {2} and for {3}", voter, term, voteOf[key], candidate);
    voteOf[key] = candidate;
    key = term * 10 + candidate;
    if (!(key in tally)) { tally[key] = 0; }
    tally[key] = tally[key] + 1;
  }

  fun Advance(id: int, term: int) {
    assert term >= termOf[id], format("node {0} went back from term {1} to term {2}", id, termOf[id], term);
    termOf[id] = term;
  }
}

// Progress (bounded): once the driver starts the elections, some node becomes leader.
// This holds because the driver keeps its promise: at most one crash, every request is
// eventually answered, and if every candidate of the contested round loses, one
// node's timer fires alone after every earlier message was handled.
event ElectionsStarted;

spec SomeLeaderElected observes ElectionsStarted, BecameLeader {
  start state Idle {
    on ElectionsStarted goto Electing;
    on BecameLeader do { assert false, "leader elected before elections started"; }
  }
  hot state Electing {
    on ElectionsStarted do { assert false, "elections started twice"; }
    on BecameLeader goto Elected;
  }
  state Elected {
    on ElectionsStarted do { assert false, "elections started twice"; }
    ignore BecameLeader;
  }
}

// Progress: every candidacy hears back from both peers (each RequestVote is answered,
// by followers, candidates and leaders alike), unless the same node has since started
// a newer candidacy. Not selected where a peer is deliberately unreachable.
spec CandidaciesConclude observes ClusterSize, ElectionStarted, VoteAnswered, HeardAll {
  // Replies a candidacy waits for: one per peer.
  var peers: int;
  // candidate -> term of its latest candidacy still waiting for replies
  var open: map[int, int];
  // term * 10 + candidate -> voters that answered it
  var answers: map[int, set[int]];

  // No candidacy is open. Requests reach each voter in the order they were sent, so
  // every answer to a superseded candidacy comes before the answers that close the
  // newer one.
  start state Quiet {
    on ClusterSize do (n: int) { peers = n - 1; }
    on ElectionStarted do (s: (id: int, term: int)) { Stood(s.id, s.term); }
    on VoteAnswered do { assert false, "a request was answered while no candidacy was open"; }
    on HeardAll do { assert false, "a candidate heard all replies while no candidacy was open"; }
  }

  hot state Waiting {
    on ElectionStarted do (s: (id: int, term: int)) { Stood(s.id, s.term); }
    on VoteAnswered do (a: (voter: int, candidate: int, reqTerm: int, term: int, granted: bool)) {
      Answered(a);
    }
    on HeardAll do (h: (id: int, term: int)) { Heard(h.id, h.term); }
  }

  fun Stood(id: int, term: int) {
    // A new candidacy supersedes the node's older one.
    open[id] = term;
    answers[term * 10 + id] = default(set[int]);
    goto Waiting;
  }

  fun Answered(a: (voter: int, candidate: int, reqTerm: int, term: int, granted: bool)) {
    var key: int;
    key = a.reqTerm * 10 + a.candidate;
    assert !(a.voter in answers[key]),
      format("node {0} answered node {1}'s term-{2} request twice", a.voter, a.candidate, a.reqTerm);
    answers[key] += (a.voter);
  }

  fun Heard(id: int, term: int) {
    assert id in open && open[id] == term,
      format("node {0} heard all replies for term {1}, which is not its open candidacy", id, term);
    assert sizeof(answers[term * 10 + id]) == peers,
      format("node {0} claims all replies for term {1}, but {2} of {3} were sent", id, term, sizeof(answers[term * 10 + id]), peers);
    open -= (id);
    if (sizeof(open) == 0) { goto Quiet; }
  }
}

// Progress: the client request is answered, accepted or refused.
spec ProbeAnswered observes ProbeSent, ClientAnswered {
  start state Idle {
    on ProbeSent goto Waiting;
    on ClientAnswered do { assert false, "a client request was answered before it was sent"; }
  }

  hot state Waiting {
    on ProbeSent do { assert false, "more than one client request sent"; }
    on ClientAnswered goto Answered;
  }

  state Answered {
    on ProbeSent do { assert false, "more than one client request sent"; }
    on ClientAnswered do { assert false, "more answers than client requests"; }
  }
}
