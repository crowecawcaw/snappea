// The idempotency contract, over what the card processor did and what each client
// sent and received. The processor and the clients are the environment (PTst), so
// their announcements are ground truth; the specs never look at the key table.

// Safety: the processor charges each key at most once, for an amount a request with
// that key asked for, and every result a client gets for a key, first or retried,
// concurrent or late, is the result of that one charge and answers a request for the
// charged amount.
spec ChargedOnce observes Attempted, ProcessorCharged, Answered {
  // key -> amounts the clients asked for
  var asked: map[int, set[int]];
  // key -> the one charge the processor ran
  var charged: map[int, (amount: int, result: ChargeResult)];

  start state Watching {
    on Attempted do (p: (payment: int, key: int, amount: int)) {
      if (!(p.key in asked)) { asked[p.key] = default(set[int]); }
      asked[p.key] += (p.amount);
    }
    on ProcessorCharged do (p: (key: int, amount: int, result: ChargeResult)) {
      assert p.key in asked, format("key {0} charged but never requested", p.key);
      assert !(p.key in charged), format("key {0} charged twice", p.key);
      assert p.amount in asked[p.key],
        format("key {0} charged {1}, which no request with that key asked for", p.key, p.amount);
      charged[p.key] = (amount = p.amount, result = p.result);
    }
    on Answered do (p: (payment: int, key: int, amount: int, result: ChargeResult)) {
      assert p.key in charged,
        format("payment {0} answered {1} for key {2} without any charge", p.payment, p.result, p.key);
      assert p.result == charged[p.key].result,
        format("payment {0} answered {1} for key {2}, but its charge was {3}",
               p.payment, p.result, p.key, charged[p.key].result);
      assert p.amount == charged[p.key].amount,
        format("payment {0} asked for {1} with key {2} and got the result of a charge of {3}",
               p.payment, p.amount, p.key, charged[p.key].amount);
    }
  }
}

// Safety: the two refusals are only given for their reason. "Retry later" needs
// another attempt with the same key, since only that can hold the key in progress.
// "Rejected" needs a request with the same key and another amount, and is never the
// answer to a request for the amount the key was charged.
spec HonestRefusals observes Attempted, ProcessorCharged, ToldToRetry, Rejected {
  // key -> attempts made with it
  var attempts: map[int, int];
  var asked: map[int, set[int]];
  var chargedAmount: map[int, int];

  start state Watching {
    on Attempted do (p: (payment: int, key: int, amount: int)) {
      if (!(p.key in attempts)) {
        attempts[p.key] = 0;
        asked[p.key] = default(set[int]);
      }
      attempts[p.key] = attempts[p.key] + 1;
      asked[p.key] += (p.amount);
    }
    on ProcessorCharged do (p: (key: int, amount: int, result: ChargeResult)) {
      chargedAmount[p.key] = p.amount;
    }
    on ToldToRetry do (p: (payment: int, key: int)) {
      assert p.key in attempts && attempts[p.key] >= 2,
        format("payment {0} told to retry key {1}, but no other attempt with that key was made",
               p.payment, p.key);
    }
    on Rejected do (p: (payment: int, key: int, amount: int)) {
      assert p.key in asked && sizeof(asked[p.key]) >= 2,
        format("payment {0} rejected, but key {1} was never used with another amount", p.payment, p.key);
      if (p.key in chargedAmount) {
        assert chargedAmount[p.key] != p.amount,
          format("payment {0} rejected, but key {1} was charged the amount it asked for", p.payment, p.key);
      }
    }
  }
}

// Progress: every payment the driver plans gets a final answer, a result or a
// rejection. Hot while any is outstanding, so a client that is told to retry forever,
// or never answered, fails.
spec EveryPaymentAnswered observes PaymentsPlanned, Answered, Rejected {
  var planned: int;
  var settled: set[int];

  start state Idle {
    on PaymentsPlanned do (n: int) {
      assert n > 0, "planned no payments";
      planned = n;
      goto Waiting;
    }
    on Answered do { assert false, "answer before any payment was planned"; }
    on Rejected do { assert false, "rejection before any payment was planned"; }
  }

  hot state Waiting {
    on Answered do (p: (payment: int, key: int, amount: int, result: ChargeResult)) {
      settled += (p.payment);
      if (sizeof(settled) == planned) { goto Done; }
    }
    on Rejected do (p: (payment: int, key: int, amount: int)) {
      settled += (p.payment);
      if (sizeof(settled) == planned) { goto Done; }
    }
  }

  state Done {
    ignore Answered, Rejected;
  }
}
