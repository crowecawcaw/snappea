// The idempotency-key table. One record per key: in progress and owned by a server, or
// done with the result every later request with that key gets. Each handler is one
// atomic write or read.

// done: false while the request that claimed the key is charging.
type KeyRecord = (owner: int, amount: int, done: bool, result: ChargeResult);

machine KeyStore {
  var records: map[int, KeyRecord];

  start state Serving {
    on Claim do (c: (api: machine, owner: int, key: int, amount: int)) {
      if (!(c.key in records)) {
        // The unique-key insert: only one request ever gets Claimed for a free key.
        records[c.key] = (owner = c.owner, amount = c.amount, done = false, result = Declined);
        send c.api, ClaimResponse, (key = c.key, status = Claimed, result = Declined);
        return;
      }
      // The key belongs to a payment of another amount, whatever state it is in.
      if (records[c.key].amount != c.amount) {
        send c.api, ClaimResponse, (key = c.key, status = Mismatch, result = Declined);
        return;
      }
      if (records[c.key].done) {
        send c.api, ClaimResponse, (key = c.key, status = Finished, result = records[c.key].result);
        return;
      }
      send c.api, ClaimResponse, (key = c.key, status = InProgress, result = Declined);
    }

    on Complete do (c: (key: int, result: ChargeResult)) {
      if (c.key in records && !records[c.key].done) {
        records[c.key].done = true;
        records[c.key].result = c.result;
      }
    }

    on Release do (r: (key: int)) {
      if (r.key in records && !records[r.key].done) {
        records -= (r.key);
      }
    }

    on ListOwned do (l: (api: machine, owner: int)) {
      var key: int;
      var owned: seq[int];
      foreach (key in keys(records)) {
        if (records[key].owner == l.owner && !records[key].done) {
          owned += (sizeof(owned), key);
        }
      }
      send l.api, OwnedKeys, owned;
    }
  }
}
