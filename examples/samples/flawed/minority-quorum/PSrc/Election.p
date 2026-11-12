// Leader election in a cluster of nodes, as in Raft: a node whose election timer
// fires becomes a candidate in a new term (one past the newest term it has seen),
// votes for itself and asks every other node for its vote, one RPC at a time. A node grants at most one
// vote per term, to the first candidate that asks in a term newer than its last vote.
// A candidate that reaches a quorum of votes becomes leader for that term.
//
// Real primitives (atomic on one node): start an election, handle a vote request
// (compare term, record the vote durably, reply), count a vote reply. Messages
// between nodes arrive in any order.

// The election timer fired; the payload is the cluster membership.
event eStartElection: map[int, machine];
// voter: the node the request is addressed to.
type tVoteReq = (term: int, candidate: int, voter: int, from: machine);
event eRequestVote: tVoteReq;
event eVoteResp: (granted: bool);
