// Drivers open the accounts and start one worker per transfer, announcing each
// account, the plan and each transfer first, so the specs know what must happen.

fun OpenAccounts(n: int): Database {
  var balances: map[int, int];
  var id: int;
  id = 1;
  while (id <= n) {
    announce eAccountOpened, (account = id, balance = 10);
    balances[id] = 10;
    id = id + 1;
  }
  return new Database(balances);
}

fun StartFlawed(db: Database, t: tTransfer) {
  announce eTransferRequested, t;
  new ArgumentOrderWorker((db = db, transfer = t));
}

fun StartFixed(db: Database, t: tTransfer) {
  announce eTransferRequested, t;
  new OrderedWorker((db = db, transfer = t));
}

// Two transfers in opposite directions between the same two accounts.
machine OppositeTransfersFlawed {
  start state Init {
    entry {
      var db: Database;
      db = OpenAccounts(2);
      announce eTransfersPlanned, 2;
      StartFlawed(db, (transfer = 1, src = 1, dst = 2, amount = 3));
      StartFlawed(db, (transfer = 2, src = 2, dst = 1, amount = 4));
    }
  }
}

machine OppositeTransfers {
  start state Init {
    entry {
      var db: Database;
      db = OpenAccounts(2);
      announce eTransfersPlanned, 2;
      StartFixed(db, (transfer = 1, src = 1, dst = 2, amount = 3));
      StartFixed(db, (transfer = 2, src = 2, dst = 1, amount = 4));
    }
  }
}

// Three transfers around a cycle of three accounts (1 -> 2, 2 -> 3, 3 -> 1). No two
// transfers are opposite, yet argument order deadlocks here too: each worker holds
// one account and waits for the next. A fix that only special-cases reversed pairs
// fails this test.
machine CycleOfThree {
  start state Init {
    entry {
      var db: Database;
      db = OpenAccounts(3);
      announce eTransfersPlanned, 3;
      StartFixed(db, (transfer = 1, src = 1, dst = 2, amount = 3));
      StartFixed(db, (transfer = 2, src = 2, dst = 3, amount = 4));
      StartFixed(db, (transfer = 3, src = 3, dst = 1, amount = 5));
    }
  }
}

machine CycleOfThreeFlawed {
  start state Init {
    entry {
      var db: Database;
      db = OpenAccounts(3);
      announce eTransfersPlanned, 3;
      StartFlawed(db, (transfer = 1, src = 1, dst = 2, amount = 3));
      StartFlawed(db, (transfer = 2, src = 2, dst = 3, amount = 4));
      StartFlawed(db, (transfer = 3, src = 3, dst = 1, amount = 5));
    }
  }
}

// expect-bug: liveness bug in hot state 'TransferPending'
test tcBugOppositeTransfers [main=OppositeTransfersFlawed]:
  assert TransfersIsolated, TransfersComplete in
  { OppositeTransfersFlawed, Database, ArgumentOrderWorker };

// expect-bug: liveness bug in hot state 'TransferPending'
test tcBugCycleOfThree [main=CycleOfThreeFlawed]:
  assert TransfersIsolated, TransfersComplete in
  { CycleOfThreeFlawed, Database, ArgumentOrderWorker };

test tcFixedOppositeTransfers [main=OppositeTransfers]:
  assert TransfersIsolated, TransfersComplete in
  { OppositeTransfers, Database, OrderedWorker };

test tcFixedCycleOfThree [main=CycleOfThree]:
  assert TransfersIsolated, TransfersComplete in
  { CycleOfThree, Database, OrderedWorker };
