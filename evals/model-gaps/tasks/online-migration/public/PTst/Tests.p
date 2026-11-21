// The environment: a driver that seeds the old table, runs the migration and then
// starts the application's writers and readers. It announces what it asked for.
// The old store and the reader are given; the new store, the router, the writer and
// the migrator are the design.

type Reads = (key: int, reads: int);
type Plan = (seed: map[int, int], writers: seq[seq[Op]], readers: seq[Reads]);

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

fun Put(req: int, key: int, value: int): seq[Op] {
  var ops: seq[Op];
  ops += (0, (req = req, key = key, value = value, deleted = false));
  return ops;
}

// Migrates the table, and starts the plan's writers and readers once the migration
// is done.
machine MigrateThenUse {
  var plan: Plan;
  var router: machine;
  var oldStore: OldStore;
  var newStore: machine;

  start state Migrating {
    entry (p: Plan) {
      var scan: seq[int];
      var key: int;
      plan = p;
      AnnouncePlan(plan);
      oldStore = new OldStore(plan.seed);
      newStore = new NewStore();
      router = new Router((oldStore = oldStore, newStore = newStore));
      foreach (key in keys(plan.seed)) {
        scan += (sizeof(scan), key);
      }
      new Migrator((owner = this, router = router, oldStore = oldStore, newStore = newStore, scan = scan));
    }
    on MigrationDone do {
      StartClients(router, oldStore, newStore, plan);
    }
  }
}

// One key. After the migration a writer puts 20 and a reader reads the key twice.
machine OneKey {
  start state Run {
    entry {
      var plan: Plan;
      plan.seed[1] = 10;
      plan.writers += (0, Put(1, 1, 20));
      plan.readers += (0, (key = 1, reads = 2));
      new MigrateThenUse(plan);
    }
  }
}

// Two keys. After the migration a writer puts key 2 and a reader reads key 1.
machine TwoKeys {
  start state Run {
    entry {
      var plan: Plan;
      plan.seed[1] = 10;
      plan.seed[2] = 20;
      plan.writers += (0, Put(1, 2, 30));
      plan.readers += (0, (key = 1, reads = 2));
      new MigrateThenUse(plan);
    }
  }
}

test tcOneKey [main=OneKey]:
  assert MigrationSafety, MigrationCompletes in
  { OneKey, MigrateThenUse, OldStore, NewStore, Router, Writer, Reader, Migrator };

test tcTwoKeys [main=TwoKeys]:
  assert MigrationSafety, MigrationCompletes in
  { TwoKeys, MigrateThenUse, OldStore, NewStore, Router, Writer, Reader, Migrator };
