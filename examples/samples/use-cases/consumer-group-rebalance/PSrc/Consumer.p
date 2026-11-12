// A group member. For each assigned partition it fetches the committed offset, then
// processes records one at a time and commits the next offset after each, tagged with
// its generation. Processing happens before the commit, so a crash or a rebalance in
// between means the record is processed again by the next owner (at-least-once).
//
// A new assignment replaces the current work at once (eager rebalance). Replies tagged
// with an older generation are leftovers and are dropped. Within one generation the
// consumer waits for each reply before its next request, so a reply with the current
// generation is always for the current partition.
// A rejected commit or offset fetch means the consumer was fenced: it rejoins the group.

// From the application (the test driver): shut down cleanly.
event eShutdown;

machine Consumer {
  var coordinator: machine;
  var id: int;
  // records in each partition's log; offsets run 0 .. records - 1
  var records: int;
  var gen: int;
  var todo: seq[int];
  var partition: int;
  var offset: int;

  start state Joining {
    entry (input: (coordinator: machine, id: int, records: int)) {
      coordinator = input.coordinator;
      id = input.id;
      records = input.records;
      send coordinator, eJoin, (consumer = this, id = id);
      goto Idle;
    }
  }

  // No partition to work on: waiting for an assignment.
  state Idle {
    on eAssignment do (a: (gen: int, partitions: seq[int])) { Adopt(a); }
    ignore eFetched, eCommitResp;
    on eShutdown goto Stopped with { Leave(); }
  }

  state Working {
    entry {
      if (sizeof(todo) == 0) { goto Idle; }
      partition = todo[0];
      todo -= (0);
      send coordinator, eFetch, (consumer = this, id = id, gen = gen, partition = partition);
    }
    on eAssignment do (a: (gen: int, partitions: seq[int])) { Adopt(a); }
    on eFetched do (r: (gen: int, partition: int, ok: bool, offset: int)) {
      if (r.gen != gen) { return; }
      if (!r.ok) {
        Rejoin();
      } else {
        offset = r.offset;
        Process();
      }
    }
    on eCommitResp do (r: (gen: int, partition: int, ok: bool)) {
      if (r.gen != gen) { return; }
      if (!r.ok) {
        Rejoin();
      } else {
        // snappea-lint: allow mutation:remove-statement -- observation only: dropping it removes a check, not behavior
        announce eCommitAcked, (member = id, partition = partition, offset = offset + 1);
        offset = offset + 1;
        Process();
      }
    }
    on eShutdown goto Stopped with { Leave(); }
  }

  state Stopped {
    ignore eAssignment, eFetched, eCommitResp, eShutdown;
  }

  fun Adopt(a: (gen: int, partitions: seq[int])) {
    gen = a.gen;
    todo = a.partitions;
    announce eAdopted, (member = id, gen = gen, partitions = todo);
    goto Working;
  }

  fun Process() {
    if (offset >= records) {
      // End of this partition's log: on to the next one.
      goto Working;
    }
    announce eProcessed, (member = id, partition = partition, offset = offset);
    send coordinator, eCommit, (consumer = this, id = id, gen = gen, partition = partition, offset = offset + 1);
  }

  // Fenced: this consumer was expelled (a member that only missed a rebalance gets the
  // new assignment first and drops the stale reply above).
  fun Rejoin() {
    send coordinator, eJoin, (consumer = this, id = id);
    // snappea-lint: allow mutation:remove-statement -- equivalent: Working adopts the next assignment too, and no reply for the current generation is still on its way
    goto Idle;
  }

  fun Leave() {
    send coordinator, eLeave, (id = id,);
  }
}
