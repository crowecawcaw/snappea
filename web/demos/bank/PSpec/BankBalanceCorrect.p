// Bank withdrawals: clients ask the server to debit accounts. The database owns the
// balances. The abstract-server test intentionally violates the minimum balance rule.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

// This file defines two P specifications BankBalanceIsAlwaysCorrect (safety property):
// BankBalanceIsAlwaysCorrect checks the global invariant that the account-balance
// communicated to the client by the bank is always correct and the bank never removes more
// money from the account than that withdrawn by the client! Also, if the bank denies a
// withdraw request then it is only because the withdrawal would reduce the account balance
// to below 10. GuaranteedWithDrawProgress (liveness property): GuaranteedWithDrawProgress
// checks the liveness (or progress) property that all withdraw requests submitted by the
// client are eventually responded. Note: stating that "BankBalanceIsAlwaysCorrect checks
// that if the bank denies a withdraw request then the request would reduce the balance to
// below 10 (< 10)" is equivalent to state that "if there is enough money in the account -
// at least 10 (>= 10), then the request must not error". Hence, the two properties
// BankBalanceIsAlwaysCorrect and GuaranteedWithDrawProgress together ensure that every
// withdraw request if allowed will eventually succeed and the bank cannot block correct
// withdrawal requests.

// event: initialize the monitor with the initial account balances for all clients when the
// system starts
event Spec_BankBalanceIsAlwaysCorrect_Init: map[int, int];

// BankBalanceIsAlwaysCorrect checks the global invariant that the account balance
// communicated to the client by the bank is always correct and there is no error on the
// banks side with the implementation of the withdraw logic. For checking this property the
// spec machine observes the withdraw request (WithdrawRequest) and response
// (WithdrawResponse). - On receiving the WithdrawRequest, it adds the request in the
// pending-withdraws-map so that on receiving a response for this withdraw we can assert
// that the amount of money deducted from the account is same as what was requested by the
// client. - On receiving the WithdrawResponse, we look up the corresponding withdraw
// request and check that: the new account balance is correct and if the withdraw failed it
// is because the withdraw will make the account balance go below 10 dollars which is
// against the bank policies!
spec BankBalanceIsAlwaysCorrect observes WithdrawRequest,  WithdrawResponse, Spec_BankBalanceIsAlwaysCorrect_Init {
  // keep track of the bank balance for each client: map from accountId to bank balance.
  var bankBalance: map[int, int];
  // keep track of the pending withdraw requests that have not been responded yet.
  // map from reqId -> withdraw request
  var pendingWithDraws: map[int, WithdrawRequestData];

  start state Init {
    on Spec_BankBalanceIsAlwaysCorrect_Init goto WaitForWithDrawReqAndResp with (balance: map[int, int]){
      bankBalance = balance;
    }
  }

  state WaitForWithDrawReqAndResp {
    on WithdrawRequest do (request: WithdrawRequestData) {
      assert request.accountId in bankBalance,
      format ("Unknown accountId {0} in the withdraw request. Valid accountIds = {1}",
      request.accountId, keys(bankBalance));
      pendingWithDraws[request.requestId] = request;
    }

    on WithdrawResponse do (response: WithdrawResponseData) {
      assert response.accountId in bankBalance,
      format ("Unknown accountId {0} in the withdraw response!", response.accountId);
      assert response.requestId in pendingWithDraws,
      format ("Unknown requestId {0} in the withdraw response!", response.requestId);
      assert response.balance >= 10,
      "Bank balance in all accounts must always be greater than or equal to 10!!";

      if(response.status == WITHDRAW_SUCCESS) {
        assert response.balance == bankBalance[response.accountId] - pendingWithDraws[response.requestId].amount,
        format ("Bank balance for the account {0} is {1} and not the expected value {2}, Bank is lying!",
        response.accountId, response.balance, bankBalance[response.accountId] - pendingWithDraws[response.requestId].amount);
        // update the new account balance
        bankBalance[response.accountId] = response.balance;
      }

      else {
        // bank can only reject a request if it will drop the balance below 10
        assert bankBalance[response.accountId] - pendingWithDraws[response.requestId].amount < 10,
        format ("Bank must accept the withdraw request for {0}, bank balance is {1}!",
        pendingWithDraws[response.requestId].amount, bankBalance[response.accountId]);
        // if withdraw failed then the account balance must remain the same
        assert bankBalance[response.accountId] == response.balance,
        format ("Withdraw failed BUT the account balance changed! actual: {0}, bank said: {1}",
        bankBalance[response.accountId], response.balance);
      }
    }
  }
}

// GuaranteedWithDrawProgress checks the liveness (or progress) property that all withdraw
// requests submitted by the client are eventually responded.
// snappea-lint: allow unused-payload-field -- progress only needs request ids; the safety spec checks the rest
spec GuaranteedWithDrawProgress observes WithdrawRequest, WithdrawResponse {
  // keep track of the pending withdraw requests
  var pendingWithdrawals: set[int];

  start state NopendingRequests {
    on WithdrawRequest goto PendingReqs with (request: WithdrawRequestData) {
      pendingWithdrawals += (request.requestId);
    }
  }

  hot state PendingReqs {
    on WithdrawResponse do (response: WithdrawResponseData) {
      assert response.requestId in pendingWithdrawals,
      format ("unexpected requestId: {0} received, expected one of {1}", response.requestId, pendingWithdrawals);
      pendingWithdrawals -= (response.requestId);
      if(sizeof(pendingWithdrawals) == 0) // all requests have been responded
      goto NopendingRequests;
    }

    on WithdrawRequest goto PendingReqs with (request: WithdrawRequestData){
      pendingWithdrawals += (request.requestId);
    }
  }
}
