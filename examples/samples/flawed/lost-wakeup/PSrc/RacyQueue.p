// A blocking queue built from a lock-protected queue and a wait list, the way a
// condition variable is used. Each handler is one critical section (one lock hold).
//
// FLAWED: the consumer checks for an item in one critical section and registers as a
// waiter in another, like
//
//     if (queue.tryTake() == null) { waiters.add(me); park(); }
//
// A put that lands between the two finds no waiter to wake, and the consumer then parks
// beside an item nobody will tell it about.

event TryTake: (client: machine, consumer: int);
event TakeResult: (ok: bool, item: int);
event Wait: (client: machine, consumer: int);
event Wake;

machine RacyQueue {
  var items: seq[int];
  var waiters: seq[(client: machine, consumer: int)];

  start state Serving {
    on TryTake do (r: (client: machine, consumer: int)) {
      var item: int;
      if (sizeof(items) > 0) {
        item = items[0];
        items -= (0);
        announce Taken, (consumer = r.consumer, item = item);
        send r.client, TakeResult, (ok = true, item = item);
      } else {
        send r.client, TakeResult, (ok = false, item = 0);
      }
    }
    on Wait do (w: (client: machine, consumer: int)) {
      // BUG: registers without looking at the queue again.
      waiters += (sizeof(waiters), w);
      announce Sleeping, w.consumer;
    }
    on Put do (item: int) {
      items += (sizeof(items), item);
      announce Queued, item;
      if (sizeof(waiters) > 0) {
        announce Woken, waiters[0].consumer;
        send waiters[0].client, Wake;
        waiters -= (0);
      }
    }
  }
}

machine RacyConsumer {
  start state Consuming {
    entry (p: (queue: machine, consumer: int, wants: int)) {
      var taken: int;
      var ok: bool;
      while (taken < p.wants) {
        send p.queue, TryTake, (client = this, consumer = p.consumer);
        // The contract checks item values at Taken; this consumer models waiting,
        // not application work on the returned value.
        receive { case TakeResult: (r: (ok: bool, item: int)) { ok = r.ok; } }
        if (ok) {
          taken = taken + 1;
        } else {
          // Another critical section: the put that would wake us can run before this.
          send p.queue, Wait, (client = this, consumer = p.consumer);
          receive { case Wake: { } }
        }
      }
    }
  }
}
