// Focused negative control: the same restart/voting protocol as Node, but a vote
// kept only in memory is lost on restart. Its test crashes a non-candidate voter
// between two requests in the same term, exposing a second vote.
machine ForgetfulNode {
  // Durable.
  var id: int;
  var term: int;
  var votedTerm: int;
  // One record makes the crash boundary explicit: no campaign bookkeeping survives.
  var campaign: (peers: seq[machine], driver: machine, term: int, votes: int,
    replies: int, leaderTerm: int);

  start state Init {
    entry (d: Disk) {
      id = d.id;
      term = d.term;
      votedTerm = d.votedTerm;
      goto Follower;
    }
  }

  state Follower {
    on ElectionTimeout goto Candidate;
    // A follower that adopts a higher term stays a follower.
    on VoteReq do (r: (term: int, candidate: machine, cid: int)) { HandleVoteReq(r); }
    on VoteResp do (p: (reqTerm: int, term: int, granted: bool)) { HandleReply(p); }
    on ClientReq do { Answer(false); }
    on Crash do Restart;
  }

  state Candidate {
    entry (t: (peers: seq[machine], driver: machine)) {
      var p: machine;
      term = term + 1;
      votedTerm = term;
      announce ElectionStarted, (id = id, term = term);
      campaign = (peers = t.peers, driver = t.driver, term = term,
        votes = 1, replies = 0, leaderTerm = 0);
      foreach (p in campaign.peers) {
        send p, VoteReq, (term = term, candidate = this, cid = id);
      }
    }
    // A new timeout starts a new election in a higher term; replies to the old one are stale.
    on ElectionTimeout goto Candidate;
    on VoteReq do (r: (term: int, candidate: machine, cid: int)) {
      if (HandleVoteReq(r)) {
        goto Follower;
      }
    }
    on VoteResp do (p: (reqTerm: int, term: int, granted: bool)) {
      // Count the grant before HandleReply concludes the round, so the last reply can
      // be the winning one. Only grants for the current term count: a reply to an
      // older candidacy is stale.
      if (p.granted && p.reqTerm == term) {
        campaign.votes = campaign.votes + 1;
        // A majority of the cluster (the peers and this node), counting its own vote.
        if (campaign.votes * 2 > sizeof(campaign.peers) + 1) { campaign.leaderTerm = term; }
      }
      if (HandleReply(p)) {
        goto Follower;
      }
      if (campaign.leaderTerm == term) { goto Leader; }
    }
    on ClientReq do { Answer(false); }
    on Crash do Restart;
  }

  state Leader {
    entry {
      announce BecameLeader, (id = id, term = term);
    }
    // A leader's heartbeats keep its own timer from firing.
    ignore ElectionTimeout;
    // Only the leader accepts client requests. A leader that missed a step-down would
    // accept them in a term it did not win.
    on ClientReq do { Answer(true); }
    on Crash do Restart;
    on VoteReq do (r: (term: int, candidate: machine, cid: int)) {
      if (HandleVoteReq(r)) {
        goto Follower;
      }
    }
    on VoteResp do (p: (reqTerm: int, term: int, granted: bool)) {
      if (HandleReply(p)) {
        goto Follower;
      }
    }
  }

  fun Restart(injector: machine) {
    votedTerm = 0; // BUG: this vote should have been durable.
    campaign = default((peers: seq[machine], driver: machine, term: int, votes: int,
      replies: int, leaderTerm: int));
    send injector, Restarted, (id = id, term = term);
    goto Follower;
  }

  // Accept a client request as leader, or refuse it (a real node would redirect to
  // the leader it knows). The answer is the commit point.
  fun Answer(accepted: bool) {
    announce ClientAnswered, (id = id, term = term, accepted = accepted);
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
    announce VoteAnswered, (voter = id, candidate = r.cid, reqTerm = r.term, term = term, granted = grant);
    send r.candidate, VoteResp, (reqTerm = r.term, term = term, granted = grant);
    return stepDown;
  }

  // Replies to an intact campaign count in any role. Pre-crash replies are stale.
  // A higher term in a reply means another election moved on: step down.
  fun HandleReply(p: (reqTerm: int, term: int, granted: bool)): bool {
    if (p.reqTerm == campaign.term) {
      campaign.replies = campaign.replies + 1;
      if (campaign.replies == sizeof(campaign.peers)) {
        announce HeardAll, (id = id, term = campaign.term);
        if (campaign.leaderTerm != campaign.term) { send campaign.driver, RoundLost, (id = id, term = campaign.term); }
      }
    }
    if (p.term > term) {
      term = p.term;
      return true;
    }
    return false;
  }
}
