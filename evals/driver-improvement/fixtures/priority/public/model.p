machine PriorityQueue {
  var priorities: map[int, int];
  var admission: map[int, int];
  var active: set[int];
  var serial: int;
  start state Ready {
    on eCommand do (c: Command) {
      var accepted: bool;
      var chosen: int;
      var key: int;
      var best: int;
      var rank: int;
      var found: bool;
      chosen = -1;
      if (c.op == 0) {
        if (!(c.id in active)) {
          accepted = true;
          priorities[c.id] = c.priority;
          admission[c.id] = serial;
          serial = serial + 1;
          active += (c.id);
        }
      } else if (c.op == 1) {
        if (c.id in active) {
          accepted = true;
          priorities[c.id] = c.priority;
        }
      } else if (c.op == 2) {
        if (c.id in active) {
          accepted = true;
          active -= (c.id);
        }
      } else {
        foreach (key in active) {
          rank = priorities[key];
          if (!found || rank < best || (rank == best && admission[key] < admission[chosen])) {
            found = true;
            chosen = key;
            best = rank;
          }
        }
        if (found) { active -= (chosen); }
      }
      announce eObserved, (op=c.op, id=c.id, priority=c.priority,
        accepted=accepted, result=chosen);
      send c.reply, eAck;
    }
    on eFinish do { announce eFinished; }
  }
}
