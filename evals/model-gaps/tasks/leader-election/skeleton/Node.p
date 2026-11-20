// Raft-style leader election (voting only, no log). See DESIGN.md.
//
// The node announces the events PSpec/Contract.p observes (ElectionStarted,
// VoteAnswered, BecameLeader, HeardAll, ClientAnswered) at its commit points.

// The election timer fired. The timer service hands the candidate the current
// membership (its peers) and whom to tell if it loses.
event ElectionTimeout: (peers: seq[machine], driver: machine);
event VoteReq: (term: int, candidate: machine, cid: int);
// reqTerm: the term the request was for; term: the voter's term after handling it.
event VoteResp: (reqTerm: int, term: int, granted: bool);
event Crash: machine;
// Restart acknowledgment to the failure injector, after restoring durable state.
event Restarted: (id: int, term: int);
// To the timer service: this node heard every reply to its candidacy for `term` and
// did not win. When every candidate of a round has lost, the service lets one timer
// fire alone.
event RoundLost: (id: int, term: int);

// A client request, such as a write, that only the leader may accept. The node
// announces its answer (ClientAnswered); the reply to the client is not modeled.
event ClientReq;

// Durable state a node starts from (all zero for a fresh node).
type DurableState = (id: int, term: int, votedTerm: int);

machine Node {
  start state Init {
    entry (d: DurableState) {
      // TODO
    }
  }
}
