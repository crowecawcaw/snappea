// Contract: a withdrawal succeeds exactly when the balance covers it, and every reply
// reports the balance the history of successful withdrawals implies. "Unavailable"
// changes nothing. Every request gets an answer.

// Announced by the driver before it starts: the opening balance and how many
// withdrawals follow.
event Opened: (balance: int, withdrawals: int);
// Announced by the driver as it asks.
event WithdrawRequested: (id: int, amount: int);

// Replies are observed as they are sent, inside the account's atomic step, so the spec
// sees withdrawals in the order they committed.
spec WithdrawalsMatchBalance observes Opened, WithdrawRequested, WithdrawResponse, Unavailable {
  // The balance rebuilt from the history, independent of the account.
  var expected: int;
  // id -> amount, for each withdrawal still waiting for its answer
  var pending: map[int, int];
  var unanswered: int;

  start state Idle {
    on Opened goto Open with (o: (balance: int, withdrawals: int)) {
      expected = o.balance;
      unanswered = o.withdrawals;
    }
  }

  // Hot until every planned withdrawal is answered.
  hot state Open {
    on WithdrawRequested do (r: (id: int, amount: int)) {
      assert !(r.id in pending), format("withdrawal {0} requested twice", r.id);
      pending[r.id] = r.amount;
    }
    on WithdrawResponse do (r: (id: int, ok: bool, balance: int)) {
      var amount: int;
      amount = Answer(r.id);
      if (amount <= expected) {
        assert r.ok, format("withdrawal {0} of {1} refused although the balance was {2}", r.id, amount, expected);
        expected = expected - amount;
      } else {
        assert !r.ok, format("withdrawal {0} of {1} paid out of a balance of {2}", r.id, amount, expected);
      }
      assert r.balance == expected,
        format("withdrawal {0} reports balance {1}, expected {2}", r.id, r.balance, expected);
      Settle();
    }
    on Unavailable do (u: (id: int)) {
      Answer(u.id);
      Settle();
    }
  }

  state AllAnswered {
    on WithdrawRequested do (r: (id: int, amount: int)) {
      assert false, format("withdrawal {0} requested after every planned one was answered", r.id);
    }
  }

  // Marks a withdrawal answered and returns its amount.
  fun Answer(id: int): int {
    var amount: int;
    assert id in pending, format("withdrawal {0} answered, but it is not waiting for an answer", id);
    amount = pending[id];
    pending -= (id);
    unanswered = unanswered - 1;
    return amount;
  }

  fun Settle() {
    if (unanswered == 0) {
      goto AllAnswered;
    }
  }
}
