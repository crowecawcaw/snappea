// No lost update: the committed values go 1, 2, 3, ... and by the end every planned
// increment is committed and every client has finished. The same contract as
// 07-receive, plus each client's end.

// Announced by the driver before it starts any client: one increment per client.
event IncrementsPlanned: int;
// Announced by the store when a compare-and-swap commits.
event Committed: (value: int);
// Announced by a client when its increment is done.
event ClientDone: (client: machine);

spec IncrementsApplied observes IncrementsPlanned, Committed, ClientDone {
  var planned: int;
  var committed: int;
  var finished: set[machine];

  start state Idle {
    on IncrementsPlanned goto Applying with (n: int) {
      assert n > 0, "planned no increments";
      planned = n;
    }
  }

  // Hot until every increment is committed and every client has finished.
  hot state Applying {
    on Committed do (c: (value: int)) {
      committed = committed + 1;
      assert committed <= planned, format("{0} increments committed, only {1} planned", committed, planned);
      assert c.value == committed,
        format("commit {0} wrote {1}: an increment was lost or applied twice", committed, c.value);
      Settle();
    }
    on ClientDone do (d: (client: machine)) {
      assert !(d.client in finished), format("{0} finished twice", d.client);
      finished += (d.client);
      assert sizeof(finished) <= committed,
        format("{0} finished: {1} clients done, but only {2} increments committed", d.client, sizeof(finished), committed);
      Settle();
    }
  }

  state Applied {}

  fun Settle() {
    if (committed == planned && sizeof(finished) == planned) {
      goto Applied;
    }
  }
}
