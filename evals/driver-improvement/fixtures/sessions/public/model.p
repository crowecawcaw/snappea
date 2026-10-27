machine ResourceManager {
  var owners: map[int, int];
  start state Ready {
    on eCommand do (c: Command) {
      var current: int;
      var accepted: bool;
      var result: Result;
      current = 0;
      if (c.resource in owners) { current = owners[c.resource]; }
      accepted = false;
      if (c.kind == 0) {
        // Acquire is repeatable by the current owner, never by a contender.
        if (current == 0 || current == c.client) {
          owners[c.resource] = c.client;
          accepted = true;
        }
      } else if (c.kind == 1) {
        // Only the current owner can release this particular resource.
        if (current == c.client) {
          owners[c.resource] = 0;
          accepted = true;
        }
      } else {
        // Inspection has no side effect.
        accepted = true;
      }
      current = 0;
      if (c.resource in owners) { current = owners[c.resource]; }
      result = (kind = c.kind, client = c.client, resource = c.resource,
                accepted = accepted, owner = current);
      announce eObserved, result;
      send c.reply, eReply, result;
    }
  }
}
