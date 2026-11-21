// The group coordinator. It owns the membership, the generation number, the
// partition assignment and the committed offsets. Each handler is one atomic step on
// the coordinator's state. See DESIGN.md.
//
// It announces GenerationChanged and OffsetCommitted (PSpec/Contract.p) at its commit
// points. Partitions are numbered from 0; a partition with no commit is at offset 0.

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
  start state Serving {
    // n: the number of partitions
    entry (n: int) {
      // TODO
    }
  }
}
