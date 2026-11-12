// THE FLAW. A payment API with idempotency keys, written the obvious way:
//   1. look the key up; if it has a result, return it;
//   2. charge the card;
//   3. record key -> result.
// Each step is atomic, but nothing marks the key between steps 1 and 3. A double
// click, or a client retry while the first request is still at the gateway, sends a
// second request that also finds nothing at step 1 and charges again.
//
// Real primitives: lookup (SELECT by key), record (INSERT key, result).

event eLookup: (worker: machine, key: int);
event eLookupResp: (found: bool, charge: int);
event eRecord: (key: int, charge: int);

machine KeyTable {
  var results: map[int, int];
  start state Serving {
    on eLookup do (l: (worker: machine, key: int)) {
      if (l.key in results) {
        send l.worker, eLookupResp, (found = true, charge = results[l.key]);
      } else {
        send l.worker, eLookupResp, (found = false, charge = 0);
      }
    }
    on eRecord do (r: (key: int, charge: int)) {
      results[r.key] = r.charge;
    }
  }
}

// One API worker per incoming request.
machine LookupThenRecordWorker {
  var client: machine;
  var table: KeyTable;
  var gateway: Gateway;
  var request: tRequest;

  start state Looking {
    entry (w: (client: machine, table: KeyTable, gateway: Gateway, request: tRequest)) {
      client = w.client;
      table = w.table;
      gateway = w.gateway;
      request = w.request;
      send table, eLookup, (worker = this, key = request.key);
    }
    on eLookupResp do (r: (found: bool, charge: int)) {
      if (r.found) {
        Reply(r.charge);
        return;
      }
      // BUG: the key is still unmarked while the charge is in flight.
      send gateway, eCharge, (client = this, key = request.key, amount = request.amount);
    }
    on eChargeResp do (r: (charge: int)) {
      send table, eRecord, (key = request.key, charge = r.charge);
      Reply(r.charge);
    }
  }

  fun Reply(charge: int) {
    announce eReplied, (req = request.req, charge = charge);
    send client, eChargeResult, (req = request.req, charge = charge);
  }
}
