// The contract after mutation testing. Each check is here because a mutant survived
// without it (see README.md). Replies are observed as they are sent, inside the
// account's atomic step, so the spec sees withdrawals in the order they committed.
spec WithdrawalsMatchBalance observes Opened, WithdrawRequested, WithdrawResponse {
  // The balance rebuilt from the history, independent of the account.
  var expected: int;
  // id -> amount, for each withdrawal still waiting for its answer
  var pending: map[int, int];
  var unanswered: int;

  start state Idle {
    on Opened goto Open with (o: (balance: int, withdrawals: int)) {
      assert o.withdrawals > 0, "planned no withdrawals";
      expected = o.balance;
      unanswered = o.withdrawals;
    }
  }

  // Hot until every withdrawal is answered: kills a removed reply.
  hot state Open {
    on WithdrawRequested do (r: (id: int, amount: int)) {
      assert !(r.id in pending), format("withdrawal {0} requested twice", r.id);
      pending[r.id] = r.amount;
    }
    on WithdrawResponse do (r: (id: int, ok: bool, balance: int)) {
      var amount: int;
      assert r.id in pending, format("withdrawal {0} answered, but it is not waiting for an answer", r.id);
      amount = pending[r.id];
      pending -= (r.id);
      // Accept exactly when the balance covers it: kills `<=` -> `<` and forced conditions.
      assert r.ok == (amount <= expected),
        format("withdrawal {0} of {1} on balance {2}: ok = {3}", r.id, amount, expected, r.ok);
      if (r.ok) {
        expected = expected - amount;
      }
      // The reported balance is the real one: kills a removed or wrong debit.
      assert r.balance == expected,
        format("withdrawal {0} reports balance {1}, expected {2}", r.id, r.balance, expected);
      unanswered = unanswered - 1;
      if (unanswered == 0) {
        goto AllAnswered;
      }
    }
  }

  state AllAnswered {
    on WithdrawRequested do (r: (id: int, amount: int)) {
      assert false, format("withdrawal {0} requested after every planned one was answered", r.id);
    }
  }
}
