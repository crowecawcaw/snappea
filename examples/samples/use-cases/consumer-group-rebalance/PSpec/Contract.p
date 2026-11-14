// The consumer-group contract, over events the coordinator announces when it starts a
// generation or decides a commit, and events consumers announce when they adopt an
// assignment or process a record. The specs rebuild ownership and offsets from that
// history; they never read the coordinator.

event GenerationChanged: (gen: int, members: seq[int], owner: map[int, int]);
event OffsetCommitted: (partition: int, offset: int, gen: int, member: int);
event CommitRejected: (partition: int, gen: int, member: int);
event FetchRejected: (partition: int, gen: int, member: int);
event FetchServed: (partition: int, gen: int, member: int);
// A consumer was told its commit was accepted.
event CommitAcked: (member: int, partition: int, offset: int);
event Adopted: (member: int, gen: int, partitions: seq[int]);
event Processed: (member: int, partition: int, offset: int);

// Safety.
// - Generations only move forward.
// - A commit or offset fetch is accepted only from the partition's owner in the
//   current generation, and one from that owner is never rejected.
// - Committed offsets never go back, and only cover records someone processed.
// - Each generation lists each member once (a member never rejoins) and assigns every
//   partition to exactly one member (none if the group is empty), and nothing else.
// - Within a generation, a partition is held by at most one consumer, and only by the
//   one the coordinator assigned it to.
// - A consumer whose commit or offset fetch was rejected stops working on that
//   generation's assignment (it rejoins instead of processing on as a zombie), and a
//   consumer is told a commit succeeded only if it was accepted.
spec GroupSafety observes GroupPlanned, GenerationChanged, OffsetCommitted, CommitRejected, FetchRejected, FetchServed,
    CommitAcked, Adopted, Processed {
  var accepted: set[(member: int, partition: int, offset: int)];
  var partitions: int;
  // member -> the generation in which one of its commits was rejected
  var fenced: map[int, int];
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
    // snappea-lint: allow unused-payload-field -- GroupProgress checks the record count
    on GroupPlanned do (p: (partitions: int, records: int)) {
      partitions = p.partitions;
    }
    on GenerationChanged do (p: (gen: int, members: seq[int], owner: map[int, int])) {
      var part: int;
      var i: int;
      assert p.gen > gen, format("generation {0} started after generation {1}", p.gen, gen);
      while (i < sizeof(p.members)) {
        assert !(p.members[i] in Prefix(p.members, i)),
          format("generation {0} lists member {1} twice: it rejoined while still a member", p.gen, p.members[i]);
        i = i + 1;
      }
      foreach (part in keys(p.owner)) {
        assert part < partitions, format("generation {0} assigns partition {1}, which does not exist", p.gen, part);
        assert p.owner[part] in p.members,
          format("generation {0} gives partition {1} to {2}, which is not a member", p.gen, part, p.owner[part]);
      }
      assert sizeof(p.members) == 0 || sizeof(p.owner) == partitions,
        format("generation {0} leaves a partition without an owner", p.gen);
      gen = p.gen;
      owner = p.owner;
      assigned[gen] = p.owner;
    }
    on Adopted do (p: (member: int, gen: int, partitions: seq[int])) {
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
    on Processed do (p: (member: int, partition: int, offset: int)) {
      if (!(p.partition in processed)) {
        processed[p.partition] = default(set[int]);
      }
      processed[p.partition] += (p.offset);
      // A zombie may process records it no longer owns (at-least-once), but a
      // consumer that never got an assignment must not process anything.
      assert p.member in held, format("member {0} processed before it had any assignment", p.member);
      assert !(p.member in fenced) || fenced[p.member] != held[p.member].gen,
        format("member {0} kept processing partition {1} after its commit was rejected", p.member, p.partition);
    }
    on OffsetCommitted do (p: (partition: int, offset: int, gen: int, member: int)) {
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
      accepted += ((member = p.member, partition = p.partition, offset = p.offset));
    }
    on CommitAcked do (p: (member: int, partition: int, offset: int)) {
      assert p in accepted,
        format("member {0} was told offset {1} of partition {2} was committed, but it was rejected", p.member, p.offset, p.partition);
    }
    on FetchServed do (p: (partition: int, gen: int, member: int)) {
      assert p.gen == gen && p.partition in owner && owner[p.partition] == p.member,
        format("offset fetch by member {0} for partition {1} in generation {2} served in generation {3}",
               p.member, p.partition, p.gen, gen);
    }
    on FetchRejected do (p: (partition: int, gen: int, member: int)) {
      fenced[p.member] = p.gen;
      assert p.gen != gen || !(p.partition in owner) || owner[p.partition] != p.member,
        format("offset fetch by member {0}, the current owner of partition {1}, was rejected", p.member, p.partition);
    }
    on CommitRejected do (p: (partition: int, gen: int, member: int)) {
      fenced[p.member] = p.gen;
      assert p.gen != gen || !(p.partition in owner) || owner[p.partition] != p.member,
        format("commit by member {0}, the current owner of partition {1}, was rejected",
               p.member, p.partition);
    }
  }
}

