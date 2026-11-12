// Drivers play the client: they announce each request, then hand it to a fresh API
// worker, as a load balancer would. Two requests with the same key model a double
// click, or a client that retried because the first response was slow.

fun SendFlawed(client: machine, table: KeyTable, gateway: Gateway, r: tRequest) {
  announce eRequested, r;
  new LookupThenRecordWorker((client = client, table = table, gateway = gateway, request = r));
}

fun SendFixed(client: machine, table: ClaimTable, gateway: Gateway, r: tRequest) {
  announce eRequested, r;
  new ClaimingWorker((client = client, table = table, gateway = gateway, request = r));
}

machine DoubleClickFlawed {
  start state Init {
    entry {
      var table: KeyTable;
      var gateway: Gateway;
      table = new KeyTable();
      gateway = new Gateway();
      announce eRequestsPlanned, 2;
      SendFlawed(this, table, gateway, (req = 1, key = 7, amount = 10));
      SendFlawed(this, table, gateway, (req = 2, key = 7, amount = 10));
    }
    ignore eChargeResult;
  }
}

// The race: the second request arrives while the first is anywhere in its flow.
machine DoubleClick {
  start state Init {
    entry {
      var table: ClaimTable;
      var gateway: Gateway;
      table = new ClaimTable();
      gateway = new Gateway();
      announce eRequestsPlanned, 2;
      SendFixed(this, table, gateway, (req = 1, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 2, key = 7, amount = 10));
    }
    ignore eChargeResult;
  }
}

// A duplicate and an unrelated key at once: the unrelated request must neither wait
// for nor share a result with key 7.
machine MixedKeys {
  start state Init {
    entry {
      var table: ClaimTable;
      var gateway: Gateway;
      table = new ClaimTable();
      gateway = new Gateway();
      announce eRequestsPlanned, 3;
      SendFixed(this, table, gateway, (req = 1, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 2, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 3, key = 8, amount = 20));
    }
    ignore eChargeResult;
  }
}

// Three clicks: two duplicates can be waiting on the same in-progress key, and both
// must be released with the first request's charge.
machine TripleClick {
  start state Init {
    entry {
      var table: ClaimTable;
      var gateway: Gateway;
      table = new ClaimTable();
      gateway = new Gateway();
      announce eRequestsPlanned, 3;
      SendFixed(this, table, gateway, (req = 1, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 2, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 3, key = 7, amount = 10));
    }
    ignore eChargeResult;
  }
}

// Lifecycle boundary: the retry arrives after the first request has returned, so it
// must be answered from the stored result, while a new key is charged normally.
// (The flawed design passes this one too: sequential retries are the easy case.)
machine RetryAfterDone {
  var table: ClaimTable;
  var gateway: Gateway;
  start state First {
    entry {
      table = new ClaimTable();
      gateway = new Gateway();
      announce eRequestsPlanned, 3;
      SendFixed(this, table, gateway, (req = 1, key = 7, amount = 10));
    }
    on eChargeResult goto Retry;
  }
  state Retry {
    entry {
      SendFixed(this, table, gateway, (req = 2, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 3, key = 8, amount = 20));
    }
    ignore eChargeResult;
  }
}

// expect-bug: double charge: key 7 charged twice
test tcBugDoubleClick [main=DoubleClickFlawed]:
  assert ChargedOnce, EveryRequestAnswered in
  { DoubleClickFlawed, KeyTable, Gateway, LookupThenRecordWorker };

test tcFixedDoubleClick [main=DoubleClick]:
  assert ChargedOnce, EveryRequestAnswered in
  { DoubleClick, ClaimTable, Gateway, ClaimingWorker };

test tcFixedMixedKeys [main=MixedKeys]:
  assert ChargedOnce, EveryRequestAnswered in
  { MixedKeys, ClaimTable, Gateway, ClaimingWorker };

test tcFixedTripleClick [main=TripleClick]:
  assert ChargedOnce, EveryRequestAnswered in
  { TripleClick, ClaimTable, Gateway, ClaimingWorker };

test tcFixedRetryAfterDone [main=RetryAfterDone]:
  assert ChargedOnce, EveryRequestAnswered in
  { RetryAfterDone, ClaimTable, Gateway, ClaimingWorker };
