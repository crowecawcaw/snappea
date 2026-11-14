// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

type BecomeLeaderData = (term:TermId, leader:Server, log: seq[ServerLog], commitIndex: LogIndex);
event BecomeLeader: BecomeLeaderData;

spec SafetyOneLeader observes BecomeLeader {
  // every (term, leader) pair announced so far
  var elected: set[(term: TermId, leader: Server)];
  start state Init {
    entry {
      elected = default(set[(term: TermId, leader: Server)]);
    }

    on BecomeLeader do (payload: BecomeLeaderData) {
      var prev: (term: TermId, leader: Server);
      foreach (prev in elected) {
        assert prev.term != payload.term || prev.leader == payload.leader,
        format("At term {0} there are multiple leaders: {1} and {2}.", payload.term, prev.leader, payload.leader);
      }

      elected += ((term=payload.term, leader=payload.leader));
    }
  }
}
