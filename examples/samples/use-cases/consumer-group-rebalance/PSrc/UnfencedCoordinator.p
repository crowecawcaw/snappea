// A plausible broken coordinator, kept as a negative control: it accepts every commit,
// whatever its generation (offset fetches are still fenced). A consumer that was expelled keeps processing its old
// partition; its late commit is accepted after the new owner committed further, and
// moves the offset back. tcBugNoGenerationCheck shows the contract catching it.
machine UnfencedCoordinator {
  var partitions: int;
  var gen: int;
  var members: map[int, machine];
  var order: seq[int];
  var owner: map[int, int];
  var committed: map[int, int];

  start state Serving {
    entry (n: int) {
      partitions = n;
    }
    on Join do (j: (consumer: machine, id: int)) {
      members[j.id] = j.consumer;
      order += (sizeof(order), j.id);
      Rebalance();
    }
    on LeaveGroup do (l: (id: int)) { Remove(l.id); }
    on Expire do (e: (id: int)) { Remove(e.id); }
    // Offset fetches are fenced as in Coordinator; only commits are not.
    on Fetch do (f: (consumer: machine, id: int, gen: int, partition: int)) {
      var offset: int;
      if (f.partition in committed) { offset = committed[f.partition]; }
      if (f.gen == gen && f.partition in owner && owner[f.partition] == f.id) {
        announce FetchServed, (partition = f.partition, gen = f.gen, member = f.id);
        send f.consumer, Fetched, (gen = f.gen, partition = f.partition, ok = true, offset = offset);
        return;
      }
      announce FetchRejected, (partition = f.partition, gen = f.gen, member = f.id);
      send f.consumer, Fetched, (gen = f.gen, partition = f.partition, ok = false, offset = 0);
    }
    on Commit do (c: (consumer: machine, id: int, gen: int, partition: int, offset: int)) {
      // BUG: no generation or ownership check.
      committed[c.partition] = c.offset;
      announce OffsetCommitted, (partition = c.partition, offset = c.offset, gen = c.gen, member = c.id);
      send c.consumer, CommitResponse, (gen = c.gen, partition = c.partition, ok = true);
    }
  }

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
}
