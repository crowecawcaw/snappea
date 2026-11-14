// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

spec SafetyLogMatching observes NotifyLog {
  // Raft's Log Matching: if two logs hold the same entry at an index, they agree on every
  // entry before it. Compares the latest log each server reported.
  var allLogs: map[Server, seq[ServerLog]];

  start state MonitorLogUpdates {
    entry {
      allLogs = default(map[Server, seq[ServerLog]]);
    }

    on NotifyLog do (payload: (server: Server, log: seq[ServerLog])) {
      var other: Server;
      allLogs[payload.server] = payload.log;
      foreach (other in keys(allLogs)) {
        checkLogMatching(payload.server, other);
      }
    }
  }

  fun checkLogMatching(a: Server, b: Server) {
    var i: int;
    // the highest index where both logs hold the same entry
    i = sizeof(allLogs[a]) - 1;
    if (sizeof(allLogs[b]) < sizeof(allLogs[a])) {
      i = sizeof(allLogs[b]) - 1;
    }

    while (i >= 0 && allLogs[a][i] != allLogs[b][i]) {
      i = i - 1;
    }

    // every entry before it must match too
    while (i >= 0) {
      assert allLogs[a][i] == allLogs[b][i],
      format("logs of {0} and {1} share a later entry but differ at index {2}: {3} vs {4}", a, b, i, allLogs[a][i], allLogs[b][i]);
      i = i - 1;
    }
  }
}
