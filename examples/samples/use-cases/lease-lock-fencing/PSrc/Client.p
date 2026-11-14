// A client that needs the lock to write: acquire (retry while busy, give up after
// maxAttempts tries), write with its fencing token, release. Between the grant and a
// write it can stall for any time (a GC pause); its lease can expire meanwhile.

// How a client behaves once granted. The driver picks it; a real client does not know.
//   Normal:  write, release
//   Pauses:  park until the driver resumes it (a long GC pause), then write, release
//   Crashes: die holding the lease; only expiry frees the lock
enum Behavior { Normal, Pauses, Crashes }
enum Outcome { Wrote, Fenced, GaveUp, Crashed }

// Test hooks for a deterministic pause: the client parks after its grant and waits.
event Parked: int;
event Resume;
event ClientDone: (id: int, outcome: Outcome);

machine Client {
  var owner: machine;
  var lock: LockService;
  var storage: machine;
  var id: int;
  var behavior: Behavior;
  var writes: int;
  var maxAttempts: int;
  var attempts: int;
  var token: int;

  start state Acquiring {
    entry (input: (owner: machine, lock: LockService, storage: machine, id: int,
                   behavior: Behavior, writes: int, maxAttempts: int)) {
      owner = input.owner;
      lock = input.lock;
      storage = input.storage;
      id = input.id;
      behavior = input.behavior;
      writes = input.writes;
      maxAttempts = input.maxAttempts;
      TryAcquire();
    }
    on AcquireResp do (r: (granted: bool, token: int)) {
      if (r.granted) {
        token = r.token;
        if (behavior == Crashes) { goto Done, Crashed; }
        if (behavior == Pauses) {
          send owner, Parked, id;
          goto Paused;
        }
        goto Writing;
      }
      if (attempts < maxAttempts) {
        TryAcquire();
        return;
      }
      goto Done, GaveUp;
    }
  }

  state Paused {
    on Resume goto Writing;
  }

  // Write `writes` times with the same token (a retried or multi-part write), stopping
  // at the first rejection, then release.
  state Writing {
    entry {
      send storage, Write, (client = this, id = id, token = token);
    }
    on WriteResp do (ok: bool) {
      writes = writes - 1;
      if (ok && writes > 0) { goto Writing; }
      // Release even when fenced: the lease may still be ours if it never expired.
      send lock, Release, (client = this, token = token);
      receive { case ReleaseResp: { } }
      if (ok) { goto Done, Wrote; }
      goto Done, Fenced;
    }
  }

  state Done {
    entry (outcome: Outcome) {
      // Specs observe sent events too, so this one send reports the outcome to both.
      // (A crashed client reporting is test bookkeeping: the driver needs to know.)
      send owner, ClientDone, (id = id, outcome = outcome);
    }
  }

  fun TryAcquire() {
    attempts = attempts + 1;
    send lock, Acquire, (client = this, id = id);
  }
}
