// THE FLAW. A payment API with idempotency keys, written the obvious way:
//   1. look the key up; if it has a result, return it;
//   2. charge the card;
//   3. record key -> result.
// Each step is atomic, but nothing marks the key between steps 1 and 3. A double
// click, or a client retry while the first request is still at the gateway, sends a
// second request that also finds nothing at step 1 and charges again.
//
// Real primitives: lookup (SELECT by key), record (INSERT key, result).

event Lookup: (worker: machine, key: int);
event LookupResp: (found: bool, charge: int);
event Record: (key: int, charge: int);

machine KeyTable {
  var results: map[int, int];
  start state Serving {
    on Lookup do (l: (worker: machine, key: int)) {
      if (l.key in results) {
        send l.worker, LookupResp, (found = true, charge = results[l.key]);
      } else {
        send l.worker, LookupResp, (found = false, charge = 0);
      }
    }
    on Record do (r: (key: int, charge: int)) {
      results[r.key] = r.charge;
    }
  }
}

// One API worker per incoming request.
machine LookupThenRecordWorker {
  var client: machine;
  var table: KeyTable;
  var gateway: Gateway;
  var request: Request;

  start state Looking {
    entry (w: (client: machine, table: KeyTable, gateway: Gateway, request: Request)) {
      client = w.client;
      table = w.table;
      gateway = w.gateway;
      request = w.request;
      send table, Lookup, (worker = this, key = request.key);
    }
    on LookupResp do (r: (found: bool, charge: int)) {
      if (r.found) {
        Reply(r.charge);
        return;
      }
      // BUG: the key is still unmarked while the charge is in flight.
      send gateway, Charge, (client = this, key = request.key, amount = request.amount);
    }
    on ChargeResp do (r: (charge: int)) {
      send table, Record, (key = request.key, charge = r.charge);
      Reply(r.charge);
    }
  }

  fun Reply(charge: int) {
    announce Replied, (req = request.req, charge = charge);
    send client, ChargeResult, (req = request.req, charge = charge);
  }
}
