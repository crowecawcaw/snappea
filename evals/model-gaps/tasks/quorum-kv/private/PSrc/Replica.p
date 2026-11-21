// A replica: one stored (version, value), a read and a conditional write.

// True when version a is newer than version b: a higher counter, or the same counter
// and a higher writer id.
fun Newer(a: Version, b: Version): bool {
  if (a.counter != b.counter) { return a.counter > b.counter; }
  return a.writer > b.writer;
}

machine Replica {
  var id: int;
  var held: Stamped;

  start state Serving {
    entry (replicaId: int) { id = replicaId; }

    on ReplicaRead do (r: (client: machine, op: int)) {
      send r.client, ReplicaReadResp, (replica = id, op = r.op, held = held);
    }

    on ReplicaWrite do (w: (client: machine, op: int, write: Stamped)) {
      // Keep the newest: a late or repeated write must not move the replica back.
      if (Newer(w.write.version, held.version)) {
        held = w.write;
      }
      // commit point: the conditional write
      announce Stored, (replica = id, write = w.write, holds = held);
      // Always ack: the coordinator waits for it also when nothing changed.
      send w.client, ReplicaWriteResp, (replica = id, op = w.op);
    }
  }
}
