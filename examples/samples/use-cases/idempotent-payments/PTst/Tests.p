// The environment: the card processor and the clients. Neither is the design under
// test, so they live here and are not mutated; what they announce is ground truth.

event eTopUp: (driver: machine, amount: int);
event eToppedUp;
event eClientDone;

// The card processor: approves while the card has funds. Funds can be topped up.
machine CardProcessor {
  var funds: int;
  start state Serving {
    entry (initial: int) { funds = initial; }
    on eProcess do (p: (api: machine, key: int, amount: int)) {
      var result: tResult;
      result = Declined;
      if (p.amount <= funds) {
        funds = funds - p.amount;
        result = Approved;
      }
      announce eProcessorCharged, (key = p.key, amount = p.amount, result = result);
      send p.api, eProcessResp, result;
    }
    on eTopUp do (t: (driver: machine, amount: int)) {
      funds = funds + t.amount;
      send t.driver, eToppedUp;
    }
  }
}

// A client paying with idempotency key `key`. It sends every attempt in `apis` at once:
// one attempt is a plain request; two model a double click (two servers), or a
// timed-out first attempt retried before or after the first one finished (its
// response counts as lost to the user, but is still checked).
machine PayingClient {
  var owner: machine;
  var outstanding: int;
  start state Paying {
    entry (input: (owner: machine, key: int, amount: int, apis: seq[machine])) {
      var api: machine;
      owner = input.owner;
      foreach (api in input.apis) {
        announce eAttempted, (key = input.key, amount = input.amount);
        send api, eCharge, (client = this, key = input.key, amount = input.amount);
        outstanding = outstanding + 1;
      }
    }
    on eChargeResp do (r: (key: int, result: tResult)) {
      announce eAnswered, r;
      outstanding = outstanding - 1;
      if (outstanding == 0) { send owner, eClientDone; }
    }
  }
}

type tSystem = (processor: CardProcessor, a: machine, b: machine);

fun NewSystem(funds: int): tSystem {
  var store: KeyStore;
  var processor: CardProcessor;
  store = new KeyStore();
  processor = new CardProcessor(funds);
  return (processor = processor, a = new PaymentApi((store = store, processor = processor)),
          b = new PaymentApi((store = store, processor = processor)));
}

fun Pay(owner: machine, key: int, amount: int, apis: seq[machine]) {
  new PayingClient((owner = owner, key = key, amount = amount, apis = apis));
}

fun Both(sys: tSystem): seq[machine] {
  var apis: seq[machine];
  apis += (0, sys.a);
  apis += (1, sys.b);
  return apis;
}

fun Only(api: machine): seq[machine] {
  var apis: seq[machine];
  apis += (0, api);
  return apis;
}

// Double click: the same key reaches both servers at once. Every order of claims,
// the processor call and completion is explored: one claims and charges, the other
// finds the key in progress (waits for the result) or done (returns it).
machine DoubleClick {
  start state Run {
    entry (sys: tSystem) {
      announce eAttemptsPlanned, 2;
      Pay(this, 1, 5, Both(sys));
    }
    ignore eClientDone;
  }
}

// Lost response, then retry: the first attempt is answered, the user never saw it,
// and the client retries the same key on the other server. The retry must return the
// saved result without charging again.
machine LostResponseRetry {
  var sys: tSystem;
  start state First {
    entry (s: tSystem) {
      sys = s;
      announce eAttemptsPlanned, 2;
      Pay(this, 1, 5, Only(sys.a));
    }
    on eClientDone goto Retry;
  }
  state Retry {
    entry { Pay(this, 1, 5, Only(sys.b)); }
    ignore eClientDone;
  }
}

// A declined payment stays declined: the card has no funds, key 1 is declined, the
// card is topped up, and the retry of key 1 must still return Declined. A new key 2
// then succeeds, so the decline is per key, not stuck on the card.
machine DeclinedStaysDeclined {
  var sys: tSystem;
  start state First {
    entry (s: tSystem) {
      sys = s;
      announce eAttemptsPlanned, 3;
      Pay(this, 1, 5, Only(sys.a));
    }
    on eClientDone goto TopUp;
  }
  state TopUp {
    entry { send sys.processor, eTopUp, (driver = this, amount = 5); }
    on eToppedUp goto Retry;
  }
  state Retry {
    entry { Pay(this, 1, 5, Only(sys.b)); }
    on eClientDone goto NewKey;
  }
  state NewKey {
    entry { Pay(this, 2, 5, Only(sys.a)); }
    ignore eClientDone;
  }
}

// Independent keys: two payments with different keys at once, on different servers,
// with funds for only one. Exactly one is approved; neither waits for the other or
// gets the other's result.
machine IndependentKeys {
  start state Run {
    entry (sys: tSystem) {
      announce eAttemptsPlanned, 2;
      Pay(this, 1, 5, Only(sys.a));
      Pay(this, 2, 5, Only(sys.b));
    }
    ignore eClientDone;
  }
}

machine MainDoubleClick { start state Init { entry { new DoubleClick(NewSystem(10)); } } }
machine MainLostResponseRetry { start state Init { entry { new LostResponseRetry(NewSystem(10)); } } }
machine MainDeclinedStaysDeclined { start state Init { entry { new DeclinedStaysDeclined(NewSystem(0)); } } }
machine MainIndependentKeys { start state Init { entry { new IndependentKeys(NewSystem(5)); } } }
machine MainBugCheckThenAct {
  start state Init {
    entry {
      var store: PlainKeyStore;
      var processor: CardProcessor;
      store = new PlainKeyStore();
      processor = new CardProcessor(10);
      new DoubleClick((processor = processor,
        a = new CheckThenActApi((store = store, processor = processor)),
        b = new CheckThenActApi((store = store, processor = processor))));
    }
  }
}

module Payments = { KeyStore, PaymentApi, CardProcessor, PayingClient };

test tcDoubleClick [main=MainDoubleClick]:
  assert ChargedOnce, EveryAttemptAnswered in (union Payments, { MainDoubleClick, DoubleClick });

test tcLostResponseRetry [main=MainLostResponseRetry]:
  assert ChargedOnce, EveryAttemptAnswered in
  (union Payments, { MainLostResponseRetry, LostResponseRetry });

test tcDeclinedStaysDeclined [main=MainDeclinedStaysDeclined]:
  assert ChargedOnce, EveryAttemptAnswered in
  (union Payments, { MainDeclinedStaysDeclined, DeclinedStaysDeclined });

test tcIndependentKeys [main=MainIndependentKeys]:
  assert ChargedOnce, EveryAttemptAnswered in
  (union Payments, { MainIndependentKeys, IndependentKeys });

// expect-bug: key 1 charged twice
test tcBugCheckThenAct [main=MainBugCheckThenAct]:
  assert ChargedOnce, EveryAttemptAnswered in
  { MainBugCheckThenAct, DoubleClick, PlainKeyStore, CheckThenActApi, CardProcessor, PayingClient };
