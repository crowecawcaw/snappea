// Progress: every request a client makes is eventually answered, exactly once.

// Announced by each client (a test driver) when it makes a request.
event Requested: (id: int);

spec RequestsAnswered observes Requested, Response {
  var pending: set[int];

  // Cold: nothing is owed, so a run may end here.
  start cold state AllAnswered {
    on Requested do (r: (id: int)) {
      Owe(r.id);
      goto Waiting;
    }
    on Response do (p: (id: int)) {
      assert false, format("response to request {0}, which nobody is waiting for", p.id);
    }
  }

  // Hot: a request is outstanding. If the run ends here, the check fails.
  hot state Waiting {
    on Requested do (r: (id: int)) {
      Owe(r.id);
    }
    on Response do (p: (id: int)) {
      assert p.id in pending, format("response to request {0}, which nobody is waiting for", p.id);
      pending -= (p.id);
      if (sizeof(pending) == 0) {
        goto AllAnswered;
      }
    }
  }

  fun Owe(id: int) {
    assert !(id in pending), format("request {0} made twice", id);
    pending += (id);
  }
}
