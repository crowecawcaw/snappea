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

// `n` instances (ids 0..n-1) behind a load balancer, and an autoscaler for them.
fun StartFleet(n: int, requests: int, scaleIns: int): (lb: machine, autoscaler: machine) {
  var instances: map[int, machine];
  var lb: machine;
  var i: int;
  announce FleetPlanned, (instances = n, requests = requests, scaleIns = scaleIns);
  lb = new LoadBalancer();
  while (i < n) {
    instances[i] = new Instance((id = i, lb = lb));
    i = i + 1;
  }
  send lb, Join, instances;
  return (lb = lb, autoscaler = new Autoscaler((lb = lb, instances = instances)));
}

// A client's request is served; then instance 1 is removed.
machine ServeThenScaleIn {
  start state Init {
    entry {
      var f: (lb: machine, autoscaler: machine);
      var trigger: machine;
      f = StartFleet(2, 1, 1);
      trigger = new ScaleInTrigger((autoscaler = f.autoscaler, insts = Ints1(1), after = 1));
      new Client((lb = f.lb, owner = trigger, ids = Ints1(1)));
    }
  }
}

// Two requests in a row are served; then instance 0 is removed, and the request to
// remove instance 1, the last one, is refused.
machine ServeThenRemoveBoth {
  start state Init {
    entry {
      var f: (lb: machine, autoscaler: machine);
      var trigger: machine;
      f = StartFleet(2, 2, 2);
      trigger = new ScaleInTrigger((autoscaler = f.autoscaler, insts = Ints2(0, 1), after = 1));
      new Client((lb = f.lb, owner = trigger, ids = Ints2(1, 2)));
    }
  }
}

test tcServeThenScaleIn [main=ServeThenScaleIn]:
  assert DrainSafety, RequestsFinish in
  { ServeThenScaleIn, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger };

test tcServeThenRemoveBoth [main=ServeThenRemoveBoth]:
  assert DrainSafety, RequestsFinish in
  { ServeThenRemoveBoth, LoadBalancer, Delivery, Instance, Autoscaler, Client, ScaleInTrigger };
