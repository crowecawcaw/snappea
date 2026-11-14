// THE FIX. Claim the key before the side effect. `claim` is one atomic
// insert-if-absent: the first request inserts key -> in progress and goes on to
// charge; a request that finds the key in progress waits until the first one
// completes and then returns the same result; a request that finds it done returns
// the stored result at once.
//
// Real primitives: claim (INSERT ... ON CONFLICT DO NOTHING, then read the row, with
// the second reader blocking on the row until the first commits, or polling), and
// complete (UPDATE key SET result, state = done), which releases the waiters.

event Claim: (worker: machine, key: int);
// claimed: this request owns the key and must charge. Otherwise charge is the result.
event ClaimResp: (claimed: bool, charge: int);
event Complete: (key: int, charge: int);

// One row per key. A row that is not done is the in-progress marker; the workers
// blocked on it are listed in the row until it completes.
type KeyRow = (done: bool, charge: int, waiting: seq[machine]);

machine ClaimTable {
  var rows: map[int, KeyRow];

  start state Serving {
    on Claim do (c: (worker: machine, key: int)) {
      if (!(c.key in rows)) {
        rows[c.key] = (done = false, charge = 0, waiting = default(seq[machine]));
        send c.worker, ClaimResp, (claimed = true, charge = 0);
        return;
      }
      if (rows[c.key].done) {
        send c.worker, ClaimResp, (claimed = false, charge = rows[c.key].charge);
        return;
      }
      rows[c.key].waiting += (sizeof(rows[c.key].waiting), c.worker);
    }
    on Complete do (c: (key: int, charge: int)) {
      var w: machine;
      foreach (w in rows[c.key].waiting) {
        send w, ClaimResp, (claimed = false, charge = c.charge);
      }
      rows[c.key] = (done = true, charge = c.charge, waiting = default(seq[machine]));
    }
  }
}

machine ClaimingWorker {
  var client: machine;
  var table: ClaimTable;
  var gateway: Gateway;
  var request: Request;

  start state Claiming {
    entry (w: (client: machine, table: ClaimTable, gateway: Gateway, request: Request)) {
      client = w.client;
      table = w.table;
      gateway = w.gateway;
      request = w.request;
      send table, Claim, (worker = this, key = request.key);
    }
    on ClaimResp do (r: (claimed: bool, charge: int)) {
      if (!r.claimed) {
        // Someone else charged (or is charging) for this key: return their result.
        Reply(r.charge);
        return;
      }
      send gateway, Charge, (client = this, key = request.key, amount = request.amount);
    }
    on ChargeResp do (r: (charge: int)) {
      send table, Complete, (key = request.key, charge = r.charge);
      Reply(r.charge);
    }
  }

  fun Reply(charge: int) {
    announce Replied, (req = request.req, charge = charge);
    send client, ChargeResult, (req = request.req, charge = charge);
  }
}
