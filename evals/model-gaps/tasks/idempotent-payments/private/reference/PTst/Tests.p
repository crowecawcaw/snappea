// The environment and the test cases. The card processor, the network in front of it
// and the clients are not the design: what they announce is ground truth.

event TopUp: (driver: machine, amount: int);
event ToppedUp;
// Client to its owner: the payment got its first final answer.
event ClientDone;
// To a client: its attempt timed out. The payload is the servers the load balancer
// reaches now; the client sends another attempt to one of them.
event ChargeTimeout: seq[machine];

// The card processor: approves while the card has funds, and remembers what it did
// with each key. Funds can be topped up.
machine CardProcessor {
  var funds: int;
  var results: map[int, ChargeResult];

  start state Serving {
    entry (initial: int) { funds = initial; }
    on Process do (p: (api: machine, key: int, amount: int)) {
      var result: ChargeResult;
      result = Declined;
      if (p.amount <= funds) {
        funds = funds - p.amount;
        result = Approved;
      }
      results[p.key] = result;
      announce ProcessorCharged, (key = p.key, amount = p.amount, result = result);
      send p.api, ProcessResponse, (key = p.key, result = result);
    }
    on Lookup do (l: (api: machine, key: int)) {
      if (l.key in results) {
        send l.api, LookupResponse, (key = l.key, found = true, result = results[l.key]);
      } else {
        send l.api, LookupResponse, (key = l.key, found = false, result = Declined);
      }
    }
    on TopUp do (t: (driver: machine, amount: int)) {
      funds = funds + t.amount;
      send t.driver, ToppedUp;
    }
  }
}

// What goes wrong at the card processor's door, once, at the first Process.
//   Reliable:            nothing.
//   LoseReply:           the reply is dropped; the server gets a ProcessTimeout.
//   DelayReply:          the server gets a ProcessTimeout, and the reply arrives later.
//   CrashBeforeProcess:  the server process crashes before its Process leaves the
//                        machine: the card is not charged.
//   CrashAfterProcess:   the server process crashes after its Process left: the card
//                        is charged and the reply reaches nobody.
enum Fault { Reliable, LoseReply, DelayReply, CrashBeforeProcess, CrashAfterProcess }

// To the driver: the process on this server crashed. The driver restarts it.
event ServerCrashed: machine;

// A reply delivered by its own sender, so the scheduler can deliver it before or after
// anything the link sends next.
machine LateDelivery {
  start state Deliver {
    entry (d: (dest: machine, reply: (key: int, result: ChargeResult))) {
      send d.dest, ProcessResponse, d.reply;
    }
  }
}

// The network between the servers and the card processor, and the place where the
// fault happens. Requests that left a server always get through, in order.
//
// A crash stops the server process at once: `halt` removes it with everything it knew
// and had queued. It happens here, while the process waits for the processor and does
// nothing else, so the dead process sends nothing after its crash.
machine ProcessorLink {
  var processor: machine;
  var fault: Fault;
  var driver: machine;
  // key -> the server waiting for the reply
  var charging: map[int, machine];
  var asking: map[int, machine];

  start state Forwarding {
    entry (input: (processor: machine, fault: Fault, driver: machine)) {
      processor = input.processor;
      fault = input.fault;
      driver = input.driver;
    }
    on Process do (p: (api: machine, key: int, amount: int)) {
      charging[p.key] = p.api;
      if (fault == CrashBeforeProcess || fault == CrashAfterProcess) {
        if (fault == CrashAfterProcess) {
          send processor, Process, (api = this, key = p.key, amount = p.amount);
        }
        fault = Reliable;
        send p.api, halt;
        send driver, ServerCrashed, p.api;
        return;
      }
      send processor, Process, (api = this, key = p.key, amount = p.amount);
    }
    on ProcessResponse do (r: (key: int, result: ChargeResult)) {
      if (fault == LoseReply || fault == DelayReply) {
        send charging[r.key], ProcessTimeout, (key = r.key,);
        if (fault == DelayReply) {
          new LateDelivery((dest = charging[r.key], reply = r));
        }
        fault = Reliable;
        return;
      }
      // A reply to a crashed process goes nowhere: a halted machine drops it.
      send charging[r.key], ProcessResponse, r;
    }
    on Lookup do (l: (api: machine, key: int)) {
      asking[l.key] = l.api;
      send processor, Lookup, (api = this, key = l.key);
    }
    on LookupResponse do (r: (key: int, found: bool, result: ChargeResult)) {
      send asking[r.key], LookupResponse, r;
    }
  }
}

