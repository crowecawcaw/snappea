// Drivers. Consumers and the producer run the design; only these setup machines are
// drivers. Each announces the plan (who takes how many) before starting anything.

fun Items(n: int) : seq[int] {
  var items: seq[int];
  while (sizeof(items) < n) { items += (sizeof(items), 10 * (sizeof(items) + 1)); }
  return items;
}

// The race in its smallest form: one consumer finds the queue empty, one item is put.
machine RacyOneItem {
  start state Init {
    entry {
      var queue: machine;
      var plan: map[int, int];
      plan[1] = 1;
      announce QueuePlanned, plan;
      queue = new RacyQueue();
      new RacyConsumer((queue = queue, consumer = 1, wants = 1));
      new Producer((queue = queue, items = Items(1)));
    }
  }
}

// One consumer takes two items: it may take one that is already queued, sleep on an
// empty queue, or be woken by a put; every order of put, take and sleep is explored.
machine OneConsumerTwoItems {
  start state Init {
    entry {
      var queue: machine;
      var plan: map[int, int];
      plan[1] = 2;
      announce QueuePlanned, plan;
      queue = new WaitQueue();
      new WaitingConsumer((queue = queue, consumer = 1, wants = 2));
      new Producer((queue = queue, items = Items(2)));
    }
  }
}

// Two consumers, one item each: both can sleep at once, a put wakes only one, and a
// woken consumer can find its item taken by the other and must sleep again.
machine TwoConsumers {
  start state Init {
    entry {
      var queue: machine;
      var plan: map[int, int];
      plan[1] = 1;
      plan[2] = 1;
      announce QueuePlanned, plan;
      queue = new WaitQueue();
      new WaitingConsumer((queue = queue, consumer = 1, wants = 1));
      new WaitingConsumer((queue = queue, consumer = 2, wants = 1));
      new Producer((queue = queue, items = Items(2)));
    }
  }
}

// Covered: a consumer went to sleep, then a put arrived and woke it.
scenario PutWakesSleeper observes Sleeping, Queued, Woken {
  start hot state WaitSleep {
    on Sleeping goto WaitPut;
  }
  hot state WaitPut {
    on Queued goto WaitWake;
  }
  hot state WaitWake {
    on Woken goto Covered;
  }
  cold state Covered {
  }
}

test tcFixedOneConsumerTwoItems [main=OneConsumerTwoItems]:
  assert TakeBlocksOnlyWhenEmpty in { OneConsumerTwoItems, WaitQueue, WaitingConsumer, Producer };

test tcFixedTwoConsumers [main=TwoConsumers]:
  assert TakeBlocksOnlyWhenEmpty in { TwoConsumers, WaitQueue, WaitingConsumer, Producer };

// The only blocking receive left at quiescence is the consumer waiting for Wake.
// Match the deadlock message without depending on machine-allocation order.
// snappea: expect-bug "is waiting to receive an event"
test tcBugCheckThenWait [main=RacyOneItem]:
  assert TakeBlocksOnlyWhenEmpty in { RacyOneItem, RacyQueue, RacyConsumer, Producer };