// The first n entries of a sequence.
fun Prefix(s: seq[int], n: int): seq[int] {
  var out: seq[int];
  while (sizeof(out) < n) {
    out += (sizeof(out), s[sizeof(out)]);
  }
  return out;
}

// Progress: every record of every partition is eventually processed and committed,
// given the bounded membership changes the driver makes, and a live consumer that was
// fenced out of the group (its commit rejected while it was not a member) eventually
// rejoins. The driver announces the partitions and records before any consumer
// starts. (A consumer that shuts down sends its last commit before its leave, so it is
// never fenced as a non-member.)
event GroupPlanned: (partitions: int, records: int);

spec GroupProgress observes GroupPlanned, OffsetCommitted, CommitRejected, FetchRejected, GenerationChanged {
  var records: int;
  // partition -> records not yet committed
  var left: map[int, int];
  var members: seq[int];
  // fenced live consumers that have not rejoined yet
  var outside: set[int];

  start state Idle {
    on GroupPlanned do (p: (partitions: int, records: int)) {
      var part: int;
      assert p.partitions > 0 && p.records > 0, "planned no work";
      records = p.records;
      while (part < p.partitions) {
        left[part] = p.records;
        part = part + 1;
      }
      goto Busy;
    }
    on OffsetCommitted do { assert false, "commit with no work planned"; }
    on CommitRejected do { assert false, "commit with no work planned"; }
    on FetchRejected do { assert false, "fetch with no work planned"; }
    on GenerationChanged do { assert false, "generation with no work planned"; }
  }

  hot state Busy {
    on GroupPlanned do { assert false, "work planned twice"; }

    // snappea-lint: allow unused-payload-field -- GroupSafety checks generations and members
    on OffsetCommitted do (p: (partition: int, offset: int, gen: int, member: int)) {
      assert p.partition in left, format("commit to unknown partition {0}", p.partition);
      assert p.offset <= records, format("partition {0} committed offset {1} past its end", p.partition, p.offset);
      left[p.partition] = records - p.offset;
      Check();
    }

    // snappea-lint: allow unused-payload-field -- GroupSafety checks why it was rejected
    on CommitRejected do (p: (partition: int, gen: int, member: int)) { Fenced(p.member); }

    // snappea-lint: allow unused-payload-field -- GroupSafety checks why it was rejected
    on FetchRejected do (p: (partition: int, gen: int, member: int)) { Fenced(p.member); }
    on GenerationChanged do (p: (gen: int, members: seq[int], owner: map[int, int])) { Generation(p); }
  }

  // Everything committed and nobody left outside. A zombie can still be fenced and
  // rejoin from here, which reopens the obligation.
  state Done {
    on GroupPlanned do { assert false, "work planned twice"; }
    on OffsetCommitted do { assert false, "commit after every partition was fully committed"; }

    // snappea-lint: allow unused-payload-field -- GroupSafety checks why it was rejected
    on CommitRejected do (p: (partition: int, gen: int, member: int)) { Fenced(p.member); }

    // snappea-lint: allow unused-payload-field -- GroupSafety checks why it was rejected
    on FetchRejected do (p: (partition: int, gen: int, member: int)) { Fenced(p.member); }
    on GenerationChanged do (p: (gen: int, members: seq[int], owner: map[int, int])) { Generation(p); }
  }

  fun Fenced(member: int) {
    if (!(member in members)) {
      outside += (member);
    }
    Check();
  }

  // snappea-lint: allow unused-payload-field -- GroupSafety checks the assignment
  fun Generation(p: (gen: int, members: seq[int], owner: map[int, int])) {
    var m: int;
    members = p.members;
    foreach (m in p.members) {
      outside -= (m);
    }
    Check();
  }

  fun Check() {
    var part: int;
    foreach (part in keys(left)) {
      if (left[part] > 0) {
        goto Busy;
      }
    }
    if (sizeof(outside) > 0) {
      goto Busy;
    }
    goto Done;
  }
}
