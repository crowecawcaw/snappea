// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

spec SafetyLeaderCompleteness observes BecomeLeader {
  var committedLogs: set[ServerLog];

  start state Init {
    entry {
      committedLogs = default(set[ServerLog]);
      goto MonitoringLeaderChange;
    }
  }

  state MonitoringLeaderChange {
    // snappea-lint: allow unused-payload-field -- this rule compares logs; SafetyOneLeader checks the term and leader
    on BecomeLeader do (payload: (term:int, leader:Server, log: seq[ServerLog], commitIndex: int)) {
      // check all already-committed logs is remain committed in the new leader's log
      var i: int;
      var e: ServerLog;
      foreach (e in committedLogs) {
        assert e in payload.log, format("{0} was committed before but not in the new leader's log", e);
      }

      // add the committed logs
      i = 0;
      while (i <= payload.commitIndex) {
        committedLogs += (payload.log[i]);
        i = i + 1;
      }
    }
  }
}
