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

// The simple path: one payment, one server.
machine SinglePayment {
  start state Run {
    entry {
      var sys: PaymentSystem;
      sys = NewSystem(10, Reliable, this);
      announce PaymentsPlanned, 1;
      Pay(this, 1, 1, 5, Only(sys.a));
    }
    ignore ClientDone;
  }
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

module Payments = { KeyStore, PaymentApi, CardProcessor, ProcessorLink, LateDelivery, PayingClient };

test tcSinglePayment [main=SinglePayment]:
  assert ChargedOnce in (union Payments, { SinglePayment });

test tcRetryAfterAnswer [main=RetryAfterAnswer]:
  assert ChargedOnce in (union Payments, { RetryAfterAnswer });
