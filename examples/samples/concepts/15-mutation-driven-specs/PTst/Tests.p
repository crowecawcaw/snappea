// A client asks for one withdrawal and waits for the answer.
machine Client {
  start state Asking {
    entry (input: (account: Account, id: int, amount: int)) {
      announce WithdrawRequested, (id = input.id, amount = input.amount);
      send input.account, Withdraw, (client = this, id = input.id, amount = input.amount);
    }
    on WithdrawResponse goto Answered;
  }

  state Answered {}
}

// Two clients withdraw from a balance of 10 at once. Amounts include 0 (the fast
// path), an amount both can have (4), one only the first can have (6 after 4), and
// exactly the balance (10).
machine TwoWithdrawals {
  start state Init {
    entry {
      var account: Account;
      var amounts: seq[int];
      amounts += (0, 0);
      amounts += (1, 4);
      amounts += (2, 6);
      amounts += (3, 10);
      announce Opened, (balance = 10, withdrawals = 2);
      account = new Account(10);
      new Client((account = account, id = 1, amount = amounts[choose(4)]));
      new Client((account = account, id = 2, amount = amounts[choose(4)]));
    }
  }
}

test tcTwoWithdrawals [main=TwoWithdrawals]:
  assert WithdrawalsMatchBalance in { TwoWithdrawals, Account, Client };
