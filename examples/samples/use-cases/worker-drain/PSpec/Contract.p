// The scale-in contract, over events the load balancer, the instances and the
// autoscaler announce at their commit points, and what the drivers (clients and the
// scale-in trigger) announce they asked for. The specs rebuild each instance's
// lifecycle (registered -> deregistered -> draining -> terminated) from those events.

// Driver: `instances` instances are registered; clients will send `requests` requests
// and the scale-in trigger will ask `scaleIns` times.
event FleetPlanned: (instances: int, requests: int, scaleIns: int);
// Client (driver): sent request `id` / got its response.
event RequestSent: (id: int);
event GotResponse: (id: int);
// Scale-in trigger (driver): asked to remove `inst`.
event ScaleInRequested: (inst: int);

// Load balancer: sent request `id` to instance `inst`.
event Routed: (id: int, inst: int);
// Load balancer: removed `inst` from rotation; it routes nothing there from now on.
event Deregistered: (inst: int);
// Instance: took request `id` / finished it and replied / refused it while draining.
event Accepted: (id: int, inst: int);
event Served: (id: int, inst: int);
event Bounced: (id: int, inst: int);
// Instance: started draining / was terminated.
event DrainStarted: (inst: int);
event Terminated: (inst: int);
// Autoscaler: refused to remove `inst` because it is the last instance.
event ScaleInRefused: (inst: int);

// Safety: the load balancer never routes to a deregistered or terminated instance, and
// routes a request again only after its instance refused it or is gone; an instance
// drains only after it was deregistered, accepts nothing while draining, refuses only
// while draining, says nothing once terminated, and is terminated only after draining
// with nothing in flight; the last instance is never removed.
spec DrainSafety observes FleetPlanned, Routed, Deregistered, Accepted, Served, Bounced, DrainStarted,
    Terminated, ScaleInRefused {
  var registered: set[int];
  var draining: set[int];
  var terminated: set[int];
  // instance -> requests it accepted and has not served
  var inFlight: map[int, set[int]];
  // request -> instance it was last routed to, until that instance takes or refuses it
  var routedTo: map[int, int];

  start state Init {
    // snappea-lint: allow unused-payload-field -- RequestsAndScaleInsFinish counts requests and scale-ins
    on FleetPlanned do (p: (instances: int, requests: int, scaleIns: int)) {
      var i: int;
      while (i < p.instances) {
        registered += (i);
        inFlight[i] = default(set[int]);
        i = i + 1;
      }
      goto Watching;
    }
    on Routed do { assert false, "request routed before the fleet was planned"; }
    on Deregistered do { assert false, "deregistration before the fleet was planned"; }
    on Accepted do { assert false, "request accepted before the fleet was planned"; }
    on Served do { assert false, "request served before the fleet was planned"; }
    on Bounced do { assert false, "request bounced before the fleet was planned"; }
    on DrainStarted do { assert false, "drain before the fleet was planned"; }
    on Terminated do { assert false, "termination before the fleet was planned"; }
    on ScaleInRefused do { assert false, "scale-in refused before the fleet was planned"; }
  }

  state Watching {
    on FleetPlanned do { assert false, "fleet planned twice"; }
    on Routed do (r: (id: int, inst: int)) {
      assert !(r.inst in terminated), format("request {0} routed to terminated instance {1}", r.id, r.inst);
      assert r.inst in registered, format("request {0} routed to deregistered instance {1}", r.id, r.inst);
      // Routing a request again is legitimate only when its last instance refused it,
      // or is gone (the connection was refused).
      assert !(r.id in routedTo) || routedTo[r.id] in terminated,
        format("request {0} routed again while instance {1} still had it", r.id, routedTo[r.id]);
      routedTo[r.id] = r.inst;
    }
    on Deregistered do (d: (inst: int)) {
      assert d.inst in registered, format("instance {0} deregistered twice", d.inst);
      assert sizeof(registered) > 1, format("last instance {0} was deregistered", d.inst);
      registered -= (d.inst);
    }
    on Accepted do (a: (id: int, inst: int)) {
      assert !(a.inst in terminated), format("terminated instance {0} accepted request {1}", a.inst, a.id);
      assert !(a.inst in draining), format("draining instance {0} accepted new request {1}", a.inst, a.id);
      Delivered(a.id, a.inst);
      inFlight[a.inst] += (a.id);
    }
    // snappea-lint: allow unused-payload-field -- DrainSafety checks which instance served it
    on Served do (s: (id: int, inst: int)) {
      assert s.id in inFlight[s.inst], format("instance {0} served request {1} it never accepted", s.inst, s.id);
      inFlight[s.inst] -= (s.id);
    }
    on Bounced do (b: (id: int, inst: int)) {
      assert !(b.inst in terminated), format("terminated instance {0} answered request {1}", b.inst, b.id);
      assert b.inst in draining, format("instance {0} refused request {1} although it was serving", b.inst, b.id);
      Delivered(b.id, b.inst);
    }
    on DrainStarted do (d: (inst: int)) {
      assert !(d.inst in registered),
        format("instance {0} started draining while the load balancer still routes to it", d.inst);
      assert !(d.inst in draining), format("instance {0} drained twice", d.inst);
      draining += (d.inst);
    }
    on Terminated do (t: (inst: int)) {
      var id: int;
      assert t.inst in draining, format("instance {0} terminated without draining", t.inst);
      foreach (id in inFlight[t.inst]) {
        assert false, format("instance {0} terminated with request {1} in flight", t.inst, id);
      }
      draining -= (t.inst);
      terminated += (t.inst);
    }
    on ScaleInRefused do (r: (inst: int)) {
      assert sizeof(registered) == 1 && r.inst in registered,
        format("scale-in of instance {0} refused although it is not the last instance", r.inst);
    }
  }

  fun Delivered(id: int, inst: int) {
    assert id in routedTo && routedTo[id] == inst,
      format("instance {0} got request {1}, which was not routed to it", inst, id);
    routedTo -= (id);
  }
}

