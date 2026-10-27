spec PriorityContract observes eObserved, eFinished {
  var active: map[int, int];
  var order: map[int, int];
  var serial: int;
  var calls: int;
  start hot state Checking {
    on eObserved do (o: Observation) {
      var key: int;
      calls = calls + 1;
      assert o.op >= 0 && o.op <= 3;
      if (o.op != 3) { assert o.id >= 0, "item IDs must be nonnegative"; }
      if (o.op == 0) {
        assert o.accepted == !(o.id in active), "admission result";
        if (o.accepted) {
          active[o.id] = o.priority;
          order[o.id] = serial;
          serial = serial + 1;
        }
      } else if (o.op == 1) {
        assert o.accepted == (o.id in active), "priority update result";
        if (o.accepted) { active[o.id] = o.priority; }
      } else if (o.op == 2) {
        assert o.accepted == (o.id in active), "cancellation result";
        if (o.accepted) { active -= (o.id); }
      } else {
        if (sizeof(active) == 0) {
          assert o.result == -1, "empty pop";
        } else {
          assert o.result in active, "pop must return an active item";
          foreach (key in keys(active)) {
            assert active[o.result] <= active[key], "priority order";
            if (active[o.result] == active[key]) {
              assert order[o.result] <= order[key], "stable admission order";
            }
          }
          active -= (o.result);
        }
      }
    }
    on eFinished do {
      assert calls >= 8, "driver must exercise at least eight commands";
      goto Finished;
    }
  }
  cold state Finished { }
}
