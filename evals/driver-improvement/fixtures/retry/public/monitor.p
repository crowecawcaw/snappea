// Independent public-history oracle. No access to the model's internal record.
spec ConfigurationContract observes eObserved, eFinished {
  var version: int;
  var value: int;
  var memo: map[int, Outcome];
  var commands: map[int, Command];
  var observed: int;
  start hot state Ready {
    on eObserved do (p: (command: Command, outcome: Outcome, version: int, value: int)) {
      var expected: Outcome;
      assert p.command.id >= 0 && p.command.expected >= 0 && p.command.value >= 0, "driver sent an out-of-contract negative argument";
      if (p.command.id in memo) {
        assert commands[p.command.id] == p.command, "driver reused a request ID with different arguments";
        expected = memo[p.command.id];
      } else {
        commands[p.command.id] = p.command;
        expected.accepted = p.command.expected == version;
        if (expected.accepted) {
          version = version + 1;
          value = p.command.value;
        }
        expected.version = version;
        memo[p.command.id] = expected;
      }
      assert p.outcome == expected, "request result changed or violated compare/set";
      assert p.version == version, "request unexpectedly changed the version";
      assert p.value == value, "request unexpectedly changed the configuration";
      observed = observed + 1;
    }
    on eFinished do {
      assert observed >= 4, "driver finished without exercising the API";
      goto Done;
    }
  }
  cold state Done { }
}
