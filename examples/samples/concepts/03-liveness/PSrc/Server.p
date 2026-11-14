// The design: a server that hands one request at a time to a worker. While the worker
// is busy, new requests wait in the server's queue (defer) until it is idle again.

// One unit of work: the request id and who to answer.
type Task = (client: machine, id: int);

event Request: Task;
event Response: (id: int);
event Job: Task;
event JobDone: Task;

machine Server {
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
    // Keep new requests queued until the worker is free.
    defer Request;
    on JobDone do (r: Task) {
      send r.client, Response, (id = r.id,);
      goto Idle;
    }
  }
}

// Does the work for one request at a time and reports back.
machine Worker {
  var server: machine;

  start state Working {
    entry (s: machine) {
      server = s;
    }
    on Job do (r: Task) {
      send server, JobDone, r;
    }
  }
}
