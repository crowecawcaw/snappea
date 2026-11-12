event eAllocatorCreated: int;
event eIdRequested;

// Safety: no id is handed out twice, every id is in range, and a refusal happens only
// once every id has been handed out.
spec UniqueIds observes eAllocatorCreated, eIdResp, eNoIdsLeft {
  var limit: int;
  var given: set[int];

  start state Setup {
    on eAllocatorCreated do (n: int) {
      limit = n;
      goto Checking;
    }
  }

  state Checking {
    on eIdResp do (r: (id: int)) {
      assert !(r.id in given), format("id {0} given twice", r.id);
      assert r.id >= 0 && r.id < limit, format("id {0} is outside 0..{1}", r.id, limit - 1);
      given += (r.id);
    }
    on eNoIdsLeft do {
      assert sizeof(given) == limit,
        format("refused although only {0} of {1} ids were given", sizeof(given), limit);
    }
  }
}

// Progress: every request gets an id or a refusal.
spec RequestsAnswered observes eIdRequested, eIdResp, eNoIdsLeft {
  var outstanding: int;

  start state Idle {
    on eIdRequested do { Owe(); }
  }

  hot state Waiting {
    on eIdRequested do { Owe(); }
    on eIdResp do { Answer(); }
    on eNoIdsLeft do { Answer(); }
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
