// A key-value store replicated on N = 3 replicas, for one key.
//
// Versions: the coordinator (the client library) assigns them with a read phase. It
// reads a quorum, takes the highest counter seen, and writes counter + 1. Because any
// completed write is on W = 2 replicas and every read quorum of R = 2 overlaps it
// (R + W > N), a write that starts after another completed always gets a higher
// version: no synchronized clocks needed, unlike client-supplied timestamps. Two
// concurrent writers can pick the same counter, so the version is (counter, writer id),
// and the writer id breaks the tie.
//
// Real atomic primitives, one handler each on a replica:
//   read:  return the stored (version, value)
//   write: store (version, value) if version is higher than the stored one; ack
// A write's message to the replica outside its quorum can arrive after the write
// returned, and even after a newer write: the version check keeps the newer value.
// Everything a client does between them can interleave with other clients.

// A write's version: its counter, then the writer id to break ties. (0, 0) is the
// initial value's version.
type Version = (counter: int, writer: int);

// True when version a is newer than version b: a higher counter, or the same counter
// and a higher writer id.
fun Newer(a: Version, b: Version): bool {
  if (a.counter > b.counter) { return true; }
  if (a.counter < b.counter) { return false; }
  // snappea-lint: allow mutation:swap-operator -- a version names one write, so an equal version carries the value already stored
  return a.writer > b.writer;
}

event ReplicaRead: (client: machine);
event ReplicaReadResp: (version: Version, value: int);
event ReplicaWrite: (client: machine, version: Version, value: int);
event ReplicaWriteResp;

machine Replica {
  var id: int;
  var version: Version;
  var value: int;

  start state Serving {
    entry (replicaId: int) { id = replicaId; }
    on ReplicaRead do (r: (client: machine)) {
      send r.client, ReplicaReadResp, (version = version, value = value);
    }
    on ReplicaWrite do (w: (client: machine, version: Version, value: int)) {
      // Last writer wins by version; an older version arriving late is ignored.
      if (Newer(w.version, version)) {
        version = w.version;
        value = w.value;
      }
      // commit point: the conditional write
      announce Stored, (replica = id, write = (version = w.version, value = w.value),
                        holds = (version = version, value = value));
      send w.client, ReplicaWriteResp;
    }
  }
}
