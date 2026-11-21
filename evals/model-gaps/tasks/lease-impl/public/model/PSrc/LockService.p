// A lock service that grants leases. Each grant carries a fencing token, one higher
// than the last. A lease ends when its holder releases it or when it expires.
//
// Real atomic primitives (one step of the lock service each):
//   acquire: free -> held(token = last + 1), or busy
//   release: held(token) -> free, only for the caller's own token
//   expire:  held(token) -> free, only if that token still holds it

event Acquire: (client: machine, id: int);
event AcquireResp: (granted: bool, token: int);
event Release: (client: machine, token: int);
event ReleaseResp;
// From a lease's timer: its time to live has passed.
event LeaseExpire: int;

machine LockService {
  var lastToken: int;
  // token of the live lease; 0 when the lock is free
  var holder: int;

  start state Serving {
    on Acquire do (r: (client: machine, id: int)) {
      if (holder != 0) {
        announce Busy, r.id;
        send r.client, AcquireResp, (granted = false, token = 0);
        return;
      }
      lastToken = lastToken + 1;
      holder = lastToken;
      announce Granted, (id = r.id, token = holder);
      new LeaseTimer((lock = this, token = holder));
      send r.client, AcquireResp, (granted = true, token = holder);
    }
    on Release do (r: (client: machine, token: int)) {
      // A holder whose lease already expired must not free its successor's lease.
      if (r.token == holder) {
        holder = 0;
        announce Released, r.token;
      }
      send r.client, ReleaseResp;
    }
    on LeaseExpire do (token: int) {
      if (token == holder) {
        holder = 0;
        announce Expired, token;
      }
    }
  }
}

// One timer per lease. It fires once, at a moment the scheduler picks: before the
// holder writes, while it is paused, or after it released.
machine LeaseTimer {
  start state Armed {
    entry (t: (lock: machine, token: int)) {
      send t.lock, LeaseExpire, t.token;
    }
  }
}
