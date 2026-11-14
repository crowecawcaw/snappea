// Contract: every increment the application asks for is applied exactly once, so the
// committed values go 1, 2, 3, ... up to the number of increments requested.

event IncrementsPlanned: int;
event Committed: (value: int);

spec IncrementsApplied observes IncrementsPlanned, Committed {
  var planned: int;
  var committed: int;

  start state Setup {
    on IncrementsPlanned do (n: int) {
      planned = n;
      goto Applying;
    }
  }

  // Hot until every requested increment has committed.
  hot state Applying {
    on Committed do (c: (value: int)) {
      committed = committed + 1;
      assert committed <= planned, format("{0} increments committed, only {1} requested", committed, planned);
      assert c.value == committed,
        format("commit {0} wrote {1}: an increment was lost or applied twice", committed, c.value);
      if (committed == planned) {
        goto Applied;
      }
    }
  }

  state Applied {}
}
