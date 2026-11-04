// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

type BecomeLeaderData = (term:TermId, leader:Server, log: seq[ServerLog], commitIndex: LogIndex);
event BecomeLeader: BecomeLeaderData;

spec SafetyOneLeader observes BecomeLeader{
  var termToLeader: map[int, Server];
  start state Init {
    entry {
      termToLeader = default(map[int, Server]);
    }

    on BecomeLeader do (payload: (term:int, leader:Server, log: seq[ServerLog], commitIndex: int)) {
      if (payload.term in keys(termToLeader)) {
        assert termToLeader[payload.term] == payload.leader, format("At term {0} there are multiple leaders: {1} and {2}.", payload.term, termToLeader[payload.term], payload.leader);
      }

      termToLeader += (payload.term, payload.leader);
    }
  }
}
