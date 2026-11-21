// The autoscaler. See DESIGN.md. It is created with the load balancer and every
// instance by id, and announces ScaleInRefused (declared in Fleet.p).

machine Autoscaler {
  start state Starting {
    entry (input: (lb: machine, instances: map[int, machine])) {
      // TODO
    }
  }
}
