// Public history reported by the driver and service. The contract does not
// inspect service variables. Requests create obligations; only unique commits
// advance the effect total. Several requests may be outstanding at once.
event eRequest: int;
event eCommit: (id: int, total: int);
event eReply: (id: int, result: int, currentTotal: int);
event eScenarioDone;

spec RetryContract observes eRequest, eCommit, eReply, eScenarioDone {
  var requested: map[int, bool];
  var commitResults: map[int, int];
  var returned: map[int, bool];
  var requestedIds: int;
  var expectedTotal: int;
  var returnedIds: int;

  start hot state AwaitingOutcome {
    on eRequest do (id: int) {
      if (!(id in requested)) {
        requested[id] = true;
        requestedIds = requestedIds + 1;
      }
    }
    on eCommit do (commit: (id: int, total: int)) {
      assert commit.id in requested, "commit without a request";
      assert !(commit.id in commitResults), "one request caused more than one effect";
      expectedTotal = expectedTotal + 1;
      assert commit.total == expectedTotal,
        format("expected effect total {0} at commit, got {1}", expectedTotal, commit.total);
      commitResults[commit.id] = commit.total;
    }
    on eReply do (outcome: (id: int, result: int, currentTotal: int)) {
      assert outcome.id in requested, "reply without a request";
      assert outcome.id in commitResults, "successful outcome without a commit";
      assert outcome.result == commitResults[outcome.id],
        format("request {0} replayed result {1}, expected original result {2}",
          outcome.id, outcome.result, commitResults[outcome.id]);
      // Selected specs observe the actual eReply send before client delivery.
      // Receipt order at different clients would not identify this snapshot.
      assert outcome.currentTotal == expectedTotal,
        format("expected current effect total {0}, got {1}",
          expectedTotal, outcome.currentTotal);
      if (!(outcome.id in returned)) {
        returned[outcome.id] = true;
        returnedIds = returnedIds + 1;
      }
    }
    on eScenarioDone do {
      assert requestedIds > 0, "scenario completed without any requests";
      assert expectedTotal == requestedIds, "some requested effects never committed";
      assert returnedIds == requestedIds,
        format("expected outcomes for {0} operations, got {1}", requestedIds, returnedIds);
      goto Complete;
    }
  }

  state Complete {}
}
