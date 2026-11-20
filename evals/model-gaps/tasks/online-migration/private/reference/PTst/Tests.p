// Drivers: they seed the old table, start writers, readers and the migrator, and
// announce what they asked for. Writers, readers, the router and the migrator are the
// design under test.

type Reads = (key: int, reads: int);
type Plan = (seed: map[int, int], writers: seq[seq[Op]], readers: seq[Reads]);

// Starts everything at once, so writes and reads race with the backfill and cutover.
fun Migrate(owner: machine, newStore: machine, plan: Plan) {
  var oldStore: OldStore;
  AnnouncePlan(plan);
  oldStore = new OldStore(plan.seed);
  MigrateVia(owner, oldStore, new Router((oldStore = oldStore, newStore = newStore)), newStore, plan);
}

fun MigrateVia(owner: machine, oldStore: OldStore, router: machine, newStore: machine, plan: Plan) {
  var scan: seq[int];
  var key: int;
  foreach (key in keys(plan.seed)) {
    scan += (sizeof(scan), key);
  }
  new Migrator((owner = owner, router = router, oldStore = oldStore, newStore = newStore, scan = scan));
  StartClients(router, oldStore, newStore, plan);
}

fun AnnouncePlan(plan: Plan) {
  var ops: seq[Op];
  var op: Op;
  var writes: int;
  var i: int;
  foreach (ops in plan.writers) { writes = writes + sizeof(ops); }
  announce MigrationPlanned, (writes = writes, seed = plan.seed);
  foreach (ops in plan.writers) {
    foreach (op in ops) { announce WriteRequested, op; }
  }
  while (i < sizeof(plan.readers)) {
    announce ReadsPlanned, (reader = i + 1, key = plan.readers[i].key, reads = plan.readers[i].reads);
    i = i + 1;
  }
}

fun StartClients(router: machine, oldStore: OldStore, newStore: machine, plan: Plan) {
  var ops: seq[Op];
  var i: int;
  foreach (ops in plan.writers) {
    new Writer((router = router, oldStore = oldStore, newStore = newStore, ops = ops));
  }
  while (i < sizeof(plan.readers)) {
    new Reader((router = router, id = i + 1, key = plan.readers[i].key, reads = plan.readers[i].reads));
    i = i + 1;
  }
}

fun Seed1(): map[int, int] {
  var seed: map[int, int];
  seed[1] = 10;
  return seed;
}

fun Put(req: int, key: int, value: int): seq[Op] {
  var ops: seq[Op];
  ops += (0, (req = req, key = key, value = value, deleted = false));
  return ops;
}

fun Delete(req: int, key: int): seq[Op] {
  var ops: seq[Op];
  ops += (0, (req = req, key = key, value = 0, deleted = true));
  return ops;
}

fun OneWriter(ops: seq[Op]): Plan {
  var plan: Plan;
  plan.seed = Seed1();
  plan.writers += (0, ops);
  return plan;
}

// The classic: the backfill reads key 1 (10), the writer dual-writes 20, then the copy
// of 10 arrives at new. Every interleaving is explored.
machine BackfillRacesWriter {
  start state Run {
    entry { Migrate(this, new NewStore(), OneWriter(Put(1, 1, 20))); }
    ignore MigrationDone;
  }
}

// A delete during the backfill: the copy of the deleted row must not bring it back.
machine DeleteDuringBackfill {
  start state Run {
    entry { Migrate(this, new NewStore(), OneWriter(Delete(1, 1))); }
    ignore MigrationDone;
  }
}

// Two writers on one key: their dual-writes can reach new in the opposite order from
// the one old committed them in, while the backfill copies the key too.
machine TwoWritersOneKey {
  start state Run {
    entry {
      var plan: Plan;
      plan = OneWriter(Put(1, 1, 20));
      plan.writers += (1, Put(2, 1, 30));
      Migrate(this, new NewStore(), plan);
    }
    ignore MigrationDone;
  }
}

// A reader reads key 1 twice while a writer updates it and reads are cut over:
// no read may go back to an older version.
machine ReadsAcrossCutover {
  start state Run {
    entry {
      var plan: Plan;
      plan = OneWriter(Put(1, 1, 20));
      plan.readers += (0, (key = 1, reads = 2));
      Migrate(this, new NewStore(), plan);
    }
    ignore MigrationDone;
  }
}

// Lifecycle boundary: a write and a read that start only after cutover. Writes still
// go old then new, and reads now come from new.
machine WriteAfterCutover {
  var plan: Plan;
  var router: Router;
  var oldStore: OldStore;
  var newStore: NewStore;

  start state Migrating {
    entry {
      var scan: seq[int];
      plan = OneWriter(Put(1, 1, 20));
      plan.readers += (0, (key = 1, reads = 2));
      AnnouncePlan(plan);
      oldStore = new OldStore(plan.seed);
      newStore = new NewStore();
      router = new Router((oldStore = oldStore, newStore = newStore));
      scan += (0, 1);
      new Migrator((owner = this, router = router, oldStore = oldStore, newStore = newStore, scan = scan));
    }
    on MigrationDone do {
      StartClients(router, oldStore, newStore, plan);
    }
  }
}

