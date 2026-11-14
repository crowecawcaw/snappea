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
    on Next do (c: machine) {
      last = last + 1;
      send c, Ticket, last;
      announce Issued, (client = c, ticket = last);
    }
    on Crash do {
      send owner, Exited;
      raise halt;
    }
  }
}