type PaymentInput = (owner: machine, payment: int, key: int, amount: int, servers: seq[machine]);

// A client making one payment. It sends one attempt to every server in `servers` at
// once: one server is a plain request, two are a double click. Told to retry, or after
// a timeout, it sends the same request again to any server it can reach. It announces
// every attempt and every answer.
machine PayingClient {
  var me: PaymentInput;
  var settled: bool;

  start state Paying {
    entry (input: PaymentInput) {
      var api: machine;
      me = input;
      foreach (api in me.servers) { Attempt(api); }
    }
    on ChargeConflict do (c: (key: int)) {
      announce ToldToRetry, (payment = me.payment, key = c.key);
      Attempt(choose(me.servers));
    }
    on ChargeTimeout do (reachable: seq[machine]) {
      me.servers = reachable;
      Attempt(choose(me.servers));
    }
    on ChargeResponse do (r: (key: int, result: ChargeResult)) {
      announce Answered, (payment = me.payment, key = r.key, amount = me.amount, result = r.result);
      Settle();
    }
    on ChargeRejected do (r: (key: int)) {
      announce Rejected, (payment = me.payment, key = r.key, amount = me.amount);
      Settle();
    }
  }

  fun Attempt(api: machine) {
    announce Attempted, (payment = me.payment, key = me.key, amount = me.amount);
    send api, Charge, (client = this, key = me.key, amount = me.amount);
  }

  fun Settle() {
    if (!settled) {
      settled = true;
      send me.owner, ClientDone;
    }
  }
}

// store, processor: what a server is created with. card: the card processor itself.
// a, b: the processes on servers 1 and 2.
type PaymentSystem = (store: machine, processor: machine, card: machine, a: machine, b: machine);

// Two servers, one key table, one card with `funds`. Only a system with a fault talks
// to the processor through a link, which keeps the other schedules small. `driver`
// hears of a crash.
fun NewSystem(funds: int, fault: Fault, driver: machine): PaymentSystem {
  var store: machine;
  var card: machine;
  var processor: machine;
  store = new KeyStore();
  card = new CardProcessor(funds);
  processor = card;
  if (fault != Reliable) {
    processor = new ProcessorLink((processor = card, fault = fault, driver = driver));
  }
  return (store = store, processor = processor, card = card,
          a = new PaymentApi((store = store, processor = processor, id = 1, restarted = false)),
          b = new PaymentApi((store = store, processor = processor, id = 2, restarted = false)));
}

// The process that starts on server `id` after its process crashed.
fun Restart(sys: PaymentSystem, id: int): machine {
  return new PaymentApi((store = sys.store, processor = sys.processor, id = id, restarted = true));
}

fun Pay(owner: machine, payment: int, key: int, amount: int, servers: seq[machine]): machine {
  return new PayingClient((owner = owner, payment = payment, key = key, amount = amount, servers = servers));
}

fun Only(api: machine): seq[machine] {
  var s: seq[machine];
  s += (0, api);
  return s;
}

fun Both(x: machine, y: machine): seq[machine] {
  var s: seq[machine];
  s += (0, x);
  s += (1, y);
  return s;
}

