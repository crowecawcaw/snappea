spec OwnershipContract observes eObserved, eFinished {
  var expected: map[int, int];
  var completed: int;
  start hot state Checking {
    on eObserved do (r: Result) {
      var owner: int;
      var allowed: bool;
      assert r.client == 1 || r.client == 2, "client outside model bounds";
      assert r.resource == 1 || r.resource == 2, "resource outside model bounds";
      assert r.kind >= 0 && r.kind <= 2, "command outside model bounds";
      owner = 0;
      if (r.resource in expected) { owner = expected[r.resource]; }
      if (r.kind == 0) {
        allowed = owner == 0 || owner == r.client;
        assert r.accepted == allowed, "incorrect acquisition result";
        if (allowed) { expected[r.resource] = r.client; }
      } else if (r.kind == 1) {
        allowed = owner == r.client;
        assert r.accepted == allowed, "release by non-owner accepted";
        if (allowed) { expected[r.resource] = 0; }
      } else {
        assert r.accepted, "inspection must complete";
      }
      owner = 0;
      if (r.resource in expected) { owner = expected[r.resource]; }
      assert r.owner == owner, "resource ownership changed incorrectly";
      completed = completed + 1;
    }
    on eFinished do (p: (expected: int)) {
      assert completed > 0, "driver performed no operations";
      assert completed == p.expected, "missing completed operation";
      goto Finished;
    }
  }
  state Finished {}
}
