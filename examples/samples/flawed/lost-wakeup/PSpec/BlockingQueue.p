// The blocking-queue contract, stated over events the queue announces inside its
// critical sections (put, take, a consumer going to sleep, a consumer being woken) and
// the plan the driver announces. It holds for any wake-up protocol.

// From the driver, before anything starts: how many items each consumer will take
// (consumer id -> count). The producer puts exactly their sum.
event QueuePlanned: map[int, int];
event Queued: int;
event Taken: (consumer: int, item: int);
event Sleeping: int;
event Woken: int;

// Safety: items come out in the order they went in, each once, only to a consumer that
// still wants one, and never to a sleeping consumer.
// Progress: every planned item is eventually taken (hot state ItemsOutstanding). A
// consumer left asleep beside a queued item never takes it, so a lost wake-up ends the
// run in this hot state, or, when nothing else can run, as a deadlock.
spec TakeBlocksOnlyWhenEmpty observes QueuePlanned, Queued, Taken, Sleeping, Woken {
  var wants: map[int, int];
  var queued: seq[int];
  var sleeping: set[int];
  var outstanding: int;

  start state Idle {
    on QueuePlanned do (plan: map[int, int]) {
      var c: int;
      wants = plan;
      foreach (c in keys(plan)) { outstanding = outstanding + plan[c]; }
      assert outstanding > 0, "planned no items";
      goto ItemsOutstanding;
    }
    on Queued do { assert false, "item queued before the plan"; }
    on Taken do { assert false, "item taken before the plan"; }
    on Sleeping do { assert false, "consumer slept before the plan"; }
    on Woken do { assert false, "consumer woken before the plan"; }
  }

  hot state ItemsOutstanding {
    on QueuePlanned do { assert false, "queue planned twice"; }
    on Queued do (item: int) { Queued(item); }
    on Taken do (p: (consumer: int, item: int)) { Taken(p.consumer, p.item); }
    on Sleeping do (c: int) { Sleeping(c); }
    on Woken do (c: int) { Woken(c); }
  }

  state AllTaken {
    on QueuePlanned do { assert false, "queue planned twice"; }
    on Queued do (item: int) { assert false, format("item {0} queued beyond the plan", item); }
    on Taken do (p: (consumer: int, item: int)) {
      assert false, format("consumer {0} took item {1} beyond the plan", p.consumer, p.item);
    }
    on Sleeping do (c: int) { assert false, format("consumer {0} slept after every item was taken", c); }
    on Woken do (c: int) { assert false, format("consumer {0} woken after every item was taken", c); }
  }

  fun Queued(item: int) {
    queued += (sizeof(queued), item);
    Settle();
  }

  fun Taken(consumer: int, item: int) {
    assert consumer in wants && wants[consumer] > 0,
      format("consumer {0} took item {1} but wanted no more", consumer, item);
    assert !(consumer in sleeping), format("consumer {0} took item {1} while asleep", consumer, item);
    assert sizeof(queued) > 0, format("consumer {0} took item {1} from an empty queue", consumer, item);
    assert queued[0] == item,
      format("consumer {0} took item {1}, but the oldest queued item is {2}", consumer, item, queued[0]);
    queued -= (0);
    wants[consumer] = wants[consumer] - 1;
    outstanding = outstanding - 1;
    Settle();
  }

  fun Sleeping(c: int) {
    assert c in wants && wants[c] > 0, format("consumer {0} went to sleep but wants no more items", c);
    assert !(c in sleeping), format("consumer {0} went to sleep twice", c);
    sleeping += (c);
    Settle();
  }

  fun Woken(c: int) {
    assert c in sleeping, format("consumer {0} woken but was not asleep", c);
    sleeping -= (c);
    Settle();
  }

  fun Settle() {
    if (outstanding == 0) { goto AllTaken; }
  }
}