// The answer was lost on the way to the user, who retries the same key on the other
// server: the retry returns the saved result, no second charge.
machine RetryAfterAnswer {
  var sys: PaymentSystem;
  start state First {
    entry {
      sys = NewSystem(10, Reliable, this);
      announce PaymentsPlanned, 2;
      Pay(this, 1, 1, 5, Only(sys.a));
    }
    on ClientDone goto Retry;
  }
  state Retry {
    entry { Pay(this, 2, 1, 5, Only(sys.b)); }
    ignore ClientDone;
  }
}

// Double click: the same key reaches both servers at once. One claims and charges; the
// other finds the key in progress (retry later) or done (the saved result).
machine DoubleClick {
  start state Run {
    entry {
      var sys: PaymentSystem;
      sys = NewSystem(10, Reliable, this);
      announce PaymentsPlanned, 1;
      Pay(this, 1, 1, 5, Both(sys.a, sys.b));
    }
    ignore ClientDone;
  }
}

// Double click that lands on one server twice, with another key queued behind it: the
// server takes them one at a time and loses none.
machine BusyServer {
  start state Run {
    entry {
      var sys: PaymentSystem;
      sys = NewSystem(5, Reliable, this);
      announce PaymentsPlanned, 2;
      Pay(this, 1, 1, 5, Both(sys.a, sys.a));
      Pay(this, 2, 2, 5, Only(sys.a));
    }
    ignore ClientDone;
  }
}

// A declined payment stays declined: no funds, key 1 is declined, the card is topped
// up, and the retry of key 1 still returns Declined without another charge. A new key
// then succeeds.
machine DeclinedStaysDeclined {
  var sys: PaymentSystem;
  start state First {
    entry {
      sys = NewSystem(0, Reliable, this);
      announce PaymentsPlanned, 3;
      Pay(this, 1, 1, 5, Only(sys.a));
    }
    on ClientDone goto ToppingUp;
  }
  state ToppingUp {
    entry { send sys.card, TopUp, (driver = this, amount = 5); }
    on ToppedUp goto Retry;
  }
  state Retry {
    entry { Pay(this, 2, 1, 5, Only(sys.b)); }
    on ClientDone goto NewKey;
  }
  state NewKey {
    entry { Pay(this, 3, 2, 5, Only(sys.a)); }
    ignore ClientDone;
  }
}

// Two keys at once on two servers, funds for one: one is approved, one declined, and
// neither is refused or answered because of the other.
machine IndependentKeys {
  start state Run {
    entry {
      var sys: PaymentSystem;
      sys = NewSystem(5, Reliable, this);
      announce PaymentsPlanned, 2;
      Pay(this, 1, 1, 5, Only(sys.a));
      Pay(this, 2, 2, 5, Only(sys.b));
    }
    ignore ClientDone;
  }
}

// Key 1 paid 5 and is done. The key comes back with amount 7: rejected, not answered
// with the result for 5 and not charged. The original request still gets its result.
machine KeyReused {
  var sys: PaymentSystem;
  start state First {
    entry {
      sys = NewSystem(20, Reliable, this);
      announce PaymentsPlanned, 3;
      Pay(this, 1, 1, 5, Only(sys.a));
    }
    on ClientDone goto OtherAmount;
  }
  state OtherAmount {
    entry { Pay(this, 2, 1, 7, Only(sys.b)); }
    on ClientDone goto SameAgain;
  }
  state SameAgain {
    entry { Pay(this, 3, 1, 5, Only(sys.b)); }
    ignore ClientDone;
  }
}

// The same key with two amounts at once, on two servers: whichever claims first is
// charged, the other is rejected.
machine KeyReusedAtOnce {
  start state Run {
    entry {
      var sys: PaymentSystem;
      sys = NewSystem(20, Reliable, this);
      announce PaymentsPlanned, 2;
      Pay(this, 1, 1, 5, Only(sys.a));
      Pay(this, 2, 1, 7, Only(sys.b));
    }
    ignore ClientDone;
  }
}

