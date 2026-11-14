// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

spec SafetyLeaderCompleteness observes BecomeLeader {
  // Raft's Leader Completeness: an entry committed in a term is in the log of every
  // leader of a later term. Each committed entry maps to the term of the first leader
  // seen to have committed it: it was committed in that term or earlier. A leader of an
  // earlier term (elected late, from delayed votes) need not have it.
  var committedLogs: map[ServerLog, int];

  start state Init {
    entry {
      committedLogs = default(map[ServerLog, int]);
      goto MonitoringLeaderChange;
    }
  }

  state MonitoringLeaderChange {
    on BecomeLeader do (payload: (term:int, leader:Server, log: seq[ServerLog], commitIndex: int)) {
      var i: int;
      var e: ServerLog;
      // every entry committed by an earlier term must be in the new leader's log
      foreach (e in keys(committedLogs)) {
        assert committedLogs[e] >= payload.term || e in payload.log,
        format("{0} was committed by term {1} but is not in the log of the leader of term {2}", e, committedLogs[e], payload.term);
      }

      // record the entries this leader has committed, by the earliest term seen
      i = 0;
      while (i <= payload.commitIndex) {
        if (!(payload.log[i] in committedLogs) || payload.term < committedLogs[payload.log[i]]) {
          committedLogs[payload.log[i]] = payload.term;
        }
        i = i + 1;
      }
    }
  }
}
