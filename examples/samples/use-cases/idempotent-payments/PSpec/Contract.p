// The idempotency contract, over what the card processor did and what each client
// received. The card processor and the clients are the environment (PTst), so their
// announcements are ground truth; the specs never look at the key table.

// The card processor ran a charge for `key`. Announced by the processor.
event ProcessorCharged: (key: int, amount: int, result: ChargeResult);
// A client sent one attempt of a payment. Announced by the client.
event Attempted: (key: int, amount: int);
// A client received an answer. Announced by the client, for every response it gets.
event Answered: (key: int, result: ChargeResult);
// The driver plans this many attempts in total.
event AttemptsPlanned: int;

// Safety: the processor charges each key at most once, with the requested amount, and
// every answer for a key, first or retried, concurrent or late, equals the result the
// processor gave that key, including a decline. Keys are independent: an answer for
// one key never carries another key's result, because each key's answer is checked
// against its own charge.
spec ChargedOnce observes Attempted, ProcessorCharged, Answered {
  // key -> amount the client asked for
  var amounts: map[int, int];
  // key -> what the processor decided
  var charged: map[int, ChargeResult];

  start state Watching {
    on Attempted do (p: (key: int, amount: int)) {
      if (p.key in amounts) {
        assert amounts[p.key] == p.amount, format("key {0} reused with another amount", p.key);
      }
      amounts[p.key] = p.amount;
    }
    on ProcessorCharged do (p: (key: int, amount: int, result: ChargeResult)) {
      assert p.key in amounts, format("key {0} charged but never requested", p.key);
      assert !(p.key in charged), format("key {0} charged twice", p.key);
      assert p.amount == amounts[p.key],
        format("key {0} charged {1}, but the client asked for {2}", p.key, p.amount, amounts[p.key]);
      charged[p.key] = p.result;
    }
    on Answered do (p: (key: int, result: ChargeResult)) {
      assert p.key in charged, format("key {0} answered {1} without any charge", p.key, p.result);
      assert p.result == charged[p.key],
        format("key {0} answered {1}, but its charge was {2}", p.key, p.result, charged[p.key]);
    }
  }
}

// Progress: every attempt the driver plans gets an answer for its own key. Hot while
// any is outstanding, so a request that waits forever on an in-progress key fails,
// even if some other key got an extra answer.
spec EveryAttemptAnswered observes AttemptsPlanned, Attempted, Answered {
  var unsent: int;
  // key -> attempts sent and not yet answered
  var outstanding: map[int, int];
  var unanswered: int;

  start state Idle {
    on AttemptsPlanned do (n: int) {
      assert n > 0, "planned no attempts";
      unanswered = n;
      unsent = n;
      goto Waiting;
    }
    on Attempted do { assert false, "attempt before any were planned"; }
    on Answered do { assert false, "answer before any attempt was planned"; }
  }

  hot state Waiting {
    on AttemptsPlanned do { assert false, "attempts planned twice"; }

    on Attempted do (p: (key: int, amount: int)) {
      assert unsent > 0, "more attempts than planned";
      unsent = unsent - 1;
      if (!(p.key in outstanding)) { outstanding[p.key] = 0; }
      outstanding[p.key] = outstanding[p.key] + 1;
    }

    on Answered do (p: (key: int, result: ChargeResult)) {
      assert p.key in outstanding && outstanding[p.key] > 0,
        format("key {0} got more answers than attempts", p.key);
      outstanding[p.key] = outstanding[p.key] - 1;
      unanswered = unanswered - 1;
      if (unanswered == 0) { goto Done; }
    }
  }

  state Done {
    on AttemptsPlanned do { assert false, "attempts planned twice"; }
    on Attempted do { assert false, "attempt after every attempt was answered"; }
    on Answered do { assert false, "answer after every attempt was answered"; }
  }
}
