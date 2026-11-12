// Contract: every increment the application asks for is applied exactly once, so the
// committed values go 1, 2, 3, ... up to the number of increments requested.

event eIncrementsPlanned: int;
event eCommitted: (value: int);

spec IncrementsApplied observes eIncrementsPlanned, eCommitted {
  var planned: int;
  var committed: int;

  start state Setup {
    on eIncrementsPlanned do (n: int) {
      planned = n;
      goto Applying;
    }
  }

  // Hot until every requested increment has committed.
  hot state Applying {
    on eCommitted do (c: (value: int)) {
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
