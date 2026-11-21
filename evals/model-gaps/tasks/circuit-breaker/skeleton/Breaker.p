// The circuit breaker. See DESIGN.md; the events are in Events.p.
//
// The machine's name and what it takes when created are fixed. It starts Closed.

machine Breaker {
  start state Running {
    entry (input: (dependency: machine, timers: machine, threshold: int)) {
      // TODO
    }
  }
}
