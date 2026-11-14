// A plausible broken design, kept as a negative control: while busy it drops new
// requests instead of queueing them. No reply is ever wrong, so a safety spec passes;
// the client simply waits forever. tcBugDropsWhileBusy shows the hot state catching it.
machine DroppingServer {
  var worker: Worker;

  start state Init {
    entry {
      worker = new Worker(this);
      goto Idle;
    }
  }

  state Idle {
    on Request do (r: Task) {
      send worker, Job, r;
      goto Busy;
    }
  }

  state Busy {
    // BUG: should be `defer Request;`.
    ignore Request;
    on JobDone do (r: Task) {
      send r.client, Response, (id = r.id,);
      goto Idle;
    }
  }
}
