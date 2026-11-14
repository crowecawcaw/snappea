// Driver: open an account with 2, then withdraw twice. Each amount is a choice from 0 to
// 3, so the checker tries below, exactly at and above the balance, in both orders.
machine TwoWithdrawals {
  var account: Account;
  var made: int;

  start state Init {
    entry {
      announce Opened, 2;
      account = new Account(2);
      goto Withdrawing;
    }
  }

  state Withdrawing {
    entry {
      var amount: int;
      amount = choose(4);
      made = made + 1;
      announce WithdrawRequested, amount;
      send account, Withdraw, (client = this, amount = amount);
    }
    on WithdrawResponse do { Next(); }
    on Unavailable do { Next(); }
  }

  state Done {}

  fun Next() {
    if (made < 2) {
      goto Withdrawing;
    }
    goto Done;
  }
}

test tcTwoWithdrawals [main=TwoWithdrawals]:
  assert WithdrawalsMatchBalance in { TwoWithdrawals, Account };
