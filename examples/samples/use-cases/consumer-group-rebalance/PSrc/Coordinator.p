// The group coordinator. It owns the membership, the generation number, the
// partition assignment and the committed offsets. Each handler is one atomic step on
// the coordinator's state, as in Kafka where one broker owns the group.
//
// On every membership change (join, leave, session expiry) it starts a new
// generation and sends every member its partitions. A commit (or offset fetch) is
// accepted only from the consumer that owns the partition in the current generation:
// a consumer that missed a rebalance (a zombie) is fenced.

event Join: (consumer: machine, id: int);
event LeaveGroup: (id: int);
// From the session timer: this member stopped heartbeating, as far as we know.
event Expire: (id: int);
event Assignment: (gen: int, partitions: seq[int]);
event Fetch: (consumer: machine, id: int, gen: int, partition: int);
// Replies echo the member, generation and partition of the request.
event Fetched: (member: int, gen: int, partition: int, ok: bool, offset: int);
event Commit: (consumer: machine, id: int, gen: int, partition: int, offset: int);
event CommitResponse: (member: int, gen: int, partition: int, offset: int, ok: bool);

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
    on Join do (j: (consumer: machine, id: int)) {
      // A retried join from a current member changes nothing: every generation's
      // assignment was already sent to every member, so its assignment is on its way.
      if (j.id in order) { return; }
      members[j.id] = j.consumer;
      order += (sizeof(order), j.id);
      Rebalance();
    }
    on LeaveGroup do (l: (id: int)) { Remove(l.id); }
    on Expire do (e: (id: int)) { Remove(e.id); }
    // Fetching the committed offset is fenced like a commit, so a consumer that was
    // expelled while it had nothing left to commit still finds out and rejoins.
    on Fetch do (f: (consumer: machine, id: int, gen: int, partition: int)) {
      if (Owns(f.id, f.gen, f.partition)) {
        send f.consumer, Fetched, (member = f.id, gen = f.gen, partition = f.partition, ok = true, offset = Committed(f.partition));
        return;
      }
      send f.consumer, Fetched, (member = f.id, gen = f.gen, partition = f.partition, ok = false, offset = 0);
    }
    on Commit do (c: (consumer: machine, id: int, gen: int, partition: int, offset: int)) {
      if (Owns(c.id, c.gen, c.partition)) {
        // commit point: the offset write
        committed[c.partition] = c.offset;
        announce OffsetCommitted, (partition = c.partition, offset = c.offset, gen = c.gen, member = c.id);
        send c.consumer, CommitResponse, (member = c.id, gen = c.gen, partition = c.partition, offset = c.offset, ok = true);
        return;
      }
      send c.consumer, CommitResponse, (member = c.id, gen = c.gen, partition = c.partition, offset = c.offset, ok = false);
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
    announce GenerationChanged, (gen = gen, members = order, owner = owner);
    foreach (id in order) {
      send members[id], Assignment, (gen = gen, partitions = Owned(id));
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
