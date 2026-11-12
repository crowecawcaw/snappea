// A plausible broken coordinator, kept as a negative control: it accepts every commit,
// whatever its generation. A consumer that was expelled keeps processing its old
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
    on eJoin do (j: (consumer: machine, id: int)) {
      members[j.id] = j.consumer;
      order += (sizeof(order), j.id);
      Rebalance();
    }
    on eLeave do (l: (id: int)) { Remove(l.id); }
    on eExpire do (e: (id: int)) { Remove(e.id); }
    on eFetch do (f: (consumer: machine, gen: int, partition: int)) {
      var offset: int;
      if (f.partition in committed) { offset = committed[f.partition]; }
      send f.consumer, eFetched, (gen = f.gen, partition = f.partition, offset = offset);
    }
    on eCommit do (c: (consumer: machine, id: int, gen: int, partition: int, offset: int)) {
      // BUG: no generation or ownership check.
      committed[c.partition] = c.offset;
      announce eCommitted, (partition = c.partition, offset = c.offset, gen = c.gen, member = c.id);
      send c.consumer, eCommitResp, (gen = c.gen, partition = c.partition, ok = true);
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
}