// Two keys, two writers each doing one put or delete on either key, and a reader:
// keys must not leak into each other, and contended and uncontended keys mix.
machine GeneratedMigration {
  start state Run {
    entry {
      var plan: Plan;
      var i: int;
      plan.seed = Seed1();
      plan.seed[2] = 20;
      while (i < 2) {
        if ($) {
          plan.writers += (i, Put(i + 1, choose(2) + 1, 30 + i));
        } else {
          plan.writers += (i, Delete(i + 1, choose(2) + 1));
        }
        i = i + 1;
      }
      // Key 3 was never written: reading it must find nothing, before and after cutover.
      plan.readers += (0, (key = choose(3) + 1, reads = 2));
      Migrate(this, new NewStore(), plan);
    }
    ignore MigrationDone;
  }
}

// No writer: a reader reads key 1 three times while the backfill runs and reads are cut
// over. A read routed to old must not be served after the flip.
machine ReaderDuringCutover {
  start state Run {
    entry {
      var plan: Plan;
      plan.seed = Seed1();
      plan.readers += (0, (key = 1, reads = 3));
      Migrate(this, new NewStore(), plan);
    }
    ignore MigrationDone;
  }
}

// One writer with two writes in a row, a put and then a delete of key 1, and a reader:
// the second write can ask for its slot while the cutover waits for the first.
machine PutThenDelete {
  start state Run {
    entry {
      var plan: Plan;
      var ops: seq[Op];
      ops = Put(1, 1, 20);
      ops += (1, (req = 2, key = 1, value = 0, deleted = true));
      plan = OneWriter(ops);
      plan.readers += (0, (key = 1, reads = 2));
      Migrate(this, new NewStore(), plan);
    }
    ignore MigrationDone;
  }
}

// A delete and two reads that start only after cutover: the deleted key reads as
// deleted, at the delete's version.
machine DeleteAfterCutover {
  var plan: Plan;
  var router: Router;
  var oldStore: OldStore;
  var newStore: NewStore;

  start state Migrating {
    entry {
      var scan: seq[int];
      plan = OneWriter(Delete(1, 1));
      plan.readers += (0, (key = 1, reads = 2));
      AnnouncePlan(plan);
      oldStore = new OldStore(plan.seed);
      newStore = new NewStore();
      router = new Router((oldStore = oldStore, newStore = newStore));
      scan += (0, 1);
      new Migrator((owner = this, router = router, oldStore = oldStore, newStore = newStore, scan = scan));
    }
    on MigrationDone do {
      StartClients(router, oldStore, newStore, plan);
    }
  }
}

test tcBackfillRacesWriter [main=BackfillRacesWriter]:
  assert MigrationSafety, MigrationCompletes in
  { BackfillRacesWriter, OldStore, NewStore, Router, Writer, Reader, Migrator };

test tcDeleteDuringBackfill [main=DeleteDuringBackfill]:
  assert MigrationSafety, MigrationCompletes in
  { DeleteDuringBackfill, OldStore, NewStore, Router, Writer, Reader, Migrator };

test tcTwoWritersOneKey [main=TwoWritersOneKey]:
  assert MigrationSafety, MigrationCompletes in
  { TwoWritersOneKey, OldStore, NewStore, Router, Writer, Reader, Migrator };

test tcReadsAcrossCutover [main=ReadsAcrossCutover]:
  assert MigrationSafety, MigrationCompletes in
  { ReadsAcrossCutover, OldStore, NewStore, Router, Writer, Reader, Migrator };

test tcWriteAfterCutover [main=WriteAfterCutover]:
  assert MigrationSafety, MigrationCompletes in
  { WriteAfterCutover, OldStore, NewStore, Router, Writer, Reader, Migrator };

test tcGeneratedMigration [main=GeneratedMigration]:
  assert MigrationSafety, MigrationCompletes in
  { GeneratedMigration, OldStore, NewStore, Router, Writer, Reader, Migrator };

test tcReaderDuringCutover [main=ReaderDuringCutover]:
  assert MigrationSafety, MigrationCompletes in
  { ReaderDuringCutover, OldStore, NewStore, Router, Writer, Reader, Migrator };

test tcPutThenDelete [main=PutThenDelete]:
  assert MigrationSafety, MigrationCompletes in
  { PutThenDelete, OldStore, NewStore, Router, Writer, Reader, Migrator };

test tcDeleteAfterCutover [main=DeleteAfterCutover]:
  assert MigrationSafety, MigrationCompletes in
  { DeleteAfterCutover, OldStore, NewStore, Router, Writer, Reader, Migrator };
