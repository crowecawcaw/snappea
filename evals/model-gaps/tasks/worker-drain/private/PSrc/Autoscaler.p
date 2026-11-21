// The autoscaler. See DESIGN.md. It is created with the load balancer and every
// instance by id, and announces ScaleInRefused.

machine Autoscaler {
  var lb: machine;
  // every instance, by id
  var instances: map[int, machine];
  // instances in service: not removed and not being removed
  var live: set[int];
  // the instance being removed
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
      if (!(s.inst in live)) {
        announce ScaleInRefused, (inst = s.inst, reason = NotInService);
        return;
      }
      if (sizeof(live) <= 1) {
        // Removing the last instance would leave requests nowhere to go.
        announce ScaleInRefused, (inst = s.inst, reason = LastInstance);
        return;
      }
      target = s.inst;
      live -= (target);
      send lb, Deregister, (inst = target, from = this);
      goto Deregistering;
    }
    // No step in progress: a timer or a repeated answer from the last removal.
    ignore StepTimeout, DeregisterAck, Drained;
  }

  // One scale-in at a time: later requests wait.
  state Deregistering {
    on DeregisterAck do (a: (inst: int)) {
      // A repeated answer from an earlier removal says nothing about this one.
      if (a.inst != target) { return; }
      send instances[target], Drain, this;
      goto Draining;
    }
    on StepTimeout do {
      send lb, Deregister, (inst = target, from = this);
    }
    ignore Drained;
    defer ScaleIn;
  }

  state Draining {
    on Drained do (d: (inst: int)) {
      // A repeated answer from an earlier removal says nothing about this one.
      if (d.inst != target) { return; }
      send instances[target], Terminate;
      goto Idle;
    }
    on StepTimeout do {
      send instances[target], Drain, this;
    }
    ignore DeregisterAck;
    defer ScaleIn;
  }
}
