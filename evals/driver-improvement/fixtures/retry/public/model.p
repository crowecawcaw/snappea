// A handler is one atomic operation against the durable configuration record.
machine Configuration {
  var version: int;
  var value: int;
  var memo: map[int, Outcome];
  start state Ready {
    on eSet do (p: (caller: machine, command: Command)) {
      var result: Outcome;
      if (p.command.id in memo) {
        result = memo[p.command.id];
      } else {
        result.accepted = p.command.expected == version;
        if (result.accepted) {
          version = version + 1;
          value = p.command.value;
        }
        result.version = version;
        memo[p.command.id] = result;
      }
      announce eObserved, (command = p.command, outcome = result, version = version, value = value);
      send p.caller, eReply, result;
    }
  }
}
