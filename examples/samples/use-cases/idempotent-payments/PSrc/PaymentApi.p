// One payment API server. Several run behind a load balancer, so a double click or a
// retry can reach a different server than the first request. For each request:
// claim the key; if this request claimed it, call the card processor, record the
// result, reply; if the key is done, reply its saved result; if another request is
// still charging it, wait for that result and reply it.

event Charge: (client: machine, key: int, amount: int);
event ChargeResponse: (key: int, result: ChargeResult);
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
      receive { case ClaimResponse: (c: (key: int, claim: ClaimStatus, result: ChargeResult)) {
        claim = c.claim;
        result = c.result;
      } }
      if (claim == Claimed) {
        // Only the request that claimed the key ever calls the processor.
        send processor, Process, (api = this, key = r.key, amount = r.amount);
        receive { case ProcessResponse: (p: ChargeResult) { result = p; } }
        send store, Complete, (key = r.key, result = result);
      } else if (claim == InProgress) {
        receive { case KeyDone: (d: (key: int, result: ChargeResult)) { result = d.result; } }
      }
      send r.client, ChargeResponse, (key = r.key, result = result);
    }
  }
}
