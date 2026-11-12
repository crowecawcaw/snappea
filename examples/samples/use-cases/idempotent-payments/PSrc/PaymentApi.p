// One payment API server. Several run behind a load balancer, so a double click or a
// retry can reach a different server than the first request. For each request:
// claim the key; if this request claimed it, call the card processor, record the
// result, reply; if the key is done, reply its saved result; if another request is
// still charging it, wait for that result and reply it.

event eCharge: (client: machine, key: int, amount: int);
event eChargeResp: (key: int, result: tResult);
// The card processor (an external service).
event eProcess: (api: machine, key: int, amount: int);
event eProcessResp: tResult;

machine PaymentApi {
  var store: KeyStore;
  var processor: machine;

  start state Serving {
    entry (input: (store: KeyStore, processor: machine)) {
      store = input.store;
      processor = input.processor;
    }
    on eCharge do (r: (client: machine, key: int, amount: int)) {
      var claim: tClaim;
      var result: tResult;
      send store, eClaim, (api = this, key = r.key);
      receive { case eClaimResp: (c: (key: int, claim: tClaim, result: tResult)) {
        claim = c.claim;
        result = c.result;
      } }
      if (claim == Claimed) {
        // Only the request that claimed the key ever calls the processor.
        send processor, eProcess, (api = this, key = r.key, amount = r.amount);
        receive { case eProcessResp: (p: tResult) { result = p; } }
        send store, eComplete, (key = r.key, result = result);
      } else if (claim == InProgress) {
        receive { case eKeyDone: (d: (key: int, result: tResult)) { result = d.result; } }
      }
      send r.client, eChargeResp, (key = r.key, result = result);
    }
  }
}
