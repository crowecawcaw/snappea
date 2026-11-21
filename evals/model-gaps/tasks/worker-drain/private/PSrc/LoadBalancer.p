// The load balancer. See DESIGN.md. It announces Routed and Deregistered.

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
      // A repeated command (the autoscaler's timer fired) is only acknowledged again.
      if (d.inst in rotation) {
        rotation -= (d.inst);
        announce Deregistered, (inst = d.inst,);
      }
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
    // In transit on its own: it can arrive after the instance was told to drain.
    new Delivery((destination = rotation[inst], message = Work, payload = w));
  }
}
