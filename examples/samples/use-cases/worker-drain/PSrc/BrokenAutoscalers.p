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
    on eScaleIn do (s: (inst: int)) {
      target = s.inst;
      send lb, eDeregister, (inst = target, from = this);
      goto Deregistering;
    }
    ignore eDrained;
  }

  state Deregistering {
    on eDeregisterAck goto Idle with {
      send instances[target], eDrain, this;
      // BUG: does not wait for eDrained.
      send instances[target], eTerminate;
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
    on eScaleIn do (s: (inst: int)) {
      target = s.inst;
      send lb, eDeregister, (inst = target, from = this);
      // BUG: drains before the load balancer acknowledged the deregistration.
      send instances[target], eDrain, this;
      goto Waiting;
    }
  }

  state Waiting {
    on eDrained do {
      send instances[target], eTerminate;
    }
    ignore eDeregisterAck;
  }
}
