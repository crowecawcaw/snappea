// The network for replica writes. Kept with the tests because it is environment behavior, not client
// logic, and so is not mutated.
//
// P enqueues a message the moment it is sent, so two messages to one replica are
// handled in the order they were sent, even from different clients. A real network
// can deliver a write long after a later request. Deliver models that: a courier
// machine carries the write and sends it when the scheduler runs it.

// Sends a replica write through its own courier, so it can arrive after messages sent
// later by anyone.
fun Deliver(replica: machine, w: (client: machine, version: int, value: int)) {
  new Courier((dest = replica, w = w));
}

machine Courier {
  start state Carry {
    entry (c: (dest: machine, w: (client: machine, version: int, value: int))) {
      send c.dest, eReplicaWrite, c.w;
    }
  }
}
