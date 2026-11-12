// A key-value store replicated on N = 3 replicas, for one key.
//
// Versions: the coordinator (the client library) assigns them with a read phase. It
// reads a quorum, takes the highest counter seen, and writes counter + 1. Because any
// completed write is on W = 2 replicas and every read quorum of R = 2 overlaps it
// (R + W > N), a write that starts after another completed always gets a higher
// version: no synchronized clocks needed, unlike client-supplied timestamps. Two
// concurrent writers can pick the same counter, so the version is (counter, writer id),
// encoded as counter * 10 + writer id, and the writer id breaks the tie.
//
// Real atomic primitives, one handler each on a replica:
//   read:  return the stored (version, value)
//   write: store (version, value) if version is higher than the stored one; ack
// Everything a client does between them can interleave with other clients.

event eReplicaRead: (client: machine);
event eReplicaReadResp: (version: int, value: int);
event eReplicaWrite: (client: machine, version: int, value: int);
event eReplicaWriteResp;

machine Replica {
  var version: int;
  var value: int;

  start state Serving {
    on eReplicaRead do (r: (client: machine)) {
      send r.client, eReplicaReadResp, (version = version, value = value);
    }
    on eReplicaWrite do (w: (client: machine, version: int, value: int)) {
      // Last writer wins by version; an older version arriving late is ignored.
      if (w.version > version) {
        version = w.version;
        value = w.value;
      }
      send w.client, eReplicaWriteResp;
    }
  }
}
