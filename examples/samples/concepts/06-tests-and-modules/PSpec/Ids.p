event AllocatorCreated: int;
event IdRequested;

// Safety: no id is handed out twice, every id is in range, and a refusal happens only
// once every id has been handed out.
spec UniqueIds observes AllocatorCreated, IdResponse, NoIdsLeft {
  var limit: int;
  var given: set[int];

  start state Setup {
    on AllocatorCreated do (n: int) {
      limit = n;
      goto Checking;
    }
  }

  state Checking {
    on IdResponse do (r: (id: int)) {
      assert !(r.id in given), format("id {0} given twice", r.id);
      assert r.id >= 0 && r.id < limit, format("id {0} is outside 0..{1}", r.id, limit - 1);
      given += (r.id);
    }
    on NoIdsLeft do {
      assert sizeof(given) == limit,
        format("refused although only {0} of {1} ids were given", sizeof(given), limit);
    }
  }
}

// Progress: every request gets an id or a refusal.
spec RequestsAnswered observes IdRequested, IdResponse, NoIdsLeft {
  var outstanding: int;

  start state Idle {
    on IdRequested do { Owe(); }
  }

  hot state Waiting {
    on IdRequested do { Owe(); }
    on IdResponse do { Answer(); }
    on NoIdsLeft do { Answer(); }
  }

  fun Owe() {
    outstanding = outstanding + 1;
    goto Waiting;
  }

  fun Answer() {
    outstanding = outstanding - 1;
    if (outstanding == 0) {
      goto Idle;
    }
  }
}
