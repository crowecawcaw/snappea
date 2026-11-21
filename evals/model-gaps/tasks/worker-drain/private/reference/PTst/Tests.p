// The test environment and the test cases. The load balancer, the instances and the
// autoscaler are the design under test; the machines here play the clients, whoever
// asks for scale-ins, and the autoscaler's step timer.

// Client -> the scale-in trigger: every response this client waits for has arrived.
event ClientDone;
// Step timer to itself: fire once more.
event Tick;

// Sends `ids` one after another, each after the previous response, then tells `owner`.
machine Client {
  var lb: machine;
  var owner: machine;
  var ids: seq[int];
  var next: int;

  start state Sending {
    entry (c: (lb: machine, owner: machine, ids: seq[int])) {
      lb = c.lb;
      owner = c.owner;
      ids = c.ids;
      Send();
    }
    on Response do (r: (id: int)) {
      assert r.id == ids[next], format("client got response {0} while waiting for {1}", r.id, ids[next]);
      announce GotResponse, r;
      next = next + 1;
      if (next < sizeof(ids)) { Send(); } else { send owner, ClientDone; }
    }
  }

  fun Send() {
    announce RequestSent, (id = ids[next],);
    send lb, Request, (id = ids[next], client = this);
  }
}

// Asks the autoscaler to remove each instance in `insts`, in order. With `after` = 0
// it asks at once, so the scale-ins race with the clients; otherwise it first waits
// until `after` clients are done.
machine ScaleInTrigger {
  var autoscaler: machine;
  var insts: seq[int];
  var waiting: int;

  start state Waiting {
    entry (t: (autoscaler: machine, insts: seq[int], after: int)) {
      autoscaler = t.autoscaler;
      insts = t.insts;
      waiting = t.after;
      if (waiting == 0) { goto Asked; }
    }
    on ClientDone do {
      waiting = waiting - 1;
      if (waiting == 0) { goto Asked; }
    }
  }

  state Asked {
    entry {
      var i: int;
      foreach (i in insts) {
        announce ScaleInRequested, (inst = i,);
        send autoscaler, ScaleIn, (inst = i,);
      }
    }
    ignore ClientDone;
  }
}

