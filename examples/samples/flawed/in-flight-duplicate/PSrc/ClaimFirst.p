// THE FIX. Claim the key before the side effect. `claim` is one atomic
// insert-if-absent: the first request inserts key -> in progress and goes on to
// charge; a request that finds the key in progress waits until the first one
// completes and then returns the same result; a request that finds it done returns
// the stored result at once.
//
// Real primitives: claim (INSERT ... ON CONFLICT DO NOTHING, then read the row, with
// the second reader blocking on the row until the first commits, or polling), and
// complete (UPDATE key SET result, state = done), which releases the waiters.

event eClaim: (worker: machine, key: int);
// claimed: this request owns the key and must charge. Otherwise charge is the result.
event eClaimResp: (claimed: bool, charge: int);
event eComplete: (key: int, charge: int);

// One row per key. A row that is not done is the in-progress marker; the workers
// blocked on it are listed in the row until it completes.
type tKeyRow = (done: bool, charge: int, waiting: seq[machine]);

machine ClaimTable {
  var rows: map[int, tKeyRow];

  start state Serving {
    on eClaim do (c: (worker: machine, key: int)) {
      if (!(c.key in rows)) {
        rows[c.key] = (done = false, charge = 0, waiting = default(seq[machine]));
        send c.worker, eClaimResp, (claimed = true, charge = 0);
        return;
      }
      if (rows[c.key].done) {
        send c.worker, eClaimResp, (claimed = false, charge = rows[c.key].charge);
        return;
      }
      rows[c.key].waiting += (sizeof(rows[c.key].waiting), c.worker);
    }
    on eComplete do (c: (key: int, charge: int)) {
      var w: machine;
      foreach (w in rows[c.key].waiting) {
        send w, eClaimResp, (claimed = false, charge = c.charge);
      }
      rows[c.key] = (done = true, charge = c.charge, waiting = default(seq[machine]));
    }
  }
}

machine ClaimingWorker {
  var client: machine;
  var table: ClaimTable;
  var gateway: Gateway;
  var request: tRequest;

  start state Claiming {
    entry (w: (client: machine, table: ClaimTable, gateway: Gateway, request: tRequest)) {
      client = w.client;
      table = w.table;
      gateway = w.gateway;
      request = w.request;
      send table, eClaim, (worker = this, key = request.key);
    }
    on eClaimResp do (r: (claimed: bool, charge: int)) {
      if (!r.claimed) {
        // Someone else charged (or is charging) for this key: return their result.
        Reply(r.charge);
        return;
      }
      send gateway, eCharge, (client = this, key = request.key, amount = request.amount);
    }
    on eChargeResp do (r: (charge: int)) {
      send table, eComplete, (key = request.key, charge = r.charge);
      Reply(r.charge);
    }
  }

  fun Reply(charge: int) {
    announce eReplied, (req = request.req, charge = charge);
    send client, eChargeResult, (req = request.req, charge = charge);
  }
}
