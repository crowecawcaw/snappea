// Drivers play the client: they announce each request, then hand it to a fresh API
// worker, as a load balancer would. Two requests with the same key model a double
// click, or a client that retried because the first response was slow.

fun SendFlawed(client: machine, table: KeyTable, gateway: Gateway, r: Request) {
  announce Requested, r;
  new LookupThenRecordWorker((client = client, table = table, gateway = gateway, request = r));
}

fun SendFixed(client: machine, table: ClaimTable, gateway: Gateway, r: Request) {
  announce Requested, r;
  new ClaimingWorker((client = client, table = table, gateway = gateway, request = r));
}

machine DoubleClickFlawed {
  start state Init {
    entry {
      var table: KeyTable;
      var gateway: Gateway;
      table = new KeyTable();
      gateway = new Gateway();
      announce RequestsPlanned, 2;
      SendFlawed(this, table, gateway, (req = 1, key = 7, amount = 10));
      SendFlawed(this, table, gateway, (req = 2, key = 7, amount = 10));
    }
    // The specs check each answer as it is sent; the client needs nothing more.
    ignore ChargeResult;
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
      announce RequestsPlanned, 2;
      SendFixed(this, table, gateway, (req = 1, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 2, key = 7, amount = 10));
    }
    // The specs check each answer as it is sent; the client needs nothing more.
    ignore ChargeResult;
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
      announce RequestsPlanned, 3;
      SendFixed(this, table, gateway, (req = 1, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 2, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 3, key = 8, amount = 20));
    }
    // The specs check each answer as it is sent; the client needs nothing more.
    ignore ChargeResult;
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
      announce RequestsPlanned, 3;
      SendFixed(this, table, gateway, (req = 1, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 2, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 3, key = 7, amount = 10));
    }
    // The specs check each answer as it is sent; the client needs nothing more.
    ignore ChargeResult;
  }
}

// Lifecycle boundary: the retry arrives after the first request has returned, so it
// must be answered from the stored result, while a new key is charged normally.
// Sequential retries are the easy case: the flawed design records the key before
// the retry arrives, too.
machine RetryAfterDone {
  var table: ClaimTable;
  var gateway: Gateway;
  start state First {
    entry {
      table = new ClaimTable();
      gateway = new Gateway();
      announce RequestsPlanned, 3;
      SendFixed(this, table, gateway, (req = 1, key = 7, amount = 10));
    }
    on ChargeResult goto Retry;
  }
  state Retry {
    entry {
      SendFixed(this, table, gateway, (req = 2, key = 7, amount = 10));
      SendFixed(this, table, gateway, (req = 3, key = 8, amount = 20));
    }
    // The specs check each answer as it is sent; the client needs nothing more.
    ignore ChargeResult;
  }
}

// snappea: expect-bug "double charge: key 7 charged twice (charges 1 and 2)"
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