// The autoscaler's step timer: fires `count` times, each at a moment the scheduler
// picks, whether or not the answer the autoscaler waits for is only slow.
machine StepTimer {
  var autoscaler: machine;
  var left: int;

  start state Running {
    entry (t: (autoscaler: machine, count: int)) {
      autoscaler = t.autoscaler;
      left = t.count;
      if (left > 0) { send this, Tick; }
    }
    on Tick do {
      send autoscaler, StepTimeout;
      left = left - 1;
      if (left > 0) { send this, Tick; }
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

// The network path from the load balancer to one instance: whatever the load balancer
// sends there is in transit for a while, however the design sends it. Everything else
// reaches the instance directly.
machine Link {
  var instance: machine;

  start state Forwarding {
    entry (i: machine) { instance = i; }
    on Work do (w: WorkRequest) {
      new Delivery((destination = instance, message = Work, payload = w));
    }
  }
}

// `n` instances (ids 0..n-1) behind a load balancer, and an autoscaler for them. The
// load balancer reaches each instance through a Link.
fun StartFleet(n: int, requests: int, scaleIns: int): (lb: machine, autoscaler: machine) {
  var instances: map[int, machine];
  var links: map[int, machine];
  var lb: machine;
  var i: int;
  announce FleetPlanned, (instances = n, requests = requests, scaleIns = scaleIns);
  lb = new LoadBalancer();
  while (i < n) {
    instances[i] = new Instance((id = i, lb = lb));
    links[i] = new Link(instances[i]);
    i = i + 1;
  }
  send lb, Join, links;
  return (lb = lb, autoscaler = new Autoscaler((lb = lb, instances = instances)));
}

fun Ints3(a: int, b: int, c: int): seq[int] {
  var s: seq[int];
  s = Ints2(a, b);
  s += (2, c);
  return s;
}

// What one test drives. clients: the request ids each client sends, one client per
// entry. scaleIns: the instances to remove, in order. sequential: ask for the
// scale-ins only after every client is done. timeouts: how often the step timer fires.
type Plan = (instances: int, clients: seq[seq[int]], scaleIns: seq[int], sequential: bool, timeouts: int);

fun Run(p: Plan) {
  var f: (lb: machine, autoscaler: machine);
  var trigger: machine;
  var ids: seq[int];
  var requests: int;
  var after: int;
  foreach (ids in p.clients) { requests = requests + sizeof(ids); }
  f = StartFleet(p.instances, requests, sizeof(p.scaleIns));
  if (p.sequential) { after = sizeof(p.clients); }
  trigger = new ScaleInTrigger((autoscaler = f.autoscaler, insts = p.scaleIns, after = after));
  new StepTimer((autoscaler = f.autoscaler, count = p.timeouts));
  foreach (ids in p.clients) {
    new Client((lb = f.lb, owner = trigger, ids = ids));
  }
}

fun Clients1(a: seq[int]): seq[seq[int]] {
  var s: seq[seq[int]];
  s += (0, a);
  return s;
}

fun Clients2(a: seq[int], b: seq[int]): seq[seq[int]] {
  var s: seq[seq[int]];
  s += (0, a);
  s += (1, b);
  return s;
}

// A client's request is served; then instance 1 is removed. The simple path.
machine ServeThenScaleIn {
  start state Init {
    entry {
      Run((instances = 2, clients = Clients1(Ints1(1)), scaleIns = Ints1(1), sequential = true, timeouts = 0));
    }
  }
}

// Two clients send a request each while instance 0 is removed: requests in flight on the
// draining instance (it must report when the last one finishes), on the other one, and
// routed around the deregistration.
machine ScaleInUnderLoad {
  start state Init {
    entry {
      Run((instances = 2, clients = Clients2(Ints1(1), Ints1(2)), scaleIns = Ints1(0), sequential = false, timeouts = 0));
    }
  }
}

// A client sends three requests in a row while instance 1 is removed: each is routed at
// any moment relative to the removal, so one can reach the instance while it drains
// or after it is gone, and must be served by the other.
machine LateRequestDuringDrain {
  start state Init {
    entry {
      Run((instances = 2, clients = Clients1(Ints3(1, 2, 3)), scaleIns = Ints1(1), sequential = false, timeouts = 0));
    }
  }
}

// Remove 0, then 1: the second request is refused as the last instance, and both of a
// client's requests are still served.
machine LastInstanceRefused {
  start state Init {
    entry {
      Run((instances = 2, clients = Clients1(Ints2(1, 2)), scaleIns = Ints2(0, 1), sequential = false, timeouts = 0));
    }
  }
}

// Instance 0 is asked for twice. The second request finds it removed: not in service,
// although by then only one instance is left.
machine RepeatedScaleIn {
  start state Init {
    entry {
      Run((instances = 2, clients = Clients1(Ints1(1)), scaleIns = Ints2(0, 0), sequential = false, timeouts = 0));
    }
  }
}

// A request for an instance the autoscaler never had is refused; the one after it for
// instance 1 goes through.
machine UnknownInstance {
  start state Init {
    entry {
      Run((instances = 2, clients = Clients1(Ints1(1)), scaleIns = Ints2(7, 1), sequential = false, timeouts = 0));
    }
  }
}

// The step timer fires twice while instance 0 is removed under load: the load balancer
// or the instance gets its command again, possibly with a request still in flight, and
// the autoscaler gets answers twice.
machine TimerFiresDuringRemoval {
  start state Init {
    entry {
      Run((instances = 2, clients = Clients2(Ints1(1), Ints1(2)), scaleIns = Ints1(0), sequential = false, timeouts = 2));
    }
  }
}

// The step timer fires with no removal in progress: before a scale-in request, after a
// refusal, after a removal.
machine TimerFiresWhenIdle {
  start state Init {
    entry {
      Run((instances = 2, clients = Clients1(Ints1(1)), scaleIns = Ints2(5, 0), sequential = true, timeouts = 3));
    }
  }
}

// Instances 0 and 1 of three are removed one after the other under load; the second
// request waits for the first removal.
machine TwoRemovalsOfThree {
  start state Init {
    entry {
      Run((instances = 3, clients = Clients2(Ints1(1), Ints1(2)), scaleIns = Ints2(0, 1), sequential = false, timeouts = 0));
    }
  }
}

// Two removals with the step timer firing: a repeated answer about instance 2 can reach
// the autoscaler while it removes instance 0, which may still have a request in flight.
machine LateAnswersFromFirstRemoval {
  start state Init {
    entry {
      Run((instances = 3, clients = Clients2(Ints2(1, 2), Ints2(3, 4)), scaleIns = Ints2(2, 0), sequential = false, timeouts = 2));
    }
  }
}

// Every instance of three is asked for; the last request is refused. The timer fires
// once somewhere.
machine RemoveAllButOne {
  start state Init {
    entry {
      Run((instances = 3, clients = Clients1(Ints2(1, 2)), scaleIns = Ints3(1, 2, 0), sequential = false, timeouts = 1));
    }
  }
}

// A removal with no client at all, and the timer firing: the lifecycle on its own.
machine NoRequests {
  start state Init {
    entry {
      Run((instances = 2, clients = default(seq[seq[int]]), scaleIns = Ints2(1, 1), sequential = false, timeouts = 2));
    }
  }
}

test tcServeThenScaleIn [main=ServeThenScaleIn]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { ServeThenScaleIn, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcScaleInUnderLoad [main=ScaleInUnderLoad]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { ScaleInUnderLoad, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcLateRequestDuringDrain [main=LateRequestDuringDrain]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { LateRequestDuringDrain, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcLastInstanceRefused [main=LastInstanceRefused]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { LastInstanceRefused, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcRepeatedScaleIn [main=RepeatedScaleIn]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { RepeatedScaleIn, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcUnknownInstance [main=UnknownInstance]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { UnknownInstance, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcTimerFiresDuringRemoval [main=TimerFiresDuringRemoval]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { TimerFiresDuringRemoval, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcTimerFiresWhenIdle [main=TimerFiresWhenIdle]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { TimerFiresWhenIdle, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcTwoRemovalsOfThree [main=TwoRemovalsOfThree]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { TwoRemovalsOfThree, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcLateAnswersFromFirstRemoval [main=LateAnswersFromFirstRemoval]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { LateAnswersFromFirstRemoval, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcRemoveAllButOne [main=RemoveAllButOne]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { RemoveAllButOne, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };

test tcNoRequests [main=NoRequests]:
  assert DrainSafety, RequestsFinish, ScaleInsFinish in
  { NoRequests, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger, StepTimer, Link };
