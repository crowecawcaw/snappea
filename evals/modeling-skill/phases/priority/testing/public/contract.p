// This monitor derives queue contents and stable admission order from public calls.
spec QueueContract observes eSubmit, ePop, eReprioritize, eCancel {
  var active: map[string, bool];
  var priorities: map[string, int];
  var orders: map[string, int];
  var admitted: seq[string];
  var next_order: int;
  var expected: string;
  var chosen: string;
  var key: string;
  var best_priority: int;
  var best_order: int;
  var found: bool;
  var index: int;

  start state Watching {
    entry {
      active = default(map[string, bool]);
      priorities = default(map[string, int]);
      orders = default(map[string, int]);
      admitted = default(seq[string]);
      next_order = 0;
    }

    on eSubmit do (p: (item_id: string, priority: int, accepted: bool)) {
      assert p.item_id != "" && p.item_id != "-", "item id must be nonempty and cannot be '-'";
      if (p.item_id in active && active[p.item_id]) {
        assert !p.accepted, "duplicate item submission was accepted";
      } else {
        assert p.accepted, "new item submission was rejected";
        active[p.item_id] = true;
        priorities[p.item_id] = p.priority;
        orders[p.item_id] = next_order;
        admitted += (sizeof(admitted), p.item_id);
        next_order = next_order + 1;
      }
    }

    on eReprioritize do (p: (item_id: string, priority: int, accepted: bool)) {
      if (p.item_id in active && active[p.item_id]) {
        assert p.accepted, "reprioritizing a queued item was rejected";
        priorities[p.item_id] = p.priority;
      } else {
        assert !p.accepted, "reprioritizing a missing item was accepted";
      }
    }

    on eCancel do (p: (item_id: string, accepted: bool)) {
      if (p.item_id in active && active[p.item_id]) {
        assert p.accepted, "cancelling a queued item was rejected";
        active[p.item_id] = false;
      } else {
        assert !p.accepted, "cancelling a missing item was accepted";
      }
    }

    on ePop do (p: (item_id: string)) {
      found = false;
      chosen = "-";
      best_priority = 0;
      best_order = 0;
      index = 0;
      while (index < sizeof(admitted)) {
        key = admitted[index];
        if (active[key] && (!found || priorities[key] < best_priority ||
            (priorities[key] == best_priority && orders[key] < best_order))) {
          found = true;
          chosen = key;
          best_priority = priorities[key];
          best_order = orders[key];
        }
        index = index + 1;
      }
      expected = chosen;
      assert p.item_id == expected, "pop did not return the minimum-priority earliest item";
      if (found) { active[chosen] = false; }
    }
  }
}
