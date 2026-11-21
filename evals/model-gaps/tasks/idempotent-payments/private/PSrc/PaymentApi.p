// One API server process. Several servers run behind a load balancer, so a double
// click or a retry can reach another server than the first attempt. A process handles
// one request at a time: claim the key, charge if it got the claim, record the result,
// answer. A process that starts after a crash first settles the keys the crashed
// process left in progress.

machine PaymentApi {
  var store: machine;
  var processor: machine;
  var id: int;
  // the request being handled
  var cur: (client: machine, key: int, amount: int);
  // recovery: lookups not yet answered
  var pending: int;

  start state Booting {
    entry (input: ApiInit) {
      store = input.store;
      processor = input.processor;
      id = input.id;
      if (input.restarted) {
        send store, ListOwned, (api = this, owner = id);
        goto Recovering;
      }
      goto Idle;
    }
  }

  state Idle {
    on Charge do (r: (client: machine, key: int, amount: int)) {
      cur = r;
      send store, Claim, (api = this, owner = id, key = r.key, amount = r.amount);
      goto Claiming;
    }
    // Late replies about a request that is already answered.
    ignore ProcessResponse, LookupResponse;
  }

  state Claiming {
    defer Charge;
    ignore ProcessResponse, LookupResponse;
    on ClaimResponse do (c: (key: int, status: ClaimStatus, result: ChargeResult)) {
      if (c.status == Mismatch) {
        send cur.client, ChargeRejected, (key = cur.key,);
        goto Idle;
      }
      if (c.status == InProgress) {
        send cur.client, ChargeConflict, (key = cur.key,);
        goto Idle;
      }
      if (c.status == Finished) {
        send cur.client, ChargeResponse, (key = cur.key, result = c.result);
        goto Idle;
      }
      // Only the request that claimed the key ever calls the processor.
      send processor, Process, (api = this, key = cur.key, amount = cur.amount);
      goto Charging;
    }
  }

  state Charging {
    defer Charge;
    on ProcessResponse do (p: (key: int, result: ChargeResult)) {
      // A late reply for an earlier key says nothing about this one.
      if (p.key != cur.key) { return; }
      Finish(p.result);
    }
    on ProcessTimeout do (t: (key: int)) {
      // The outcome is unknown: ask, never charge again.
      send processor, Lookup, (api = this, key = cur.key);
    }
    on LookupResponse do (l: (key: int, found: bool, result: ChargeResult)) {
      if (l.key != cur.key) { return; }
      if (!l.found) {
        // Never charged: give the key up and let the client try again.
        send store, Release, (key = cur.key,);
        send cur.client, ChargeConflict, (key = cur.key,);
        goto Idle;
      }
      Finish(l.result);
    }
  }

  // After a crash: every key the crashed process left in progress is either charged
  // (record the result) or not (release it, so a retry can claim it again).
  state Recovering {
    defer Charge;
    on OwnedKeys do (owned: seq[int]) {
      var key: int;
      foreach (key in owned) {
        // The crashed process may have charged it: ask before deciding.
        send processor, Lookup, (api = this, key = key);
      }
      pending = sizeof(owned);
      if (pending == 0) { goto Idle; }
    }
    on LookupResponse do (l: (key: int, found: bool, result: ChargeResult)) {
      if (l.found) {
        send store, Complete, (key = l.key, result = l.result);
      } else {
        send store, Release, (key = l.key,);
      }
      pending = pending - 1;
      if (pending == 0) { goto Idle; }
    }
  }

  fun Finish(result: ChargeResult) {
    // Declined is a result like any other: it is recorded and stays.
    send store, Complete, (key = cur.key, result = result);
    send cur.client, ChargeResponse, (key = cur.key, result = result);
    goto Idle;
  }
}
