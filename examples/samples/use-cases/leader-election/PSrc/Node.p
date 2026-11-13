// Raft-style leader election (voting only, no log).
//
// Real atomic primitives, one handler each:
//   timeout:      term + 1 and a vote for itself (both persisted), then RequestVote
//                 to every peer
//   RequestVote:  adopt a higher term; grant if this node has not voted in the
//                 request's term; persist the vote, then reply
//   vote reply:   adopt a higher term, or count a grant for the current term
// `term` and `votedTerm` are durable: written to disk before any reply. Everything
// else is volatile. (Raft also persists `votedFor` so a retried request from the same
// candidate is granted again; this model has no retries, so "voted in this term" is
// the whole rule.)
//
// Crash: eCrash is a crash followed by a restart, in one step, and only followers
// crash in these tests. RequestVote is retried until answered, so requests sent while
// the node was down count as delayed, not lost, and the time it spent down is just
// more message delay, which the scheduler already explores. What a crash can break is
// what the node remembers: only durable fields survive.

// The election timer fired. The timer service hands the candidate the current
// membership (its peers) and whom to tell if it loses.
event eTimeout: (peers: seq[machine], driver: machine);
event eVoteReq: (term: int, candidate: machine, cid: int);
// reqTerm: the term the request was for; term: the voter's term after handling it.
event eVoteResp: (reqTerm: int, term: int, granted: bool);
event eCrash;
// To the timer service: this node heard every reply to its candidacy for `term` and
// did not win. When every candidate of a round has lost, the service lets one timer
// fire alone.
event eRoundLost: (id: int, term: int);

// A client request, such as a write, that only the leader may accept. The node
// announces its answer (eClientAnswered); the reply to the client is not modeled.
event eClientReq;

// Durable state a node starts from (all zero for a fresh node).
type tDisk = (id: int, term: int, votedTerm: int);

machine Node {
  // Durable.
  var id: int;
  var term: int;
  var votedTerm: int;
  // Volatile: the candidacy in progress.
  var peers: seq[machine];
  var driver: machine;
  var electionTerm: int;
  var votes: int;
  var replies: int;
  // The last term this node won; a candidacy in any other term is lost.
  var leaderTerm: int;

  start state Init {
    entry (d: tDisk) {
      id = d.id;
      term = d.term;
      votedTerm = d.votedTerm;
      goto Follower;
    }
  }

  state Follower {
    on eTimeout goto Candidate;
    // A follower that adopts a higher term stays a follower.
    on eVoteReq do (r: (term: int, candidate: machine, cid: int)) { HandleVoteReq(r); }
    on eVoteResp do (p: (reqTerm: int, term: int, granted: bool)) { HandleReply(p); }
    on eClientReq do { Answer(false); }
    // Crash and restart. A follower's volatile fields are unused until its next
    // candidacy resets them, and durable fields survive, so nothing changes.
    ignore eCrash;
  }

  state Candidate {
    entry (t: (peers: seq[machine], driver: machine)) {
      var p: machine;
      peers = t.peers;
      driver = t.driver;
      term = term + 1;
      votedTerm = term;
      announce eStood, (id = id, term = term);
      electionTerm = term;
      votes = 1;
      replies = 0;
      foreach (p in peers) {
        send p, eVoteReq, (term = term, candidate = this, cid = id);
      }
    }
    // A new timeout starts a new election in a higher term; replies to the old one are stale.
    on eTimeout goto Candidate;
    on eVoteReq do (r: (term: int, candidate: machine, cid: int)) {
      if (HandleVoteReq(r)) {
        // snappea-lint: allow mutation:remove-statement -- a candidate that saw a higher term acts like a follower: it counts only replies for its own term, refuses clients and stands again on its next timeout
        goto Follower;
      }
    }
    on eVoteResp do (p: (reqTerm: int, term: int, granted: bool)) {
      // Count the grant before HandleReply concludes the round, so the last reply can
      // be the winning one. Only grants for the current term count: a reply to an
      // older candidacy is stale.
      if (p.granted && p.reqTerm == term) {
        votes = votes + 1;
        // A majority of the cluster (the peers and this node), counting its own vote.
        if (votes * 2 > sizeof(peers) + 1) { leaderTerm = term; }
      }
      if (HandleReply(p)) {
        // snappea-lint: allow mutation:remove-statement -- a candidate that saw a higher term acts like a follower: it counts only replies for its own term, refuses clients and stands again on its next timeout
        goto Follower;
      }
      if (leaderTerm == term) { goto Leader; }
    }
    on eClientReq do { Answer(false); }
  }

  state Leader {
    entry {
      announce eBecameLeader, (id = id, term = term);
    }
    // A leader's heartbeats keep its own timer from firing.
    ignore eTimeout;
    // Only the leader accepts client requests. A leader that missed a step-down would
    // accept them in a term it did not win.
    on eClientReq do { Answer(true); }
    on eVoteReq do (r: (term: int, candidate: machine, cid: int)) {
      if (HandleVoteReq(r)) {
        goto Follower;
      }
    }
    on eVoteResp do (p: (reqTerm: int, term: int, granted: bool)) {
      if (HandleReply(p)) {
        goto Follower;
      }
    }
  }

  // Accept a client request as leader, or refuse it (a real node would redirect to
  // the leader it knows). The answer is the commit point.
  fun Answer(accepted: bool) {
    announce eClientAnswered, (id = id, term = term, accepted = accepted);
  }

  // Returns true when a higher term means this node must step down to follower.
  fun HandleVoteReq(r: (term: int, candidate: machine, cid: int)): bool {
    var grant: bool;
    var stepDown: bool;
    if (r.term > term) {
      term = r.term;
      stepDown = true;
    }
    if (r.term == term && votedTerm != term) {
      votedTerm = term;
      grant = true;
    }
    announce eAnswered, (voter = id, candidate = r.cid, reqTerm = r.term, term = term, granted = grant);
    send r.candidate, eVoteResp, (reqTerm = r.term, term = term, granted = grant);
    return stepDown;
  }

  // Every reply, in any role, counts toward hearing back from the whole cluster.
  // A higher term in a reply means another election moved on: step down.
  fun HandleReply(p: (reqTerm: int, term: int, granted: bool)): bool {
    if (p.reqTerm == electionTerm) {
      replies = replies + 1;
      if (replies == sizeof(peers)) {
        announce eHeardAll, (id = id, term = electionTerm);
        if (leaderTerm != electionTerm) { send driver, eRoundLost, (id = id, term = electionTerm); }
      }
    }
    if (p.term > term) {
      term = p.term;
      return true;
    }
    return false;
  }
}
