// Scale-in behind a load balancer. To remove an instance, the autoscaler:
//   1. tells the load balancer to deregister it, and waits for the ack;
//   2. tells the instance to drain (finish in-flight requests, accept no new ones),
//      and waits for "drained";
//   3. terminates it.
// It refuses to remove the last instance.
//
// Atomic steps: each machine handles one message at a time. The load balancer's route
// and deregister are atomic with respect to each other, but a request it routed just
// before deregistering travels separately (a Courier) and can reach the instance after
// it started draining, or after it was terminated (the connection is refused). Either
// way the request comes back to the load balancer, which routes it again.

type tWork = (id: int, client: machine);

event eRequest: tWork;
event eResponse: (id: int);
event eWork: tWork;
// Instance -> load balancer: I did not take this request; route it elsewhere.
event eBounce: tWork;
// Driver -> load balancer: the instances to put in rotation at start.
event eJoin: map[int, machine];
event eDeregister: (inst: int, from: machine);
event eDeregisterAck: (inst: int);
event eDrain: machine;
event eDrained: (inst: int);
event eTerminate;
// Instance to itself: the request's work is done.
event eFinish: int;
// Driver -> autoscaler: remove instance `inst`.
event eScaleIn: (inst: int);

machine LoadBalancer {
  // instance id -> instance, for instances in rotation
  var rotation: map[int, machine];

  start state Starting {
    on eJoin goto Routing with (instances: map[int, machine]) {
      rotation = instances;
    }
    defer eRequest;
  }

  state Routing {
    on eRequest do (w: tWork) { Route(w); }
    on eBounce do (w: tWork) { Route(w); }
    on eDeregister do (d: (inst: int, from: machine)) {
      rotation -= (d.inst);
      announce eDeregistered, (inst = d.inst,);
      send d.from, eDeregisterAck, (inst = d.inst,);
    }
  }

  // Any instance in rotation will do (the real policy may be least-connections).
  fun Route(w: tWork) {
    var ids: seq[int];
    var inst: int;
    ids = keys(rotation);
    inst = ids[choose(sizeof(ids))];
    announce eRouted, (id = w.id, inst = inst);
    new Courier((dest = rotation[inst], work = w));
  }
}

// One routed request on its own connection to the instance.
machine Courier {
  start state Sending {
    entry (c: (dest: machine, work: tWork)) {
      send c.dest, eWork, c.work;
    }
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
    on eWork do (w: tWork) {
      inFlight[w.id] = w.client;
      announce eAccepted, (id = w.id, inst = id);
      send this, eFinish, w.id;
    }
    on eFinish do (r: int) { Finish(r); }
    on eDrain do (from: machine) {
      autoscaler = from;
      announce eDrainStarted, (inst = id,);
      ReportIfDrained();
      goto Draining;
    }
  }

  state Draining {
    on eWork do (w: tWork) { Bounce(w); }
    on eFinish do (r: int) {
      Finish(r);
      ReportIfDrained();
    }
    on eTerminate do {
      announce eTerminated, (inst = id,);
      goto Terminated;
    }
  }

  // The process is gone and says nothing. A request still in transit gets
  // "connection refused", which the load balancer treats as a bounce: modeled as the
  // bounce reaching the load balancer without the instance announcing anything.
  state Terminated {
    on eWork do (w: tWork) {
      send lb, eBounce, w;
    }
  }

  fun Finish(r: int) {
    var client: machine;
    client = inFlight[r];
    inFlight -= (r);
    announce eServed, (id = r, inst = id);
    send client, eResponse, (id = r,);
  }

  fun Bounce(w: tWork) {
    announce eBounced, (id = w.id, inst = id);
    send lb, eBounce, w;
  }

  fun ReportIfDrained() {
    if (sizeof(inFlight) == 0) {
      send autoscaler, eDrained, (inst = id,);
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
    on eScaleIn do (s: (inst: int)) {
      if (sizeof(live) <= 1) {
        // Removing the last instance would leave requests nowhere to go.
        announce eScaleInRefused, (inst = s.inst,);
        return;
      }
      target = s.inst;
      live -= (target);
      send lb, eDeregister, (inst = target, from = this);
      goto Deregistering;
    }
  }

  // One scale-in at a time: later requests wait.
  state Deregistering {
    on eDeregisterAck goto Draining with {
      send instances[target], eDrain, this;
    }
    defer eScaleIn;
  }

  state Draining {
    on eDrained do {
      send instances[target], eTerminate;
      goto Idle;
    }
    defer eScaleIn;
  }
}