// Progress: every request gets exactly one response, and every scale-in request ends
// with the instance terminated or the request refused.
spec RequestsAndScaleInsFinish observes FleetPlanned, RequestSent, GotResponse, Served, ScaleInRequested,
    Terminated, ScaleInRefused {
  var requestsLeft: int;
  var scaleInsLeft: int;
  var outstanding: set[int];
  var served: set[int];
  var asked: set[int];

  start state Init {
    // snappea-lint: allow unused-payload-field -- RequestsAndScaleInsFinish counts requests and scale-ins
    on FleetPlanned do (p: (instances: int, requests: int, scaleIns: int)) {
      assert p.instances > 0 && p.requests > 0, "planned no instances or no requests";
      requestsLeft = p.requests;
      scaleInsLeft = p.scaleIns;
      goto Busy;
    }
    on RequestSent do { assert false, "request before the fleet was planned"; }
    on GotResponse do { assert false, "response before the fleet was planned"; }
    on Served do { assert false, "request served before the fleet was planned"; }
    on ScaleInRequested do { assert false, "scale-in before the fleet was planned"; }
    on Terminated do { assert false, "termination before the fleet was planned"; }
    on ScaleInRefused do { assert false, "refusal before the fleet was planned"; }
  }

  hot state Busy {
    on FleetPlanned do { assert false, "fleet planned twice"; }
    on RequestSent do (r: (id: int)) {
      assert !(r.id in outstanding), format("request {0} outstanding twice", r.id);
      outstanding += (r.id);
    }
    // snappea-lint: allow unused-payload-field -- DrainSafety checks which instance served it
    on Served do (s: (id: int, inst: int)) {
      assert s.id in outstanding, format("request {0} served but never outstanding", s.id);
      assert !(s.id in served), format("request {0} served twice", s.id);
      served += (s.id);
    }
    on GotResponse do (r: (id: int)) {
      assert r.id in served, format("client got a response to request {0}, which no instance served", r.id);
      outstanding -= (r.id);
      requestsLeft = requestsLeft - 1;
      Check();
    }
    on ScaleInRequested do (s: (inst: int)) {
      assert !(s.inst in asked), format("scale-in of instance {0} asked twice", s.inst);
      asked += (s.inst);
    }
    on Terminated do (t: (inst: int)) { Resolve(t.inst); }
    on ScaleInRefused do (r: (inst: int)) { Resolve(r.inst); }
  }

  state Finished {
    on FleetPlanned do { assert false, "fleet planned twice"; }
    on RequestSent do { assert false, "request after everything finished"; }
    on GotResponse do { assert false, "response after everything finished"; }
    on Served do { assert false, "request served after everything finished"; }
    on ScaleInRequested do { assert false, "scale-in after everything finished"; }
    on Terminated do { assert false, "termination after everything finished"; }
    on ScaleInRefused do { assert false, "refusal after everything finished"; }
  }

  fun Resolve(inst: int) {
    assert inst in asked, format("instance {0} removed or refused without a scale-in request", inst);
    asked -= (inst);
    scaleInsLeft = scaleInsLeft - 1;
    Check();
  }

  fun Check() {
    if (requestsLeft == 0 && scaleInsLeft == 0) { goto Finished; }
  }
}
