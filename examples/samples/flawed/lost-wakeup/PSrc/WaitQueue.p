// FIXED: taking an item and registering as a waiter are one critical section. The queue
// either hands back an item or records the consumer as a waiter while it still holds the
// lock, so a put either sees the item gone or sees the waiter and wakes it. This is what
// `while (queue.isEmpty()) cond.wait(lock);` does: wait() releases the lock and parks
// atomically. (Re-checking the queue after registering, before parking, also works.)
// A woken consumer tries again, since another consumer may have taken the item first.

event eTakeOrWait: (client: machine, consumer: int);
event eItem: int;

machine WaitQueue {
  var items: seq[int];
  var waiters: seq[(client: machine, consumer: int)];

  start state Serving {
    on eTakeOrWait do (r: (client: machine, consumer: int)) {
      var item: int;
      if (sizeof(items) > 0) {
        item = items[0];
        items -= (0);
        announce eTaken, (consumer = r.consumer, item = item);
        send r.client, eItem, item;
      } else {
        waiters += (sizeof(waiters), r);
        announce eSleeping, r.consumer;
      }
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

machine WaitingConsumer {
  start state Consuming {
    entry (p: (queue: machine, consumer: int, wants: int)) {
      var taken: int;
      while (taken < p.wants) {
        send p.queue, eTakeOrWait, (client = this, consumer = p.consumer);
        receive {
          case eItem: (item: int) { taken = taken + 1; }
          case eWake: { }
        }
      }
    }
  }
}
