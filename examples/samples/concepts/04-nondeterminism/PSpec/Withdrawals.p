// Contract: a withdrawal succeeds exactly when the balance covers it, and every reply
// reports the balance the history of successful withdrawals implies. "Unavailable"
// changes nothing. Every request gets an answer.

event eOpened: int;
event eWithdrawRequested: int;

spec WithdrawalsMatchBalance observes eOpened, eWithdrawRequested, eWithdrawResp, eUnavailable {
  var expected: int;
  var amount: int;

  start state Closed {
    on eOpened do (initial: int) {
      expected = initial;
      goto Idle;
    }
  }

  state Idle {
    on eWithdrawRequested do (a: int) {
      amount = a;
      goto Pending;
    }
  }

  hot state Pending {
    on eWithdrawResp do (r: (ok: bool, balance: int)) {
      if (amount <= expected) {
        assert r.ok, format("withdrawal of {0} refused although the balance was {1}", amount, expected);
        expected = expected - amount;
      } else {
        assert !r.ok, format("withdrawal of {0} paid out of a balance of {1}", amount, expected);
      }
      assert r.balance == expected, format("reply says balance {0}, expected {1}", r.balance, expected);
      goto Idle;
    }
    on eUnavailable goto Idle;
  }
}
