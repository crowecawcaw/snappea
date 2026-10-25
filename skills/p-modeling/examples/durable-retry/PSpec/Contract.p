// Public history reported by the driver and service. The contract does not
// inspect service variables; it derives its expected work from requests.
event eRequest: int;
event eCommit: int;
event eOutcome: (id: int, total: int);

spec RetryContract observes eRequest, eCommit, eOutcome {
  var requested: map[int, bool];
  var committed: map[int, bool];
  var expectedTotal: int;
  var outcomes: int;

  start hot state AwaitingOutcome {
    on eRequest do (id: int) {
      if (!(id in requested)) {
        requested[id] = true;
        expectedTotal = expectedTotal + 1;
      }
    }
    on eCommit do (id: int) {
      assert id in requested, "commit without a request";
      assert !(id in committed), "one request caused more than one effect";
      committed[id] = true;
    }
    on eOutcome do (result: (id: int, total: int)) {
      assert result.id in requested, "outcome without a request";
      assert result.id in committed, "successful outcome without a commit";
      assert outcomes == 0, "driver reported more than one final outcome";
      assert result.total == expectedTotal,
        format("expected total {0}, got {1}", expectedTotal, result.total);
      outcomes = outcomes + 1;
      goto Complete;
    }
  }

  state Complete {}
}
