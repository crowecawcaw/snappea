// Bank withdrawals: clients ask the server to debit accounts. The database owns the
// balances. The abstract-server test intentionally violates the minimum balance rule.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// The AbstractBankServer provides an abstract implementation of the BankServer where it
// abstract away the interaction between the BankServer and Database. The
// AbstractBankServer machine is used to demonstrate how one can replace a complex
// component in P with its abstraction that hides a lot of its internal complexity. In this
// case, instead of storing the balance in a separate database the abstraction store the
// information locally and abstracts away the complexity of bank server interaction with
// the database. For the client, it still exposes the same interface/behavior. Hence, when
// checking the correctness of the client it doesnt matter whether we use BankServer or the
// AbstractBankServer

machine AbstractBankServer {
  // account balance: map from account-id to balance
  var balance: map[int, int];
  start state WaitForWithdrawRequests {
    entry (init_balance: map[int, int])
    {
      balance = init_balance;
    }

    on WithdrawRequest do (withdrawalRequest: WithdrawRequestData) {
      assert withdrawalRequest.accountId in balance, "Invalid accountId received in the withdraw request!";
      if(balance[withdrawalRequest.accountId] - withdrawalRequest.amount > 10) /* hint: bug */
      {
        balance[withdrawalRequest.accountId] = balance[withdrawalRequest.accountId] - withdrawalRequest.amount;
        send withdrawalRequest.source, WithdrawResponse,
        (status = WITHDRAW_SUCCESS, accountId = withdrawalRequest.accountId, balance = balance[withdrawalRequest.accountId], requestId = withdrawalRequest.requestId);
      }

      else {
        send withdrawalRequest.source, WithdrawResponse,
        (status = WITHDRAW_ERROR, accountId = withdrawalRequest.accountId, balance = balance[withdrawalRequest.accountId], requestId = withdrawalRequest.requestId);
      }
    }
  }
}
