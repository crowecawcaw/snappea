// The idempotency contract, stated over events alone: what the client asked for, what
// the payment gateway actually charged, and what each request returned. It does not
// look at the idempotency table, so it judges both designs by the same rule.

// The driver announces how many requests it will send, then each one before sending it.
event RequestsPlanned: int;
event Requested: (req: int, key: int, amount: int);
// The gateway announces each charge when it commits.
event Charged: (key: int, amount: int, charge: int);
// The API worker announces the result it returns to the client.
event Replied: (req: int, charge: int);

// Safety: one idempotency key causes at most one charge, for the amount requested.
spec ChargedOnce observes Requested, Charged {
  // key -> amount the client asked for
  var amounts: map[int, int];
  // key -> charge already made for it
  var charged: map[int, int];
  var chargeIds: set[int];

  start state Watching {

    on Requested do (p: (req: int, key: int, amount: int)) {
      if (p.key in amounts) {
        assert amounts[p.key] == p.amount, format("key {0} reused with a different amount", p.key);
      }
      amounts[p.key] = p.amount;
    }
    on Charged do (p: (key: int, amount: int, charge: int)) {
      assert p.key in amounts, format("charge {0} for key {1}, which nobody requested", p.charge, p.key);
      assert !(p.key in charged),
        format("double charge: key {0} charged twice (charges {1} and {2})", p.key, charged[p.key], p.charge);
      assert p.amount == amounts[p.key],
        format("key {0} charged {1}, but the client asked for {2}", p.key, p.amount, amounts[p.key]);
      assert !(p.charge in chargeIds), format("gateway reused charge id {0}", p.charge);
      chargeIds += (p.charge);
      charged[p.key] = p.charge;
    }
  }
}

// Progress: every request returns, and returns the one charge made for its key, so a
// duplicate sees the same result as the original. Hot until every planned request
// has returned.
spec EveryRequestAnswered observes RequestsPlanned, Requested, Charged, Replied {
  // req -> key, for requests that have not returned yet
  var pending: map[int, int];
  var charged: map[int, int];
  var unfinished: int;

  start state Idle {
    on RequestsPlanned do (n: int) {
      assert n > 0, "planned no requests";
      unfinished = n;
      goto Busy;
    }
    on Requested do { assert false, "request sent without a plan"; }
    on Charged do { assert false, "charge with no request in progress"; }
    on Replied do { assert false, "reply with no request in progress"; }
  }

  hot state Busy {
    on RequestsPlanned do { assert false, "requests planned twice"; }

    on Requested do (p: (req: int, key: int, amount: int)) {
      assert !(p.req in pending), format("request {0} sent twice", p.req);
      pending[p.req] = p.key;
    }

    on Charged do (p: (key: int, amount: int, charge: int)) {
      charged[p.key] = p.charge;
    }
    on Replied do (p: (req: int, charge: int)) {
      assert p.req in pending, format("request {0} returned twice or was never sent", p.req);
      assert pending[p.req] in charged,
        format("request {0} returned charge {1}, but key {2} was never charged", p.req, p.charge, pending[p.req]);
      assert p.charge == charged[pending[p.req]],
        format("request {0} returned charge {1}, but key {2} was charged as {3}",
          p.req, p.charge, pending[p.req], charged[pending[p.req]]);
      pending -= (p.req);
      unfinished = unfinished - 1;
      if (unfinished == 0) { goto Idle; }
    }
  }
}
