// A worker instance. See DESIGN.md. It is created with its id and the load balancer,
// and announces Accepted, Served, Bounced, DrainStarted and Terminated (declared in
// Fleet.p).

machine Instance {
  start state Starting {
    entry (input: (id: int, lb: machine)) {
      // TODO
    }
  }
}
