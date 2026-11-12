// The group coordinator. It owns the membership, the generation number, the
// partition assignment and the committed offsets. Each handler is one atomic step on
// the coordinator's state, as in Kafka where one broker owns the group.
//
// On every membership change (join, leave, session expiry) it starts a new
// generation and sends every member its partitions. A commit (or offset fetch) is
// accepted only from the consumer that owns the partition in the current generation:
// a consumer that missed a rebalance (a zombie) is fenced.

event eJoin: (consumer: machine, id: int);
event eLeave: (id: int);
// From the session timer: this member stopped heartbeating, as far as we know.
event eExpire: (id: int);
event eAssignment: (gen: int, partitions: seq[int]);
event eFetch: (consumer: machine, id: int, gen: int, partition: int);
event eFetched: (gen: int, partition: int, ok: bool, offset: int);
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
      // Only a non-member rejoins. A member fenced by a newer generation got that
      // generation's assignment before the rejection (messages from the coordinator
      // arrive in order), so it drops the stale rejection instead of rejoining.
      members[j.id] = j.consumer;
      order += (sizeof(order), j.id);
      Rebalance();
    }
    on eLeave do (l: (id: int)) { Remove(l.id); }
    on eExpire do (e: (id: int)) { Remove(e.id); }
    // Fetching the committed offset is fenced like a commit, so a consumer that was
    // expelled while it had nothing left to commit still finds out and rejoins.
    on eFetch do (f: (consumer: machine, id: int, gen: int, partition: int)) {
      if (Owns(f.id, f.gen, f.partition)) {
        // snappea-lint: allow mutation:remove-statement -- observation only: dropping it removes a check, not behavior
        announce eFetchServed, (partition = f.partition, gen = f.gen, member = f.id);
        send f.consumer, eFetched, (gen = f.gen, partition = f.partition, ok = true, offset = Committed(f.partition));
        return;
      }
      // snappea-lint: allow mutation:remove-statement -- observation only: dropping it removes a check, not behavior
      announce eFetchRejected, (partition = f.partition, gen = f.gen, member = f.id);
      send f.consumer, eFetched, (gen = f.gen, partition = f.partition, ok = false, offset = 0);
    }
    on eCommit do (c: (consumer: machine, id: int, gen: int, partition: int, offset: int)) {
      if (Owns(c.id, c.gen, c.partition)) {
        committed[c.partition] = c.offset;
        announce eCommitted, (partition = c.partition, offset = c.offset, gen = c.gen, member = c.id);
        send c.consumer, eCommitResp, (gen = c.gen, partition = c.partition, ok = true);
        return;
      }
      // snappea-lint: allow mutation:remove-statement -- observation only: dropping it removes a check, not behavior
      announce eCommitRejected, (partition = c.partition, offset = c.offset, gen = c.gen, member = c.id);
      send c.consumer, eCommitResp, (gen = c.gen, partition = c.partition, ok = false);
    }
  }

  // `members` stays a registry of how to reach each id; `order` is the membership.
  fun Remove(id: int) {
    var i: int;
    while (i < sizeof(order)) {
      if (order[i] == id) {
        order -= (i);
        Rebalance();
        return;
      }
      i = i + 1;
    }
  }

  // Round-robin over members in join order.
  fun Rebalance() {
    var next: map[int, int];
    var p: int;
    var id: int;
    gen = gen + 1;
    while (p < partitions && sizeof(order) > 0) {
      next[p] = order[p % sizeof(order)];
      p = p + 1;
    }
    owner = next;
    announce eGeneration, (gen = gen, members = order, owner = owner);
    foreach (id in order) {
      send members[id], eAssignment, (gen = gen, partitions = Owned(id));
    }
  }

  fun Owned(id: int): seq[int] {
    var parts: seq[int];
    var p: int;
    foreach (p in keys(owner)) {
      if (owner[p] == id) { parts += (sizeof(parts), p); }
    }
    return parts;
  }

  fun Owns(id: int, g: int, p: int): bool {
    return g == gen && p in owner && owner[p] == id;
  }

  fun Committed(p: int): int {
    if (p in committed) { return committed[p]; }
    return 0;
  }
}
