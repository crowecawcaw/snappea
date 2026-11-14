// Clients, the scale-in trigger, and the test cases. The load balancer, instances
// and autoscaler are the design under test; BBDelivery models independent transit.

// Sends `ids` one after another, each after the previous response.
machine Client {
  var lb: machine;
  var ids: seq[int];
  var next: int;

  start state Sending {
    entry (c: (lb: machine, ids: seq[int])) {
      lb = c.lb;
      ids = c.ids;
      Send();
    }
    on Response do (r: (id: int)) {
      assert r.id == ids[next], format("client got response {0} while waiting for {1}", r.id, ids[next]);
      announce GotResponse, r;
      next = next + 1;
      if (next < sizeof(ids)) { Send(); }
    }
  }

  fun Send() {
    announce RequestSent, (id = ids[next],);
    send lb, Request, (id = ids[next], client = this);
  }
}

// Asks the autoscaler to remove each instance in `insts`, in order, at a moment the
// scheduler picks.
machine ScaleInTrigger {
  start state Asking {
    entry (t: (autoscaler: machine, insts: seq[int])) {
      var i: int;
      foreach (i in t.insts) {
        announce ScaleInRequested, (inst = i,);
        send t.autoscaler, ScaleIn, (inst = i,);
      }
    }
  }
}

fun Ints1(a: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  return s;
}

fun Ints2(a: int, b: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  s += (1, b);
  return s;
}

// Two instances (ids 0 and 1) behind a load balancer. Returns the instances by id and
// the load balancer, without an autoscaler (each test picks one).
fun StartFleet(requests: int, scaleIns: int): (lb: LoadBalancer, instances: map[int, machine]) {
  var instances: map[int, machine];
  var lb: LoadBalancer;
  var i: int;
  announce FleetPlanned, (instances = 2, requests = requests, scaleIns = scaleIns);
  lb = new LoadBalancer();
  while (i < 2) {
    instances[i] = new Instance((id = i, lb = lb));
    i = i + 1;
  }
  send lb, Join, instances;
  return (lb = lb, instances = instances);
}

// Two clients send a request each while instance 0 is removed: requests in flight on
// the instance being drained, on the other one, and routed around the deregistration.
machine ScaleInUnderLoad {
  start state Init {
    entry {
      var f: (lb: LoadBalancer, instances: map[int, machine]);
      f = StartFleet(2, 1);
      new ScaleInTrigger((autoscaler = new Autoscaler((lb = f.lb, instances = f.instances)), insts = Ints1(0)));
      new Client((lb = f.lb, ids = Ints1(1)));
      new Client((lb = f.lb, ids = Ints1(2)));
    }
  }
}

// One request, routed at any moment relative to the removal of instance 1: before
// deregistration (it may arrive while 1 drains, or after 1 is gone) or after.
machine LateRequestDuringDrain {
  start state Init {
    entry {
      var f: (lb: LoadBalancer, instances: map[int, machine]);
      f = StartFleet(1, 1);
      new ScaleInTrigger((autoscaler = new Autoscaler((lb = f.lb, instances = f.instances)), insts = Ints1(1)));
      new Client((lb = f.lb, ids = Ints1(1)));
    }
  }
}

// Remove instance 0, then instance 1: the second must be refused, and a client sending
// two requests in a row still gets both served by the instance that remains.
machine LastInstanceRefused {
  start state Init {
    entry {
      var f: (lb: LoadBalancer, instances: map[int, machine]);
      f = StartFleet(2, 2);
      new ScaleInTrigger((autoscaler = new Autoscaler((lb = f.lb, instances = f.instances)), insts = Ints2(0, 1)));
      new Client((lb = f.lb, ids = Ints2(1, 2)));
    }
  }
}

machine BuggyTerminateEarly {
  start state Init {
    entry {
      var f: (lb: LoadBalancer, instances: map[int, machine]);
      f = StartFleet(2, 1);
      new ScaleInTrigger((autoscaler = new HastyAutoscaler((lb = f.lb, instances = f.instances)), insts = Ints1(0)));
      new Client((lb = f.lb, ids = Ints1(1)));
      new Client((lb = f.lb, ids = Ints1(2)));
    }
  }
}

machine BuggyDrainBeforeDeregistered {
  start state Init {
    entry {
      var f: (lb: LoadBalancer, instances: map[int, machine]);
      f = StartFleet(1, 1);
      new ScaleInTrigger((autoscaler = new EarlyDrainAutoscaler((lb = f.lb, instances = f.instances)), insts = Ints1(0)));
      new Client((lb = f.lb, ids = Ints1(1)));
    }
  }
}

// Covered: a request routed to the instance before deregistration reached it while it
// was draining (or gone), and was served elsewhere.
scenario LateRequestRerouted observes Routed, DrainStarted, Bounced, Served {
  var bounced: int;
  start hot state Watching {
    on Bounced do (b: (id: int, inst: int)) { bounced = b.id; goto WaitServed; }
  }
  hot state WaitServed {
    on Served do (s: (id: int, inst: int)) { if (s.id == bounced) { goto Covered; } }
  }
  cold state Covered {
  }
}

test tcScaleInUnderLoad [main=ScaleInUnderLoad]:
  assert DrainSafety, RequestsAndScaleInsFinish in
  { ScaleInUnderLoad, LoadBalancer, BBDelivery, Instance, Autoscaler, Client, ScaleInTrigger };

test tcLateRequestDuringDrain [main=LateRequestDuringDrain]:
  assert DrainSafety, RequestsAndScaleInsFinish in
  { LateRequestDuringDrain, LoadBalancer, BBDelivery, Instance, Autoscaler, Client, ScaleInTrigger };

test tcLastInstanceRefused [main=LastInstanceRefused]:
  assert DrainSafety, RequestsAndScaleInsFinish in
  { LastInstanceRefused, LoadBalancer, BBDelivery, Instance, Autoscaler, Client, ScaleInTrigger };

// snappea: expect-bug "terminated with request"
test tcBugTerminateEarly [main=BuggyTerminateEarly]:
  assert DrainSafety, RequestsAndScaleInsFinish in
  { BuggyTerminateEarly, LoadBalancer, BBDelivery, Instance, HastyAutoscaler, Client, ScaleInTrigger };

// snappea: expect-bug "started draining while the load balancer still routes to it"
test tcBugDrainBeforeDeregistered [main=BuggyDrainBeforeDeregistered]:
  assert DrainSafety, RequestsAndScaleInsFinish in
  { BuggyDrainBeforeDeregistered, LoadBalancer, BBDelivery, Instance, EarlyDrainAutoscaler, Client, ScaleInTrigger };
