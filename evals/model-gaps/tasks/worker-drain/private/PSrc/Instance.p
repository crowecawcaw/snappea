// A worker instance. See DESIGN.md. It is created with its id and the load balancer,
// and announces Accepted, Served, Bounced, DrainStarted and Terminated.

machine Instance {
  var id: int;
  var lb: machine;
  var autoscaler: machine;
  // request id -> client, for requests accepted and not finished
  var inFlight: map[int, machine];

  start state Starting {
    entry (input: (id: int, lb: machine)) {
      id = input.id;
      lb = input.lb;
      goto Serving;
    }
  }

  state Serving {
    on Work do (w: WorkRequest) { Accept(w); }
    on Finish do (r: int) { Complete(r); }
    on Drain do (from: machine) {
      autoscaler = from;
      announce DrainStarted, (inst = id,);
      ReportIfDrained();
      goto Draining;
    }
  }

  state Draining {
    // Routed before the load balancer deregistered this instance, arriving late.
    on Work do (w: WorkRequest) { Refuse(w); }
    on Finish do (r: int) {
      Complete(r);
      // The last one in flight may just have finished.
      ReportIfDrained();
    }
    // The autoscaler's timer fired and it asked again: answer again, if it is true.
    on Drain do (again: machine) {
      autoscaler = again;
      ReportIfDrained();
    }
    on Terminate do {
      announce Terminated, (inst = id,);
      goto Gone;
    }
  }

  // The process is gone and says nothing. A request still in transit gets
  // "connection refused", which the load balancer treats as a bounce: modeled as the
  // bounce reaching the load balancer without the instance announcing anything.
  state Gone {
    on Work do (w: WorkRequest) {
      send lb, Bounce, w;
    }
    ignore Drain, Terminate;
  }

  fun Accept(w: WorkRequest) {
    inFlight[w.id] = w.client;
    announce Accepted, (id = w.id, inst = id);
    // The work takes time.
    new Delivery((destination = this, message = Finish, payload = w.id));
  }

  fun Complete(r: int) {
    var client: machine;
    client = inFlight[r];
    inFlight -= (r);
    announce Served, (id = r, inst = id);
    send client, Response, (id = r,);
  }

  fun Refuse(w: WorkRequest) {
    announce Bounced, (id = w.id, inst = id);
    send lb, Bounce, w;
  }

  fun ReportIfDrained() {
    if (sizeof(inFlight) == 0) {
      send autoscaler, Drained, (inst = id,);
    }
  }
}
