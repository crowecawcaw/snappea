// No lost update: every write moves the version and the value up by exactly one, and by
// the end every planned increment is written and every client has finished.

// Announced by the driver before it starts any client: one increment per client.
event eIncrementsPlanned: int;
// Announced by the store when a compare-and-swap commits.
event eWritten: (client: machine, version: int, value: int);
// Announced by a client when its increment is done.
event eClientDone: machine;

spec NoLostUpdate observes eIncrementsPlanned, eWritten, eClientDone {
  var planned: int;
  var writes: int;
  var finished: set[machine];

  start state Idle {
    on eIncrementsPlanned goto Counting with (n: int) {
      assert n > 0, "planned no increments";
      planned = n;
    }
    on eWritten do { assert false, "write before any increment was planned"; }
    on eClientDone do { assert false, "client finished before any increment was planned"; }
  }

  // Hot until every increment is written and every client has finished.
  hot state Counting {
    on eIncrementsPlanned do { assert false, "increments planned twice"; }
    on eWritten do (w: (client: machine, version: int, value: int)) {
      writes = writes + 1;
      assert w.value == writes, format("{0} wrote {1}, expected {2}: an update was lost", w.client, w.value, writes);
      assert w.version == writes, format("{0} wrote version {1}, expected {2}", w.client, w.version, writes);
      Settle();
    }
    on eClientDone do (c: machine) {
      assert !(c in finished), format("{0} finished twice", c);
      finished += (c);
      Settle();
    }
  }

  state AllDone {
    on eIncrementsPlanned do { assert false, "increments planned after all were done"; }
    on eWritten do { assert false, "write after every increment was done"; }
    on eClientDone do { assert false, "client finished after every increment was done"; }
  }

  fun Settle() {
    if (writes == planned && sizeof(finished) == planned) { goto AllDone; }
  }
}
