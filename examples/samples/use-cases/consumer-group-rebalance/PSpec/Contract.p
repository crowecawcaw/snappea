// The consumer-group contract, over events the coordinator announces when it starts a
// generation or decides a commit, and events consumers announce when they adopt an
// assignment or process a record. The specs rebuild ownership and offsets from that
// history; they never read the coordinator.

event eGeneration: (gen: int, owner: map[int, int]);
event eCommitted: (partition: int, offset: int, gen: int, member: int);
event eCommitRejected: (partition: int, offset: int, gen: int, member: int);
event eAdopted: (member: int, gen: int, partitions: seq[int]);
event eProcessed: (member: int, partition: int, offset: int);

// Safety.
// - Generations only move forward.
// - A commit is accepted only from the partition's owner in the current generation,
//   and a commit from that owner is never rejected.
// - Committed offsets never go back, and only cover records someone processed.
// - Within the current generation, a partition is held by at most one consumer, and
//   only by the one the coordinator assigned it to.
spec GroupSafety observes eGeneration, eCommitted, eCommitRejected, eAdopted, eProcessed {
  var gen: int;
  var owner: map[int, int];
  // generation -> its assignment
  var assigned: map[int, map[int, int]];
  var committed: map[int, int];
  // partition -> offsets processed at least once
  var processed: map[int, set[int]];
  // member -> (generation, partitions) it currently works on
  var held: map[int, (gen: int, partitions: seq[int])];

  start state Watching {
    on eGeneration do (p: (gen: int, owner: map[int, int])) {
      assert p.gen > gen, format("generation {0} started after generation {1}", p.gen, gen);
      gen = p.gen;
      owner = p.owner;
      assigned[gen] = p.owner;
    }
    on eAdopted do (p: (member: int, gen: int, partitions: seq[int])) {
      var part: int;
      var other: int;
      assert p.gen in assigned, format("member {0} adopted generation {1}, which never started", p.member, p.gen);
      foreach (part in p.partitions) {
        assert part in assigned[p.gen] && assigned[p.gen][part] == p.member,
          format("member {0} took partition {1} in generation {2}, which the coordinator did not give it", p.member, part, p.gen);
        foreach (other in keys(held)) {
          assert other == p.member || held[other].gen != p.gen || !(part in held[other].partitions),
            format("members {0} and {1} both hold partition {2} in generation {3}", other, p.member, part, p.gen);
        }
      }
      held[p.member] = (gen = p.gen, partitions = p.partitions);
    }
    on eProcessed do (p: (member: int, partition: int, offset: int)) {
      if (!(p.partition in processed)) {
        processed[p.partition] = default(set[int]);
      }
      processed[p.partition] += (p.offset);
      // A zombie may process records it no longer owns (at-least-once), but a
      // consumer that never got an assignment must not process anything.
      assert p.member in held, format("member {0} processed before it had any assignment", p.member);
    }
    on eCommitted do (p: (partition: int, offset: int, gen: int, member: int)) {
      var o: int;
      assert p.gen == gen, format("commit from generation {0} accepted in generation {1}", p.gen, gen);
      assert p.partition in owner && owner[p.partition] == p.member,
        format("member {0} committed partition {1}, which it does not own", p.member, p.partition);
      assert !(p.partition in committed) || p.offset >= committed[p.partition],
        format("partition {0} committed offset went back from {1} to {2}", p.partition, committed[p.partition], p.offset);
      while (o < p.offset) {
        assert p.partition in processed && o in processed[p.partition],
          format("partition {0} committed offset {1} before record {2} was processed", p.partition, p.offset, o);
        o = o + 1;
      }
      committed[p.partition] = p.offset;
    }
    on eCommitRejected do (p: (partition: int, offset: int, gen: int, member: int)) {
      assert p.gen != gen || !(p.partition in owner) || owner[p.partition] != p.member,
        format("commit of offset {0} by member {1}, the current owner of partition {2}, was rejected",
               p.offset, p.member, p.partition);
    }
  }
}

// Progress: every record of every partition is eventually processed and committed,
// given the bounded membership changes the driver makes. The driver announces the
// partitions and how many records each holds before any consumer starts.
event eGroupPlanned: (partitions: int, records: int);

spec AllCommitted observes eGroupPlanned, eCommitted {
  var records: int;
  // partition -> records not yet committed
  var left: map[int, int];

  start state Idle {
    on eGroupPlanned do (p: (partitions: int, records: int)) {
      var part: int;
      assert p.partitions > 0 && p.records > 0, "planned no work";
      records = p.records;
      while (part < p.partitions) {
        left[part] = p.records;
        part = part + 1;
      }
      goto Busy;
    }
    on eCommitted do { assert false, "commit with no work planned"; }
  }

  hot state Busy {
    on eGroupPlanned do { assert false, "work planned twice"; }
    // snappea-lint: allow unused-payload-field -- GroupSafety checks generations and members
    on eCommitted do (p: (partition: int, offset: int, gen: int, member: int)) {
      var part: int;
      assert p.partition in left, format("commit to unknown partition {0}", p.partition);
      assert p.offset <= records, format("partition {0} committed offset {1} past its end", p.partition, p.offset);
      left[p.partition] = records - p.offset;
      foreach (part in keys(left)) {
        if (left[part] > 0) { return; }
      }
      goto Done;
    }
  }

  state Done {
    on eCommitted do { assert false, "commit after every partition was fully committed"; }
    on eGroupPlanned do { assert false, "work planned twice"; }
  }
}
