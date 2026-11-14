// THE FIX. Quorum = n / 2 + 1, a strict majority. Any two majorities of the same
// cluster share at least one node, and that node votes once per term, so two
// candidates cannot both reach a quorum in the same term. With 4 nodes the quorum is
// 3: a 2-2 split vote elects nobody, and a later term is needed. That is the price of
// safety (and why clusters usually have an odd size: 5 nodes tolerate 2 failures, 4
// nodes only 1).

machine MajorityNode {
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
      if (grants >= sizeof(peers) / 2 + 1) {
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
