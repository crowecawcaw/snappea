// The group coordinator. It owns the membership, the generation number, the
// partition assignment and the committed offsets. Each handler is one atomic step on
// the coordinator's state, as in Kafka where one broker owns the group.
//
// On every membership change (join, leave, session expiry) it starts a new
// generation and sends every member its partitions. A commit is accepted only from
// the consumer that owns the partition in the current generation: a consumer that
// missed a rebalance (a zombie) is fenced.

event eJoin: (consumer: machine, id: int);
event eLeave: (id: int);
// From the session timer: this member stopped heartbeating, as far as we know.
event eExpire: (id: int);
event eAssignment: (gen: int, partitions: seq[int]);
event eFetch: (consumer: machine, gen: int, partition: int);
event eFetched: (gen: int, partition: int, offset: int);
event eCommit: (consumer: machine, id: int, gen: int, partition: int, offset: int);
event eCommitResp: (gen: int, partition: int, ok: bool);

machine Coordinator {
  var partitions: int;
  var gen: int;
  // member id -> its machine, and the ids in join order
  var members: map[int, machine];
  var order: seq[int];
  // partition -> owning member id, current generation only
  var owner: map[int, int];
  var committed: map[int, int];

  start state Serving {
    entry (n: int) {
      partitions = n;
    }
    on eJoin do (j: (consumer: machine, id: int)) {
      if (j.id in members) {
        // Still a member: it was fenced because a rebalance happened, and that
        // generation's assignment is already on its way to it. Nothing to do.
        return;
      }
      members[j.id] = j.consumer;
      order += (sizeof(order), j.id);
      Rebalance();
    }
    on eLeave do (l: (id: int)) { Remove(l.id); }
    on eExpire do (e: (id: int)) { Remove(e.id); }
    on eFetch do (f: (consumer: machine, gen: int, partition: int)) {
      send f.consumer, eFetched, (gen = f.gen, partition = f.partition, offset = Committed(f.partition));
    }
    on eCommit do (c: (consumer: machine, id: int, gen: int, partition: int, offset: int)) {
      if (c.gen == gen && c.partition in owner && owner[c.partition] == c.id) {
        committed[c.partition] = c.offset;
        announce eCommitted, (partition = c.partition, offset = c.offset, gen = c.gen, member = c.id);
        send c.consumer, eCommitResp, (gen = c.gen, partition = c.partition, ok = true);
        return;
      }
      announce eCommitRejected, (partition = c.partition, offset = c.offset, gen = c.gen, member = c.id);
      send c.consumer, eCommitResp, (gen = c.gen, partition = c.partition, ok = false);
    }
  }

  fun Remove(id: int) {
    var i: int;
    if (!(id in members)) { return; }
    members -= (id);
    while (order[i] != id) { i = i + 1; }
    order -= (i);
    Rebalance();
  }

  // Round-robin over members in join order.
  fun Rebalance() {
    var p: int;
    var id: int;
    gen = gen + 1;
    owner = default(map[int, int]);
    while (p < partitions && sizeof(order) > 0) {
      owner[p] = order[p % sizeof(order)];
      p = p + 1;
    }
    announce eGeneration, (gen = gen, owner = owner);
    foreach (id in order) {
      send members[id], eAssignment, (gen = gen, partitions = Owned(id));
    }
  }

  fun Owned(id: int): seq[int] {
    var parts: seq[int];
    var p: int;
    while (p < partitions) {
      if (p in owner && owner[p] == id) { parts += (sizeof(parts), p); }
      p = p + 1;
    }
    return parts;
  }

  fun Committed(p: int): int {
    if (p in committed) { return committed[p]; }
    return 0;
  }
}
