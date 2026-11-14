// One payment API server. Several run behind a load balancer, so a double click or a
// retry can reach a different server than the first request. For each request:
// claim the key; if this request claimed it, call the card processor, record the
// result, reply; if the key is done, reply its saved result; if another request is
// still charging it, reply "retry later" (HTTP 409) and let the client retry.
//
// Each server handles one request at a time: it waits for the key table and the
// processor in `receive`, so their replies always belong to the current request.

event Charge: (client: machine, key: int, amount: int);
event ChargeResponse: (key: int, result: ChargeResult);
// The key is still being charged by another request: retry later (HTTP 409).
event ChargeConflict: (key: int);
// The card processor (an external service).
event Process: (api: machine, key: int, amount: int);
event ProcessResponse: ChargeResult;

machine PaymentApi {
  var store: KeyStore;
  var processor: machine;

  start state Serving {
    entry (input: (store: KeyStore, processor: machine)) {
      store = input.store;
      processor = input.processor;
    }
    on Charge do (r: (client: machine, key: int, amount: int)) {
      var claim: ClaimStatus;
      var result: ChargeResult;
      send store, Claim, (api = this, key = r.key);
      receive { case ClaimResponse: (c: (claim: ClaimStatus, result: ChargeResult)) {
        claim = c.claim;
        result = c.result;
      } }
      if (claim == InProgress) {
        send r.client, ChargeConflict, (key = r.key,);
        return;
      }
      if (claim == Claimed) {
        // Only the request that claimed the key ever calls the processor.
        send processor, Process, (api = this, key = r.key, amount = r.amount);
        receive { case ProcessResponse: (p: ChargeResult) { result = p; } }
        send store, Complete, (key = r.key, result = result);
      }
      send r.client, ChargeResponse, (key = r.key, result = result);
    }
  }
}
