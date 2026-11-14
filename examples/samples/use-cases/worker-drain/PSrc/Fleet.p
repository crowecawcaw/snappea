// Scale-in behind a load balancer. To remove an instance, the autoscaler:
//   1. tells the load balancer to deregister it, and waits for the ack;
//   2. tells the instance to drain (finish in-flight requests, accept no new ones),
//      and waits for "drained";
//   3. terminates it.
// It refuses to remove the last instance.
//
// Atomic steps: each machine handles one message at a time. The load balancer's route
// and deregister are atomic with respect to each other, but a request it routed just
// before deregistering travels separately (a BBDelivery) and can reach the instance after
// it started draining, or after it was terminated (the connection is refused). Either
// way the request comes back to the load balancer, which routes it again.

type WorkRequest = (id: int, client: machine);

event Request: WorkRequest;
event Response: (id: int);
event Work: WorkRequest;
// Instance -> load balancer: I did not take this request; route it elsewhere.
event Bounce: WorkRequest;
// Driver -> load balancer: the instances to put in rotation at start.
event Join: map[int, machine];
event Deregister: (inst: int, from: machine);
event DeregisterAck: (inst: int);
event Drain: machine;
event Drained: (inst: int);
event Terminate;
// Instance to itself: the request's work is done.
event Finish: int;
// Driver -> autoscaler: remove instance `inst`.
event ScaleIn: (inst: int);

machine LoadBalancer {
  // instance id -> instance, for instances in rotation
  var rotation: map[int, machine];

  start state Starting {
    on Join goto Routing with (instances: map[int, machine]) {
      rotation = instances;
    }
    defer Request;
  }

  state Routing {
    on Request do (w: WorkRequest) { Route(w); }
    on Bounce do (w: WorkRequest) { Route(w); }
    on Deregister do (d: (inst: int, from: machine)) {
      rotation -= (d.inst);
      announce Deregistered, (inst = d.inst,);
      send d.from, DeregisterAck, (inst = d.inst,);
    }
  }

  // Any instance in rotation will do (the real policy may be least-connections).
  fun Route(w: WorkRequest) {
    var ids: seq[int];
    var inst: int;
    ids = keys(rotation);
    inst = ids[choose(sizeof(ids))];
    announce Routed, (id = w.id, inst = inst);
    new BBDelivery((destination = rotation[inst], message = Work, payload = w));
  }
}

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
    on Work do (w: WorkRequest) {
      inFlight[w.id] = w.client;
      announce Accepted, (id = w.id, inst = id);
      send this, Finish, w.id;
    }
    on Finish do (r: int) { Complete(r); }
    on Drain do (from: machine) {
      autoscaler = from;
      announce DrainStarted, (inst = id,);
      ReportIfDrained();
      goto Draining;
    }
  }

  state Draining {
    on Work do (w: WorkRequest) { Refuse(w); }
    on Finish do (r: int) {
      Complete(r);
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

machine Autoscaler {
  var lb: machine;
  // every instance, by id
  var instances: map[int, machine];
  // instances not being removed
  var live: set[int];
  var target: int;

  start state Starting {
    entry (input: (lb: machine, instances: map[int, machine])) {
      var i: int;
      lb = input.lb;
      instances = input.instances;
      foreach (i in keys(instances)) {
        live += (i);
      }
      goto Idle;
    }
  }

  state Idle {
    on ScaleIn do (s: (inst: int)) {
      if (sizeof(live) <= 1) {
        // Removing the last instance would leave requests nowhere to go.
        announce ScaleInRefused, (inst = s.inst,);
        return;
      }
      target = s.inst;
      live -= (target);
      send lb, Deregister, (inst = target, from = this);
      goto Deregistering;
    }
  }

  // One scale-in at a time: later requests wait.
  state Deregistering {
    on DeregisterAck goto Draining with {
      send instances[target], Drain, this;
    }
    defer ScaleIn;
  }

  state Draining {
    on Drained do {
      send instances[target], Terminate;
      goto Idle;
    }
    defer ScaleIn;
  }
}
