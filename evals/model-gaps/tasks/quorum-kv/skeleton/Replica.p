// A replica. See DESIGN.md. It is created with its id (1, 2 or 3) and announces
// Stored (declared in Events.p).

machine Replica {
  start state Serving {
    entry (replicaId: int) {
      // TODO
    }
  }
}
