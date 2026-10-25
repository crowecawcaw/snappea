// Public history reported by the driver and service. The contract does not
// inspect service variables; it derives its expected work from requests.
event eRequest: int;
event eCommit: (id: int, total: int);
event eOutcome: (id: int, result: int, currentTotal: int);
event eScenarioDone;

spec RetryContract observes eRequest, eCommit, eOutcome, eScenarioDone {
  var requested: map[int, bool];
  var commitResults: map[int, int];
  var returned: map[int, bool];
  var expectedTotal: int;
  var returnedIds: int;

  start hot state AwaitingOutcome {
    on eRequest do (id: int) {
      if (!(id in requested)) {
        requested[id] = true;
        expectedTotal = expectedTotal + 1;
      }
    }
    on eCommit do (commit: (id: int, total: int)) {
      assert commit.id in requested, "commit without a request";
      assert !(commit.id in commitResults), "one request caused more than one effect";
      assert commit.total == expectedTotal,
        format("expected effect total {0} at commit, got {1}", expectedTotal, commit.total);
      commitResults[commit.id] = commit.total;
    }
    on eOutcome do (outcome: (id: int, result: int, currentTotal: int)) {
      assert outcome.id in requested, "outcome without a request";
      assert outcome.id in commitResults, "successful outcome without a commit";
      assert outcome.result == commitResults[outcome.id],
        format("request {0} replayed result {1}, expected original result {2}",
          outcome.id, outcome.result, commitResults[outcome.id]);
      assert outcome.currentTotal == expectedTotal,
        format("expected current effect total {0}, got {1}",
          expectedTotal, outcome.currentTotal);
      if (!(outcome.id in returned)) {
        returned[outcome.id] = true;
        returnedIds = returnedIds + 1;
      }
    }
    on eScenarioDone do {
      assert returnedIds == expectedTotal,
        format("expected outcomes for {0} operations, got {1}", expectedTotal, returnedIds);
      goto Complete;
    }
  }

  state Complete {}
}
