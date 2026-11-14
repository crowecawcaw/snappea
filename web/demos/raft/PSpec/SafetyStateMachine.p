// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

event EntryApplied: (logIndex: LogIndex, log: ServerLog);

// Raft's State Machine Safety: no two servers apply different entries at the same index.
spec SafetyStateMachine observes EntryApplied {
  var appliedEntries: map[LogIndex, ServerLog];
  start state MonitorEntryApplied {
    entry {
      appliedEntries = default(map[LogIndex, ServerLog]);
    }

    on EntryApplied do (payload: (logIndex: LogIndex, log: ServerLog)) {
      if (!(payload.logIndex in keys(appliedEntries))) {
        appliedEntries[payload.logIndex] = payload.log;
      }

      assert appliedEntries[payload.logIndex] == payload.log,
      format("Entry@{0}={1} applied before does not match with {2}", payload.logIndex, appliedEntries[payload.logIndex], payload.log);
    }
  }
}
