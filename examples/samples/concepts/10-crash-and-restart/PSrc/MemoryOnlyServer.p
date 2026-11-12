// Negative control: the counter lives only in memory. It is fast and correct until the
// process crashes; the restarted process starts again from ticket 1.
// tcBugMemoryOnly shows the spec catching it.
machine MemoryOnlyServer {
  var owner: machine;
  var last: int;

  start state Serving {
    entry (input: (storage: Storage, owner: machine)) {
      // BUG: storage is never used, so nothing survives a crash.
      owner = input.owner;
    }
    on eNext do (c: machine) {
      last = last + 1;
      send c, eTicket, last;
      announce eIssued, (client = c, ticket = last);
    }
    on eCrash do {
      send owner, eExited;
      raise halt;
    }
  }
}
