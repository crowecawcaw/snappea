// Bank withdrawals: clients ask the server to debit accounts. The database owns the
// balances. The abstract-server test intentionally violates the minimum balance rule.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

/** Events used to communicate between the bank server and the backend database **/
// event: send update the database, i.e. the `balance` associated with the `accountId`
event UpdateQuery: (accountId: int, balance: int);
// event: send a read request for the `accountId`.
event ReadQuery: (accountId: int);
// event: send a response (`balance`) corresponding to the read request for an `accountId`
event ReadQueryResponse: (accountId: int, balance: int);

// The BankServer machine uses a database machine as a service to store the bank balance
// for all its clients. On receiving an WithdrawRequest (withdraw requests) from a client,
// it reads the current balance for the account, if there is enough money in the account
// then it updates the new balance in the database after withdrawal and sends a response
// back to the client.
machine BankServer {
  var database: Database;

  start state Init {
    entry (initialBalance: map[int, int]){
      database = new Database((server = this, initialBalance = initialBalance));
      goto WaitForWithdrawRequests;
    }
  }

  state WaitForWithdrawRequests {
    on WithdrawRequest do (withdrawalRequest: WithdrawRequestData) {
      var currentBalance: int;
      var response: WithdrawResponseData;

      // read the current account balance from the database
      currentBalance = ReadBankBalance(database, withdrawalRequest.accountId);
      // if there is enough money in account after withdrawal
      if(currentBalance - withdrawalRequest.amount >= 10) {
        UpdateBankBalance(database, withdrawalRequest.accountId, currentBalance - withdrawalRequest.amount);
        response = (status = WITHDRAW_SUCCESS, accountId = withdrawalRequest.accountId, balance = currentBalance - withdrawalRequest.amount, requestId = withdrawalRequest.requestId);
      }

      else // not enough money after withdraw
      {
        response = (status = WITHDRAW_ERROR, accountId = withdrawalRequest.accountId, balance = currentBalance, requestId = withdrawalRequest.requestId);
      }

      // send response to the client
      send withdrawalRequest.source, WithdrawResponse, response;
    }
  }
}

// The Database machine acts as a helper service for the Bank server and stores the bank
// balance for each account. There are two API's or functions to interact with the
// Database: ReadBankBalance and UpdateBankBalance.
machine Database {
  var server: BankServer;
  var balance: map[int, int];
  start state Init {
    entry(input: (server : BankServer, initialBalance: map[int, int])){
      server = input.server;
      balance = input.initialBalance;
    }

    on UpdateQuery do (query: (accountId: int, balance: int)) {
      assert query.accountId in balance, "Invalid accountId received in the update query!";
      balance[query.accountId] = query.balance;
    }

    on ReadQuery do (query: (accountId: int))
    {
      assert query.accountId in balance, "Invalid accountId received in the read query!";
      send server, ReadQueryResponse, (accountId = query.accountId, balance = balance[query.accountId]);
    }
  }
}

// Function to read the bank balance corresponding to the accountId
fun ReadBankBalance(database: Database, accountId: int) : int {
  var currentBalance: int;
  send database, ReadQuery, (accountId = accountId,);
  receive {
    case ReadQueryResponse: (response: (accountId: int, balance: int)) {
      currentBalance = response.balance;
    }
  }

  return currentBalance;
}

// Function to update the account balance for the account Id
fun UpdateBankBalance(database: Database, accountId: int, balance: int)
{
  send database, UpdateQuery, (accountId = accountId, balance = balance);
}
