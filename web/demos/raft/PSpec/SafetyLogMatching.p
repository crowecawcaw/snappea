// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

spec SafetyLogMatching observes NotifyLog {
  var allLogs: map[Server, seq[ServerLog]];

  start state MonitorLogUpdates {
    entry {
      allLogs = default(map[Server, seq[ServerLog]]);
    }

    // snappea-lint: allow unused-payload-field -- log matching compares logs whenever they are reported, in any order
    on NotifyLog do (payload: (timestamp: int, server: Server, log: seq[ServerLog])) {
      var i: int;
      var j: int;
      var s1: Server;
      var s2: Server;
      if (!(payload.server in allLogs)) {
        allLogs[payload.server] = payload.log;
      }

      i = 0;
      while (i < sizeof(keys(allLogs))) {
        s1 = keys(allLogs)[i];
        j = i + 1;
        while (j < sizeof(keys(allLogs))) {
          s2 = keys(allLogs)[j];
          if (sizeof(allLogs[s1]) > sizeof(allLogs[s2])) {
            assert checkLogMatching(allLogs[s2], allLogs[s1]);
          }
          else {
            assert checkLogMatching(allLogs[s1], allLogs[s2]);
          }

          j = j + 1;
        }

        i = i + 1;
      }
    }
  }

  fun checkLogMatching(xs: seq[ServerLog], ys: seq[ServerLog]): bool {
    var i: int;
    var highestMatch: int;
    var logsA: seq[ServerLog];
    var logsB: seq[ServerLog];
    if (sizeof(xs) > sizeof(ys)) {
      logsA = ys;
      logsB = xs;
    }
    else {
      logsA = xs;
      logsB = ys;
    }

    i = sizeof(logsA) - 1;
    while (i >= 0 && logsA[i] != logsB[i]) {
      i = i - 1;
    }

    while (i >= 0) {
      if (logsA[i] != logsB[i]) {
        return false;
      }

      i = i - 1;
    }

    return true;
  }
}
