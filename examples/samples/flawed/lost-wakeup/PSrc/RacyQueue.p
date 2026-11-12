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

event eTryTake: (client: machine, consumer: int);
event eTakeResult: (ok: bool, item: int);
event eWait: (client: machine, consumer: int);
event eWake;

machine RacyQueue {
  var items: seq[int];
  var waiters: seq[(client: machine, consumer: int)];

  start state Serving {
    on eTryTake do (r: (client: machine, consumer: int)) {
      var item: int;
      if (sizeof(items) > 0) {
        item = items[0];
        items -= (0);
        announce eTaken, (consumer = r.consumer, item = item);
        send r.client, eTakeResult, (ok = true, item = item);
      } else {
        send r.client, eTakeResult, (ok = false, item = 0);
      }
    }
    on eWait do (w: (client: machine, consumer: int)) {
      // BUG: registers without looking at the queue again.
      waiters += (sizeof(waiters), w);
      announce eSleeping, w.consumer;
    }
    on ePut do (item: int) {
      items += (sizeof(items), item);
      announce eQueued, item;
      if (sizeof(waiters) > 0) {
        announce eWoken, waiters[0].consumer;
        send waiters[0].client, eWake;
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
        send p.queue, eTryTake, (client = this, consumer = p.consumer);
        receive { case eTakeResult: (r: (ok: bool, item: int)) { ok = r.ok; } }
        if (ok) {
          taken = taken + 1;
        } else {
          // Another critical section: the put that would wake us can run before this.
          send p.queue, eWait, (client = this, consumer = p.consumer);
          receive { case eWake: { } }
        }
      }
    }
  }
}