// Server 1's Process reply for key 1 is lost or late: it must find out what happened
// without charging again. Then key 2 goes to the same server with no funds left, so a
// late Approved for key 1 must not be taken as key 2's result.
machine ProcessorReplyFault {
  var sys: PaymentSystem;
  start state First {
    entry (fault: Fault) {
      sys = NewSystem(5, fault, this);
      announce PaymentsPlanned, 2;
      Pay(this, 1, 1, 5, Only(sys.a));
    }
    on ClientDone goto Next;
  }
  state Next {
    entry { Pay(this, 2, 2, 5, Only(sys.a)); }
    ignore ClientDone;
  }
}

// Server 1 crashes while it charges key 1, before or after its Process left, and
// restarts. The client times out and retries on either server; server 2 never hears
// from server 1. The key must not stay in progress and must not be charged twice.
machine CrashWhileCharging {
  var sys: PaymentSystem;
  var client: machine;
  start state Run {
    entry (fault: Fault) {
      sys = NewSystem(10, fault, this);
      announce PaymentsPlanned, 1;
      client = Pay(this, 1, 1, 5, Only(sys.a));
    }
    on ServerCrashed do (old: machine) {
      send client, ChargeTimeout, Both(Restart(sys, 1), sys.b);
    }
    ignore ClientDone;
  }
}

// Server 1 crashes after its Process for key 1 left. The client retries on server 2
// only, and key 2 goes to the restarted server 1 at once, with no funds left: it waits
// until the server has settled key 1, and both get their own result.
machine CrashThenNextKey {
  var sys: PaymentSystem;
  var client: machine;
  start state Run {
    entry {
      sys = NewSystem(5, CrashAfterProcess, this);
      announce PaymentsPlanned, 2;
      client = Pay(this, 1, 1, 5, Only(sys.a));
    }
    on ServerCrashed do (old: machine) {
      var restarted: machine;
      restarted = Restart(sys, 1);
      Pay(this, 2, 2, 5, Only(restarted));
      send client, ChargeTimeout, Only(sys.b);
    }
    ignore ClientDone;
  }
}

machine MainReplyLost { start state Init { entry { new ProcessorReplyFault(LoseReply); } } }
machine MainReplyLate { start state Init { entry { new ProcessorReplyFault(DelayReply); } } }
machine MainCrashBeforeCharge { start state Init { entry { new CrashWhileCharging(CrashBeforeProcess); } } }
machine MainCrashAfterCharge { start state Init { entry { new CrashWhileCharging(CrashAfterProcess); } } }

module Payments = { KeyStore, PaymentApi, CardProcessor, ProcessorLink, LateDelivery, PayingClient };

test tcRetryAfterAnswer [main=RetryAfterAnswer]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in (union Payments, { RetryAfterAnswer });

test tcDoubleClick [main=DoubleClick]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in (union Payments, { DoubleClick });

test tcBusyServer [main=BusyServer]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in (union Payments, { BusyServer });

test tcDeclinedStaysDeclined [main=DeclinedStaysDeclined]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in (union Payments, { DeclinedStaysDeclined });

test tcIndependentKeys [main=IndependentKeys]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in (union Payments, { IndependentKeys });

test tcKeyReused [main=KeyReused]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in (union Payments, { KeyReused });

test tcKeyReusedAtOnce [main=KeyReusedAtOnce]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in (union Payments, { KeyReusedAtOnce });

test tcProcessorReplyLost [main=MainReplyLost]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in
  (union Payments, { MainReplyLost, ProcessorReplyFault });

test tcProcessorReplyLate [main=MainReplyLate]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in
  (union Payments, { MainReplyLate, ProcessorReplyFault });

test tcCrashBeforeCharge [main=MainCrashBeforeCharge]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in
  (union Payments, { MainCrashBeforeCharge, CrashWhileCharging });

test tcCrashAfterCharge [main=MainCrashAfterCharge]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in
  (union Payments, { MainCrashAfterCharge, CrashWhileCharging });

test tcCrashThenNextKey [main=CrashThenNextKey]:
  assert ChargedOnce, HonestRefusals, EveryPaymentAnswered in (union Payments, { CrashThenNextKey });
