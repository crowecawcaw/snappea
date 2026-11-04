// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

event EntryApplied: (logIndex: LogIndex, term: TermId, key: KeyT, value: ValueT, transactionId: TransactionId, client: Client);

spec SafetyStateMachine observes EntryApplied {
  var appliedEntries: map[int, ServerLog];
  start state MonitorEntryApplied {
    entry {
      appliedEntries = default(map[int, ServerLog]);
    }

    on EntryApplied do (payload: (logIndex: LogIndex, term: TermId, key: KeyT, value: ValueT, transactionId: TransactionId, client: Client)) {
      var log: ServerLog;
      log = (term=payload.term, key=payload.key, value=payload.value, client=payload.client, transactionId=payload.transactionId);
      if (!(payload.logIndex in keys(appliedEntries))) {
        appliedEntries[payload.logIndex] = log;
      }

      assert appliedEntries[payload.logIndex] == log,
      format("Entry@{0}={1} applied before does not match with {2}", payload.logIndex, appliedEntries[payload.logIndex], log);
    }
  }
}
