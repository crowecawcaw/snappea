// THE FLAW. The quorum is computed as n / 2. With an odd cluster that is a minority,
// and someone would notice; the code was written and tested with 4 nodes, where 4 / 2
// = 2 looks like "half the votes". But two disjoint pairs are each "half": candidates
// 1 and 3 in the same term can each get their own vote plus one other, and both win.

machine HalfQuorumNode {
  var id: int;
  // the whole cluster, including this node; given when its election timer fires
  var peers: map[int, machine];
  // newest term this node has voted in (it votes in no older term after it)
  var votedTerm: int;
  // candidate state: the term, votes so far, and the nodes still to ask, in order
  var term: int;
  var grants: int;
  var toAsk: seq[int];

  start state Starting {
    entry (myId: int) {
      id = myId;
      goto Following;
    }
  }

  state Following {
    on StartElection do (p: map[int, machine]) {
      var peer: int;
      peers = p;
      term = votedTerm + 1;
      votedTerm = term;
      grants = 1;
      announce Candidacy, (candidate = id, term = term);
      announce VoteGranted, (term = term, voter = id, candidate = id);
      foreach (peer in keys(peers)) {
        if (peer != id) { toAsk += (sizeof(toAsk), peer); }
      }
      goto Campaigning;
    }
    on RequestVote do (r: VoteReq) { Vote(r); }
  }

  // Asks one node at a time, so each request is an RPC the candidate waits on.
  state Campaigning {
    entry {
      send peers[toAsk[0]], RequestVote, (term = term, candidate = id, voter = toAsk[0], from = this);
      toAsk -= (0);
    }
    on RequestVote do (r: VoteReq) { Vote(r); }
    on VoteResp do (v: (granted: bool)) {
      if (v.granted) { grants = grants + 1; }
    // BUG: n / 2 is not a majority when n is even.
      if (grants >= sizeof(peers) / 2) {
        announce ElectionEnded, (term = term, candidate = id, won = true);
        goto Following;
      }
      if (sizeof(toAsk) == 0) {
        // Every node has answered and we lack a quorum: nobody else will vote for us.
        announce ElectionEnded, (term = term, candidate = id, won = false);
        goto Following;
      }
      goto Campaigning;
    }
  }

  fun Vote(r: VoteReq) {
    if (r.term > votedTerm) {
      votedTerm = r.term;
      announce VoteGranted, (term = r.term, voter = id, candidate = r.candidate);
      send r.from, VoteResp, (granted = true,);
      return;
    }
    announce VoteDenied, (term = r.term, voter = id, candidate = r.candidate);
    send r.from, VoteResp, (granted = false,);
  }
}
