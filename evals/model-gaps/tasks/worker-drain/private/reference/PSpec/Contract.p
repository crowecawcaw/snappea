// The scale-in contract (DESIGN.md), stated over the events the load balancer, the
// instances and the autoscaler announce (declared in PSrc/Fleet.p) and what the test
// environment announces it asked for. The specs rebuild each instance's lifecycle
// (in rotation -> deregistered -> draining -> terminated) from those events.

// Safety. Routing: the load balancer never routes to a deregistered or terminated
// instance, and routes a request again only after its instance refused it or is gone.
// Lifecycle: an instance is deregistered once, starts draining once and only after it
// was deregistered, accepts nothing while draining, refuses only while draining, says
// nothing once terminated, and is terminated once, only while draining with nothing in
// flight. Refusals: the last instance is never removed, and a refusal gives the reason
// that holds.
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
      assert d.inst in registered, format("instance {0} deregistered twice, or never in rotation", d.inst);
      assert sizeof(registered) > 1, format("last instance {0} was deregistered", d.inst);
      registered -= (d.inst);
    }
    on Accepted do (a: (id: int, inst: int)) {
      assert a.inst in inFlight, format("unknown instance {0} accepted request {1}", a.inst, a.id);
      assert !(a.inst in terminated), format("terminated instance {0} accepted request {1}", a.inst, a.id);
      assert !(a.inst in draining), format("draining instance {0} accepted new request {1}", a.inst, a.id);
      Delivered(a.id, a.inst);
      inFlight[a.inst] += (a.id);
    }
    on Served do (s: (id: int, inst: int)) {
      assert s.inst in inFlight && s.id in inFlight[s.inst],
        format("instance {0} served request {1} it had not accepted", s.inst, s.id);
      inFlight[s.inst] -= (s.id);
    }
    on Bounced do (b: (id: int, inst: int)) {
      assert !(b.inst in terminated), format("terminated instance {0} answered request {1}", b.inst, b.id);
      assert b.inst in draining, format("instance {0} refused request {1} although it was serving", b.inst, b.id);
      Delivered(b.id, b.inst);
    }
    on DrainStarted do (d: (inst: int)) {
      assert d.inst in inFlight, format("unknown instance {0} started draining", d.inst);
      assert !(d.inst in terminated), format("terminated instance {0} started draining", d.inst);
      assert !(d.inst in draining), format("instance {0} started draining twice", d.inst);
      assert !(d.inst in registered),
        format("instance {0} started draining while the load balancer still routes to it", d.inst);
      draining += (d.inst);
    }
    on Terminated do (t: (inst: int)) {
      var id: int;
      assert !(t.inst in terminated), format("instance {0} terminated twice", t.inst);
      assert t.inst in draining, format("instance {0} terminated without draining", t.inst);
      foreach (id in inFlight[t.inst]) {
        assert false, format("instance {0} terminated with request {1} in flight", t.inst, id);
      }
      draining -= (t.inst);
      terminated += (t.inst);
    }
    on ScaleInRefused do (r: (inst: int, reason: RefuseReason)) {
      // Scale-ins run one at a time, so when the autoscaler decides, the instances in
      // service are exactly those still in rotation.
      if (r.reason == LastInstance) {
        assert r.inst in registered,
          format("scale-in of instance {0} refused as the last instance, but it is not in service", r.inst);
        assert sizeof(registered) == 1,
          format("scale-in of instance {0} refused as the last instance, but {1} are in service", r.inst, sizeof(registered));
      } else {
        assert !(r.inst in registered),
          format("scale-in of instance {0} refused as not in service, but it is", r.inst);
      }
    }
  }

  fun Delivered(id: int, inst: int) {
    assert id in routedTo && routedTo[id] == inst,
      format("instance {0} got request {1}, which was not routed to it", inst, id);
    routedTo -= (id);
  }
}

// Progress: every request gets exactly one response, served by an instance.
spec RequestsFinish observes FleetPlanned, RequestSent, Served, GotResponse {
  var requestsLeft: int;
  var outstanding: set[int];
  var served: set[int];

  start state Init {
    on FleetPlanned do (p: (instances: int, requests: int, scaleIns: int)) {
      assert p.instances > 0, "planned no instances";
      requestsLeft = p.requests;
      if (requestsLeft > 0) { goto Busy; } else { goto Finished; }
    }
    on RequestSent do { assert false, "request before the fleet was planned"; }
    on Served do { assert false, "request served before the fleet was planned"; }
    on GotResponse do { assert false, "response before the fleet was planned"; }
  }

  hot state Busy {
    on FleetPlanned do { assert false, "fleet planned twice"; }
    on RequestSent do (r: (id: int)) {
      assert !(r.id in outstanding) && !(r.id in served), format("request {0} sent twice", r.id);
      outstanding += (r.id);
    }
    on Served do (s: (id: int, inst: int)) {
      assert s.id in outstanding, format("request {0} served but not outstanding", s.id);
      assert !(s.id in served), format("request {0} served twice", s.id);
      served += (s.id);
    }
    on GotResponse do (r: (id: int)) {
      assert r.id in outstanding, format("client got a second response to request {0}", r.id);
      assert r.id in served, format("client got a response to request {0}, which no instance served", r.id);
      outstanding -= (r.id);
      requestsLeft = requestsLeft - 1;
      if (requestsLeft == 0) { goto Finished; }
    }
  }

  state Finished {
    on FleetPlanned do { assert false, "fleet planned twice"; }
    on RequestSent do { assert false, "request after every planned request finished"; }
    on Served do { assert false, "request served after every planned request finished"; }
    on GotResponse do { assert false, "response after every planned request finished"; }
  }
}

// Progress: every scale-in request ends, with the instance terminated or the request
// refused.
spec ScaleInsFinish observes FleetPlanned, ScaleInRequested, Terminated, ScaleInRefused {
  var scaleInsLeft: int;
  // instance -> scale-in requests for it that have not ended
  var asked: map[int, int];

  start state Init {
    on FleetPlanned do (p: (instances: int, requests: int, scaleIns: int)) {
      scaleInsLeft = p.scaleIns;
      if (scaleInsLeft > 0) { goto Busy; } else { goto Finished; }
    }
    on ScaleInRequested do { assert false, "scale-in before the fleet was planned"; }
    on Terminated do { assert false, "termination before the fleet was planned"; }
    on ScaleInRefused do { assert false, "refusal before the fleet was planned"; }
  }

  hot state Busy {
    on FleetPlanned do { assert false, "fleet planned twice"; }
    on ScaleInRequested do (s: (inst: int)) {
      if (!(s.inst in asked)) { asked[s.inst] = 0; }
      asked[s.inst] = asked[s.inst] + 1;
    }
    on Terminated do (t: (inst: int)) { Resolve(t.inst); }
    on ScaleInRefused do (r: (inst: int, reason: RefuseReason)) { Resolve(r.inst); }
  }

  state Finished {
    on FleetPlanned do { assert false, "fleet planned twice"; }
    on ScaleInRequested do { assert false, "scale-in after every planned scale-in ended"; }
    on Terminated do { assert false, "termination after every planned scale-in ended"; }
    on ScaleInRefused do { assert false, "refusal after every planned scale-in ended"; }
  }

  fun Resolve(inst: int) {
    assert inst in asked && asked[inst] > 0,
      format("instance {0} removed or refused without a scale-in request for it", inst);
    asked[inst] = asked[inst] - 1;
    scaleInsLeft = scaleInsLeft - 1;
    if (scaleInsLeft == 0) { goto Finished; }
  }
}
