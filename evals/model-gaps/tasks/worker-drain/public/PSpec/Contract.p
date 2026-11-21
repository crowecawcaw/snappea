// The scale-in contract (DESIGN.md), stated over the events the load balancer, the
// instances and the autoscaler announce (declared in PSrc/Fleet.p) and what the test
// environment announces it asked for.

// Safety: the load balancer never routes to a deregistered or terminated instance, the
// last instance is never taken out of rotation, and no instance is terminated with a
// request in flight (no accepted request is dropped).
spec DrainSafety observes FleetPlanned, Routed, Deregistered, Accepted, Served, Terminated {
  var registered: set[int];
  var terminated: set[int];
  // instance -> requests it accepted and has not served
  var inFlight: map[int, set[int]];

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
    on Terminated do { assert false, "termination before the fleet was planned"; }
  }

  state Watching {
    on FleetPlanned do { assert false, "fleet planned twice"; }
    on Routed do (r: (id: int, inst: int)) {
      assert !(r.inst in terminated), format("request {0} routed to terminated instance {1}", r.id, r.inst);
      assert r.inst in registered, format("request {0} routed to deregistered instance {1}", r.id, r.inst);
    }
    on Deregistered do (d: (inst: int)) {
      assert sizeof(registered) > 1 || !(d.inst in registered),
        format("last instance {0} was deregistered", d.inst);
      registered -= (d.inst);
    }
    on Accepted do (a: (id: int, inst: int)) {
      assert a.inst in inFlight, format("unknown instance {0} accepted request {1}", a.inst, a.id);
      inFlight[a.inst] += (a.id);
    }
    on Served do (s: (id: int, inst: int)) {
      assert s.inst in inFlight && s.id in inFlight[s.inst],
        format("instance {0} served request {1} it had not accepted", s.inst, s.id);
      inFlight[s.inst] -= (s.id);
    }
    on Terminated do (t: (inst: int)) {
      var id: int;
      assert t.inst in inFlight, format("unknown instance {0} terminated", t.inst);
      foreach (id in inFlight[t.inst]) {
        assert false, format("instance {0} terminated with request {1} in flight", t.inst, id);
      }
      terminated += (t.inst);
    }
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
