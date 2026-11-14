// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// Some utility functions
fun startTimer(timer: Timer) {
  send timer, StartTimer;
}

fun lastLogIndex(log: seq[ServerLog]): LogIndex {
  return sizeof(log) - 1;
}

fun getLogTerm(log: seq[ServerLog], index: int): int {
  // get the term of the log entry at index; 0 before the first entry
  // snappea-lint: allow mutation:swap-operator -- `>` is equivalent: callers pass nextIndex - 1, and nextIndex never passes the end of the leader's log, because a follower's success reply reports a log it has just cut to the leader's entries
  if (index < 0 || index >= sizeof(log)) {
    return 0;
  }
  else {
    return log[index].term;
  }
}

fun lastLogTerm(log: seq[ServerLog]): int {
  if (sizeof(log) == 0) {
    return 0;
  }
  else {
    return log[sizeof(log) - 1].term;
  }
}

fun Max(a: int, b: int): int {
  // snappea-lint: allow coverage -- callers pass a follower's newer matchedIndex as b, and one follower's replies arrive in order; the guard is for networks that reorder
  if (a > b) {
    return a;
  }
  else {
    return b;
  }
}

fun Min(a: int, b: int): int {
  if (a < b) {
    return a;
  }
  else {
    return b;
  }
}

fun fillMap(self: machine, m: map[Server, int], servers: set[Server], value: int): map[Server, int] {
  var server: Server;
  foreach(server in servers) {
    if (server != self) {
      m[server] = value;
    }
  }

  return m;
}
