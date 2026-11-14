// Drivers. Readers and the writer run the application's algorithm, so they live in
// PSrc; only these setup machines are drivers.

type WorkloadConfig = (cache: machine, reads: seq[int], writes: seq[int]);

// Runs the readers (reads[i] reads for reader i + 1) and one writer concurrently. When
// all have finished it announces quiescence and runs a probe reader that reads twice:
// the first read may fill the cache, the second must hit with the database's value.
machine Workload {
  var w: WorkloadConfig;
  var db: Database;
  var running: int;

  start state Concurrent {
    entry (input: WorkloadConfig) {
      var i: int;
      var ops: int;
      w = input;
      db = new Database();
      ops = sizeof(w.writes);
      while (i < sizeof(w.reads)) {
        ops = ops + w.reads[i];
        i = i + 1;
      }
      announce WorkPlanned, ops;
      i = 0;
      while (i < sizeof(w.reads)) {
        new Reader((owner = this, cache = w.cache, db = db, id = i + 1, reads = w.reads[i]));
        i = i + 1;
      }
      new Writer((owner = this, cache = w.cache, db = db, writes = w.writes));
      running = sizeof(w.reads) + 1;
    }
    on ClientDone do {
      running = running - 1;
      if (running == 0) { goto Probe; }
    }
  }

  state Probe {
    entry {
      announce Quiesced;
      announce WorkPlanned, 2;
      new Reader((owner = this, cache = w.cache, db = db, id = 99, reads = 2));
    }
    on ClientDone do { announce ProbeDone; }
  }
}

fun Seq2(a: int, b: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  s += (1, b);
  return s;
}

fun Seq1(a: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  return s;
}

// The core race, explored exhaustively: one reader misses and reads the database
// while the writer commits and deletes. In some orders the reader's fill arrives after
// the delete, carrying the value it read before the write; the lease must refuse it.
machine StaleFillRace {
  start state Init {
    entry {
      new Workload((cache = new LeaseCache(), reads = Seq1(1), writes = Seq1(1)));
    }
  }
}

// Two readers miss and read the database while the writer commits and
// deletes. The second miss replaces the first reader's lease, so either fill can be
// refused. Sampled: every ordering is about 90k schedules.
machine TwoReadersOneWriter {
  start state Init {
    entry {
      new Workload((cache = new LeaseCache(), reads = Seq2(1, 1), writes = Seq1(1)));
    }
  }
}

// Generated: each reader reads once or twice and the writer writes once or twice, so
// hits on a filled cache, a second invalidation, and leases revoked by either write
// all appear. Sampled within the time budget.
machine GeneratedWorkload {
  start state Init {
    entry {
      var writes: seq[int];
      writes = Seq1(1);
      if ($) { writes += (1, 2); }
      new Workload((cache = new LeaseCache(), reads = Seq2(choose(2) + 1, choose(2) + 1), writes = writes));
    }
  }
}

// Lifecycle boundary, in order: a reader fills the cache, then a write completes, then
// the reader reads again. The second read must miss and see the new value, not hit the
// entry filled before the write. Concurrent tests reach a filled-then-deleted entry
// only in some schedules; here it happens every time.
machine ReadWriteRead {
  var cache: LeaseCache;
  var db: Database;
  start state FirstRead {
    entry {
      cache = new LeaseCache();
      db = new Database();
      announce WorkPlanned, 4;
      new Reader((owner = this, cache = cache, db = db, id = 1, reads = 2));
    }
    on ClientDone goto Write;
  }
  state Write {
    entry { new Writer((owner = this, cache = cache, db = db, writes = Seq1(1))); }
    on ClientDone goto SecondRead;
  }
  state SecondRead {
    entry { new Reader((owner = this, cache = cache, db = db, id = 1, reads = 1)); }
    ignore ClientDone;
  }
}

machine BuggyStaleFillRace {
  start state Init {
    entry {
      new Workload((cache = new NaiveCache(), reads = Seq1(1), writes = Seq1(1)));
    }
  }
}

test tcStaleFillRace [main=StaleFillRace]:
  assert CacheFresh, OperationsFinish in
  { StaleFillRace, Workload, Database, LeaseCache, Reader, Writer };

test tcTwoReadersOneWriter [main=TwoReadersOneWriter]:
  assert CacheFresh, OperationsFinish in
  { TwoReadersOneWriter, Workload, Database, LeaseCache, Reader, Writer };

test tcGeneratedWorkload [main=GeneratedWorkload]:
  assert CacheFresh, OperationsFinish in
  { GeneratedWorkload, Workload, Database, LeaseCache, Reader, Writer };

test tcReadWriteRead [main=ReadWriteRead]:
  assert CacheFresh, OperationsFinish in
  { ReadWriteRead, Database, LeaseCache, Reader, Writer };

// snappea: expect-bug "stale value"
test tcBugNaiveCache [main=BuggyStaleFillRace]:
  assert CacheFresh, OperationsFinish in
  { BuggyStaleFillRace, Workload, Database, NaiveCache, Reader, Writer };
