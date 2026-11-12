// Negative control, kept apart so its code is not part of the passing tests: the API
// looks the key up, charges if it is absent, and only then records the result. Two
// duplicates that look up before either records both charge.
// tcBugCheckThenAct shows the contract catching it.

event eLookup: (api: machine, key: int);
event eLookupResp: (found: bool, result: tResult);
event ePut: (key: int, result: tResult);

// A plain key-value table: get and put, no conditional insert.
machine PlainKeyStore {
  var results: map[int, tResult];
  start state Serving {
    on eLookup do (l: (api: machine, key: int)) {
      if (l.key in results) {
        send l.api, eLookupResp, (found = true, result = results[l.key]);
      } else {
        send l.api, eLookupResp, (found = false, result = Declined);
      }
    }
    on ePut do (p: (key: int, result: tResult)) { results[p.key] = p.result; }
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
    on eCharge do (r: (client: machine, key: int, amount: int)) {
      var found: bool;
      var result: tResult;
      send store, eLookup, (api = this, key = r.key);
      receive { case eLookupResp: (l: (found: bool, result: tResult)) {
        found = l.found;
        result = l.result;
      } }
      if (!found) {
        // BUG: another server can look the key up between the lookup and the put.
        send processor, eProcess, (api = this, key = r.key, amount = r.amount);
        receive { case eProcessResp: (p: tResult) { result = p; } }
        send store, ePut, (key = r.key, result = result);
      }
      send r.client, eChargeResp, (key = r.key, result = result);
    }
  }
}
