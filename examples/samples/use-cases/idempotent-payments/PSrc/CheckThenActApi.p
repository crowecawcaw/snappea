// Negative control, kept apart so its code is not part of the passing tests: the API
// looks the key up, charges if it is absent, and only then records the result. Two
// duplicates that look up before either records both charge.
// tcBugCheckThenAct shows the contract catching it.

event Lookup: (api: machine, key: int);
event LookupResponse: (found: bool, result: ChargeResult);
event Put: (key: int, result: ChargeResult);

// A plain key-value table: get and put, no conditional insert.
machine PlainKeyStore {
  var results: map[int, ChargeResult];
  start state Serving {
    on Lookup do (l: (api: machine, key: int)) {
      if (l.key in results) {
        send l.api, LookupResponse, (found = true, result = results[l.key]);
      } else {
        send l.api, LookupResponse, (found = false, result = Declined);
      }
    }
    on Put do (p: (key: int, result: ChargeResult)) { results[p.key] = p.result; }
  }
}

machine CheckThenActApi {
  var store: PlainKeyStore;
  var processor: machine;

  start state Serving {
    entry (input: (store: PlainKeyStore, processor: machine)) {
      store = input.store;
      processor = input.processor;
    }
    on Charge do (r: (client: machine, key: int, amount: int)) {
      var found: bool;
      var result: ChargeResult;
      send store, Lookup, (api = this, key = r.key);
      receive { case LookupResponse: (l: (found: bool, result: ChargeResult)) {
        found = l.found;
        result = l.result;
      } }
      if (!found) {
        // BUG: another server can look the key up between the lookup and the put.
        send processor, Process, (api = this, key = r.key, amount = r.amount);
        receive { case ProcessResponse: (p: ChargeResult) { result = p; } }
        send store, Put, (key = r.key, result = result);
      }
      send r.client, ChargeResponse, (key = r.key, result = result);
    }
  }
}
