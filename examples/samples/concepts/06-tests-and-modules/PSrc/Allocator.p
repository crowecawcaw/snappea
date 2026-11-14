// The design: an allocator hands out unique ids from 0 to limit - 1, and refuses once
// they are all used. Each request is one atomic step.

event Alloc: (client: machine);
event IdResponse: (id: int);
event NoIdsLeft;

machine Allocator {
  var limit: int;
  var next: int;

  start state Serving {
    entry (n: int) {
      limit = n;
    }
    on Alloc do (a: (client: machine)) {
      if (next < limit) {
        send a.client, IdResponse, (id = next,);
        next = next + 1;
      } else {
        send a.client, NoIdsLeft;
      }
    }
  }
}

// A plausible broken variant, kept as a negative control: when the ids run out it
// starts again from 0 instead of refusing. tcBugWrapsAround swaps it in for Allocator.
machine WrappingAllocator {
  var limit: int;
  var next: int;

  start state Serving {
    entry (n: int) {
      limit = n;
    }
    on Alloc do (a: (client: machine)) {
      if (next == limit) {
        next = 0;
      }
      send a.client, IdResponse, (id = next,);
      next = next + 1;
    }
  }
}

// The design's callers. They model the outside world, not the design, so they are
// marked as a driver: they are not mutated, and the checker reports their limits.
// snappea: driver
machine Caller {
  var allocator: Allocator;
  var requests: int;
  var made: int;

  start state Asking {
    entry (input: (allocator: Allocator, requests: int)) {
      allocator = input.allocator;
      requests = input.requests;
      Ask();
    }
    on IdResponse do { Next(); }
    on NoIdsLeft do { Next(); }
  }

  state Done {}

  fun Ask() {
    made = made + 1;
    announce IdRequested;
    send allocator, Alloc, (client = this,);
  }

  fun Next() {
    if (made < requests) {
      Ask();
    } else {
      goto Done;
    }
  }
}
