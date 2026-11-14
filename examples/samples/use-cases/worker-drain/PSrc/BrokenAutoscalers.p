// Negative controls, separate machines so their code is excluded from the passing
// tests' coverage.

// Terminates right after asking the instance to drain, without waiting for "drained":
// requests the instance accepted are dropped.
machine HastyAutoscaler {
  var lb: machine;
  var instances: map[int, machine];
  var target: int;

  start state Starting {
    entry (input: (lb: machine, instances: map[int, machine])) {
      lb = input.lb;
      instances = input.instances;
      goto Idle;
    }
  }

  state Idle {
    on ScaleIn do (s: (inst: int)) {
      target = s.inst;
      send lb, Deregister, (inst = target, from = this);
      goto Deregistering;
    }
    ignore Drained;
  }

  state Deregistering {
    on DeregisterAck goto Idle with {
      send instances[target], Drain, this;
      // BUG: does not wait for Drained.
      send instances[target], Terminate;
    }
  }
}

// Asks the instance to drain at the same time as it asks the load balancer to
// deregister it: the load balancer can still route new requests to the draining
// instance.
machine EarlyDrainAutoscaler {
  var lb: machine;
  var instances: map[int, machine];
  var target: int;

  start state Starting {
    entry (input: (lb: machine, instances: map[int, machine])) {
      lb = input.lb;
      instances = input.instances;
      goto Idle;
    }
  }

  state Idle {
    on ScaleIn do (s: (inst: int)) {
      target = s.inst;
      send lb, Deregister, (inst = target, from = this);
      // BUG: drains before the load balancer acknowledged the deregistration.
      send instances[target], Drain, this;
      goto Waiting;
    }
  }

  state Waiting {
    on Drained do {
      send instances[target], Terminate;
    }
    ignore DeregisterAck;
  }
}
