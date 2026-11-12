// Raft-style leader election (voting only, no log) among three nodes.
//
// Real atomic primitives, one per handler:
//   timeout:      term + 1, vote for self (persisted), then broadcast RequestVote
//   RequestVote:  adopt a higher term; grant if this term's vote is free or already
//                 the candidate's; persist the vote before replying
//   vote reply:   adopt a higher term, or count a grant for the current term
// `term`, `votedTerm` and `votedFor` are durable (written to disk before any reply).
// Everything else is volatile and lost in a crash.
//
// Crash: eCrash is a crash followed by a restart, in one step. The node loses its
// volatile state and comes back as a follower. RequestVote is retried until answered,
// so requests sent while it was down count as delayed, not lost, and the time it spent
// down is just more message delay, which the scheduler already explores.

// The election timer fired. The timer service hands the candidate the current
// membership (its peers) and whom to tell when it has heard back from all of them.
event eTimeout: (peers: seq[machine], driver: machine);
event eVoteReq: (term: int, candidate: machine, cid: int);
// reqTerm: the term the request was for; term: the voter's term after handling it.
event eVoteResp: (reqTerm: int, term: int, granted: bool);
event eCrash;
// To the driver (the timer service): this node has heard every reply to its
// candidacy for `term` and did not win it.
event eRoundLost: (id: int, term: int);

machine Node {
  // Durable.
  var term: int;
  var votedTerm: int;
  var votedFor: int;
  var id: int;
  // Volatile: the candidacy in progress.
  var peers: seq[machine];
  var driver: machine;
  var electionTerm: int;
  var votes: int;
  var replies: int;
  var won: bool;

  start state Init {
    entry (nodeId: int) {
      id = nodeId;
      goto Follower;
    }
  }

  state Follower {
    on eTimeout goto Candidate;
    // A follower that adopts a higher term stays a follower.
    on eVoteReq do (r: (term: int, candidate: machine, cid: int)) { HandleVoteReq(r); }
    on eVoteResp do (p: (reqTerm: int, term: int, granted: bool)) { HandleReply(p); }
    on eCrash goto Restarted;
  }

  state Candidate {
    entry (t: (peers: seq[machine], driver: machine)) {
      var p: machine;
      peers = t.peers;
      driver = t.driver;
      term = term + 1;
      votedTerm = term;
      votedFor = id;
      announce eVoted, (voter = id, term = term, candidate = id);
      electionTerm = term;
      votes = 1;
      replies = 0;
      won = false;
      foreach (p in peers) {
        send p, eVoteReq, (term = term, candidate = this, cid = id);
      }
    }
    // A new timeout starts a new election in a higher term; replies to the old one are stale.
    on eTimeout goto Candidate;
    on eVoteReq do (r: (term: int, candidate: machine, cid: int)) {
      if (HandleVoteReq(r)) { goto Follower; }
    }
    on eVoteResp do (p: (reqTerm: int, term: int, granted: bool)) {
      if (HandleReply(p)) { goto Follower; }
      // Count only grants for the current term: a reply to an older candidacy is stale.
      if (p.granted && p.reqTerm == term) {
        votes = votes + 1;
        // Majority of three is two, including the candidate's own vote.
        if (votes >= 2) { goto Leader; }
      }
    }
    on eCrash goto Restarted;
  }

  state Leader {
    entry {
      won = true;
      announce eBecameLeader, (id = id, term = term);
    }
    // A leader's heartbeats keep its own timer from firing.
    ignore eTimeout;
    on eVoteReq do (r: (term: int, candidate: machine, cid: int)) {
      if (HandleVoteReq(r)) { goto Follower; }
    }
    on eVoteResp do (p: (reqTerm: int, term: int, granted: bool)) {
      if (HandleReply(p)) { goto Follower; }
    }
    on eCrash goto Restarted;
  }

  // Crash and restart. Durable fields survive. The volatile candidacy fields are
  // reset by the next Candidate entry, before use.
  state Restarted {
    entry { goto Follower; }
  }

  // Returns true when a higher term means this node must step down to follower.
  fun HandleVoteReq(r: (term: int, candidate: machine, cid: int)): bool {
    var grant: bool;
    var stepDown: bool;
    if (r.term > term) {
      term = r.term;
      stepDown = true;
    }
    if (r.term == term && (votedTerm != term || votedFor == r.cid)) {
      votedTerm = term;
      votedFor = r.cid;
      grant = true;
      announce eVoted, (voter = id, term = term, candidate = r.cid);
    }
    send r.candidate, eVoteResp, (reqTerm = r.term, term = term, granted = grant);
    return stepDown;
  }

  // Every reply, in any state, counts toward hearing back from the whole cluster.
  // A higher term in a reply means another election moved on: step down.
  fun HandleReply(p: (reqTerm: int, term: int, granted: bool)): bool {
    if (p.reqTerm == electionTerm) {
      replies = replies + 1;
      if (replies == sizeof(peers) && !won) {
        send driver, eRoundLost, (id = id, term = electionTerm);
      }
    }
    if (p.term > term) {
      term = p.term;
      return true;
    }
    return false;
  }
}
