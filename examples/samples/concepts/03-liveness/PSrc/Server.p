// The design: a server that hands one request at a time to a worker. While the worker
// is busy, new requests wait in the server's queue (defer) until it is idle again.

type tRequest = (client: machine, id: int);

event eRequest: tRequest;
event eResponse: (id: int);
event eJob: tRequest;
event eJobDone: tRequest;

machine Server {
  var worker: Worker;

  start state Init {
    entry {
      worker = new Worker(this);
      goto Idle;
    }
  }

  state Idle {
    on eRequest do (r: tRequest) {
      send worker, eJob, r;
      goto Busy;
    }
  }

  state Busy {
    // Keep new requests queued until the worker is free.
    defer eRequest;
    on eJobDone do (r: tRequest) {
      send r.client, eResponse, (id = r.id,);
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
    on eJob do (r: tRequest) {
      send server, eJobDone, r;
    }
  }
}
