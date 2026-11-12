// The contract after mutation testing. Each assertion is here because a mutant survived
// without it (see README.md).
spec Contract observes eOpened, eRequested, eWithdrawn, eAnswered {
  // The balance rebuilt from the history, independent of the account.
  var expected: int;
  // id -> amount asked for
  var requested: map[int, int];
  // id -> the decision committed for it
  var decided: map[int, bool];
  var unanswered: int;

  start state Idle {
    on eOpened goto Running with (o: (balance: int, withdrawals: int)) {
      assert o.withdrawals > 0, "planned no withdrawals";
      expected = o.balance;
      unanswered = o.withdrawals;
    }
    on eRequested do { assert false, "withdrawal before the account opened"; }
    on eWithdrawn do { assert false, "commit before the account opened"; }
    on eAnswered do { assert false, "answer before the account opened"; }
  }

  // Hot until every withdrawal is answered: kills "removed send eWithdrawResp".
  hot state Running {
    on eOpened do { assert false, "account opened twice"; }
    on eRequested do (r: (id: int, amount: int)) {
      assert !(r.id in requested), format("withdrawal {0} requested twice", r.id);
      requested[r.id] = r.amount;
    }
    on eWithdrawn do (w: (id: int, ok: bool, balance: int)) {
      assert w.id in requested && !(w.id in decided), format("withdrawal {0} committed twice or never requested", w.id);
      // Accept exactly when the balance covers it: kills `<=` -> `<` and forced conditions.
      assert w.ok == (requested[w.id] <= expected),
        format("withdrawal {0} of {1} on balance {2}: ok = {3}", w.id, requested[w.id], expected, w.ok);
      if (w.ok) { expected = expected - requested[w.id]; }
      // The reported balance is the real one: kills a removed or wrong debit.
      assert w.balance == expected, format("withdrawal {0} left balance {1}, expected {2}", w.id, w.balance, expected);
      decided[w.id] = w.ok;
    }
    on eAnswered do (a: (id: int, ok: bool)) {
      assert a.id in decided && a.ok == decided[a.id], format("withdrawal {0} answered ok = {1} against its commit", a.id, a.ok);
      unanswered = unanswered - 1;
      if (unanswered == 0) { goto Done; }
    }
  }

  state Done {
    on eOpened do { assert false, "account opened after the last answer"; }
    on eRequested do { assert false, "withdrawal after the last answer"; }
    on eWithdrawn do { assert false, "commit after the last answer"; }
    on eAnswered do { assert false, "answer after the last one"; }
  }
}
