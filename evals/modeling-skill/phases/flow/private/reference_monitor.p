// Private oracle for calibrating the shared hidden event histories.
spec QueueContract observes ePut, eTake, eSnapshot {
  var items: seq[int];
  var put_items: map[string, int];
  var put_results: map[string, bool];
  var take_found: map[string, bool];
  var take_items: map[string, int];
  var expected: bool;

  start state Watching {
    entry {
      items = default(seq[int]);
      put_items = default(map[string, int]);
      put_results = default(map[string, bool]);
      take_found = default(map[string, bool]);
      take_items = default(map[string, int]);
    }

    on ePut do (p: (request_id: string, item: int, accepted: bool)) {
      if (p.request_id in put_items) {
        assert p.item == put_items[p.request_id], "put retry changed its item";
        assert p.accepted == put_results[p.request_id], "put retry changed its result";
      } else {
        expected = sizeof(items) < 2;
        assert p.accepted == expected, "put result disagrees with queue capacity";
        put_items[p.request_id] = p.item;
        put_results[p.request_id] = expected;
        if (expected) { items += (sizeof(items), p.item); }
      }
    }

    on eTake do (p: (request_id: string, item: int, found: bool)) {
      if (p.request_id in take_found) {
        assert p.found == take_found[p.request_id], "take retry changed whether it found an item";
        assert p.item == take_items[p.request_id], "take retry changed its returned item";
      } else if (sizeof(items) > 0) {
        assert p.found, "take missed an item in the queue";
        assert p.item == items[0], "take did not return the oldest item";
        take_found[p.request_id] = true;
        take_items[p.request_id] = items[0];
        items -= (0);
      } else {
        assert !p.found && p.item == -1, "empty take returned an item";
        take_found[p.request_id] = false;
        take_items[p.request_id] = -1;
      }
    }

    on eSnapshot do (p: (items: seq[int])) {
      assert p.items == items, "snapshot disagrees with the queue";
    }
  }
}
