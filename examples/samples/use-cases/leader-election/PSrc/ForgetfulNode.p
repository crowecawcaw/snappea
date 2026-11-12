// A plausible broken design, kept as a negative control: identical to Node except
// that the vote is not persisted. A voter that granted its vote in a term, crashed and
// restarted can grant it again to another candidate in the same term, so two leaders
// win that term. tcBugVoteNotPersisted shows the spec catching it.
machine ForgetfulNode {
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
    // BUG: the vote was kept only in memory, so a crash and restart forgets it.
    on eCrash do { votedTerm = 0; }
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
        // snappea-lint: allow mutation:remove-statement -- voting-only model: keeping the candidate or leader role after seeing a higher term sends nothing different (no heartbeats or log)
        goto Follower;
      }
    }
    on eVoteResp do (p: (reqTerm: int, term: int, granted: bool)) {
      // Count the grant before HandleReply concludes the round, so the last reply can
      // be the winning one. Only grants for the current term count: a reply to an
      // older candidacy is stale.
      if (p.granted && p.reqTerm == term) {
        votes = votes + 1;
        // Majority of three, counting the candidate's own vote. With three nodes one
        // grant is always enough, so this is never false here.
        // snappea-lint: allow coverage -- with three nodes the first grant always completes a majority; tcOneNodeDown shows it must
        if (votes >= 2) { leaderTerm = term; }
      }
      if (HandleReply(p)) {
        // snappea-lint: allow mutation:remove-statement -- voting-only model: keeping the candidate or leader role after seeing a higher term sends nothing different (no heartbeats or log)
        goto Follower;
      }
      if (leaderTerm == term) { goto Leader; }
    }
  }

  state Leader {
    entry {
      announce eBecameLeader, (id = id, term = term);
    }
    // A leader's heartbeats keep its own timer from firing.
    ignore eTimeout;
    on eVoteReq do (r: (term: int, candidate: machine, cid: int)) {
      if (HandleVoteReq(r)) {
        // snappea-lint: allow mutation:remove-statement -- voting-only model: keeping the candidate or leader role after seeing a higher term sends nothing different (no heartbeats or log)
        goto Follower;
      }
    }
    on eVoteResp do (p: (reqTerm: int, term: int, granted: bool)) {
      if (HandleReply(p)) {
        // snappea-lint: allow mutation:remove-statement -- voting-only model: keeping the candidate or leader role after seeing a higher term sends nothing different (no heartbeats or log)
        goto Follower;
      }
    }
  }

  // Returns true when a higher term means this node must step down to follower.
  fun HandleVoteReq(r: (term: int, candidate: machine, cid: int)): bool {
    var grant: bool;
    var stepDown: bool;
    if (r.term > term) {
      term = r.term;
      // snappea-lint: allow mutation -- only drives the step-down, which is not observable in a voting-only model (see the handlers)
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
      // snappea-lint: allow mutation:flip-boolean -- only drives the step-down, which a voting-only model cannot observe (see the handlers)
      return true;
    }
    return false;
  }
}
