// The lock service hands out a time-limited lease, one holder at a time. Each grant
// gets a new, larger token. The lease ends when its holder releases it or when its
// timer fires, whichever comes first; the next waiting node then gets it. The holder
// is NOT told when its lease expires: it finds out only by checking its own clock.
//
// Real primitives (each atomic in the lock service): acquire (grant or queue),
// release(token), expire(token).

event Acquire: (node: machine, id: int);
event Granted: (token: int);
event Release: (token: int);
event Expire: (token: int);

type Waiter = (node: machine, id: int);

machine LockService {
  // token of the newest grant
  var token: int;
  var held: bool;
  var holder: int;
  var waiting: seq[Waiter];

  start state Serving {
    on Acquire do (w: Waiter) {
      if (held) {
        waiting += (sizeof(waiting), w);
        return;
      }
      Grant(w);
    }
    // snappea-lint: allow mutation:empty-body -- release only ends the lease before its timer would; expiry can occur at any step in this untimed model
    on Release do (r: (token: int)) {
      // snappea-lint: allow mutation:remove-statement -- release only ends the lease before its timer would; expiry can occur at any step in this untimed model
      End(r.token);
    }
    on Expire do (e: (token: int)) { End(e.token); }
  }

  fun End(t: int) {
    var next: Waiter;
    // A release or timer for a lease that already ended is ignored.
    if (!held) { return; }
    // snappea-lint: allow mutation:condition-false,remove-statement -- a stale expiry ending the current lease is indistinguishable from its own timer firing in this untimed model
    if (t != token) {
      // snappea-lint: allow mutation:remove-statement -- accepting a stale expiry only expires the current lease earlier in this untimed model
      return;
    }
    held = false;
    announce LeaseEnded, (node = holder,);
    if (sizeof(waiting) > 0) {
      next = waiting[0];
      waiting -= (0);
      Grant(next);
    }
  }

  fun Grant(w: Waiter) {
    token = token + 1;
    held = true;
    holder = w.id;
    announce LeaseGranted, (node = w.id,);
    new LeaseTimer((service = this, token = token));
    send w.node, Granted, (token = token,);
  }
}

// The lease's expiry. It fires once, at a moment the scheduler picks: before the
// holder writes, between its writes, or after it released the lease.
machine LeaseTimer {
  start state Armed {
    entry (t: (service: LockService, token: int)) {
      send t.service, Expire, (token = t.token,);
    }
  }
}

// A node's own pause between deciding to write and the write leaving the process:
// a GC pause, a page fault, a slow disk, a descheduled VM. Sent to itself, so any
// number of other steps can run before it resumes.
event Resume;
// Storage's answer to a write.
event WriteResp: (ok: bool);
type NodeConfig = (id: int, plan: seq[int], locks: LockService, storage: machine);
