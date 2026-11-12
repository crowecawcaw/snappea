// The lock service hands out a time-limited lease, one holder at a time. Each grant
// gets a new, larger token. The lease ends when its holder releases it or when its
// timer fires, whichever comes first; the next waiting node then gets it. The holder
// is NOT told when its lease expires: it finds out only by checking its own clock.
//
// Real primitives (each atomic in the lock service): acquire (grant or queue),
// release(token), expire(token).

event eAcquire: (node: machine, id: int);
event eGranted: (token: int);
event eRelease: (token: int);
event eExpire: (token: int);

type tWaiter = (node: machine, id: int);

machine LockService {
  // token of the newest grant
  var token: int;
  var held: bool;
  var holder: int;
  var waiting: seq[tWaiter];

  start state Serving {
    on eAcquire do (w: tWaiter) {
      if (held) {
        waiting += (sizeof(waiting), w);
        return;
      }
      Grant(w);
    }
    // snappea-lint: allow mutation -- a release only frees the lease before its timer would; the model has no time, so the two are indistinguishable
    on eRelease do (r: (token: int)) { End(r.token); }
    on eExpire do (e: (token: int)) { End(e.token); }
  }

  fun End(t: int) {
    var next: tWaiter;
    // A release or timer for a lease that already ended is ignored.
    if (!held || t != token) { return; }
    held = false;
    announce eLeaseEnded, (node = holder,);
    if (sizeof(waiting) > 0) {
      next = waiting[0];
      waiting -= (0);
      Grant(next);
    }
  }

  fun Grant(w: tWaiter) {
    token = token + 1;
    held = true;
    holder = w.id;
    announce eLeaseGranted, (node = w.id,);
    new LeaseTimer((service = this, token = token));
    send w.node, eGranted, (token = token,);
  }
}

// The lease's expiry. It fires once, at a moment the scheduler picks: before the
// holder writes, between its writes, or after it released the lease.
machine LeaseTimer {
  start state Armed {
    entry (t: (service: LockService, token: int)) {
      send t.service, eExpire, (token = t.token,);
    }
  }
}

// A node's own pause between deciding to write and the write leaving the process:
// a GC pause, a page fault, a slow disk, a descheduled VM. Sent to itself, so any
// number of other steps can run before it resumes.
event eResume;
// Storage's answer to a write.
event eWriteResp: (ok: bool);
type tNodeConfig = (id: int, plan: seq[int], locks: LockService, storage: machine);
