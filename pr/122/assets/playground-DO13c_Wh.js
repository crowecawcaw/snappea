import"./styles-DmAVEXEO.js";import{t as e}from"./react-C00JmANc.js";import{t}from"./client-Cm8CV9ie.js";import{a as n,c as r,o as i,r as a,s as o,t as s}from"./pTokens-iJb68ucT.js";import{t as c}from"./jsx-runtime-sgiCDx98.js";var l=e(),u=t(),d=Object.assign({"../../design/time/RetryBudget/PSpec/Budget.p":`// The budget is an ordinary spec. In a spec handler, clocks are those of the
// machine that announced/sent the observed event, so Elapsed(o.started) is
// measured on the client's clock.
spec ResolvesWithinDeadline observes eOutcome {
  start state Watching {
    on eOutcome do (o: (ok: bool, started: int)) {
      assert Elapsed(o.started) <= ms(1000),
        format("resolved (ok = {0}) after up to {1}us; deadline is 1000000us",
               o.ok, Elapsed(o.started));
    }
  }
}
`,"../../design/time/RetryBudget/PSrc/RetryClient.p":`/*****************************************************************************
 * A client calls a flaky server with 3 attempts, a 300ms per-attempt timeout
 * and 50ms / 100ms backoff, and must resolve (success or error) within its
 * caller's 1s deadline. No numbers in this file are delivery latencies; those
 * live in PTst/latency.toml.
 *****************************************************************************/

type tReq  = (client: Client, attempt: int);

event eReq  : tReq;
event eResp : int;          // attempt
event eTimeout : int;       // attempt
event eBackoffDone : int;
event eOutcome : (ok: bool, started: int);   // announced to specs

machine Server {
  start state Serving {
    on eReq do (r: tReq) {
      if ($) { return; }                    // dropped
      Elapse(ms(8), ms(36));
      send r.client, eResp, r.attempt;
    }
  }
}

machine Client {
  var server: Server;
  var attempt: int;
  var started: int;
  var timeoutAfter: int;
  var backoff: seq[int];

  start state Init {
    entry (s: Server) {
      server = s;
      timeoutAfter = ms(300);
      backoff += (0, ms(50));
      backoff += (1, ms(100));
      started = Mark();
      goto Trying;
    }
  }

  state Trying {
    entry {
      attempt = attempt + 1;
      send server, eReq, (client = this, attempt = attempt);
      StartTimer(this, eTimeout, attempt, timeoutAfter);
    }
    on eResp do (a: int) {
      if (a == attempt) { announce eOutcome, (ok = true, started = started); goto Done; }
    }
    on eTimeout do (a: int) {
      if (a != attempt) { return; }
      if (attempt == 3) { announce eOutcome, (ok = false, started = started); goto Done; }
      StartTimer(this, eBackoffDone, attempt, backoff[attempt - 1]);
      goto BackingOff;
    }
  }

  state BackingOff {
    on eBackoffDone goto Trying;
    ignore eResp, eTimeout;
  }

  state Done {
    ignore eResp, eTimeout, eBackoffDone;
  }
}
`,"../../design/time/RetryBudget/PSrc/Time.p":`/*****************************************************************************
 * The entire time "extension": plain P foreign functions. Times are int
 * microseconds. The checker implements them natively (causal clocks, see
 * ../../PROPOSAL.md).
 *****************************************************************************/

// The machine's local clock: worst / best case. Use for absolute deadlines.
fun Now(): int;
fun NowBest(): int;

// A stopwatch. Mark() captures the current clock; Elapsed(m) is the worst
// (ElapsedBest the best) case of now - mark, computed with the correlation
// kept, so shared history cancels. Use these for budgets, not Now() - t0.
fun Mark(): int;
fun Elapsed(m: int): int;
fun ElapsedBest(m: int): int;

// Local work taking between lo and hi microseconds.
fun Elapse(lo: int, hi: int);

fun ms(n: int): int { return n * 1000; }
`,"../../design/time/RetryBudget/PSrc/Timer.p":`/*****************************************************************************
 * A one-shot timer, one machine per armed timer.
 *
 * It needs no time semantics of its own: the new machine starts at its
 * creator's clock, Elapse advances that clock by \`after\`, so the timeout it
 * sends is stamped creation + after.
 *   untimed:                   the timeout may be delivered before replies
 *                              stamped earlier, as with today's $-timers.
 *   --timed:                   events certainly arriving earlier are handled first.
 * Cancelling is done by the holder ignoring stale tags, which is simpler and
 * race-free compared to a cancel message.
 *****************************************************************************/

type tTimer = (holder: machine, ev: event, tag: int, after: int);

machine Timer {
  start state Arming {
    entry (t: tTimer) {
      Elapse(t.after, t.after);
      send t.holder, t.ev, t.tag;
    }
  }
}

fun StartTimer(holder: machine, ev: event, tag: int, after: int) {
  new Timer((holder = holder, ev = ev, tag = tag, after = after));
}
`,"../../design/time/RetryBudget/PTst/Test.p":`machine Main {
  start state Init { entry { new Client(new Server()); } }
}

test tcRetry [main=Main]: assert ResolvesWithinDeadline in { Main, Client, Server, Timer };
`,"../../design/time/RetryBudget/PTst/latency.toml":`# Delivery latencies (\`snappea check --latency PTst/latency.toml\`).
# Ranges are "lo..hi" in us/ms/s.

[profile.same-az]
default = "0..0"            # anything unlisted, e.g. Timer -> holder
eReq    = "1ms..2ms"
eResp   = "1ms..2ms"

[profile.cross-region]
extends = "same-az"
eReq    = "60ms..90ms"
eResp   = "60ms..90ms"

# Which profile each test case uses unless \`--latency <file>:<profile>\` names one.
[tests]
tcRetry = "same-az"
`,"../../tests/upstream/Tutorial/1_ClientServer/PSpec/BankBalanceCorrect.p":`/*****************************************************
This file defines two P specifications

BankBalanceIsAlwaysCorrect (safety property):
BankBalanceIsAlwaysCorrect checks the global invariant that the account-balance communicated
to the client by the bank is always correct and the bank never removes more money from the account
than that withdrawn by the client! Also, if the bank denies a withdraw request then it is only because
the withdrawal would reduce the account balance to below 10.

GuaranteedWithDrawProgress (liveness property):
GuaranteedWithDrawProgress checks the liveness (or progress) property that all withdraw requests
submitted by the client are eventually responded.

Note: stating that "BankBalanceIsAlwaysCorrect checks that if the bank denies a withdraw request
then the request would reduce the balance to below 10 (< 10)" is equivalent to state that "if there is enough money in the account - at least 10 (>= 10), then the request must not error".
Hence, the two properties BankBalanceIsAlwaysCorrect and GuaranteedWithDrawProgress together ensure that every withdraw request if allowed will eventually succeed and the bank cannot block correct withdrawal requests.
*****************************************************/

// event: initialize the monitor with the initial account balances for all clients when the system starts
event eSpec_BankBalanceIsAlwaysCorrect_Init: map[int, int];

/****************************************************
BankBalanceIsAlwaysCorrect checks the global invariant that the account balance communicated
to the client by the bank is always correct and there is no error on the banks side with the
implementation of the withdraw logic.

For checking this property the spec machine observes the withdraw request (eWithDrawReq) and response (eWithDrawResp).
- On receiving the eWithDrawReq, it adds the request in the pending-withdraws-map so that on receiving a
response for this withdraw we can assert that the amount of money deducted from the account is same as
what was requested by the client.

- On receiving the eWithDrawResp, we look up the corresponding withdraw request and check that: the
new account balance is correct and if the withdraw failed it is because the withdraw will make the account
balance go below 10 dollars which is against the bank policies!
****************************************************/
spec BankBalanceIsAlwaysCorrect observes eWithDrawReq,  eWithDrawResp, eSpec_BankBalanceIsAlwaysCorrect_Init {
  // keep track of the bank balance for each client: map from accountId to bank balance.
  var bankBalance: map[int, int];
  // keep track of the pending withdraw requests that have not been responded yet.
  // map from reqId -> withdraw request
  var pendingWithDraws: map[int, tWithDrawReq];

  start state Init {
    on eSpec_BankBalanceIsAlwaysCorrect_Init goto WaitForWithDrawReqAndResp with (balance: map[int, int]){
      bankBalance = balance;
    }
  }

  state WaitForWithDrawReqAndResp {
    on eWithDrawReq do (req: tWithDrawReq) {
      assert req.accountId in bankBalance,
        format ("Unknown accountId {0} in the withdraw request. Valid accountIds = {1}",
          req.accountId, keys(bankBalance));
      pendingWithDraws[req.rId] = req;
    }

    on eWithDrawResp do (resp: tWithDrawResp) {
      assert resp.accountId in bankBalance,
        format ("Unknown accountId {0} in the withdraw response!", resp.accountId);
      assert resp.rId in pendingWithDraws,
        format ("Unknown rId {0} in the withdraw response!", resp.rId);
      assert resp.balance >= 10,
        "Bank balance in all accounts must always be greater than or equal to 10!!";

      if(resp.status == WITHDRAW_SUCCESS)
      {
        assert resp.balance == bankBalance[resp.accountId] - pendingWithDraws[resp.rId].amount,
          format ("Bank balance for the account {0} is {1} and not the expected value {2}, Bank is lying!",
            resp.accountId, resp.balance, bankBalance[resp.accountId] - pendingWithDraws[resp.rId].amount);
        // update the new account balance
        bankBalance[resp.accountId] = resp.balance;
      }
      else
      {
        // bank can only reject a request if it will drop the balance below 10
        assert bankBalance[resp.accountId] - pendingWithDraws[resp.rId].amount < 10,
          format ("Bank must accept the withdraw request for {0}, bank balance is {1}!",
            pendingWithDraws[resp.rId].amount, bankBalance[resp.accountId]);
        // if withdraw failed then the account balance must remain the same
        assert bankBalance[resp.accountId] == resp.balance,
          format ("Withdraw failed BUT the account balance changed! actual: {0}, bank said: {1}",
            bankBalance[resp.accountId], resp.balance);
      }
    }
  }
}

/**************************************************************************
GuaranteedWithDrawProgress checks the liveness (or progress) property that all withdraw requests
submitted by the client are eventually responded.
***************************************************************************/
spec GuaranteedWithDrawProgress observes eWithDrawReq, eWithDrawResp {
  // keep track of the pending withdraw requests
  var pendingWDReqs: set[int];

  start state NopendingRequests {
    on eWithDrawReq goto PendingReqs with (req: tWithDrawReq) {
      pendingWDReqs += (req.rId);
    }
  }

  hot state PendingReqs {
    on eWithDrawResp do (resp: tWithDrawResp) {
      assert resp.rId in pendingWDReqs,
        format ("unexpected rId: {0} received, expected one of {1}", resp.rId, pendingWDReqs);
      pendingWDReqs -= (resp.rId);
      if(sizeof(pendingWDReqs) == 0) // all requests have been responded
        goto NopendingRequests;
    }

    on eWithDrawReq goto PendingReqs with (req: tWithDrawReq){
      pendingWDReqs += (req.rId);
    }
  }
}
`,"../../tests/upstream/Tutorial/1_ClientServer/PSrc/AbstractBankServer.p":`/*********************************************************
The AbstractBankServer provides an abstract implementation of the BankServer where it abstract away
the interaction between the BankServer and Database.
The AbstractBankServer machine is used to demonstrate how one can replace a complex component in P
with its abstraction that hides a lot of its internal complexity.
In this case, instead of storing the balance in a separate database the abstraction store the information
locally and abstracts away the complexity of bank server interaction with the database.
For the client, it still exposes the same interface/behavior. Hence, when checking the correctness
of the client it doesnt matter whether we use BankServer or the AbstractBankServer
**********************************************************/

machine AbstractBankServer
{
  // account balance: map from account-id to balance
  var balance: map[int, int];
  start state WaitForWithdrawRequests {
    entry (init_balance: map[int, int])
    {
      balance = init_balance;
    }

    on eWithDrawReq do (wReq: tWithDrawReq) {
      assert wReq.accountId in balance, "Invalid accountId received in the withdraw request!";
      if(balance[wReq.accountId] - wReq.amount > 10) /* hint: bug */
      {
        balance[wReq.accountId] = balance[wReq.accountId] - wReq.amount;
        send wReq.source, eWithDrawResp,
          (status = WITHDRAW_SUCCESS, accountId = wReq.accountId, balance = balance[wReq.accountId], rId = wReq.rId);
      }
      else
      {
        send wReq.source, eWithDrawResp,
          (status = WITHDRAW_ERROR, accountId = wReq.accountId, balance = balance[wReq.accountId], rId = wReq.rId);
      }
    }
  }
}`,"../../tests/upstream/Tutorial/1_ClientServer/PSrc/Client.p":`/* User Defined Types */

// payload type associated with the eWithDrawReq, where \`source\`: client sending the withdraw request,
// \`accountId\`: account to withdraw from, \`amount\`: amount to withdraw, and \`rId\`: unique
// request Id associated with each request.
type tWithDrawReq = (source: Client, accountId: int, amount: int, rId:int);

// payload type associated with the eWithDrawResp, where \`status\`: response status (below),
// \`accountId\`: account withdrawn from, \`balance\`: bank balance after withdrawal, and
// \`rId\`: request id for which this is the response.
type tWithDrawResp = (status: tWithDrawRespStatus, accountId: int, balance: int, rId: int);

// enum representing the response status for the withdraw request
enum tWithDrawRespStatus {
  WITHDRAW_SUCCESS,
  WITHDRAW_ERROR
}

// event: withdraw request (from client to bank server)
event eWithDrawReq : tWithDrawReq;
// event: withdraw response (from bank server to client)
event eWithDrawResp: tWithDrawResp;


machine Client
{
  var server : BankServer;
  var accountId: int;
  var nextReqId : int;
  var numOfWithdrawOps: int;
  var currentBalance: int;

  start state Init {

    entry (input : (serv : BankServer, accountId: int, balance : int))
    {
      server = input.serv;
      currentBalance =  input.balance;
      accountId = input.accountId;
      // hacky: we would like request id's to be unique across all requests from clients
      nextReqId = accountId*100 + 1; // each client has a unique account id
      goto WithdrawMoney;
    }
  }

  state WithdrawMoney {
    entry {
      // If current balance is <= 10 then we need more deposits before any more withdrawal
      if(currentBalance <= 10)
        goto NoMoneyToWithDraw;

      // send withdraw request to the bank for a random amount between (1 to current balance + 1)
      send server, eWithDrawReq, (source = this, accountId = accountId, amount = WithdrawAmount(), rId = nextReqId);
      nextReqId = nextReqId + 1;
    }

    on eWithDrawResp do (resp: tWithDrawResp) {
      // bank always ensures that a client has atleast 10 dollars in the account
      assert resp.balance >= 10, "Bank balance must be greater than 10!!";
      if(resp.status == WITHDRAW_SUCCESS) // withdraw succeeded
      {
        print format ("Withdrawal with rId = {0} succeeded, new account balance = {1}", resp.rId, resp.balance);
        currentBalance = resp.balance;
      }
      else // withdraw failed
      {
        // if withdraw failed then the account balance must remain the same
        assert currentBalance == resp.balance,
          format ("Withdraw failed BUT the account balance changed! client thinks: {0}, bank balance: {1}", currentBalance, resp.balance);
        print format ("Withdrawal with rId = {0} failed, account balance = {1}", resp.rId, resp.balance);
      }

      if (currentBalance > 10)
/* Hint 3: Reduce the number of times WithdrawAmount() is called by changing the above line to the following:
      if (currentBalance > 10 && nextReqId < (accountId*100 + 5))
*/
      {
        print format ("Still have account balance = {0}, lets try and withdraw more", currentBalance);
        goto WithdrawMoney;
      }
    }
  }

  // function that returns a random integer between (1 to current balance + 1)
  fun WithdrawAmount() : int {
    return choose(currentBalance) + 1;
/* Hint 2: Reduce the number of choices by changing the above line to the following:
    return ((choose(5) * currentBalance) / 4) + 1;
*/
  }

  state NoMoneyToWithDraw {
    entry {
      // if I am here then the amount of money in my account should be exactly 10
      assert currentBalance == 10, "Hmm, I still have money that I can withdraw but I have reached NoMoneyToWithDraw state!";
      print format ("No Money to withdraw, waiting for more deposits!");
    }
  }
}
`,"../../tests/upstream/Tutorial/1_ClientServer/PSrc/ClientServerModules.p":`// Client module
module Client = { Client };

// Bank module
module Bank = { BankServer, Database };

// Abstract Bank Server module
module AbstractBank = { AbstractBankServer -> BankServer };`,"../../tests/upstream/Tutorial/1_ClientServer/PSrc/Server.p":`/** Events used to communicate between the bank server and the backend database **/
// event: send update the database, i.e. the \`balance\` associated with the \`accountId\`
event eUpdateQuery: (accountId: int, balance: int);
// event: send a read request for the \`accountId\`.
event eReadQuery: (accountId: int);
// event: send a response (\`balance\`) corresponding to the read request for an \`accountId\`
event eReadQueryResp: (accountId: int, balance: int);

/*************************************************************
The BankServer machine uses a database machine as a service to store the bank balance for all its clients.
On receiving an eWithDrawReq (withdraw requests) from a client, it reads the current balance for the account,
if there is enough money in the account then it updates the new balance in the database after withdrawal
and sends a response back to the client.
*************************************************************/
machine BankServer
{
  var database: Database;

  start state Init {
    entry (initialBalance: map[int, int]){
      database = new Database((server = this, initialBalance = initialBalance));
      goto WaitForWithdrawRequests;
    }
  }

  state WaitForWithdrawRequests {
    on eWithDrawReq do (wReq: tWithDrawReq) {
      var currentBalance: int;
      var response: tWithDrawResp;

      // read the current account balance from the database
      currentBalance = ReadBankBalance(database, wReq.accountId);
      // if there is enough money in account after withdrawal
      if(currentBalance - wReq.amount >= 10)
      {
        UpdateBankBalance(database, wReq.accountId, currentBalance - wReq.amount);
        response = (status = WITHDRAW_SUCCESS, accountId = wReq.accountId, balance = currentBalance - wReq.amount, rId = wReq.rId);
      }
      else // not enough money after withdraw
      {
        response = (status = WITHDRAW_ERROR, accountId = wReq.accountId, balance = currentBalance, rId = wReq.rId);
      }

      // send response to the client
      send wReq.source, eWithDrawResp, response;
    }
  }
}

/***************************************************************
The Database machine acts as a helper service for the Bank server and stores the bank balance for
each account. There are two API's or functions to interact with the Database:
ReadBankBalance and UpdateBankBalance.
****************************************************************/
machine Database
{
  var server: BankServer;
  var balance: map[int, int];
  start state Init {
    entry(input: (server : BankServer, initialBalance: map[int, int])){
      server = input.server;
      balance = input.initialBalance;
    }
    on eUpdateQuery do (query: (accountId: int, balance: int)) {
      assert query.accountId in balance, "Invalid accountId received in the update query!";
      balance[query.accountId] = query.balance;
    }
    on eReadQuery do (query: (accountId: int))
    {
      assert query.accountId in balance, "Invalid accountId received in the read query!";
      send server, eReadQueryResp, (accountId = query.accountId, balance = balance[query.accountId]);
    }
  }
}

// Function to read the bank balance corresponding to the accountId
fun ReadBankBalance(database: Database, accountId: int) : int {
    var currentBalance: int;
    send database, eReadQuery, (accountId = accountId,);
    receive {
      case eReadQueryResp: (resp: (accountId: int, balance: int)) {
        currentBalance = resp.balance;
      }
    }
    return currentBalance;
}

// Function to update the account balance for the account Id
fun UpdateBankBalance(database: Database, accId: int, bal: int)
{
  send database, eUpdateQuery, (accountId = accId, balance = bal);
}`,"../../tests/upstream/Tutorial/1_ClientServer/PTst/TestDriver.p":`
// Test driver that checks the system with a single Client.
machine TestWithSingleClient
{
  start state Init {
    entry {
      // since client
      SetupClientServerSystem(1);
    }
  }
}

// Test driver that checks the system with multiple Clients.
machine TestWithMultipleClients
{
  start state Init {
    entry {
      // multiple clients between (2, 4)
      SetupClientServerSystem(choose(3) + 2);
    }
  }
}

param nClients: int;

machine TestWithConfig {
  start state Init {
    entry {
      SetupClientServerSystem(nClients);
    }
  }
}

// creates a random map from accountId's to account balance of size \`numAccounts\`
fun CreateRandomInitialAccounts(numAccounts: int) : map[int, int]
{
  var i: int;
  var bankBalance: map[int, int];
  while(i < numAccounts) {
    bankBalance[i] = choose(100) + 10; // min 10 in the account
/* Hint 1: Reduce the number of choices by changing the above line to the following:
    // bankBalance[i] = choose(10) + 10; // min 10 in the account
*/
    i = i + 1;
  }
  return bankBalance;
}

// setup the client server system with one bank server and \`numClients\` clients.
fun SetupClientServerSystem(numClients: int)
{
  var i: int;
  var server: BankServer;
  var accountIds: seq[int];
  var initAccBalance: map[int, int];

  // randomly initialize the account balance for all clients
  initAccBalance = CreateRandomInitialAccounts(numClients);
  // create bank server with the init account balance
  server = new BankServer(initAccBalance);

  // before client starts sending any messages make sure we
  // initialize the monitors or specifications
  announce eSpec_BankBalanceIsAlwaysCorrect_Init, initAccBalance;

  accountIds = keys(initAccBalance);

  // create the clients
  while(i < sizeof(accountIds)) {
    new Client((serv = server, accountId = accountIds[i], balance = initAccBalance[accountIds[i]]));
    i = i + 1;
  }
}`,"../../tests/upstream/Tutorial/1_ClientServer/PTst/Testscript.p":`/* This file contains three different model checking scenarios */

// assert the properties for the single client and single server scenario
test tcSingleClient [main=TestWithSingleClient]:
  assert BankBalanceIsAlwaysCorrect, GuaranteedWithDrawProgress in
  (union Client, Bank, { TestWithSingleClient });

// assert the properties for the two clients and single server scenario
test tcMultipleClients [main=TestWithMultipleClients]:
  assert BankBalanceIsAlwaysCorrect, GuaranteedWithDrawProgress in
  (union Client, Bank, { TestWithMultipleClients });

// assert the properties for the single client and single server scenario but with abstract server
 test tcAbstractServer [main=TestWithSingleClient]:
  assert BankBalanceIsAlwaysCorrect, GuaranteedWithDrawProgress in
  (union Client, AbstractBank, { TestWithSingleClient });

// test with a parameterized number of clients
test param (nClients in [2, 3, 4]) tcParameterizedMultipleClients [main=TestWithConfig]:
  assert BankBalanceIsAlwaysCorrect, GuaranteedWithDrawProgress in
  (union Client, Bank, { TestWithConfig });
`,"../../tests/upstream/Tutorial/2_TwoPhaseCommit/PSpec/Atomicity.p":`// event: initialize the AtomicityInvariant spec monitor
event eMonitor_AtomicityInitialize: int;

/**********************************
We would like to assert the atomicity property that:
if a transaction is committed by the coordinator then it was agreed on by all participants
***********************************/
spec AtomicityInvariant observes eWriteTransResp, ePrepareResp, eMonitor_AtomicityInitialize
{
  // a map from transaction id to a map from responses status to number of participants with that response
  var participantsResponse: map[int, map[tTransStatus, int]];
  var numParticipants: int;
  start state Init {
    on eMonitor_AtomicityInitialize goto WaitForEvents with (n: int) {
      numParticipants = n;
    }
  }

  state WaitForEvents {
    on ePrepareResp do (resp: tPrepareResp){
      var transId: int;
      transId = resp.transId;

      if(!(transId in participantsResponse))
      {
        participantsResponse[transId] = default(map[tTransStatus, int]);
        participantsResponse[transId][SUCCESS] = 0;
        participantsResponse[transId][ERROR] = 0;
      }
      participantsResponse[transId][resp.status] = participantsResponse[transId][resp.status] + 1;
    }

    on eWriteTransResp do (resp: tWriteTransResp) {
      assert (resp.transId in participantsResponse || resp.status == TIMEOUT),
      format ("Write transaction was responded to the client without receiving any responses from the participants!");

      if(resp.status == SUCCESS)
      {
        assert participantsResponse[resp.transId][SUCCESS] == numParticipants,
        format ("Write transaction was responded as committed before receiving success from all participants. ") +
        format ("participants sent success: {0}, participants sent error: {1}", participantsResponse[resp.transId][SUCCESS],
        participantsResponse[resp.transId][ERROR]);
      }
      else if(resp.status == ERROR)
      {
        assert participantsResponse[resp.transId][ERROR] > 0,
          format ("Write transaction {0} was responded as failed before receiving error from atleast one participant.", resp.transId) +
          format ("participants sent success: {0}, participants sent error: {1}", participantsResponse[resp.transId][SUCCESS],
            participantsResponse[resp.transId][ERROR]);
      }
      // remove the transaction information
      participantsResponse -= (resp.transId);
    }
  }
}

/**************************************************************************
Every received transaction from a client must be eventually responded back.
Note, the usage of hot and cold states.
***************************************************************************/
spec Progress observes eWriteTransReq, eWriteTransResp {
  var pendingTransactions: int;
  start state Init {
    on eWriteTransReq goto WaitForResponses with { pendingTransactions = pendingTransactions + 1; }
  }

  hot state WaitForResponses
  {
    on eWriteTransResp do {
      pendingTransactions = pendingTransactions - 1;
      if(pendingTransactions == 0)
      {
        goto AllTransactionsFinished;
      }
    }

    on eWriteTransReq do { pendingTransactions = pendingTransactions + 1; }
  }

  cold state AllTransactionsFinished {
    on eWriteTransReq goto WaitForResponses with { pendingTransactions = pendingTransactions + 1; }
  }
}
`,"../../tests/upstream/Tutorial/2_TwoPhaseCommit/PSrc/Coordinator.p":`/* User Defined Types */

// a transaction consisting of the key, value, and the unique transaction id.
type tTrans = (key: string, val: int, transId: int);
// payload type associated with the \`eWriteTransReq\` event where \`client\`: client sending the
// transaction, \`trans\`: transaction to be committed.
type tWriteTransReq = (client: Client, trans: tTrans);
// payload type associated with the \`eWriteTransResp\` event where \`transId\` is the transaction Id
// and \`status\` is the return status of the transaction request.
type tWriteTransResp = (transId: int, status: tTransStatus);
// payload type associated with the \`eReadTransReq\` event where \`client\` is the Client machine sending
// the read request and \`key\` is the key whose value the client wants to read.
type tReadTransReq = (client: Client, key: string);
// payload type associated with the \`eReadTransResp\` event where \`val\` is the value corresponding to
// the \`key\` in the read request and \`status\` is the read status (e.g., success or failure)
type tReadTransResp = (key: string, val: int, status: tTransStatus);

// transaction status
enum tTransStatus {
  SUCCESS,
  ERROR,
  TIMEOUT
}

/* Events used by the 2PC clients to communicate with the 2PC coordinator */
// event: write transaction request (client to coordinator)
event eWriteTransReq : tWriteTransReq;
// event: write transaction response (coordinator to client)
event eWriteTransResp : tWriteTransResp;
// event: read transaction request (client to coordinator)
event eReadTransReq : tReadTransReq;
// event: read transaction response (participant to client)
event eReadTransResp: tReadTransResp;

/* Events used for communication between the coordinator and the participants */
// event: prepare request for a transaction (coordinator to participant)
event ePrepareReq: tPrepareReq;
// event: prepare response for a transaction (participant to coodinator)
event ePrepareResp: tPrepareResp;
// event: commit transaction (coordinator to participant)
event eCommitTrans: int;
// event: abort transaction (coordinator to participant)
event eAbortTrans: int;

/* User Defined Types */
// payload type associated with the \`ePrepareReq\` event
type tPrepareReq = tTrans;
// payload type assocated with the \`ePrepareResp\` event where \`participant\` is the participant machine
// sending the response, \`transId\` is the transaction id, and \`status\` is the status of the prepare
// request for that transaction.
type tPrepareResp = (participant: Participant, transId: int, status: tTransStatus);

// event: inform participant about the coordinator
event eInformCoordinator: Coordinator;

/*****************************************************************************************
The Coordinator machine receives write and read transactions from the clients. The coordinator machine
services these transactions one by one in the order in which they were received. On receiving a write
transaction the coordinator sends prepare request to all the participants and waits for prepare
responses from all the participants. Based on the responses, the coordinator either commits or aborts
the transaction. If the coordinator fails to receive agreement from participants in time, then it
timesout and aborts the transaction. On receiving a read transaction, the coordinator randomly selects
a participant and  forwards the read request to that participant.
******************************************************************************************/
machine Coordinator
{
  // set of participants
  var participants: set[Participant];
  // current write transaction being handled
  var currentWriteTransReq: tWriteTransReq;
  // previously seen transaction ids
  var seenTransIds: set[int];
  var timer: Timer;

  start state Init {
    entry (payload: set[Participant]){
      participants = payload;
      timer = CreateTimer(this);
      // inform all participants that I am the coordinator
      BroadcastToAllParticipants(eInformCoordinator, this);
      goto WaitForTransactions;
    }
  }

  state WaitForTransactions {
    on eWriteTransReq do (wTrans : tWriteTransReq) {
      if(wTrans.trans.transId in seenTransIds) // transId have to be unique
      {
        send wTrans.client, eWriteTransResp, (transId = wTrans.trans.transId, status = TIMEOUT);
        return;
      }

      currentWriteTransReq = wTrans;
      BroadcastToAllParticipants(ePrepareReq, wTrans.trans);
      //start timer while waiting for responses from all participants
      StartTimer(timer);
      goto WaitForPrepareResponses;
    }

    on eReadTransReq do (rTrans : tReadTransReq) {
      // non-deterministically pick a participant to read from.
      send choose(participants), eReadTransReq, rTrans;
    }

    // when in this state it is fine to drop these messages as they are from the previous transaction
    ignore ePrepareResp, eTimeOut;
  }

  var countPrepareResponses: int;

  state WaitForPrepareResponses {
    // defer requests, we are going to process transactions sequentially
    defer eWriteTransReq;

    on ePrepareResp do (resp : tPrepareResp) {
      // check if the response is for the current transaction else ignore it
      if (currentWriteTransReq.trans.transId == resp.transId) {
        if(resp.status == SUCCESS)
        {
          countPrepareResponses = countPrepareResponses + 1;
          // check if we have received all responses
          if(countPrepareResponses == sizeof(participants))
          {
            // lets commit the transaction
            DoGlobalCommit();
            // safe to go back and service the next transaction
            goto WaitForTransactions;
          }
        }
        else
        {
          DoGlobalAbort(ERROR);
          // safe to go back and service the next transaction
          goto WaitForTransactions;
        }
      }
    }

    // on timeout abort the transaction
    on eTimeOut goto WaitForTransactions with { DoGlobalAbort(TIMEOUT); }

    on eReadTransReq do (rTrans : tReadTransReq) {
      // non-deterministically pick a participant to read from.
      send choose(participants), eReadTransReq, rTrans;
    }

    exit {
      countPrepareResponses = 0;
    }
  }

  fun DoGlobalAbort(respStatus: tTransStatus) {
    // ask all participants to abort and fail the transaction
    BroadcastToAllParticipants(eAbortTrans, currentWriteTransReq.trans.transId);
    send currentWriteTransReq.client, eWriteTransResp, (transId = currentWriteTransReq.trans.transId, status = respStatus);
    if(respStatus != TIMEOUT)
      CancelTimer(timer);
  }

  fun DoGlobalCommit() {
    // ask all participants to commit and respond to client
    BroadcastToAllParticipants(eCommitTrans, currentWriteTransReq.trans.transId);
    send currentWriteTransReq.client, eWriteTransResp,
      (transId = currentWriteTransReq.trans.transId, status = SUCCESS);
    CancelTimer(timer);
  }

  //function to broadcast messages to all participants
  fun BroadcastToAllParticipants(message: event, payload: any)
  {
    var i: int;
    while (i < sizeof(participants)) {
      send participants[i], message, payload;
      i = i + 1;
    }
  }
}

`,"../../tests/upstream/Tutorial/2_TwoPhaseCommit/PSrc/Participant.p":`/*****************************************************************************************
Each participant maintains a local key-value store which is updated based on the
transactions committed by the coordinator. On receiving a prepare request
from the coordinator, the participant chooses to either accept or
reject the transaction.
******************************************************************************************/

machine Participant {
  // local key value store
  var kvStore: map[string, tTrans];
  // pending write transactions that have not been committed or aborted yet.
  var pendingWriteTrans: map[int, tTrans];
  // coordinator machine
  var coordinator: Coordinator;

  start state Init {
    on eInformCoordinator goto WaitForRequests with (coor: Coordinator) {
      coordinator = coor;
    }
    defer eShutDown;
  }

  state WaitForRequests {
    on eAbortTrans do (transId: int) {
      // check that abort transaction request is received for a pending transaction only.
      assert transId in pendingWriteTrans,
      format ("Abort request for a non-pending transaction, transId: {0}, pendingTrans set: {1}",
        transId, pendingWriteTrans);
      // remove the transaction from the pending transactions set
      pendingWriteTrans -= (transId);
    }

    on eCommitTrans do (transId:int) {
      // check that  commit transaction request is received for a pending transactions only.
      assert transId in pendingWriteTrans,
      format ("Commit request for a non-pending transaction, transId: {0}, pendingTrans set: {1}",
        transId, pendingWriteTrans);
      // commit the transaction locally
      kvStore[pendingWriteTrans[transId].key] = pendingWriteTrans[transId];
      // remove the transaction from the pending transactions set
      pendingWriteTrans -= (transId);
    }

    on ePrepareReq do (prepareReq :tPrepareReq) {
      // cannot receive prepare for an already pending transaction
      assert !(prepareReq.transId in pendingWriteTrans),
      format ("Duplicate transaction ids not allowed!, received transId: {0}, pending transactions: {1}",
        prepareReq.transId, pendingWriteTrans);
      // add the transaction to the pending transactions set
      pendingWriteTrans[prepareReq.transId] = prepareReq;
      // non-deterministically pick whether to accept or reject the transaction
      if (!(prepareReq.key in kvStore) || (prepareReq.key in kvStore && prepareReq.transId > kvStore[prepareReq.key].transId)) {
        send coordinator, ePrepareResp, (participant = this, transId = prepareReq.transId, status = SUCCESS);
      } else {
        send coordinator, ePrepareResp, (participant = this, transId = prepareReq.transId, status = ERROR);
      }
    }

    on eReadTransReq do (req: tReadTransReq) {
      if(req.key in kvStore)
      {
        // read successful as the key exists
        send req.client, eReadTransResp, (key = req.key, val = kvStore[req.key].val, status = SUCCESS);
      }
      else
      {
        // read failed as the key does not exist
        send req.client, eReadTransResp, (key = "", val = -1, status = ERROR);
      }
    }

    on eShutDown do {
      raise halt;
    }
  }
}

`,"../../tests/upstream/Tutorial/2_TwoPhaseCommit/PSrc/TwoPhaseCommitModules.p":`// the two phase commit module
module TwoPhaseCommit = union { Coordinator, Participant }, Timer;`,"../../tests/upstream/Tutorial/2_TwoPhaseCommit/PTst/Client.p":`/*****************************************************************************************
The client machine below implements the client of the two-phase-commit transaction service.
Each client issues N non-deterministic write-transactions,
if the transaction succeeds then it performs a read-transaction on the same key and asserts the value.
******************************************************************************************/
machine Client {
  // the coordinator machine
  var coordinator: Coordinator;
  // current transaction issued by the client
  var currTransaction : tTrans;
  // number of transactions to be issued
  var N: int;
  // uniqie client Id
  var id: int;

  start state Init {
    entry (payload : (coordinator: Coordinator, n : int, id: int)) {
      coordinator = payload.coordinator;
      N = payload.n;
      id = payload.id;
      goto SendWriteTransaction;
    }
  }

  state SendWriteTransaction {
    entry {
      currTransaction = ChooseRandomTransaction(id * 100 + N /* hack for creating unique transaction id*/);
      send coordinator, eWriteTransReq, (client = this, trans = currTransaction);
    }
    on eWriteTransResp goto ConfirmTransaction;
  }

  state ConfirmTransaction {
    entry (writeResp: tWriteTransResp) {
      // assert that if write transaction was successful then value read is the value written.
      if(writeResp.status == SUCCESS)
      {
        send coordinator, eReadTransReq, (client= this, key = currTransaction.key);
        // await response from the participant
        receive {
          case eReadTransResp: (readResp: tReadTransResp) {
            assert readResp.key == currTransaction.key && readResp.val == currTransaction.val,
              format ("Record read is not same as what was written by the client:: read - {0}, written - {1}",
            readResp.val, currTransaction.val);
          }
        }
      }
      // has more work to do?
      if(N > 0)
      {
        N = N - 1;
        goto SendWriteTransaction;
      }
    }
  }
}

fun ChooseRandomTransaction(uniqueId: int): tTrans {
  return (key = format("{0}", choose(10)), val = choose(10), transId = uniqueId);
}


// two phase commit client module
module TwoPCClient = { Client };
`,"../../tests/upstream/Tutorial/2_TwoPhaseCommit/PTst/TestDriver.p":`// type that represents the configuration of the system under test
type t2PCConfig = (
  numClients: int,
  numParticipants: int,
  numTransPerClient: int,
  failParticipants: int
);

// function that creates the two phase commit system along with the machines in its
// environment (test harness)
fun SetUpTwoPhaseCommitSystem(config: t2PCConfig)
{
  var coordinator : Coordinator;
  var participants: set[Participant];
  var i : int;

  // create participants
  while (i < config.numParticipants) {
    participants += (new Participant());
    i = i + 1;
  }

  // initialize the monitors (specifications)
  InitializeTwoPhaseCommitSpecifications(config.numParticipants);

  // create the coordinator
  coordinator = new Coordinator(participants);

  // create the clients
  i = 0;
  while(i < config.numClients)
  {
    new Client((coordinator = coordinator, n = config.numTransPerClient, id = i + 1));
    i = i + 1;
  }

  // create the failure injector if we want to inject failures
  if(config.failParticipants > 0)
  {
    CreateFailureInjector((nodes = participants, nFailures = config.failParticipants));
  }
}

fun InitializeTwoPhaseCommitSpecifications(numParticipants: int) {
  // inform the monitor the number of participants in the system
  announce eMonitor_AtomicityInitialize, numParticipants;
}

/*
This machine creates the 3 participants, 1 coordinator, and 1 clients
*/
machine SingleClientNoFailure {
  start state Init {
    entry {
      var config: t2PCConfig;

      config = (numClients = 1,
                      numParticipants = 3,
                      numTransPerClient = 2,
                      failParticipants = 0);

            SetUpTwoPhaseCommitSystem(config);
    }
  }
}

/*
This machine creates the 3 participants, 1 coordinator, and 2 clients
*/
machine MultipleClientsNoFailure {
  start state Init {
    entry {
      var config: t2PCConfig;
      config =
        (numClients = 2,
        numParticipants = 3,
        numTransPerClient = 2,
        failParticipants = 0);

        SetUpTwoPhaseCommitSystem(config);
    }
  }
}

/*
This machine creates the 3 participants, 1 coordinator, 1 Failure injector, and 2 clients
*/
machine MultipleClientsWithFailure {
  start state Init {
    entry {
      var config: t2PCConfig;
      config =
        (numClients = 2,
        numParticipants = 3,
        numTransPerClient = 2,
        failParticipants = 1);

      SetUpTwoPhaseCommitSystem(config);
    }
  }
}

// Parameters for system configuration
param pNumClients: int;
param pNumParticipants: int;
param pNumTransPerClient: int;
param pFailParticipants: int;
/*
This machine allows parameterized testing with configurable number of clients, participants, and failures
*/

machine TestWithConfig {
  start state Init {
    entry {
      var config: t2PCConfig;
      config = (
        numClients = pNumClients,
        numParticipants = pNumParticipants,
        numTransPerClient = pNumTransPerClient,
        failParticipants = pFailParticipants
      );

      SetUpTwoPhaseCommitSystem(config);
    }
  }
}
`,"../../tests/upstream/Tutorial/2_TwoPhaseCommit/PTst/TestScripts.p":`// checks that all events are handled correctly and also the local assertions in the P machines.
test tcSingleClientNoFailure [main = SingleClientNoFailure]:
  union TwoPhaseCommit, TwoPCClient, FailureInjector, { SingleClientNoFailure };

// asserts the liveness monitor along with the default properties
test tcMultipleClientsNoFailure [main = MultipleClientsNoFailure]:
  assert AtomicityInvariant, Progress in
    (union TwoPhaseCommit, TwoPCClient, FailureInjector, { MultipleClientsNoFailure });

// asserts the liveness monitor along with the default properties
test tcMultipleClientsWithFailure [main = MultipleClientsWithFailure]:
  assert Progress in (union TwoPhaseCommit, TwoPCClient, FailureInjector, { MultipleClientsWithFailure });

// parametric testing of all parameters
test param (pNumClients in [2, 3], pNumParticipants in [3, 4, 5], 
           pNumTransPerClient in [1, 2], pFailParticipants in [0, 1])
  assume (pNumParticipants > pNumClients && pFailParticipants < pNumParticipants/2)
  (2 wise) tcParametricTests [main=TestWithConfig]:
  assert AtomicityInvariant, Progress in
  (union TwoPhaseCommit, TwoPCClient, FailureInjector, { TestWithConfig });
`,"../../tests/upstream/Tutorial/3_EspressoMachine/PSpec/Safety.p":`/* Events used to inform monitor about the internal state of the CoffeeMaker */
event eInWarmUpState;
event eInReadyState;
event eInBeansGrindingState;
event eInCoffeeBrewingState;
event eErrorHappened;
event eResetPerformed;

/*********************************************
We would like to ensure that the coffee maker moves
through the expected modes of operation. We want to make sure that the coffee maker always transitions
through the following sequence of states:
Steady operation:
  WarmUp -> Ready -> GrindBeans -> MakeCoffee -> Ready
With Error:
If an error occurs in any of the states above then the Coffee machine stays in the error state until
it is reset and after which it returns to the Warmup state.

**********************************************/
spec EspressoMachineModesOfOperation
observes eInWarmUpState, eInReadyState, eInBeansGrindingState, eInCoffeeBrewingState, eErrorHappened, eResetPerformed
{
  start state StartUp {
    on eInWarmUpState goto WarmUp;
  }

  state WarmUp {
    on eErrorHappened goto Error;
    on eInReadyState goto Ready;
  }

  state Ready {
    ignore eInReadyState;
    on eInBeansGrindingState goto BeanGrinding;
    on eErrorHappened goto Error;
  }

  state BeanGrinding {
    on eInCoffeeBrewingState goto MakingCoffee;
    on eErrorHappened goto Error;
  }

  state MakingCoffee {
    on eInReadyState goto Ready;
    on eErrorHappened goto Error;
  }

  state Error {
    on eResetPerformed goto StartUp;
    ignore eErrorHappened;
  }
}

`,"../../tests/upstream/Tutorial/3_EspressoMachine/PSrc/CoffeeMaker.p":`
/* Requests or operations from the controller to coffee maker */

// event: warmup request when the coffee maker starts or resets
event eWarmUpReq;
// event: grind beans request before making coffee
event eGrindBeansReq;
// event: start brewing coffee
event eStartEspressoReq;
// event start steamer
event eStartSteamerReq;
// event: stop steamer
event eStopSteamerReq;

/* Responses from the coffee maker to the controller */
// event: completed grinding beans
event eGrindBeansCompleted;
// event: completed brewing and pouring coffee
event eEspressoCompleted;
// event: warmed up the machine and read to make coffee
event eWarmUpCompleted;

/* Error messages from the coffee maker to control panel or controller*/
// event: no water for coffee, refill water!
event eNoWaterError;
// event: no beans for coffee, refill beans!
event eNoBeansError;
// event: the heater to warm the machine is broken!
event eWarmerError;

/*****************************************************
EspressoCoffeeMaker receives requests from the control panel of the coffee machine and
based on its state e.g., whether heater is working, or it has beans and water, the maker responds
back to the controller if the operation succeeded or errored.
*****************************************************/
machine EspressoCoffeeMaker
{
  // control panel of the coffee machine that sends inputs to the coffee maker
  var controller: CoffeeMakerControlPanel;

  start state WaitForRequests {
    entry (_controller: CoffeeMakerControlPanel) {
      controller = _controller;
    }

    on eWarmUpReq do {
        send controller, eWarmUpCompleted;
    }

    on eGrindBeansReq do {
      if (!HasBeans()) {
        send controller, eNoBeansError;
      } else {
        send controller, eGrindBeansCompleted;
      }
    }

    on eStartEspressoReq do {
      if (!HasWater()) {
        send controller, eNoWaterError;
      } else {
        send controller, eEspressoCompleted;
      }
    }
    on eStartSteamerReq do {
      if (!HasWater()) {
        send controller, eNoWaterError;
      }
    }
    on eStopSteamerReq do { /* do nothing, steamer stopped */ }
  }

  // nondeterministic functions to trigger different behaviors
  fun HasBeans() : bool { return $; }
  fun HasWater() : bool { return $; }
}


`,"../../tests/upstream/Tutorial/3_EspressoMachine/PSrc/CoffeeMakerControlPanel.p":`/* Events used by the user to interact with the control panel of the Coffee Machine */
// event: make espresso button pressed
event eEspressoButtonPressed;
// event: steamer button turned off
event eSteamerButtonOff;
// event: steamer button turned on
event eSteamerButtonOn;
// event: door opened to empty grounds
event eOpenGroundsDoor;
// event: door closed after emptying grounds
event eCloseGroundsDoor;
// event: reset coffee maker button pressed
event eResetCoffeeMaker;
//event: error message from panel to the user
event eCoffeeMakerError: tCoffeeMakerState;
//event: coffee machine is ready
event eCoffeeMakerReady;
// event: coffee machine user
event eCoffeeMachineUser: machine;

// enum to represent the state of the coffee maker
enum tCoffeeMakerState {
  NotWarmedUp,
  Ready,
  NoBeansError,
  NoWaterError
}

/*
CoffeeMakerControlPanel acts as the interface between the CoffeeMaker and User
It converts the inputs from the user to appropriate inputs to the CoffeeMaker and sends responses to
the user.
*/
machine CoffeeMakerControlPanel
{
  var coffeeMaker: EspressoCoffeeMaker;
  var coffeeMakerState: tCoffeeMakerState;
  var currentUser: machine;

  start state Init {
    entry {
      coffeeMakerState = NotWarmedUp;
      coffeeMaker = new EspressoCoffeeMaker(this);
      WaitForUser();
      goto WarmUpCoffeeMaker;
    }
  }

  // block until a user shows up
  fun WaitForUser() {
      receive {
          case eCoffeeMachineUser: (user: machine) {
              currentUser = user;
          }
      }
  }

  state WarmUpCoffeeMaker {
    entry {
      // inform the specification about current state of the coffee maker
      announce eInWarmUpState;

      BeginHeatingCoffeeMaker();
    }

    on eWarmUpCompleted goto CoffeeMakerReady;

    // grounds door is opened or closed will handle it later after the coffee maker has warmed up
    defer eOpenGroundsDoor, eCloseGroundsDoor;
    // ignore these inputs from users until the maker has warmed up.
    ignore eEspressoButtonPressed, eSteamerButtonOff, eSteamerButtonOn, eResetCoffeeMaker;
    // ignore these errors and responses as they could be from previous state
    ignore eNoBeansError, eNoWaterError, eGrindBeansCompleted;
  }




  state CoffeeMakerReady {
    entry {
      // inform the specification about current state of the coffee maker
      announce eInReadyState;

      coffeeMakerState = Ready;
      send currentUser, eCoffeeMakerReady;
    }

    on eOpenGroundsDoor goto CoffeeMakerDoorOpened;
    on eEspressoButtonPressed goto CoffeeMakerRunGrind;
    on eSteamerButtonOn goto CoffeeMakerRunSteam;

    // ignore these out of order commands, these must have happened because of an error
    // from user or sensor
    ignore eSteamerButtonOff, eCloseGroundsDoor;

    // ignore commands and errors as they are from previous state
    ignore eWarmUpCompleted, eResetCoffeeMaker, eNoBeansError, eNoWaterError;
  }

  state CoffeeMakerRunGrind {
    entry {
      // inform the specification about current state of the coffee maker
      announce eInBeansGrindingState;

      GrindBeans();
    }
    on eNoBeansError goto EncounteredError with {
      coffeeMakerState = NoBeansError;
      print "No beans to grind! Please refill beans and reset the machine!";
    }

    on eNoWaterError goto EncounteredError with {
      coffeeMakerState = NoWaterError;
      print "No Water! Please refill water and reset the machine!";
    }

    on eGrindBeansCompleted goto CoffeeMakerRunEspresso;

    defer eOpenGroundsDoor, eCloseGroundsDoor, eEspressoButtonPressed;

    // Can't make steam while we are making espresso
    ignore eSteamerButtonOn, eSteamerButtonOff;

    // ignore commands that are old or cannot be handled right now
    ignore eWarmUpCompleted, eResetCoffeeMaker;
  }

  state CoffeeMakerRunEspresso {
    entry {
      // inform the specification about current state of the coffee maker
      announce eInCoffeeBrewingState;

      StartEspresso();
    }
    on eEspressoCompleted goto CoffeeMakerReady with { send currentUser, eEspressoCompleted; }

    on eNoWaterError goto EncounteredError with {
      coffeeMakerState = NoWaterError;
      print "No Water! Please refill water and reset the machine!";
    }

    // the user commands will be handled next after finishing this espresso
    defer eOpenGroundsDoor, eCloseGroundsDoor, eEspressoButtonPressed;

    // Can't make steam while we are making espresso
    ignore eSteamerButtonOn, eSteamerButtonOff;

    // ignore old commands and cannot reset when making coffee
    ignore eWarmUpCompleted, eResetCoffeeMaker;
  }

  state CoffeeMakerRunSteam {
    entry {
      StartSteamer();
    }

    on eSteamerButtonOff  goto CoffeeMakerReady with {
      StopSteamer();
    }

    on eNoWaterError goto EncounteredError with {
      coffeeMakerState = NoWaterError;
      print "No Water! Please refill water and reset the machine!";
    }

    // user might have cleaned grounds while steaming
    defer eOpenGroundsDoor, eCloseGroundsDoor;

    // can't make espresso while we are making steam
    ignore eEspressoButtonPressed, eSteamerButtonOn;
  }

  state CoffeeMakerDoorOpened {
    on eCloseGroundsDoor do {
      assert coffeeMakerState != NotWarmedUp;
      assert coffeeMakerState == Ready;
      goto CoffeeMakerReady;
    }

    // grounds door is open cannot handle these requests just ignore them
    ignore eEspressoButtonPressed, eSteamerButtonOn, eSteamerButtonOff;
  }

  state EncounteredError {
    entry {
      // inform the specification about current state of the coffee maker
      announce eErrorHappened;

      // send the error message to the client
      send currentUser, eCoffeeMakerError, coffeeMakerState;
    }

    on eResetCoffeeMaker goto WarmUpCoffeeMaker with {
      // inform the specification about current state of the coffee maker
      announce eResetPerformed;
    }

    // error, ignore these requests until reset.
    ignore eEspressoButtonPressed, eSteamerButtonOn, eSteamerButtonOff,
        eOpenGroundsDoor, eCloseGroundsDoor, eWarmUpCompleted, eEspressoCompleted, eGrindBeansCompleted;

    // ignore other simultaneous errors
    ignore eNoBeansError, eNoWaterError;
  }

  fun BeginHeatingCoffeeMaker() {
    // send an event to maker to start warming
    send coffeeMaker, eWarmUpReq;
  }

  fun StartSteamer() {
    // send an event to maker to start steaming
    send coffeeMaker, eStartSteamerReq;
  }

  fun StopSteamer() {
    // send an event to maker to stop steaming
    send coffeeMaker, eStopSteamerReq;
  }

  fun GrindBeans() {
    // send an event to maker to grind beans
    send coffeeMaker, eGrindBeansReq;
  }

  fun StartEspresso() {
    // send an event to maker to start espresso
    send coffeeMaker, eStartEspressoReq;
  }
}
`,"../../tests/upstream/Tutorial/3_EspressoMachine/PSrc/EspressoMachineModules.p":`module EspressoMachine = { CoffeeMakerControlPanel, EspressoCoffeeMaker };`,"../../tests/upstream/Tutorial/3_EspressoMachine/PTst/TestDrivers.p":`machine TestWithSaneUser
{
  start state Init {
    entry {
      // create a sane user
      new SaneUser(new CoffeeMakerControlPanel());
    }
  }
}

machine TestWithCrazyUser
{
  start state Init {
    entry {
      // create a crazy user
      new CrazyUser((coffeeMaker = new CoffeeMakerControlPanel(), nOps = 5));
    }
  }
}

// Parameters for comprehensive testing scenarios
param nOps : int;
param nUsers : int;
param waterLevel : int;
param beanLevel : int;
param enableSteamer : bool;
param cleaningMode : bool;

machine TestWithConfig
{
  start state Init {
    entry {
      // create a crazy user
      new CrazyUser((coffeeMaker = new CoffeeMakerControlPanel(), nOps = nOps));
    }
  }
}

// Test driver for multiple users, each with their own coffee machine
machine TestWithMultipleUsers
{
  var i: int;
  start state Init {
    entry {
      var coffeeMaker: CoffeeMakerControlPanel;
      
      i = 0;
      while (i < nUsers) {
        coffeeMaker = new CoffeeMakerControlPanel();
        
        if (i % 2 == 0) {
          new SaneUser(coffeeMaker);
        } else {
          new CrazyUser((coffeeMaker = coffeeMaker, nOps = nOps));
        }
        i = i + 1;
      }
    }
  }
}

// Test driver with resource constraints (water and bean levels)
machine TestWithResourceConstraints
{
  start state Init {
    entry {
      var coffeeMaker: CoffeeMakerControlPanel;
      coffeeMaker = new CoffeeMakerControlPanel();
      
      if (waterLevel > 0 && beanLevel > 0) {
        new SaneUser(coffeeMaker);
      } else {
        new CrazyUser((coffeeMaker = coffeeMaker, nOps = 2));
      }
    }
  }
}

// Test driver with mixed configurations
machine TestWithMixedConfiguration
{
  start state Init {
    entry {
      var coffeeMaker: CoffeeMakerControlPanel;
      coffeeMaker = new CoffeeMakerControlPanel();
      
      if (cleaningMode) {
        new CrazyUser((coffeeMaker = coffeeMaker, nOps = 3));
      } else if (enableSteamer) {
        new SteamerTestUser(coffeeMaker);
      } else {
        new SaneUser(coffeeMaker);
      }
    }
  }
}

`,"../../tests/upstream/Tutorial/3_EspressoMachine/PTst/TestScripts.p":`test tcSaneUserUsingCoffeeMachine [main=TestWithSaneUser]:
  assert EspressoMachineModesOfOperation in (union { TestWithSaneUser }, EspressoMachine, Users);

test tcCrazyUserUsingCoffeeMachine [main=TestWithCrazyUser]:
  assert EspressoMachineModesOfOperation in (union { TestWithCrazyUser }, EspressoMachine, Users);

// BASIC PARAMETER TESTS

// test with a parameterized number of operations for the crazy user
test param (nOps in [4, 5, 6]) tcCrazyUserParamOps [main=TestWithConfig]:
  assert EspressoMachineModesOfOperation in (union EspressoMachine, Users, { TestWithConfig });

// MULTI-PARAMETER TESTS

// Test multiple users, each with their own coffee machine, with varying operations
test param (nUsers in [1, 2, 3], nOps in [3, 5, 7]) tcMultiUserOperations [main=TestWithMultipleUsers]:
  assert EspressoMachineModesOfOperation in (union EspressoMachine, Users, { TestWithMultipleUsers });

// Test resource constraints with different levels
test param (waterLevel in [0, 25, 50, 100], beanLevel in [0, 25, 50]) tcResourceConstraints [main=TestWithResourceConstraints]:
  assert EspressoMachineModesOfOperation in (union EspressoMachine, Users, { TestWithResourceConstraints });

// BOOLEAN PARAMETER TESTS

// Test boolean configurations
test param (enableSteamer in [true, false], cleaningMode in [true, false]) tcBooleanConfigs [main=TestWithMixedConfiguration]:
  assert EspressoMachineModesOfOperation in (union EspressoMachine, Users, { TestWithMixedConfiguration, SteamerTestUser });

`,"../../tests/upstream/Tutorial/3_EspressoMachine/PTst/Users.p":`/*
A SaneUser who knows how to use the CoffeeMaker
*/
machine SaneUser {
  var coffeeMakerPanel: CoffeeMakerControlPanel;
  var cups: int;
  start state Init {
    entry (coffeeMaker: CoffeeMakerControlPanel) {
      coffeeMakerPanel = coffeeMaker;
      // inform control panel that I am the user
      send coffeeMakerPanel, eCoffeeMachineUser, this;
      // want to make 2 cups of espresso
      cups = 2;

      goto LetsMakeCoffee;
    }
  }

  state LetsMakeCoffee {
    entry {
      while (cups > 0)
      {
        // lets wait for coffee maker to be ready
        WaitForCoffeeMakerToBeReady();

        // press Espresso button
        PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressEspressoButton);

        // check the status of the machine
        receive {
          case eEspressoCompleted: {
            // lets make the next coffee
            cups = cups - 1;
          }
          case eCoffeeMakerError: (status: tCoffeeMakerState){

            // lets fill the beans or water and reset the machine
            // and go back to making espresso
            PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressResetButton);
          }
        }
      }

      // I am a good user so I will clear the coffee grounds before leaving.
      PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_ClearGrounds);

      // am done, let me exit
      raise halt;
    }
  }
}

enum tCoffeeMakerOperations {
  CM_PressEspressoButton,
  CM_PressSteamerButton,
  CM_PressResetButton,
  CM_ClearGrounds
}

/*
A crazy user who gets excited by looking at a coffee machine and starts stress testing the machine
by pressing all sorts of random button and opening/closing doors
*/
// TODO: We do not support global constants currently, they can be encoded using global functions.

machine CrazyUser {
  var coffeeMakerPanel: CoffeeMakerControlPanel;
  var numOperations: int;
  start state StartPressingButtons {
    entry (config: (coffeeMaker: CoffeeMakerControlPanel, nOps: int)) {
      var pickedOps: tCoffeeMakerOperations;

      numOperations = config.nOps;
      coffeeMakerPanel = config.coffeeMaker;

      // inform control panel that I am the user
      send coffeeMakerPanel, eCoffeeMachineUser, this;

      while(numOperations > 0)
      {
        pickedOps = PickRandomOperationToPerform();
        PerformOperationOnCoffeeMaker(coffeeMakerPanel, pickedOps);
        numOperations = numOperations - 1;
      }
    }

    // I will ignore all the responses from the coffee maker
    ignore eCoffeeMakerError, eEspressoCompleted, eCoffeeMakerReady;
  }

  // Pick a random enum value (hacky work around)
  // Currently, the choose operation does not support choose over enum value
  fun PickRandomOperationToPerform() : tCoffeeMakerOperations {
    var op_i: int;
    op_i =  choose(3);
    if(op_i == 0)
      return CM_PressEspressoButton;
    else if(op_i == 1)
      return CM_PressSteamerButton;
    else if(op_i == 2)
      return CM_PressResetButton;
    else
      return CM_ClearGrounds;
  }
}

// Specialized user for testing steamer functionality
machine SteamerTestUser
{
  var coffeeMakerPanel: CoffeeMakerControlPanel;
  
  start state Init {
    entry (coffeeMaker: CoffeeMakerControlPanel) {
      coffeeMakerPanel = coffeeMaker;
      send coffeeMakerPanel, eCoffeeMachineUser, this;
      goto TestSteamer;
    }
  }
  
  state TestSteamer {
    entry {
      // Test steamer operations specifically
      PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressSteamerButton);
      
      receive {
        case eCoffeeMakerReady: {
          // Continue with espresso after steamer test
          PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressEspressoButton);
        }
        case eCoffeeMakerError: (status: tCoffeeMakerState) {
          // Reset and try again
          PerformOperationOnCoffeeMaker(coffeeMakerPanel, CM_PressResetButton);
        }
      }
    }
    
    ignore eEspressoCompleted;
  }
}


/* Function to perform an operation on the CoffeeMaker */
fun PerformOperationOnCoffeeMaker(coffeeMakerCP: CoffeeMakerControlPanel, CM_Ops: tCoffeeMakerOperations)
{
  if(CM_Ops == CM_PressEspressoButton) {
    send coffeeMakerCP, eEspressoButtonPressed;
  }
  else if(CM_Ops == CM_PressSteamerButton) {
    send coffeeMakerCP, eSteamerButtonOn;
    // wait for some time and then release the button
    send coffeeMakerCP, eSteamerButtonOff;
  }
  else if(CM_Ops == CM_ClearGrounds)
  {
    send coffeeMakerCP, eOpenGroundsDoor;
    // empty ground and close the door
    send coffeeMakerCP, eCloseGroundsDoor;
  }
  else if(CM_Ops == CM_PressResetButton)
  {
    send coffeeMakerCP, eResetCoffeeMaker;
  }
}

fun WaitForCoffeeMakerToBeReady() {
  receive {
    case eCoffeeMakerReady: {}
    case eCoffeeMakerError: (status: tCoffeeMakerState){ raise halt; }
  }
}
module Users = { SaneUser, CrazyUser };`,"../../tests/upstream/Tutorial/4_FailureDetector/PSpec/ReliableFailureDetector.p":`/***************************************************
ReliableFailureDetector is a liveness property to assert that all nodes that have been shutdown
by the failure injector will eventually be detected by the failure detector as a failed node
***************************************************/

spec ReliableFailureDetector observes eNotifyNodesDown, eShutDown {
  // set of nodes that are shutdown by the failure injector but
  // have not been detected.
  var nodesShutdownAndNotDetected: set[Node];
  // set of nodes that have been marked as down by the failure detector
  var nodesDownDetected: set[Node];

  // State where all the nodes that are shutdown by failure injector
  // have been detected by the failure detector
  start state AllShutdownNodesAreDetected {
    on eNotifyNodesDown do (nodes: set[Node])
    {
      var i: int;
      while(i < sizeof(nodes))
      {
        nodesShutdownAndNotDetected -= (nodes[i]);
        nodesDownDetected += (nodes[i]);
        i = i + 1;
      }
    }

    on eShutDown do (node: machine) {
      if(!((node as Node) in nodesDownDetected)) {
        nodesShutdownAndNotDetected += (node as Node);
        goto NodesShutDownButNotDetected;
      }
    }
  }

  // intermediate "unstable" state where the failure detector has not detected all the nodes
  // that have been shutdown by the failure injector
  hot state NodesShutDownButNotDetected {
    on eNotifyNodesDown do (nodes: set[Node])
    {
      var i: int;
      while(i < sizeof(nodes))
      {
        nodesShutdownAndNotDetected -= (nodes[i]);
        nodesDownDetected += (nodes[i]);
        i = i + 1;
      }

      if(sizeof(nodesShutdownAndNotDetected) == 0)
        goto AllShutdownNodesAreDetected; // return to the stable state
    }

    on eShutDown do (node: machine) {
      if(!((node as Node) in nodesDownDetected))
        nodesShutdownAndNotDetected += (node as Node);
    }
  }
}`,"../../tests/upstream/Tutorial/4_FailureDetector/PSrc/Client.p":`/******************************************
Client machine that keeps track of the alive nodes in the system
*******************************************/

machine Client {
  var myViewOfAliveNodes: set[Node];

  start state Init {
    entry (nodes: set[Node]){
      myViewOfAliveNodes = nodes;
      // do something with the alive nodes
    }

    on eNotifyNodesDown do (dead_nodes: set[Node]){
      var i : int;
      print format("Nodes {0} are down!", dead_nodes);
      while(i < sizeof(dead_nodes))
      {
        myViewOfAliveNodes -= (dead_nodes[i]);
        i = i + 1;
      }
    }
  }
}`,"../../tests/upstream/Tutorial/4_FailureDetector/PSrc/FailureDetector.p":`// event: ping nodes (from failure detector to nodes)
event ePing: (fd: FailureDetector, trial: int);
// event: pong detector (response to ping) (from nodes to failure detector)
event ePong: (node: Node, trial: int);
// event: failure notification to the client (from failure detector to client)
event eNotifyNodesDown: set[Node];

/***************************************************
FailureDetector machine monitors whether a set of nodes in the system are alive (responsive).
It periodically sends ping message to each node and waits for a pong message from the nodes.
The nodes that do not send a pong message after multiple attempts are marked as down or failed
and notified to the client nodes so that they can update their view of the system.
***************************************************/
machine FailureDetector {
  // set of nodes to be monitored
  var nodes: set[Node];
  // set of registered clients
  var clients: set[Client];
  // num of ping attempts made
  var attempts: int;
  // set of alive nodes
  var alive: set[Node];
  // nodes that have responded in the current round
  var respInCurrRound: set[machine];
  // timer to wait for responses from nodes
  var timer: Timer;

  start state Init {
    entry (config: (nodes: set[Node], clients: set[Client])) {
      nodes = config.nodes;
      alive = config.nodes;
      clients = config.clients;
      timer = CreateTimer(this);
      goto SendPingsToAllNodes;
    }
  }

  state SendPingsToAllNodes {
    entry {
      var notRespondedNodes: set[Node];

      if(sizeof(alive) == 0)
        raise halt; // stop myself, no work to do, there are no alive nodes!

      // compute nodes that have not responded with pongs
      notRespondedNodes = PotentiallyDownNodes();
      // send ping events to machines that have not responded in the previous attempt
      UnReliableBroadCast(notRespondedNodes, ePing, (fd =  this, trial = attempts));
      // start wait timer to wait for pong responses
      StartTimer(timer);
    }

    on ePong do (pong: (node: Node, trial: int)) {
      // collect pong responses from alive nodes
      // no need to do any for pong messages from nodes that have marked failed.
      if (pong.node in alive) {
        respInCurrRound += (pong.node);
        if (sizeof(respInCurrRound) == sizeof(alive)) {
          // status of alive nodes has not changed
          CancelTimer(timer);
          goto ResetAndStartAgain;
        }
      }
    }

    on eTimeOut do {
      var nodesDown: set[Node];
      // one more attempt finished
      attempts = attempts + 1;
      // check if there are nodes that have not responded
      if (sizeof(respInCurrRound) < sizeof(alive) ) {
        // maximum number of attempts == 3
        if(attempts < 3) {
          // try again by re-pinging the nodes that have not responded
          goto SendPingsToAllNodes;
        }
        else
        {
          // inform clients about the nodes down
          nodesDown = ComputeNodesDownAndUpdateAliveSet();
          // notification to the client is assumed to be a reliable send so that the client gets an updated view
          ReliableBroadCast(clients, eNotifyNodesDown, nodesDown);
        }
      }
      // lets reset and restart the failure detection
      goto ResetAndStartAgain;
    }
  }

  state ResetAndStartAgain {
    entry {
      // prepare for the next detection phase
      attempts = 0;
      respInCurrRound = default(set[Node]);
      // start timer for inter-phase waiting
      StartTimer(timer);
    }
    on eTimeOut goto SendPingsToAllNodes;
    // detection has finish, these are all delayed pongs and must be ignored
    ignore ePong;
  }

  // compute the potentially down nodes
  // i.e., nodes that have not responded Pong in this round
  fun PotentiallyDownNodes() : set[Node] {
    var i: int;
    var nodesNotResponded: set[Node];
    while (i < sizeof(nodes)) {
      if (nodes[i] in alive && !(nodes[i] in respInCurrRound)) {
          nodesNotResponded += (nodes[i]);
      }
      i = i + 1;
    }
    return nodesNotResponded;
  }

  // compute the nodes that might be down and also update the alive set accordingly
  fun ComputeNodesDownAndUpdateAliveSet() : set[Node] {
    var i: int;
    var nodesDown: set[Node];
    while (i < sizeof(nodes)) {
      if (nodes[i] in alive && !(nodes[i] in respInCurrRound)) {
        alive -= (nodes[i]);
        nodesDown += (nodes[i]);
      }
      i = i + 1;
    }
    return nodesDown;
  }
}



`,"../../tests/upstream/Tutorial/4_FailureDetector/PSrc/FailureDetectorModules.p":`// module to represent the Failure Detector
module FailureDetector = (union {FailureDetector, Node, Client }, Timer);`,"../../tests/upstream/Tutorial/4_FailureDetector/PSrc/Node.p":`/****************************
Node machine sends a pong message on receiving a ping
*****************************/
machine Node {
  start state WaitForPing {
    on ePing do (req: (fd: FailureDetector, trial: int)) {
      UnReliableSend(req.fd, ePong, (node = this, trial = req.trial));
    }

    on eShutDown do {
      raise halt;
    }
  }
}`,"../../tests/upstream/Tutorial/4_FailureDetector/PTst/TestDriver.p":`type tSystemConfig = (
  numNodes: int,
  numClients: int
);

/*****************************************
Setup a system with 3 nodes and 2 clients
******************************************/
machine TestMultipleClients {
  start state Init {
    entry {
      var config: tSystemConfig;
      config = (numNodes = 3, numClients = 2);
      SetupSystemWithFailureInjector(config);
    }
  }
}

param numNodes: int;
param numClients: int;

machine TestWithConfig {
  start state Init {
    entry {
      var config: tSystemConfig;
      config = (numNodes = numNodes, numClients = numClients);
      SetupSystemWithFailureInjector(config);
    }
  }
}

// setup the system for failure detection
fun SetupSystemWithFailureInjector(config: tSystemConfig)
{
  var i : int;
  var nodes: set[Node];
  var clients: set[Client];
  // create Nodes
  while(i < config.numNodes) {
    nodes += (new Node());
    i = i + 1;
  }

  i = 0;
  // create clients
  while(i < config.numClients) {
    clients += (new Client(nodes));
    i = i + 1;
  }
  // create the failure detector
  new FailureDetector((nodes = nodes, clients = clients));

  // create the failure injector
  new FailureInjector((nodes = nodes, nFailures = sizeof(nodes)/2 + 1));
}`,"../../tests/upstream/Tutorial/4_FailureDetector/PTst/TestScript.p":`test tcTest_FailureDetector [main=TestMultipleClients]:
  assert ReliableFailureDetector in
  union { TestMultipleClients }, FailureDetector, FailureInjector;

// Test case that ensures clients don't outnumber nodes for better monitoring distribution
test param (numNodes in [3, 4, 5], numClients in [2, 3, 4]) assume (numClients <= numNodes) tcTest_BalancedLoad [main=TestWithConfig]:
  assert ReliableFailureDetector in
  union { TestWithConfig }, FailureDetector, FailureInjector;`,"../../tests/upstream/Tutorial/5_Paxos/PSpec/common.p":`// Unreliably send event \`e\` with payload \`p\` to every \`target\`
fun UnreliableBroadcast(target_machines: set[machine], e: event, payload: any) {
	var i: int;
	while (i < sizeof(target_machines)) {
		if (choose()) {
			send target_machines[i], e, payload;
		}
		i = i + 1;
	}
}

// Unreliably send event \`e\` with payload \`p\` to every \`target\`, potentially multiple times
fun UnreliableBroadcastMulti(target_machines: set[machine], e: event, payload: any) {
	var i: int;
	var n: int;
	var k: int;

	while (i < sizeof(target_machines)) {
		// Each message is sent \`k\` is that number of times 
		k = choose(3);
		// Times we've sent the packet so far
		n = 0;
		while (n < k) {		
			send target_machines[i], e, payload;
			n = n + 1;
		}
		i = i + 1;
	}
}

// Reliably send event \`e\` with payload \`p\` to every \`target\`
fun ReliableBroadcast(target_machines: set[machine], e: event, payload: any) {
	var i: int;
	while (i < sizeof(target_machines)) {
		send target_machines[i], e, payload;
		i = i + 1;
	}
}

// Reliably send event \`e\` with payload \`p\` to a majority of \`target\`. Unreliable send to remaining, potentially multiple times.
fun ReliableBroadcastMajority(target_machines: set[machine], e: event, payload: any) {
	var i: int;
	var n: int;
	var k: int;
	var majority: int;

	majority = sizeof(target_machines) / 2 + 1;

	while (i < sizeof(target_machines)) {
		// Each message is sent \`k\` is that number of times 
		k = 1;
		if (i >= majority) {
			k = choose(3);
		}
		// Times we've sent the packet so far
		n = 0;
		while (n < k) {		
			send target_machines[i], e, payload;
			n = n + 1;
		}
		i = i + 1;
	}
}
`,"../../tests/upstream/Tutorial/5_Paxos/PSpec/spec.p":`// This safety spec ensures that the value that was taught never changes (though it can be taught multiple times)
spec OneValueTaught observes eLearn {
	var decided: int;

	start state Init {
		entry {
			decided = -1;
		}

		on eLearn do (payload: (ballot: tBallot, v: tValue)) {
			assert(payload.v != -1);
			if (decided != -1) {
				assert(decided == payload.v);
			}
			decided = payload.v;
		}
	}
}`,"../../tests/upstream/Tutorial/5_Paxos/PSrc/acceptor.p":`type tBallot = int;
type tValue = int;

type tPrepareReq = (proposer: Proposer, ballot_n: tBallot, v: tValue);
event ePrepareReq: tPrepareReq;
type tPrepareRsp = (acceptor: Acceptor, promised: tBallot, v_accepted: tValue, n_accepted: tBallot);
event ePrepareRsp: tPrepareRsp;
type tAcceptReq = (proposer: Proposer, ballot_n: tBallot, v: tValue);
event eAcceptReq: tAcceptReq;
type tAcceptRsp = (acceptor: Acceptor, accepted: tBallot);
event eAcceptRsp: tAcceptRsp;

// The acceptor role in Paxos (see proposer.p for more details).
machine Acceptor {
	var n_prepared: tBallot;

	var v_accepted: tValue;
	var n_accepted: tBallot;

	start state Init {
		entry {
			n_prepared = -1;
			v_accepted = -1;
			n_accepted = -1;
			goto Accept;
		}
	}

	state Accept {
		// When we get a prepare request, we promise not to accept any prepare requests with a lower ballot number, and return any
		//  proposed value we've accepted, if any.
		on ePrepareReq do (req: tPrepareReq) {
			if (req.ballot_n > n_prepared) {
				send req.proposer, ePrepareRsp, (acceptor = this, promised = req.ballot_n, v_accepted = v_accepted, n_accepted = n_accepted);
				n_prepared = req.ballot_n;
				// print format("{0}: ({1}:{2}, {3})", this, n_accepted, v_accepted, n_prepared);
			}
			// As an optimization, we could send a NACK here to let the proposer know we got its message, allowing it to tell the difference
			//  between losing it's proposal and packet loss. It's just an optimization, so we don't do it yet.
		}

		// Once a proposer has been made the 'leader', it sends us a proposed value. To avoid accepting values from old leaders, we simply
		//  discard any messages with ballot numbers lower than the one we prepared.
		on eAcceptReq do (req: tAcceptReq) {
			if (req.ballot_n >= n_prepared) {
				v_accepted = req.v;
				n_accepted = req.ballot_n;
				// Treat accepting as "prepare, accept" the way that Lamport does in Part Time Parliament (but not in Paxos Made Simple)
				//  See https://brooker.co.za/blog/2021/11/16/paxos.html and https://stackoverflow.com/questions/29880949/contradiction-in-lamports-paxos-made-simple-paper
				n_prepared = req.ballot_n;

				send req.proposer, eAcceptRsp, (acceptor = this, accepted = req.ballot_n);
			}
			// Same story with NACKs here.
		}
	}
}`,"../../tests/upstream/Tutorial/5_Paxos/PSrc/learner.p":`event eLearn: (ballot: tBallot, v: tValue);

// The learner role in Paxos (see proposer.p for more details).
machine Learner {
	var learned_value: tValue;

	start state Init {
		entry {
			learned_value = -1;
			goto Learn;
		}
	}

	state Learn {
		on eLearn do (payload: (ballot: tBallot, v: tValue)) {
			assert(payload.v != -1);
			// This check is a belt-and-braces with the spec, but it's a useful piece of sanity
			assert((learned_value == -1) || (learned_value == payload.v));
			learned_value = payload.v;
		}
	}
}`,"../../tests/upstream/Tutorial/5_Paxos/PSrc/proposer.p":`type tProposerConfig = (jury: set[Acceptor], school: set[Learner], value_to_propose: int, proposer_id: int);

machine Proposer {
	var jury: set[Acceptor];
	var school: set[Learner];

	// The ballot number we use for the prepares and accepts that this proposer sends out
	var ballot_n: tBallot;
	var highest_proposal_n: tBallot;
	var value_to_propose: tValue;

	// A number equal to more than half the number of acceptors in the jury
	var majority: int;
	// Acceptors we have received prepare ACKs from. This is a set rather than a counter to deal with the
	//  fact that messages can be delivered multiple times in our network model.
	var prepare_acks: set[Acceptor];
	// Acceptors we have recieved accept ACKs from.
	var accept_acks: set[Acceptor];

	start state Init {
		entry (cfg : tProposerConfig) {
			jury = cfg.jury;
			school = cfg.school;
			// For now, use our id as a ballot number
			ballot_n = cfg.proposer_id;
			value_to_propose = cfg.value_to_propose;
			majority = sizeof(jury) / 2 + 1;
			goto Prepare;
		}
	}

	// Phase 1, prepare
	state Prepare {
		entry {
			var acceptor: Acceptor;
			highest_proposal_n = -1;
			// Step 1a is to fire our proposal at the whole jury
			// ReliableBroadcastMajority(jury, ePrepareReq, (proposer = this, ballot_n = ballot_n, v = value_to_propose));
			foreach (acceptor in jury) {
				send acceptor, ePrepareReq, (proposer = this, ballot_n = ballot_n, v = value_to_propose);
			}
		}

		on ePrepareRsp do (rsp: tPrepareRsp) {
			// The jury then gets back to us, saying whether they have accepted our proposal
			if (rsp.promised == ballot_n) {
				// If this acceptor has already accepted a proposal, we drop the one we're proposing and drive that forward instead.
				if (rsp.n_accepted > highest_proposal_n) {
					highest_proposal_n = rsp.n_accepted;
					value_to_propose = rsp.v_accepted;
				}
				// Add this acceptor to the set of acceptors who have ACKed our proposal
				prepare_acks += (rsp.acceptor);
				// If more than half the acceptors have ACKed our proposal, we've been elected 'leader' and its time to move to phase 2
				if (sizeof(prepare_acks) >= majority) {
					goto Accept;
				}
			}
		}
	}

	// Phase 2, accept
	state Accept {
		entry {
			// We start the accept phase by firing off our accept requests at the jury
			// ReliableBroadcastMajority(jury, eAcceptReq, (proposer = this, ballot_n = ballot_n, v = value_to_propose));
			var acceptor: Acceptor;
			foreach (acceptor in jury) {
				send acceptor, eAcceptReq, (proposer = this, ballot_n = ballot_n, v = value_to_propose);
			}
		}

		// Then get reponses from the acceptors
		on eAcceptRsp do (rsp: tAcceptRsp) {
			if (rsp.accepted == ballot_n) {
				// Add this acceptor to the set that have accepted our value
				accept_acks += (rsp.acceptor);
				// And when more than half agree, it's been decided and it's time to teach
				if (sizeof(accept_acks) >= majority) {
					goto Teach;
				}
			}
		}
		// Stale prepares can be safely ignored
		ignore ePrepareRsp;
	}

	// Phase 3, teaching.
	// We diverge a little from the paper here, which puts the acceptors in the teacher role. Doing it here adds a round-trip,
	//  but makes the learners much simpler (they just accept whatever is thrown at them).
	state Teach {
		entry {
			// ReliableBroadcast(school, eLearn, (v=value_to_propose,));
			var learner: Learner;
			foreach (learner in school) {
				send learner, eLearn, (ballot=ballot_n, v=value_to_propose);
			}
		}

		// At this point it's safe to ignore every message
		ignore eAcceptRsp;
		ignore ePrepareRsp;
	}
}

module Paxos = { Proposer, Acceptor, Learner };
`,"../../tests/upstream/Tutorial/5_Paxos/PTst/test.p":`test testBasicPaxos3on5 [main = BasicPaxos3on5]:
	assert OneValueTaught in (union Paxos, { BasicPaxos3on5 });

test testBasicPaxos3on3 [main = BasicPaxos3on3]:
	assert OneValueTaught in (union Paxos, { BasicPaxos3on3 });

test testBasicPaxos3on1 [main = BasicPaxos3on1]:
	assert OneValueTaught in (union Paxos, { BasicPaxos3on1 });

test testBasicPaxos2on3 [main = BasicPaxos2on3]:
	assert OneValueTaught in (union Paxos, { BasicPaxos2on3 });

test testBasicPaxos2on2 [main = BasicPaxos2on2]:
	assert OneValueTaught in (union Paxos, { BasicPaxos2on2 });

test testBasicPaxos1on1 [main = BasicPaxos1on1]:
	assert OneValueTaught in (union Paxos, { BasicPaxos1on1 });

type tPaxosConfig = (n_proposers: int, n_acceptors: int, n_learners: int);

event ePaxosConfig: (quorum: int);

fun SetupPaxos(cfg: tPaxosConfig) {
	var i: int;
	var proposers: set[Proposer];
	var jury: set[Acceptor];
	var school: set[Learner];
	
	var proposerCfg: tProposerConfig;

	announce ePaxosConfig, (quorum = cfg.n_acceptors / 2 + 1,);

	i = 0;
	while (i < cfg.n_acceptors) {
		i = i + 1;
		jury += (new Acceptor());
	}
	i = 0;
	while (i < cfg.n_learners) {
		i = i + 1;
		school += (new Learner());
	}
	i = 0;
	while (i < cfg.n_proposers) {
		i = i + 1;
		proposerCfg = (jury = jury, school = school, value_to_propose = i + 100 + choose(50), proposer_id = i + choose(50));
		proposers += (new Proposer(proposerCfg));
	}
}

machine BasicPaxos3on5 {
	start state Init {
		entry {
			var config: tPaxosConfig;
			config = (n_proposers = 3, n_acceptors = 5, n_learners = 1);
			SetupPaxos(config);
		}
	}
}

machine BasicPaxos3on3 {
	start state Init {
		entry {
			var config: tPaxosConfig;
			config = (n_proposers = 3, n_acceptors = 3, n_learners = 1);
			SetupPaxos(config);
		}
	}
}

machine BasicPaxos3on1 {
	start state Init {
		entry {
			var config: tPaxosConfig;
			config = (n_proposers = 3, n_acceptors = 1, n_learners = 1);
			SetupPaxos(config);
		}
	}
}

machine BasicPaxos2on3 {
	start state Init {
		entry {
			var config: tPaxosConfig;
			config = (n_proposers = 2, n_acceptors = 3, n_learners = 1);
			SetupPaxos(config);
		}
	}
}

machine BasicPaxos2on2 {
	start state Init {
		entry {
			var config: tPaxosConfig;
			config = (n_proposers = 2, n_acceptors = 2, n_learners = 1);
			SetupPaxos(config);
		}
	}
}

machine BasicPaxos1on1 {
	start state Init {
		entry {
			var config: tPaxosConfig;
			config = (n_proposers = 1, n_acceptors = 1, n_learners = 1);
			SetupPaxos(config);
		}
	}
}

machine BasicPaxos4on4 {
	start state Init {
		entry {
			var config: tPaxosConfig;
			config = (n_proposers = 4, n_acceptors = 4, n_learners = 1);
			SetupPaxos(config);
		}
	}
}
`,"../../tests/upstream/Tutorial/6_Raft/PSpec/LivenessClientsDone.p":`spec LivenessClientsDone observes eClientPutRequest, eClientGetRequest, eClientFinishedMonitor {
    var activeClients: set[Client];
    var finishedClients: set[Client];
    
    start state Init {
        entry {
            activeClients = default(set[Client]);
            finishedClients = default(set[Client]);
            goto AllClientsDone;
        }
    }

    hot state ClientsActive {
        on eClientFinishedMonitor do (c: Client) {
            finishedClients += (c);
            if (finishedClients == activeClients) {
                goto AllClientsDone;
            }
        }

        // on eClientRequest do (payload: tClientRequest) {
        //     if (payload.client == payload.sender) {
        //         activeClients += (payload.client);
        //     }
        // }

        on eClientPutRequest do (payload: tClientPutRequest) {
            if (payload.client == payload.sender) {
                activeClients += (payload.client);
            }
        }

        on eClientGetRequest do (payload: tClientGetRequest) {
            if (payload.client == payload.sender) {
                activeClients += (payload.client);
            }
        }
    }

    cold state AllClientsDone {
        on eClientPutRequest do (payload: tClientPutRequest) {
            if (payload.client == payload.sender) {
                activeClients += (payload.client);
                goto ClientsActive;
            }
        }

        on eClientGetRequest do (payload: tClientGetRequest) {
            if (payload.client == payload.sender) {
                activeClients += (payload.client);
                goto ClientsActive;
            }
        }

        on eClientFinishedMonitor do (c: Client) {
            print format("Problematic: {0}", c);
            assert false, "Received a client finished event after all clients are done.";
        }
    }
}`,"../../tests/upstream/Tutorial/6_Raft/PSpec/LivenessProgress.p":`type tRequestMetadata = (client: machine, transId: int);

spec LivenessProgress observes eClientWaitingResponse, eClientGotResponse {
    var clientRequests: set[tRequestMetadata];

    start state Init {
        entry {
            clientRequests = default(set[tRequestMetadata]);
            goto Done;
        }
    }

    hot state PendingRequestsExist {

        on eClientWaitingResponse do (payload: tRequestMetadata) {
            clientRequests += ((client=payload.client, transId=payload.transId));
        }

        on eClientGotResponse do (payload: tRequestMetadata) {
            clientRequests -= ((client=payload.client, transId=payload.transId));
            if (sizeof(clientRequests) == 0) {
                goto Done;
            }
        }

    }

    cold state Done {
        on eClientWaitingResponse do (payload: tRequestMetadata) {
            clientRequests += ((client=payload.client, transId=payload.transId));
            goto PendingRequestsExist;
        }
    }
}`,"../../tests/upstream/Tutorial/6_Raft/PSpec/SafetyLeaderCompleteness.p":`spec SafetyLeaderCompleteness observes eBecomeLeader {
    var committedLogs: set[tServerLog];

    start state Init {
        entry {
            committedLogs = default(set[tServerLog]);
            goto MonitoringLeaderChange;
        }
    }

    state MonitoringLeaderChange {
        on eBecomeLeader do (payload: (term:int, leader:Server, log: seq[tServerLog], commitIndex: int)) {
            // check all already-committed logs is remain committed in the new leader's log
            var i: int;
            var e: tServerLog;
            foreach (e in committedLogs) {
                assert e in payload.log, format("{0} was committed before but not in the new leader's log", e);
            }
            // add the committed logs
            i = 0;
            while (i <= payload.commitIndex) {
                committedLogs += (payload.log[i]);
                i = i + 1;
            }
        }
    }
}`,"../../tests/upstream/Tutorial/6_Raft/PSpec/SafetyLogMatching.p":`spec SafetyLogMatching observes eNotifyLog {
    var allLogs: map[Server, seq[tServerLog]];

    start state MonitorLogUpdates {
        entry {
            allLogs = default(map[Server, seq[tServerLog]]);
        }

        on eNotifyLog do (payload: (timestamp: int, server: Server, log: seq[tServerLog])) {
            var i: int;
            var j: int;
            var s1: Server;
            var s2: Server;
            if (!(payload.server in allLogs)) {
                allLogs[payload.server] = payload.log;
            }
            i = 0;
            while (i < sizeof(keys(allLogs))) {
                s1 = keys(allLogs)[i];
                j = i + 1;
                while (j < sizeof(keys(allLogs))) {
                    s2 = keys(allLogs)[j];
                    if (sizeof(allLogs[s1]) > sizeof(allLogs[s2])) {
                        assert checkLogMatching(allLogs[s2], allLogs[s1]);
                    } else {
                        assert checkLogMatching(allLogs[s1], allLogs[s2]);
                    }
                    j = j + 1;
                }
                i = i + 1;
            }
        }
    }

    fun checkLogMatching(xs: seq[tServerLog], ys: seq[tServerLog]): bool {
        var i: int;
        var highestMatch: int;
        var logsA: seq[tServerLog];
        var logsB: seq[tServerLog];
        if (sizeof(xs) > sizeof(ys)) {
            logsA = ys;
            logsB = xs;
        } else {
            logsA = xs;
            logsB = ys;
        }
        i = sizeof(logsA) - 1;
        while (i >= 0 && logsA[i] != logsB[i]) {
           i = i - 1;
        }
        while (i >= 0) {
            if (logsA[i] != logsB[i]) {
                return false;
            }
            i = i - 1;
        }
        return true;
    }
}
`,"../../tests/upstream/Tutorial/6_Raft/PSpec/SafetyOneLeader.p":`type tBecomeLeader = (term:TermId, leader:Server, log: seq[tServerLog], commitIndex: LogIndex);
event eBecomeLeader: tBecomeLeader;

spec SafetyOneLeader observes eBecomeLeader{
    var termToLeader: map[int, Server];
    start state Init {
        entry {
            termToLeader = default(map[int, Server]);
        }
        on eBecomeLeader do (payload: (term:int, leader:Server, log: seq[tServerLog], commitIndex: int)) {
            if (payload.term in keys(termToLeader)) {
                assert termToLeader[payload.term] == payload.leader, format("At term {0} there are multiple leaders: {1} and {2}.", payload.term, termToLeader[payload.term], payload.leader);
            }
            termToLeader += (payload.term, payload.leader);
        }
    }
}`,"../../tests/upstream/Tutorial/6_Raft/PSpec/SafetyStateMachine.p":`event eEntryApplied: (logIndex: LogIndex, term: TermId, key: KeyT, value: ValueT, transId: TransId, client: Client);

spec SafetyStateMachine observes eEntryApplied {
    var appliedEntries: map[int, tServerLog];
    start state MonitorEntryApplied {
        entry {
            appliedEntries = default(map[int, tServerLog]);
        }

        on eEntryApplied do (payload: (logIndex: LogIndex, term: TermId, key: KeyT, value: ValueT, transId: TransId, client: Client)) {
            var log: tServerLog;
            log = (term=payload.term, key=payload.key, value=payload.value, client=payload.client, transId=payload.transId);
            if (!(payload.logIndex in keys(appliedEntries))) {
                appliedEntries[payload.logIndex] = log;
            }
            assert appliedEntries[payload.logIndex] == log,
                format("Entry@{0}={1} applied before does not match with {2}", payload.logIndex, appliedEntries[payload.logIndex], log);
        }
    }
} `,"../../tests/upstream/Tutorial/6_Raft/PSpec/SafetySynchronization.p":`spec SafetySynchronization observes eClientPutRequest, eClientGetRequest, eRaftGetResponse, eRaftPutResponse, eBecomeLeader {
    var localKVStore: KVStore;
    var requestResultMap: map[Client, map[int, Result]];
    var getRequestMap: map[Client, map[TransId, KeyT]];
    var putRequestMap: map[Client, map[TransId, (key: KeyT, value: ValueT)]];
    var seenId: map[Client, set[int]];
    var respondedId : map[Client, set[int]];
    var currentLeader: Server;
    var term: TermId;

    start state Init {
        entry {
            localKVStore = newStore();
            requestResultMap = default(map[Client, map[int, Result]]);
            getRequestMap = default(map[Client, map[TransId, KeyT]]);
            putRequestMap = default(map[Client, map[TransId, (key: KeyT, value: ValueT)]]);
            seenId = default(map[Client, set[int]]);
            respondedId = default(map[Client, set[int]]);
            currentLeader = default(Server);
            term = -1;
            goto Listening;
        }
    }

    state Listening {
        on eBecomeLeader do (payload: tBecomeLeader) {
            if (payload.term > term) {
                currentLeader = payload.leader;
                term = payload.term;
            }
        }

        on eClientGetRequest do (payload: tClientGetRequest) {
            if (!(payload.client in keys(seenId))) {
                seenId[payload.client] = default(set[int]);
            }
            if (!(payload.client in keys(respondedId))) {
                respondedId[payload.client] = default(set[int]);
            }
            if (!(payload.client in keys(getRequestMap))) {
                getRequestMap[payload.client] = default(map[TransId, KeyT]);
            }
            if (payload.client == payload.sender && !(payload.transId in seenId[payload.client])) {
                seenId[payload.client] += (payload.transId);
            }
            getRequestMap[payload.client][payload.transId] = payload.key;
        }

        on eClientPutRequest do (payload: tClientPutRequest) {
            if (!(payload.client in keys(seenId))) {
                seenId[payload.client] = default(set[int]);
            }
            if (!(payload.client in keys(respondedId))) {
                respondedId[payload.client] = default(set[int]);
            }
            if (!(payload.client in keys(putRequestMap))) {
                putRequestMap[payload.client] = default(map[TransId, (key: KeyT, value: ValueT)]);
            }
            if (payload.client == payload.sender && !(payload.transId in seenId[payload.client])) {
                seenId[payload.client] += (payload.transId);
            }
            putRequestMap[payload.client][payload.transId] = (key=payload.key, value=payload.value);
        }

        on eRaftGetResponse do (payload: tRaftGetResponse) {
            var execResult: ExecutionResult;
            checkResponseValid(payload.client, payload.sender, payload.transId);
            if (!(payload.client in keys(respondedId)) && currentLeader == payload.sender) {
                respondedId[payload.client] += (payload.transId);
                execResult = executeGet(localKVStore, getRequestMap[payload.client][payload.transId]);
                assert execResult.result.success == payload.success, format("Inconsistent status: {0}", getRequestMap[payload.client][payload.transId]);
                assert execResult.result.value == payload.value, format("Inconsistent Get result! Expected {0}, got {1}", execResult.result.value, payload.value);
            }
        }

        on eRaftPutResponse do (payload: tRaftPutResponse) {
            var execResult: ExecutionResult;
            checkResponseValid(payload.client, payload.sender, payload.transId);
            if (!(payload.client in keys(respondedId)) && currentLeader == payload.sender) {
                respondedId[payload.client] += (payload.transId);
                assert payload.client in keys(putRequestMap);
                execResult = executePut(localKVStore, putRequestMap[payload.client][payload.transId].key, putRequestMap[payload.client][payload.transId].value);
                localKVStore = execResult.newState;
            }
        }
    }
    fun checkResponseValid(client: Client, sender: Server, tId: TransId) {
        assert currentLeader != default(Server), "Leader is un-initialized but got an eRaftResponse!";
        if (currentLeader == sender) {
            assert client in keys(seenId), format("Responding to a client that has not sent any request: {0}", client);
            assert tId in seenId[client], format("Responding to a non-existing transactionId {0} sent by {1}", tId, client);
        }
    }
}
`,"../../tests/upstream/Tutorial/6_Raft/PSrc/Application.p":`enum Op { 
    PUT,
    GET
}
type KeyT = int;
type ValueT = int;
type Command = (op: Op, key: KeyT, value: ValueT);
type Result = (success: bool, value: ValueT);
type ExecutionResult = (newState: KVStore, result: Result);
type KVStore = map[KeyT, ValueT];

fun IsPut(op: Op): bool {
    return op == PUT;
}

fun IsGet(op: Op): bool {
    return op == GET;
}

fun newStore(): KVStore {
    return default(KVStore);
}

fun executeGet(store: KVStore, key: KeyT): ExecutionResult {
    if (!(key in store)) {
        return (newState=store, result=(success=false, value=-1));
    }
    return (newState=store, result=(success=true, value=store[key]));
}

fun executePut(store: KVStore, key: KeyT, value: ValueT): ExecutionResult {
    store[key] = value;
    return (newState=store, result=(success=true, value=-1));
}
`,"../../tests/upstream/Tutorial/6_Raft/PSrc/Client.p":`/*
* A client that uses the Raft cluster.
* The client sends a sequence of commands to the cluster.
* It does not move on the next command until it receives a response from the cluster.
* The client retries sending the command if a certain amount of heartbeats timeouts occur.
*/

// Client requests
type tClientRequest = (transId: TransId, client: Client, cmd: Command, sender: Client);
type tClientPutRequest = (transId: TransId, client: Client, key: KeyT, value: ValueT, sender: Client);
type tClientGetRequest = (transId: TransId, client: Client, key: KeyT, sender: Client);
event eClientPutRequest: tClientPutRequest;
event eClientGetRequest: tClientGetRequest;
// event eClientRequest: tClientRequest;
// The event of notifying the monitor that the client is waiting for a response
event eClientWaitingResponse: (client: Client, transId: int);
// The event of notifying the monitor that the client got a response for a transaction
event eClientGotResponse: (client: Client, transId: int);
// The event of notifying the monitor that the client finished
event eClientFinishedMonitor: Client;
// The event of notifying the view service that the client finished
event eClientFinished: Client;

machine Client {
    var worklist: seq[Command];
    var servers: set[machine];
    var ptr: int;
    var tId: int;
    var retries: int;
    var currentCmd: Command;
    var view: View;
    var timer: Timer;

    start state Init {
        entry (config: (viewService: View, servers: set[machine], requests: seq[Command])) {
            worklist = config.requests;
            servers = config.servers;
            view = config.viewService;
            ptr = 0;
            tId = 0;
            timer = new Timer((user=this, timeoutEvent=eHeartbeatTimeout));
            goto SendOne;
        }
    }

    state SendOne {
        entry {
            var cmd: Command;
            // print format("{0} is at {1}", this, ptr);
            // print format("Worklist {0}", worklist);
            if (sizeof(worklist) == ptr) {
                // if no more work to do, go to Done
                goto Done;
            } else {
                // get the current command and increase transaction id
                currentCmd = worklist[ptr];
                ptr = ptr + 1;
                tId = tId + 1;
                broadcastToCluster();
                goto WaitForResponse;
            }
        }
    }

    state WaitForResponse {
        entry {
            announce eClientWaitingResponse, (client=this, transId=tId);
            startTimer(timer);
            retries = 0;
        }

        on eHeartbeatTimeout do {
            // print format("Client {0} timed out waiting for response {1}; current retries: {2}", this, tId, retries / 50);
            if (retries % 200 == 0) {
                // retries every 200 heartbeats
                broadcastToCluster();
            }
            retries = retries + 1;
            startTimer(timer);
        }

        on eRaftGetResponse do (resp: tRaftGetResponse) {
            handleResponse(resp.transId);
        }

        on eRaftPutResponse do (resp: tRaftPutResponse) {
            handleResponse(resp.transId);
        }
    }

    fun handleResponse(responseTransId: TransId) {
        if (responseTransId == tId) {
            announce eClientGotResponse, (client=this, transId=tId);
            retries = 0;
            goto SendOne;
        }
    }

    fun broadcastToCluster() {
        var s: machine;
        foreach (s in servers) {
            if (currentCmd.op == GET) {
                send s, eClientGetRequest, (transId=tId, client=this, key=currentCmd.key, sender=this);
            } else {
                send s, eClientPutRequest, (transId=tId, client=this, key=currentCmd.key, value=currentCmd.value, sender=this);
            }
        }
    }

    state Done {
        entry {
            announce eClientFinishedMonitor, this;
            send view, eClientFinished, this;
        }
        ignore eRaftGetResponse, eRaftPutResponse, eHeartbeatTimeout;
    }
}`,"../../tests/upstream/Tutorial/6_Raft/PSrc/RaftModules.p":`module Server = { Server };
module Timer = { Timer };
module Client = { Client };
module View = { View };`,"../../tests/upstream/Tutorial/6_Raft/PSrc/Server.p":`/*****************************************************************************
* A node in the Raft cluster.
* The implementation follows the Raft paper (Ongaro and Ousterhout, 2014).
* This implementation does not include log compaction / cluster reconfiguration.
******************************************************************************/
type ServerId = int;
type TransId = int;

// Response message to the client
type tRaftResponse = (sender: Server, client: Client, transId: TransId, result: Result);
// event eRaftResponse: tRaftResponse;
type tRaftPutResponse = (sender: Server, client: Client, transId: TransId, key: KeyT);
type tRaftGetResponse = (sender: Server, client: Client, success: bool, transId: TransId, value: ValueT);
// type tRaftGetError = (sender: Server, client: Client, transId: TransId, key: KeyT);
event eRaftPutResponse: tRaftPutResponse;
event eRaftGetResponse: tRaftGetResponse;
// event eRaftGetError: tRaftGetError;

event ePutReq: (client: Client, transId: TransId, key: KeyT, value: ValueT);
event eGetReq: (client: Client, transId: TransId, key: KeyT);
event ePutResp: (client: Client, transId: TransId, key: KeyT, value: ValueT);
event eGetResp: (client: Client, transId: TransId, success: bool, key: KeyT, value: ValueT);

// Server initialization message
event eServerInit: (myId: ServerId, cluster: set[Server], viewServer: View);

// Vote request and response
type TermId = int;
type LogIndex = int;
type tRequestVote = (term: TermId, candidate: Server, lastLogIndex: LogIndex, lastLogTerm: TermId);
event eRequestVote: tRequestVote;
type tRequestVoteReply = (sender: Server, term: TermId, voteGranted: bool);
event eRequestVoteReply: tRequestVoteReply;

// Heartbeat message and response
type tAppendEntries = (term: TermId, leader: Server, prevLogIndex: LogIndex,
                       prevLogTerm: TermId, entries: seq[tServerLog], leaderCommit: LogIndex);

event eAppendEntries: tAppendEntries;
type tAppendEntriesReply = (sender: Server, term: TermId, success: bool, matchedIndex: LogIndex);
event eAppendEntriesReply: tAppendEntriesReply;

// Resetting a server (to model crashes)
event eReset;

// events to notify the server role change to the view server
event eViewChangedFollower: Server;
event eViewChangedLeader: Server;
event eViewChangedCandidate: Server;

// server logs
type tServerLog = (term: TermId, key: KeyT, value: ValueT, client: Client, transId: TransId);

fun TermLT(t1: TermId, t2: TermId): bool {
    return t1 < t2;
}

fun IsSuccess(s: bool): bool {
    return s;
}

fun IsFailure(s: bool): bool {
    return !s;
}

// A node in the Raft cluster
machine Server {
    var serverId: ServerId;
    // the application state
    var kvStore: KVStore;
    // the leader of the cluster
    var leader: Server;
    // size of the cluster
    var clusterSize: int;
    // nodes in the cluster
    var peers: set[Server];
    // the view service
    var viewServer: View;

    // Leader state (volatile)
    var nextIndex: map[Server, int];
    var matchIndex: map[Server, int];
    // Leader state (persistent)
    var currentTerm: int;
    var logs: seq[tServerLog];

    // States for voting
    var votedFor: Server;
    var votesReceived: set[Server];

    // all servers
    var commitIndex: int;
    var lastApplied: int;

    var role: string;
    var logNotifyTimestamp: int;

    // var heartbeatTimer: Timer;
    // var clientRequestQueue: seq[tClientRequest];
    var requestSeqNum: int;
    var clientPutRequestQueue: seq[(seqNum: int, payload: tClientPutRequest)];
    var clientGetRequestQueue: seq[(seqNum: int, payload: tClientGetRequest)];
    var clientPutRequestCache: map[Client, map[int, tRaftPutResponse]];
    var clientGetRequestCache: map[Client, map[int, tRaftGetResponse]];

    start state Init {
        entry {}
        on eServerInit do (setup: (myId: ServerId, cluster: set[Server], viewServer: View)) {
            kvStore = newStore();
            serverId = setup.myId;
            assert this in setup.cluster, "Server should be in the cluster";
            clusterSize = sizeof(setup.cluster);
            peers = setup.cluster;
            nextIndex = default(map[Server, int]);
            matchIndex = default(map[Server, int]);
            votesReceived = default(set[Server]);
            logs = default(seq[tServerLog]);
            // clientRequestQueue = default(seq[tClientRequest]);
            clientPutRequestQueue = default(seq[(seqNum: int, payload: tClientPutRequest)]);
            clientGetRequestQueue = default(seq[(seqNum: int, payload: tClientGetRequest)]);
            requestSeqNum = 0;
            clientPutRequestCache = default(map[Client, map[int, tRaftPutResponse]]);
            clientGetRequestCache = default(map[Client, map[int, tRaftGetResponse]]);
            viewServer = setup.viewServer;

            currentTerm = 0;
            logNotifyTimestamp = 0;
            votedFor = default(Server);
            commitIndex = -1;
            lastApplied = -1;
            leader = default(Server);
            role = "Uninitialized";
            goto Follower;
        }
        ignore eReset, eElectionTimeout, eHeartbeatTimeout;
    }

    state Follower {
        entry {
            role = "F";
            // reset the leader upon being a follower
            leader = default(Server);
            printLog("Become follower");
            send viewServer, eViewChangedFollower, this;
        }

        on eShutdown do {
            goto FinishedServing;
        }

        on eReset do {
            reset();
        }

        on eRequestVote do (payload: tRequestVote) {
            handleRequestVote(payload);
        }

        on eClientPutRequest do (payload: tClientPutRequest) {
            // check if the request has been processed
            respondOrForwardPut(payload);
        }

        on eClientGetRequest do (payload: tClientGetRequest) {
            respondOrForwardGet(payload);
        }

        on eAppendEntries do (payload: tAppendEntries) {
            if (payload.term >= currentTerm) {
                printLog(format("heartbeat updated term from {0} to {1}", currentTerm, payload.term));
                if (leader != default(Server) && payload.leader != leader) {
                    votedFor = default(Server);
                }
                leader = payload.leader;
                updateTermAndVote(payload.term);
            }
            handleAppendEntries(payload);
            if (leader != default(Server)) {
                // propagate client requests to the leader
                drainRequestQueuesTo(leader);
            }
        }

        on eElectionTimeout do {
            printLog("Election timeout");
            goto Candidate;
        }

        on eHeartbeatTimeout do {
            notifyViewLog();
        }

        ignore eRequestVoteReply, eAppendEntriesReply;
    }

    state Candidate {
        entry {
            var peer: Server;
            var lastTerm: int;
            role = "C";
            printLog("Become candidate");
            send viewServer, eViewChangedCandidate, this;
            currentTerm = currentTerm + 1;
            votedFor = default(Server);
            votesReceived = default(set[Server]);
            votesReceived += (this);
            // if there is only 1 server, no need to request vote
            if (sizeof(votesReceived) > clusterSize / 2) {
                goto Leader;
            } else {
                printLog(format("RequestVote with term {0}", currentTerm));
                broadcastRequest(this, peers, eRequestVote,
                                    (term=currentTerm,
                                        candidate=this,
                                        lastLogIndex=lastLogIndex(logs),
                                        lastLogTerm=lastLogTerm(logs)));
            }
        }

        on eClientPutRequest do (payload: tClientPutRequest) {
            if (checkClientRequestCache(payload.client, payload.transId)) {
                return;
            }
            clientPutRequestQueue += (sizeof(clientPutRequestQueue), (seqNum=requestSeqNum, payload=payload));
            requestSeqNum = requestSeqNum + 1;
        }

        on eShutdown do {
            goto FinishedServing;
        }

        on eAppendEntries do (payload: tAppendEntries) {
            if (payload.term >= currentTerm) {
                // if there is a leader, convert to follower
                printLog(format("Received heartbeat from a leader with higher term {0}", payload.term));
                updateTermAndVote(payload.term);
                handleAppendEntries(payload);
                goto Follower;
            } else {
                printLog(format("Received smaller term heartbeat from {0}", payload.leader));
                handleAppendEntries(payload);
            }
        }

        on eAppendEntriesReply do (payload: tAppendEntriesReply) {
            if (payload.term > currentTerm) {
                printLog(format("Received AppendEntriesReply with higher term {0}", payload.term));
                updateTermAndVote(payload.term);
                goto Follower;
            }
        }

        on eRequestVoteReply do (payload: tRequestVoteReply) {
            if (payload.term > currentTerm) {
                // if there is a vote with higher term, convert to follower
                updateTermAndVote(payload.term);
                goto Follower;
            } else if (payload.voteGranted && payload.term == currentTerm) {
                // if the vote is granted for me in this term, count it.
                votesReceived += (payload.sender);
                printLog(format("vote granted by {0}; now vote={1}", payload.sender, votesReceived));
                if (sizeof(votesReceived) > clusterSize / 2) {
                    // if majority votes are received, become a leader
                    printLog(format("majority votes: {0}", votesReceived));
                    goto Leader;
                }
            } else {
                printLog(format("Received rejection/invalid reply: {0}", payload));
            }
        }
        
        on eRequestVote do (payload: tRequestVote)  {
            if (payload.term > currentTerm) {
                updateTermAndVote(payload.term);
                handleRequestVote(payload);
                goto Follower;
            } else {
                send payload.candidate, eRequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
            }
        }

        on eReset do {
            reset();
        }

        on eElectionTimeout do {
            goto Candidate;
        }

        on eHeartbeatTimeout do {
            notifyViewLog();
        }

        ignore eClientGetRequest;
    }

    state Leader {
        entry {
            role = "L";
            send viewServer, eViewChangedLeader, this;
            leader = this;
            // instantiate the nextIndex and matchIndex
            nextIndex = fillMap(this, nextIndex, peers, sizeof(logs));
            matchIndex = fillMap(this, matchIndex, peers, -1);
            announce eBecomeLeader, (term=currentTerm, leader=this, log=logs, commitIndex=commitIndex);
            // first exhaust the client request queu on its own
            drainRequestQueuesTo(this);
            printLog(format("Become leader with Log={0}, commiteIndex={1}, lastApplied={2}", logs, commitIndex, lastApplied));
            broadcastAppendEntries();
        }

        on eShutdown do {
            goto FinishedServing;
        }

        on eRequestVote do (payload: tRequestVote) {
            if (currentTerm < payload.term) {
                printLog(format("Saw higher term as a leader: currentTerm={0}, payload.term={1}", currentTerm, payload.term));
                handleRequestVote(payload);
                becomeFollower(payload.term);
            } else {
                printLog(format("Reject proposal from {0} with term {1}", payload.candidate, payload.term));
                send payload.candidate, eRequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
            }
        }

        on eRequestVoteReply do (payload: tRequestVoteReply) {
            if (payload.term > currentTerm) {
                printLog(format("received RequestVoteReply with higher term {0}", payload.term));
                updateTermAndVote(payload.term);
                goto Follower;
            }
        }

        on eHeartbeatTimeout do {
            broadcastAppendEntries();
            leaderCommits();
            notifyViewLog();
        }

        on eAppendEntries do (payload: tAppendEntries) {
            if (payload.term > currentTerm) {
                // if the leader receives an AppendEntries from another leader with a higher term
                // step down to a follower
                printLog(format("received AppendEntries with higher term {0}; handle then step down", payload.term));
                updateTermAndVote(payload.term);
                handleAppendEntries(payload);
                becomeFollower(payload.term);
            } else if (payload.term < currentTerm) {
                send payload.leader, eAppendEntriesReply, (sender=this, term=currentTerm, success=false, matchedIndex=-1);
                leaderCommits();
            }
        }

        on eAppendEntriesReply do (payload: tAppendEntriesReply) {
            printLog(format("Leader({0}, {1}) received AppendEntriesReply from {2}: {3}", this, currentTerm, payload.sender, payload));
            if (payload.term < currentTerm) {
                printLog("Ignore AppendEntriesReply with outdated term");
                return;
            }
            if (payload.term > currentTerm) {
                // if the leader receives a reply from another leader with a higher term
                // step down to a follower
                printLog(format("received reply from another leader with term {0} > currentTerm {1}", payload.term, currentTerm));
                updateTermAndVote(payload.term);
                goto Follower;
            }
            if (payload.success) {
                // if the heartbeat is accepted,
                // update the nextIndex and matchIndex for the corresponding node.
                printLog(format("entries accepted by {0}; matchIndex={1}", payload.sender, payload.matchedIndex));
                // Note that the leader can receive delayed message, so here we need to keep the
                // larger values of nextIndex and matchIndex.
                nextIndex[payload.sender] = Max(nextIndex[payload.sender], payload.matchedIndex + 1);
                matchIndex[payload.sender] = Max(matchIndex[payload.sender], payload.matchedIndex);
                leaderCommits();
            } else {
                if (payload.matchedIndex < 0) {
                    // a rejection of me as a leader in an earlier term
                    // this can happen when me and the other node simultaneously become a candidate
                    // the other node received my heartbeat from the previous term and sent me a rejection
                    // but currently, I am already at the newer term, so I can safely ignore this rejection
                    printLog(format("outdated rejection with term={0}, matchedIndex={1}", payload.term, payload.matchedIndex));
                    return;
                }
                // rejected because of log mismatch
                // now, payload.matchedIndex is the index of the (potentially) first mismatched term
                printLog(format("re-sync with {0} at index {1}", payload.sender, payload.matchedIndex));
                // still, the leader can receive outdated message. To be conservative, it is safe to synchronize from
                // a smaller index.
                nextIndex[payload.sender] = Min(payload.matchedIndex, nextIndex[payload.sender]);
            }
        }

        on eClientGetRequest do (payload: tClientGetRequest) {
            var execResult: ExecutionResult;
            if (checkClientRequestCache(payload.client, payload.transId)) {
                return;
            }
            announce eGetReq, (client=payload.client, transId=payload.transId, key=payload.key);
            execResult = executeGet(kvStore, payload.key);
            send payload.client, eRaftGetResponse, (sender=this, client=payload.client, success=execResult.result.success, transId=payload.transId, value=execResult.result.value);
            announce eGetResp, (client=payload.client, transId=payload.transId, success=execResult.result.success, key=payload.key, value=execResult.result.value);
            if (!(payload.client in keys(clientGetRequestCache))) {
                clientGetRequestCache[payload.client] = default(map[int, tRaftGetResponse]);
            }
            clientGetRequestCache[payload.client][payload.transId] = (sender=this, client=payload.client, success=execResult.result.success, transId=payload.transId, value=execResult.result.value);
        }

        on eClientPutRequest do (payload: tClientPutRequest) {
            var newEntry: tServerLog;
            var target: Server;
            var entries: seq[tServerLog];
            var i: int;
            // print format("Received client request {0}", payload);
            if (checkClientRequestCache(payload.client, payload.transId)) {
                return;
            }
            newEntry = (term=currentTerm, key=payload.key, value=payload.value, client=payload.client, transId=payload.transId);
            if (!(newEntry in logs)) {
                logs += (sizeof(logs), newEntry);
                announce ePutReq, (client=payload.client, transId=payload.transId, key=payload.key, value=payload.value);
            }
            printLog(format("Current leader logs={0}", logs));
            printLog(format("nextIndex={0}, matchIndex={1}", nextIndex, matchIndex));
            // use info of nextIndex and matchIndex to broadcast to peers
            foreach (target in peers) {
                if (target != this && nextIndex[target] < sizeof(logs)) {
                    entries = default(seq[tServerLog]);
                    i = nextIndex[target];
                    while (i < sizeof(logs)) {
                        entries += (i - nextIndex[target], logs[i]);
                        i = i + 1;
                    }
                    send target, eAppendEntries, (term=currentTerm,
                                                leader=this,
                                                prevLogIndex=nextIndex[target] - 1,
                                                prevLogTerm=getLogTerm(logs, nextIndex[target] - 1),
                                                entries=entries,
                                                leaderCommit=commitIndex);
                }
            }
            leaderCommits();
        }

        on eReset do {
            reset();
        }

        ignore eElectionTimeout;
    }

    state FinishedServing {
        entry {
            printLog("Service ended. Shutdown.");
        }
        ignore eShutdown, eRequestVote, eRequestVoteReply, eAppendEntries, eAppendEntriesReply, eClientPutRequest, eClientGetRequest, eHeartbeatTimeout, eElectionTimeout;
    }

    fun drainRequestQueuesTo(target: Server) {
        while (sizeof(clientPutRequestQueue) > 0 || sizeof(clientGetRequestQueue) > 0) {
            if (sizeof(clientPutRequestQueue) == 0) {
                send target, eClientGetRequest, forwardedGetRequest(clientGetRequestQueue[0].payload);
                clientGetRequestQueue -= (0);
            } else if (sizeof(clientGetRequestQueue) == 0) {
                send target, eClientPutRequest, forwardedPutRequest(clientPutRequestQueue[0].payload);
                clientPutRequestQueue -= (0);
            } else {
                if (clientPutRequestQueue[0].seqNum < clientGetRequestQueue[0].seqNum) {
                    send target, eClientPutRequest, forwardedPutRequest(clientPutRequestQueue[0].payload);
                    clientPutRequestQueue -= (0);
                } else {
                    send target, eClientGetRequest, forwardedGetRequest(clientGetRequestQueue[0].payload);
                    clientGetRequestQueue -= (0);
                }
            }
        }
    }

    fun respondOrForwardPut(payload: tClientPutRequest) {
        if (checkClientRequestCache(payload.client, payload.transId)) {
            return;
        }
        if (leader != default(Server)) {
            send leader, eClientPutRequest, forwardedPutRequest(payload);
        } else {
            clientPutRequestQueue += (sizeof(clientPutRequestQueue), (seqNum=requestSeqNum, payload=payload));
            requestSeqNum = requestSeqNum + 1;
        }
    }

    fun respondOrForwardGet(payload: tClientGetRequest) {
        if (checkClientRequestCache(payload.client, payload.transId)) {
            return;
        }
        if (leader != default(Server)) {
            send leader, eClientGetRequest, forwardedGetRequest(payload);
        } else {
            clientGetRequestQueue += (sizeof(clientGetRequestQueue), (seqNum=requestSeqNum, payload=payload));
            requestSeqNum = requestSeqNum + 1;
        }
    }

    fun forwardedPutRequest(req: tClientPutRequest): tClientPutRequest {
        return (transId=req.transId, client=req.client, key=req.key, value=req.value, sender=this);
    }

    fun forwardedGetRequest(req: tClientGetRequest): tClientGetRequest {
        return (transId=req.transId, client=req.client, key=req.key, sender=this);
    }

    fun handleRequestVote(reply: tRequestVote) {
        // A handler for eRequestVote messages
        leader = default(Server);
        if (reply.term < currentTerm) {
            // reject a vote with a smaller term
            printLog(format("Reject vote with term {0} < currentTerm {1}", reply.term, currentTerm));
            send reply.candidate, eRequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
        } else {
            updateTermAndVote(reply.term);
            // if votedFor is null or the candidate it has voted for
            // and the log is up-to-date, grant the vote. Reject otherwise.
            if ((votedFor == default(Server) || votedFor == reply.candidate)
                    && logUpToDateCheck(reply.lastLogIndex, reply.lastLogTerm)) {
                printLog(format("Grant vote to {0} (prev. votedFor={1})", reply.candidate, votedFor));
                votedFor = reply.candidate;
                send reply.candidate, eRequestVoteReply, (sender=this, term=currentTerm, voteGranted=true);
            } else {
                printLog(format("Reject vote with votedFor={0} and logUpToDate={1}", votedFor, logUpToDateCheck(reply.lastLogIndex, reply.lastLogTerm)));
                send reply.candidate, eRequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
            }
        }
    }

    fun handleAppendEntries(resp: tAppendEntries) {
        // handler of leader heartbeat, log synchronization
        var myLastIndex: int;
        var myLastTerm: int;
        var ptr: int;
        var mismatchedTerm: int;
        var i: int;
        var j: int;
        printLog(format("Received AppendEntries({0}) from Leader({1}) with {2}; currentTerm={3}, log={4}", resp.entries, resp.leader, resp, currentTerm, logs));
        if (resp.term < currentTerm) {
            // if the heartbeat is from a leader with an outdated term, reject the heartbeat
            printLog(format("Reject AppendEntries with term {0} < currentTerm {1}", resp.term, currentTerm));
            send resp.leader, eAppendEntriesReply, (sender=this, term=currentTerm,
                                                    success=false, matchedIndex=-1);
        } else if (sizeof(logs) <= resp.prevLogIndex) {
            // If the log is very outdated, re-sync from the end of my log
            leader = resp.leader;
            printLog(format("prevLogIndex={0} is out of range of sizeof(log)={1}", resp.prevLogIndex, sizeof(logs)));
            send resp.leader, eAppendEntriesReply, (sender=this, term=currentTerm, success=false, matchedIndex=sizeof(logs));
        } else if (resp.prevLogIndex >= 0 && logs[resp.prevLogIndex].term != resp.prevLogTerm) {
            // Log consistency check failed here;
            // search for the first occurence of the mismatched
            // term and notify the leader.
            printLog(format("Log inconsistency at index {0}: log[i].term={1} v.s. prevTerm={2}", resp.prevLogIndex, logs[resp.prevLogIndex].term, resp.prevLogTerm));
            leader = resp.leader;
            ptr = resp.prevLogIndex;
            mismatchedTerm = logs[ptr].term;
            // find the first occurrence of the mismatched term
            // this includes the optimization described in the paper
            while (ptr > 0 && logs[ptr].term == mismatchedTerm) {
                ptr = ptr - 1;
            }
            printLog(format("requesting sync. log terms={0}, mismatchedTerm={1}, prevLogIndex={2}, ptr={3}", termsOfLog(), mismatchedTerm, resp.prevLogIndex, ptr));
            send resp.leader, eAppendEntriesReply, (sender=this, term=currentTerm,
                                                    success=false, matchedIndex=ptr);
        } else {
            // Check if any entries disagree; delete all entries after it.
            printLog(format("Sync log with log={0} at index {1}. Entries={2}", logs, resp.prevLogIndex + 1, resp.entries));
            leader = resp.leader;
            i = resp.prevLogIndex + 1;
            j = 0;
            printLog(format("Before sync: logs={0}", logs));
            // find the first mismatched entry
            while (i < sizeof(logs) && j < sizeof(resp.entries)) {
                if (logs[i].term != resp.entries[j].term) {
                    break;
                }
                i = i + 1;
                j = j + 1;
            }
            // delete all entries after the mismatched entry
            while (i < sizeof(logs)) {
                logs -= (lastLogIndex(logs));
            }
            printLog(format("Inconsistency removal: logs={0}", logs));
            // Append entries from the leader
            j = 0;
            while (j < sizeof(resp.entries)) {
                if (!(resp.entries[j] in logs)) {
                    logs += (resp.prevLogIndex + 1 + j, resp.entries[j]);
                } else {
                    assert resp.entries[j] == logs[resp.prevLogIndex + 1 + j], "Inconsistent log entry";
                }
                j = j + 1;
            }
            printLog(format("After sync: logs={0}, leaderCommit={1}, commitIndex={2}", logs, resp.leaderCommit, commitIndex));
            if (resp.leaderCommit > commitIndex) {
                if (resp.leaderCommit > lastLogIndex(logs)) {
                    commitIndex = lastLogIndex(logs);
                } else {
                    commitIndex = resp.leaderCommit;
                }
            }
            executeCommands();
            send resp.leader, eAppendEntriesReply, (sender=this, term=currentTerm, success=true, matchedIndex=lastLogIndex(logs));
        }
    }
    
    fun broadcastAppendEntries() {
        // broadcasting of leader heartbeats
        var target: Server;
        var i: int;
        var j: int;
        var prevIndex: int;
        var prevTerm: int;
        var entries: seq[tServerLog];
        assert this == leader, "Only leader can broadcast AppendEntries";
        // print format("Broadcasting AppendEntries with log {0}", logs);
        foreach (target in peers) {
            if (this != target) {
                entries = default(seq[tServerLog]);
                prevIndex = nextIndex[target] - 1;
                // get prevIndex and prevIndexTerm
                if (prevIndex >= 0 && prevIndex < sizeof(logs)) {
                    prevTerm = logs[prevIndex].term;
                } else {
                    prevTerm = 0;
                }
                if (nextIndex[target] < sizeof(logs)) {
                    // if the current lead has something to send (nextIndex within range of logs)
                    i = 0;
                    j = nextIndex[target];
                    // aggregate the logs to be synchronized
                    while (j < sizeof(logs)) {
                        entries += (i, logs[j]);
                        i = i + 1;
                        j = j + 1;
                    }
                    printLog(format("Entries for {0} is {1}", target, entries));
                    send target, eAppendEntries, (term=currentTerm,
                                                    leader=this,
                                                    prevLogIndex=prevIndex,
                                                    prevLogTerm=prevTerm,
                                                    entries=entries,
                                                    leaderCommit=commitIndex);
                } else {
                    // send empty heartbeat
                    send target, eAppendEntries, (term=currentTerm,
                                                    leader=this,
                                                    prevLogIndex=prevIndex,
                                                    prevLogTerm=prevTerm,
                                                    entries=entries,
                                                    leaderCommit=commitIndex);

                }
            }
        }
    }

    fun leaderCommits() {
        // leader committing log entries
        var execResult: ExecutionResult;
        // the next commit index
        var nextCommit: int;
        // the number of match indices that are greater than or equal to nextCommit
        var validMatchIndices: int;
        var i: int;
        var target: Server;
        assert this == leader, "Only leader can execute the log on its own";
        // iteratively search for that index
        nextCommit = lastLogIndex(logs);
        printLog(format("state: nextCommit={0} commitIndex={1} matchIndex={2}", nextCommit, commitIndex, matchIndex));
        // Find the largest nextCommit such that the majority of matchIndex is greater than or equal to nextCommit
        while (nextCommit > commitIndex) {
            // counting itself first
            validMatchIndices = 1;
            foreach (target in peers) {
                if (target == this) {
                    continue;
                }
                if (matchIndex[target] >= nextCommit) {
                    validMatchIndices = validMatchIndices + 1;
                }
            }
            // if the majority of matchIndex is greater than or equal to nextCommit
            // also the term is currentTerm in this entry
            if (validMatchIndices > clusterSize / 2 && logs[nextCommit].term == currentTerm) {
                commitIndex = nextCommit;
                break;
            }
            nextCommit = nextCommit - 1;
        }
        // commitIndex = nextCommit;
        printLog(format("leader commits decision: matchIndex={0} commitIndex={1} currentTerm={2} logs={3}", matchIndex, commitIndex, currentTerm, logs));
        // commit all the logs from lastApplied + 1 to commitIndex
        while (lastApplied < commitIndex) {
            lastApplied = lastApplied + 1;
            announce eEntryApplied, (logIndex=lastApplied, term=logs[lastApplied].term, key=logs[lastApplied].key, value=logs[lastApplied].value, transId=logs[lastApplied].transId, client=logs[lastApplied].client);
            execResult = executePut(kvStore, logs[lastApplied].key, logs[lastApplied].value);
            kvStore = execResult.newState;
            printLog(format("leader committed and processed (by {0}), log: {1}", this, logs[lastApplied]));
            // execute the command and send the result back to the client
            announce ePutResp, (client=logs[lastApplied].client, transId=logs[lastApplied].transId, key=logs[lastApplied].key, value=logs[lastApplied].value);
            send logs[lastApplied].client, eRaftPutResponse, (sender=this, client=logs[lastApplied].client,
                                                                transId=logs[lastApplied].transId, key=logs[lastApplied].key);
            // update the cache
            if (!(logs[lastApplied].client in keys(clientPutRequestCache))) {
                clientPutRequestCache[logs[lastApplied].client] = default(map[int, tRaftPutResponse]);
            }
            clientPutRequestCache[logs[lastApplied].client][logs[lastApplied].transId] = (sender=this, client=logs[lastApplied].client, transId=logs[lastApplied].transId, key=logs[lastApplied].key);
        }
    }

    fun checkClientRequestCache(client: Client, transactionId: TransId): bool {
        // if the result is already in the cache, send it back
        if (client in clientGetRequestCache && transactionId in keys(clientGetRequestCache[client])) {
            send client, eRaftGetResponse, clientGetRequestCache[client][transactionId];
            return true;
        }
        if (client in clientPutRequestCache && transactionId in keys(clientPutRequestCache[client])) {
            send client, eRaftPutResponse, clientPutRequestCache[client][transactionId];
            return true;
        }
        return false;
    }

    fun printLog(msg: string) {
        // return;
        // print format("{0}[{1}@{2}]: {3}", role, this, currentTerm, msg);
    }

    fun executeCommands() {
        var execResult: ExecutionResult;
        printLog(format("commitIndex={0} lastApplied={1}", commitIndex, lastApplied));
        // Execute the command from lastApplied + 1 to commitIndex
        while (lastApplied < commitIndex) {
            lastApplied = lastApplied + 1;
            announce eEntryApplied, (logIndex=lastApplied, term=logs[lastApplied].term, key=logs[lastApplied].key, value=logs[lastApplied].value, transId=logs[lastApplied].transId, client=logs[lastApplied].client);
            execResult = executePut(kvStore, logs[lastApplied].key, logs[lastApplied].value);
            kvStore = execResult.newState;
            // clientRequestCache[logs[lastApplied].client][logs[lastApplied].transId] = (client=logs[lastApplied].client, transId=logs[lastApplied].transId, result=execResult.result);
        }
    }

    fun logUpToDateCheck(lastIndex: LogIndex, lastTerm: TermId): bool {
        // Given \`lastIndex\` and \`lastTerm\` from the other node,
        // check if its logs is up-to-date.
        if (lastTerm > lastLogTerm(logs)) {
            // the lastTerm is greater than mine, then it is up-to-date
            return true;
        }
        if (lastTerm < lastLogTerm(logs)) {
            // the lastTerm is smaller than mine, then it is not up-to-date
            return false;
        }
        // the lastTerm is the same, then whichever's log is longer is up-to-date
        return lastIndex >= lastLogIndex(logs);
    }

    fun notifyViewLog() {
        logNotifyTimestamp = logNotifyTimestamp + 1;
        send viewServer, eNotifyLog, (timestamp=logNotifyTimestamp, server=this, log=logs);
    }

    fun becomeFollower(term: int) {
        // cancelTimer(electionTimer);
        updateTermAndVote(term);
        goto Follower;
    }

    fun updateTermAndVote(term: int) {
        if (term > currentTerm) {
            // if the term is changed, then reset the votedFor since
            // it has not voted in the new term.
            votedFor = default(Server);
        }
        // the only case is currentTerm == term
        currentTerm = term;
    }

    fun termsOfLog(): seq[int] {
        var terms: seq[int];
        var i: int;
        terms = default(seq[int]);
        i = 0;
        while (i < sizeof(logs)) {
            terms += (sizeof(terms), logs[i].term);
            i = i + 1;
        }
        return terms;
    }

    fun reset() {
        // crash a node, reset volatile states
        // and become a follower (just as it starts up)
        commitIndex = -1;
        lastApplied = -1;
        nextIndex = fillMap(this, nextIndex, peers, 0);
        matchIndex = fillMap(this, matchIndex, peers, -1);
        goto Follower;
    }
}`,"../../tests/upstream/Tutorial/6_Raft/PSrc/Timer.p":`/******************************************************************
* A Timer machine that timeouts non-deterministically.
*******************************************************************/

event eStartTimer;
event eElectionTimeout;
event eHeartbeatTimeout;
event eCancelTimer;
event eTick;

machine Timer {
    var holder: machine;
    var timeoutEvent: event;

    start state Init {
        entry (setup: (user: machine, timeoutEvent: event)) {
            holder = setup.user;
            timeoutEvent = setup.timeoutEvent;
            goto TimerIdle;
        }
        ignore eCancelTimer, eTick, eStartTimer;
    }

    state TimerIdle {
        on eStartTimer do {
            goto TimerTick;
        }
        on eShutdown goto TimerShutdown;
        ignore eCancelTimer, eTick;
    }

    state TimerTick {
        entry {
            checkTick();
        }
        on eTick do {
            checkTick();
        }
        on eShutdown goto TimerShutdown;
        on eCancelTimer goto TimerIdle;
        ignore eStartTimer;
    }

    state TimerShutdown {
        ignore eStartTimer, eCancelTimer, eTick;
    }

    fun checkTick() {
        if ($) {
            // non-deterministic timeout
            send holder, timeoutEvent;
            goto TimerIdle;
        } else {
            send this, eTick;
        }
    }
}`,"../../tests/upstream/Tutorial/6_Raft/PSrc/Utils.p":`// Some utility functions
fun startTimer(timer: Timer) {
    send timer, eStartTimer;
}

fun cancelTimer(timer: Timer) {
    send timer, eCancelTimer;
}

fun restartTimer(timer: Timer) {
    cancelTimer(timer);
    startTimer(timer);
}

fun lastLogIndex(log: seq[tServerLog]): LogIndex {
    return sizeof(log) - 1;
}

fun getLogTerm(log: seq[tServerLog], index: int): int {
    // get the term of the log entry at index.
    if (index < 0 || index >= sizeof(log)) {
        return 0;
    } else {
        return log[index].term;
    }
}

fun lastLogTerm(log: seq[tServerLog]): int {
    if (sizeof(log) == 0) {
        return 0;
    } else {
        return log[sizeof(log) - 1].term;
    }
}

fun Max(a: int, b: int): int {
    if (a > b) {
        return a;
    } else {
        return b;
    }
}

fun Min(a: int, b: int): int {
    if (a < b) {
        return a;
    } else {
        return b;
    }
}

fun fillMap(self: machine, m: map[Server, int], servers: set[Server], value: int): map[Server, int] {
    var server: Server;
    foreach(server in servers) {
        if (server != self) {
            m[server] = value;
        }
    }
    return m;
}

fun broadcastRequest(self: Server, peers: set[Server], e: event, payload: any) {
    // broadcast a message to nodes except for itself.
    var peer: Server;
    foreach(peer in peers) {
        if (peer != self) {
            send peer, e, payload;
        }
    }
}
`,"../../tests/upstream/Tutorial/6_Raft/PSrc/View.p":`/******************************************************************
* A View service that manages the view of the cluster.
* It is responsible for maintaining roles of, injecting failures to
* and shutting down the nodes.
*******************************************************************/

// Shutdown message will be sent to the cluster when
// client notifies the view service that it has finished all requests AND got all responses
event eShutdown;
// A node can notify the view service about its log entires
// This is used for choosing servers that should receive an election timeout
type tTS = int;
event eNotifyLog: (timestamp: tTS, server: Server, log: seq[tServerLog]);

machine View {
    // the cluster nodes
    var servers: set[Server];
    // the probability that a node got an election timeout [0, 100]
    // this is used to model network failure (i.e. a follower does not receive heartbeat)
    var timeoutRate: int;
    // the probability that a node crashes [0, 100]
    var crashRate: int;
    // the total number failures that can be injected
    var numFailures: int;
    // the timer for the view service to perform an action
    var triggerTimer: Timer;
    // the set of clients that have finished all requests
    var clientsDone: set[machine];
    // the number of clients
    var numClients: int;

    // the set of followers, leaders and candidates
    var followers: set[Server];
    var leaders: set[Server];
    var candidates: set[Server];
    // the set of servers that have not sent a \`eRequestVote\` message
    // this set will be exahusted when the view service
    // cannot detect any leader in the cluster
    var requestVotePendingSet: set[Server];
    // the logs of nodes
    var serverLogs: map[Server, seq[tServerLog]];
    // the last seen log of each node; prevent update the entry using older logs
    var lastSeenLogs: map[Server, int];
    var candidateRoundMap: map[Server, int];
    // number of rounds of actions where the view service cannot detect any leader
    var noLeaderRounds: int;

    start state Init {
        entry (setup: (numServers: int, numClients: int, timeoutRate: int, crashRate: int, numFailures: int)) {
            var i: int;
            var server: Server;
            timeoutRate = setup.timeoutRate;
            numClients = setup.numClients;
            crashRate = setup.crashRate;
            numFailures = setup.numFailures;
            followers = default(set[Server]);
            // at a moment, there can be multiple leaders, e.g. partitioned network
            leaders = default(set[Server]);
            candidates = default(set[Server]);
            requestVotePendingSet = default(set[Server]);
            serverLogs = default(map[Server, seq[tServerLog]]);
            lastSeenLogs = default(map[Server, int]);
            candidateRoundMap = default(map[Server, int]);
            noLeaderRounds = 0;

            while (i < setup.numServers) {
                server = new Server();
                servers += (server);
                serverLogs += (server, default(seq[tServerLog]));
                i = i + 1;
            }
            i = 0;
            foreach (server in servers) {
                send server, eServerInit, (myId=i, cluster=servers, viewServer=this);
                followers += (server);
                i = i + 1;
            }
            triggerTimer = new Timer((user=this, timeoutEvent=eHeartbeatTimeout));
            clientsDone = default(set[machine]);
            i = 0;
            while (i < numClients) {
                new Client((viewService=this, servers=servers, requests=randomWorkload(3)));
                i = i + 1;
            } 
            goto Monitoring;
        }
    }

    state Monitoring {
        entry {
            var server: Server;
            startTimer(triggerTimer);
            server = choose(servers);
            candidates += (server);
            send server, eElectionTimeout;
        }

        on eNotifyLog do (payload: (timestamp:int, server: Server, log: seq[tServerLog])) {
            // only track the most up-to-date logs
            if (!(payload.server in keys(lastSeenLogs)) || lastSeenLogs[payload.server] < payload.timestamp) {
                lastSeenLogs[payload.server] = payload.timestamp;
                serverLogs[payload.server] = payload.log;
            }
        }

        on eViewChangedLeader do (server: Server) {
            // server change its role to a leader
            noLeaderRounds = 0;
            leaders += (server);
            followers -= (server);
            candidates -= (server);
        }

        on eViewChangedFollower do (server: Server) {
            // server change its role to a follower
            followers += (server);
            candidates -= (server);
            leaders -= (server);
        }

        on eViewChangedCandidate do (server: Server) {
            // server change its role to a candidate
            candidates += (server);
            followers -= (server);
            leaders -= (server);
        }

        on eHeartbeatTimeout do {
            var server: Server;
            // print format("Current view: leaders={0} followers={1} candidates={2}", leaders, followers, candidates);
            if (sizeof(leaders) == 0) {
                if (noLeaderRounds % 25 == 0 && sizeof(requestVotePendingSet) > 0) {
                    // non-deterministically choose a server to trigger an election
                    // either the current most up-to-date server or a random server
                    if ($) {
                        server = mostUpToDateServer(requestVotePendingSet);
                    } else {
                        server = choose(requestVotePendingSet);
                    }
                    requestVotePendingSet -= (server);
                    // print format("NoLeader rounds exceeded, trigger election on {0}", server);
                    send server, eElectionTimeout;
                    candidates += (server);
                    followers -= (server);
                    noLeaderRounds = 0;
                }
                if (sizeof(requestVotePendingSet) == 0) {
                    requestVotePendingSet = servers;
                }
                noLeaderRounds = noLeaderRounds + 1;
            } else {
                foreach (server in servers) {
                    send server, eHeartbeatTimeout;
                }
                foreach (server in followers) {
                    if (choose(100) < timeoutRate && numFailures > 0) {
                        // print format("Failure Injection: timeout a follower {0}", server);
                        send server, eElectionTimeout;
                        numFailures = numFailures - 1;
                        startTimer(triggerTimer);
                        return;
                    }
                }
                foreach (server in leaders) {
                    if (choose(100) < crashRate && numFailures > 0) {
                        // print format("Failure Injection: crash a leader {0}", server);
                        send server, eReset;
                        numFailures = numFailures - 1;
                        startTimer(triggerTimer);
                        return;
                    }
                }
                foreach (server in candidates) {
                    if (choose(100) < crashRate && numFailures > 0) {
                        // print format("Failure Injection: crash a candidate {0}", server);
                        send server, eReset;
                        numFailures = numFailures - 1;
                        startTimer(triggerTimer);
                        return;
                    }
                }
            }
            startTimer(triggerTimer);
        }

        on eClientFinished do (client: machine) {
            var i: int;
            clientsDone += (client);
            if (sizeof(clientsDone) == numClients) {
                i = 0;
                while (i < sizeof(servers)) {
                    send servers[i], eShutdown;
                    i = i + 1;
                }
                goto ViewServiceEnd;   
            }
        }
    }

    state ViewServiceEnd {
        entry {
            send triggerTimer, eShutdown;
        }

        ignore eClientFinished, eHeartbeatTimeout, eViewChangedCandidate, eNotifyLog, eViewChangedLeader, eViewChangedFollower;
    }

    fun mostUpToDateServer(choices: set[Server]): Server {
        // get the server with the most up-to-date logs (in the view of the view service)
        // it might not be the real most up-to-date server in the cluster because of the message delay
        var server: Server;
        var candidate: Server;
        var term: int;
        var length: int;
        term = 0;
        length = 0;
        candidate = default(Server);
        // print format("Choose candidate: server |-> logs: {0}", serverLogs);
        foreach (server in servers) {
            if (candidate == default(Server)) {
                candidate = server;
                term = lastLogTerm(serverLogs[server]);
                length = sizeof(serverLogs[server]);
            } else {
                if (sizeof(serverLogs[server]) > 0) {
                    if (term < lastLogTerm(serverLogs[server])) {
                        term = lastLogTerm(serverLogs[server]);
                        length = sizeof(serverLogs[server]);
                        candidate = server;
                    } else if (term == lastLogTerm(serverLogs[server])) {
                        if (length < sizeof(serverLogs[server])) {
                            length = sizeof(serverLogs[server]);
                            candidate = server;
                        }
                    }
                }
            }
        }
        return candidate;
    }
}
`,"../../tests/upstream/Tutorial/6_Raft/PTst/RaftTests.p":`event eRaftConfig: (quorumSize: int);

fun setUpCluster(numServers: int, numClients: int, timeoutRate: int, crashRate: int, numFailures: int): View {
    announce eRaftConfig, (quorumSize=numServers / 2 + 1,);
    return new View((numServers=numServers, numClients=numClients,
                                timeoutRate=timeoutRate, crashRate=crashRate, numFailures=numFailures));
}

fun randomWorkload(numCmd: int): seq[Command] {
    var cmds: seq[Command];
    var puts: set[int];
    var i: int;
    var key: int;
    assert numCmd <= 100, "Too many commands!";
    cmds = default(seq[Command]);
    puts = default(set[int]);
    i = 0;
    while (i < numCmd) {
        // choose an existing key or a new key
        // non-deterministically
        if (sizeof(puts) > 0 && $) {
            key = choose(puts);
        } else {
            key = choose(10);
            puts += (key);
        }
        if (sizeof(puts) == 0 || $) {
            // PUT
            cmds += (i, (op=PUT, key=key, value=choose(1024)));
        } else {
            // GET
            cmds += (i, (op=GET, key=key, value=-1));
        }
        i = i + 1;
    }
    return cmds;
}

machine OneClientOneServerReliable {
    start state Init {
        entry {
            var view: View;
            view = setUpCluster(1, 1, 0, 0, 0);
        }
    }
}

machine OneClientFiveServersReliable {
    start state Init {
        entry {
            var view: View;
            view = setUpCluster(5, 1, 0, 0, 0);
        }
    }
}

machine OneClientFiveServersUnreliable {
    start state Init {
        entry {
            var view: View;
            view = setUpCluster(5, 1, 5, 5, 100);
        }
    }
}

machine OneClientThreeServersReliable {
    start state Init {
        entry {
            var view: View;
            view = setUpCluster(3, 1, 0, 0, 0);
        }
    }
}

machine OneClientThreeServersUnreliable {
    start state Init {
        entry {
            var view: View;
            view = setUpCluster(3, 1, 5, 5, 100);
        }
    }
}

machine TwoClientsThreeServersReliable {
    start state Init {
        entry {
            var view: View;
            view = setUpCluster(3, 2, 0, 0, 0);
        }
    }
}

machine TwoClientsThreeServersUnreliable {
    start state Init {
        entry {
            var view: View;
            view = setUpCluster(3, 2, 5, 5, 100);
        }
    }
}

machine ThreeClientsOneServerReliable {
    start state Init {
        entry {
            var view: View;
            view = setUpCluster(1, 3, 0, 0, 0);
        }
    }
}
`,"../../tests/upstream/Tutorial/6_Raft/PTst/Testscript.p":`module ServingTests = { OneClientOneServerReliable,
                        OneClientThreeServersReliable,
                        OneClientThreeServersUnreliable,
                        OneClientFiveServersReliable,
                        OneClientFiveServersUnreliable,
                        TwoClientsThreeServersReliable,
                        TwoClientsThreeServersUnreliable,
                        ThreeClientsOneServerReliable};

test oneClientOneServerReliable [main=OneClientOneServerReliable]:
  assert SafetyOneLeader, SafetyLeaderCompleteness, SafetyStateMachine, SafetyLogMatching, LivenessClientsDone, LivenessProgress, SafetySynchronization in
  (union Server, Timer, Client, View, ServingTests);

test oneClientThreeServersReliable [main=OneClientThreeServersReliable]:
  assert SafetyOneLeader, SafetyLeaderCompleteness, SafetyStateMachine, SafetyLogMatching, LivenessClientsDone, LivenessProgress, SafetySynchronization in
  (union Server, Timer, Client, View, ServingTests);

test oneClientThreeServersUnreliable [main=OneClientThreeServersUnreliable]:
  assert SafetyOneLeader, SafetyLeaderCompleteness, SafetyStateMachine, SafetyLogMatching, LivenessClientsDone, LivenessProgress, SafetySynchronization in
  (union Server, Timer, Client, View, ServingTests);

test oneClientFiveServersReliable [main=OneClientFiveServersReliable]:
  assert SafetyOneLeader, SafetyLeaderCompleteness, SafetyStateMachine, SafetyLogMatching, LivenessClientsDone, LivenessProgress, SafetySynchronization in
  (union Server, Timer, Client, View, ServingTests);

test oneClientFiveServersUnreliable [main=OneClientFiveServersUnreliable]:
  assert SafetyOneLeader, SafetyLeaderCompleteness, SafetyStateMachine, SafetyLogMatching, LivenessClientsDone, LivenessProgress, SafetySynchronization in
  (union Server, Timer, Client, View, ServingTests);

test threeClientsOneServerReliable [main=ThreeClientsOneServerReliable]:
  assert SafetyOneLeader, SafetyLeaderCompleteness, SafetyStateMachine, SafetyLogMatching, LivenessClientsDone, LivenessProgress, SafetySynchronization in
  (union Server, Timer, Client, View, ServingTests);

test twoClientsThreeServersReliable [main=TwoClientsThreeServersReliable]:
  assert SafetyOneLeader, SafetyLeaderCompleteness, SafetyStateMachine, SafetyLogMatching, LivenessClientsDone, LivenessProgress, SafetySynchronization in
  (union Server, Timer, Client, View, ServingTests);

test twoClientsThreeServersUnreliable [main=TwoClientsThreeServersUnreliable]:
  assert SafetyOneLeader, SafetyLeaderCompleteness, SafetyStateMachine, SafetyLogMatching, LivenessClientsDone, LivenessProgress, SafetySynchronization in
  (union Server, Timer, Client, View, ServingTests);`,"../../tests/upstream/Tutorial/Common/FailureInjector/PSrc/FailureInjector.p":`/*
The failure injector machine randomly selects a replica machine and enqueues the special event "halt".
*/
event eDelayNodeFailure;
// event: event sent by the failure injector to shutdown a node
event eShutDown: machine;

machine FailureInjector {
  var nFailures: int;
  var nodes: set[machine];

  start state Init {
    entry (config: (nodes: set[machine], nFailures: int)) {
      nFailures = config.nFailures;
      nodes = config.nodes;
      assert nFailures < sizeof(nodes);
      goto FailOneNode;
    }
  }

  state FailOneNode {
    entry {
      var fail: machine;

      if(nFailures == 0)
        raise halt; // done with all failures
      else
      {
        if($)
        {
          fail = choose(nodes);
          send fail, eShutDown, fail;
          nodes -= (fail);
          nFailures = nFailures - 1;
        }
        else {
          send this, eDelayNodeFailure;
        }
      }
    }

    on eDelayNodeFailure goto FailOneNode;
  }
}

// function to create the failure injection
fun CreateFailureInjector(config: (nodes: set[machine], nFailures: int)) {
  new FailureInjector(config);
}

// failure injector module
module FailureInjector = { FailureInjector };`,"../../tests/upstream/Tutorial/Common/FailureInjector/PSrc/NetworkFunctions.p":`// unreliable send operation that drops messages on the ether nondeterministically
fun UnReliableSend(target: machine, message: event, payload: any) {
  // nondeterministically drop messages
  // $: choose()
  if($) send target, message, payload;
}

// unrelialbe broadcast function
fun UnReliableBroadCast(ms: set[machine], ev: event, payload: any) {
  var i: int;
  while (i < sizeof(ms)) {
    UnReliableSend(ms[i], ev, payload);
    i = i + 1;
  }
}

// relialbe broadcast function
fun ReliableBroadCast(ms: set[machine], ev: event, payload: any) {
  var i: int;
  while (i < sizeof(ms)) {
    send ms[i], ev, payload;
    i = i + 1;
  }
}`,"../../tests/upstream/Tutorial/Common/Timer/PSrc/Timer.p":`/*****************************************************************************************
The timer state machine models the non-deterministic behavior of an OS timer
******************************************************************************************/

/************************************************
Events used to interact with the timer machine
************************************************/
event eStartTimer;
event eCancelTimer;
event eTimeOut;
event eDelayedTimeOut;

machine Timer {
  // user/client of the timer
  var client: machine;

  start state Init {
    entry (_client : machine) {
      client = _client;
      goto WaitForTimerRequests;
    }
  }

  state WaitForTimerRequests {
    on eStartTimer goto TimerStarted;

    ignore eCancelTimer, eDelayedTimeOut;
  }

  state TimerStarted {
    entry {
      // Only fire the timer with a chance of 1/10 to avoid livelocks
      if(choose(10) == 0) {
        send client, eTimeOut;
        goto WaitForTimerRequests;
      } else {
        send this, eDelayedTimeOut;
      }
    }

    on eDelayedTimeOut goto TimerStarted;
    on eCancelTimer goto WaitForTimerRequests;
    defer eStartTimer;
  }
}

/************************************************
Functions or API's to interact with the OS Timer
*************************************************/
// create timer
fun CreateTimer(client: machine) : Timer {
  return new Timer(client);
}

// start timer
fun StartTimer(timer: Timer) {
  send timer, eStartTimer;
}

// cancel timer
fun CancelTimer(timer: Timer) {
  send timer, eCancelTimer;
}
`,"../../tests/upstream/Tutorial/Common/Timer/PSrc/TimerModules.p":`/* Create the timer module which consists of only the timer machine */
module Timer = { Timer };`,"../demos/hello/LostUpdate.p":`// Two clients each add one to a shared counter. The final value should be 2.
// Read and write are separate messages: another client can read before we write.
event ReadValue: Client;
event ValueRead: int;
event WriteValue: int;
event ClientFinished;
event ReadFinalValue: Main;

machine Counter {
  // P initializes integers to 0. Each machine owns its own variables.
  var value: int;

  start state Serving {
    on ReadValue do (client: Client) {
      send client, ValueRead, value;
    }

    // This overwrites the value; it does not add to it.
    on WriteValue do (newValue: int) {
      value = newValue;
    }

    on ReadFinalValue do (testHarness: Main) {
      send testHarness, ValueRead, value;
    }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      var rememberedValue: int;
      send input.counter, ReadValue, this;
      receive {
        case ValueRead: (replyValue: int) {
          rememberedValue = replyValue;
        }
      }
      // The remembered value can already be stale when this write is handled.
      send input.counter, WriteValue, rememberedValue + 1;
      send input.owner, ClientFinished;
    }
  }
}

machine Main {
  var counter: Counter;
  var finishedClients: int;

  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }

    on ClientFinished do {
      finishedClients = finishedClients + 1;
      if (finishedClients == 2) {
        send counter, ReadFinalValue, this;
        goto Verify;
      }
    }
  }

  state Verify {
    on ValueRead do (finalValue: int) {
      // The rule describes the intended result, not how to implement it.
      assert finalValue == 2, format("expected 2 increments, counter is {0}", finalValue);
    }
  }
}

// The harness creates two clients, waits for completion, then checks the rule.
// A design fix: send one Increment event and let Counter add 1 in its handler.
test tcTwoClients [main=Main]: { Main, Counter, Client };
`}),f=`../../tests/upstream/Tutorial/`,p=[{id:`lost-update`,title:`Hello P: a lost update`,blurb:`Two clients increment a shared counter with a read and then a write. Check it to find the interleaving that loses an update, then step through the trace.`,roots:[{from:`../demos/hello/`}],open:`LostUpdate.p`,testCase:`tcTwoClients`},{id:`client-server`,title:`Bank client/server`,blurb:`Clients withdraw money from a bank server backed by a database. A safety spec checks balances never drop below 10; a liveness spec checks every request gets a reply. tcAbstractServer uses a deliberately buggy abstract bank.`,roots:[{from:`${f}1_ClientServer/`}],open:`PSrc/Server.p`,testCase:`tcAbstractServer`},{id:`two-phase-commit`,title:`Two-phase commit`,blurb:`A coordinator runs two-phase commit over participants, with timers and an optional failure injector. The Atomicity spec checks every participant agrees on each transaction.`,roots:[{from:`${f}2_TwoPhaseCommit/`},{from:`${f}Common/Timer/`,to:`Common/Timer/`},{from:`${f}Common/FailureInjector/`,to:`Common/FailureInjector/`}],open:`PSrc/Coordinator.p`,testCase:`tcMultipleClientsWithFailure`},{id:`espresso`,title:`Espresso machine`,blurb:`A coffee maker and its control panel, driven by sane and 'crazy' users pressing buttons in any order. The spec checks the machine never brews in an invalid state.`,roots:[{from:`${f}3_EspressoMachine/`}],open:`PSrc/CoffeeMakerControlPanel.p`,testCase:`tcCrazyUserUsingCoffeeMachine`},{id:`failure-detector`,title:`Failure detector`,blurb:`A heartbeat-based failure detector with nodes that can crash. The spec checks that only crashed nodes are ever reported as failed.`,roots:[{from:`${f}4_FailureDetector/`},{from:`${f}Common/Timer/`,to:`Common/Timer/`},{from:`${f}Common/FailureInjector/`,to:`Common/FailureInjector/`}],open:`PSrc/FailureDetector.p`,testCase:`tcTest_FailureDetector`},{id:`paxos`,title:`Single-decree Paxos`,blurb:`Proposers, acceptors and learners. The spec checks at most one value is ever chosen.`,roots:[{from:`${f}5_Paxos/`}],open:`PSrc/proposer.p`,testCase:`testBasicPaxos3on3`},{id:`raft`,title:`Raft`,blurb:`Leader election and log replication with unreliable networks, checked against safety (one leader per term, log matching, …) and liveness specs. Larger: allow more time to explore.`,roots:[{from:`${f}6_Raft/`}],open:`PSrc/Server.p`,testCase:`oneClientThreeServersUnreliable`},{id:`retry-budget`,title:`Time: retry budget`,blurb:`Uses the snappea time extension: a client retries with timeouts and backoff, and a spec asserts every request resolves within a 1 s budget. Set timeoutAfter = ms(250) in PSrc/RetryClient.p to fix it.`,roots:[{from:`../../design/time/RetryBudget/`}],open:`PSrc/RetryClient.p`,testCase:`tcRetry`,extraArgs:`--latency PTst/latency.toml --timed`}],m={id:`custom`,title:`Your files`,blurb:`Files you opened or that came from a shared link. Edits are kept in the page URL, so copy it to share this project. Drop .p files, a folder or a .zip anywhere to open others.`,roots:[],open:``,testCase:``},h=[`PSrc/`,`PSpec/`,`PTst/`,`Common/`];function g(e){let t=e=>{let t=h.findIndex(t=>e.startsWith(t));return t<0?e.includes(`/`)?h.length:-1:t};return[...e].sort((e,n)=>t(e)-t(n)||e.localeCompare(n))}function _(e){let t={};for(let n of e.roots)for(let[e,r]of Object.entries(d))e.startsWith(n.from)&&(t[(n.to??``)+e.slice(n.from.length)]=r);return t}var v=/\.(p|toml|json|txt|md)$/i,y=/(^|\/)(\.[^/]*|__MACOSX|PCheckerOutput|PGenerated|node_modules|target)(\/|$)/,ee=500,b=2e6;function x(e){return v.test(e)&&!y.test(e)}async function S(e,t,n){let i=r(e,{filter:e=>x(e.name)&&e.originalSize<=b});for(let[e,r]of Object.entries(i))n.push({path:t+e,read:async()=>r})}async function te(e,t,n){/\.zip$/i.test(e.name)?await S(new Uint8Array(await e.arrayBuffer()),t.replace(/[^/]*$/,``),n):x(t)&&e.size<=b&&n.push({path:t,read:async()=>new Uint8Array(await e.arrayBuffer())})}function ne(e){let t=e.createReader(),n=[];return new Promise((e,r)=>{let i=()=>t.readEntries(t=>{if(!t.length)return e(n);n.push(...t),i()},r);i()})}async function re(e,t){let n=e.fullPath.replace(/^\//,``);if(e.isDirectory){if(y.test(n))return;for(let n of await ne(e))await re(n,t)}else await te(await new Promise((t,n)=>e.file(t,n)),n,t)}function C(e){let t=0;for(;;){let n=e[0].slice(t).split(`/`);if(n.length<2)break;let r=n[0]+`/`;if(!e.every(e=>e.slice(t).startsWith(r)))break;t+=r.length}return e=>e.slice(t)}async function ie(e){if(e.length>ee)throw Error(`Too many files (${e.length}); the limit is ${ee}.`);if(!e.some(e=>/\.p$/i.test(e.path)))throw Error(`No .p files found.`);let t=C(e.map(e=>e.path)),n={};for(let r of e)n[t(r.path)]=o(await r.read());return n}async function ae(e){let t=[],n=[...e.items].map(e=>e.webkitGetAsEntry?.()).filter(e=>!!e),r=[...e.files];if(n.length)for(let e of n)await re(e,t);else for(let e of r)await te(e,e.name,t);return ie(t)}async function w(e){let t=[];for(let n of[...e])await te(n,n.webkitRelativePath||n.name,t);return ie(t)}var T=[],oe=[];(()=>{let e=`lc,34,7n,7,7b,19,,,,2,,2,,,20,b,1c,l,g,,2t,7,2,6,2,2,,4,z,,u,r,2j,b,1m,9,9,,o,4,,9,,3,,5,17,3,1n,9,16,o,,x,1i,3,,i,,7,a,2,t,3,1k,,,7,2,2,2,3,9,,a,2,q,,2,3,1k,,,5,4,2,2,3,3,,u,2,3,,b,3,1k,,,8,,3,,3,k,2,m,6,,3,1k,,,7,2,2,2,3,7,3,a,2,u,,1n,5,3,3,,4,9,,14,5,1j,,,7,,3,,4,7,2,b,2,t,3,1k,,,7,,3,,4,7,2,b,2,f,,c,4,1j,2,,7,,3,,4,9,,a,2,t,3,1y,,4,6,,,,8,i,2,1p,,,8,c,8,2q,,,a,b,7,21,2,r,,,,,,4,2,1d,k,,2,5,b,,10,9,,2u,b,,6,n,4,4,3,g,4,d,,,3,6,,f,,jj,3,qa,4,s,3,t,2,u,2,1s,w,9,,19,3,,,39,2,y,,3a,c,4,c,63,5,1l,a,,,,,2,o,2,,1c,1a,2,c,k,5,1b,h,12,9,c,3,u,d,1k,e,1c,k,48,3,,l,4,,6,,2,3,5i,1s,ek,,5f,x,2da,3,3x,,2o,w,fe,6,2x,2,n9w,4,,a,w,2,28,2,7k,,3,,4,,n,5,4,,2b,2,1e,i,q,i,d,,12,8,p,d,18,4,1b,e,10,,1v,e,c,,8,2,1a,,1f,,,3,2,2,5,2,,,15,5,5,2,6k,8,,2,fn4,,kh,g,g,g,a6,2,gt,,6a,,45,5,1ae,3,,2,5,4,14,3,4,,4l,2,fx,4,1t,5,8t,2,25,6,1y,b,1d,4,3e,3,1h,f,15,,2,2,a,4,19,b,7,,1p,3,10,e,g,2,18,,c,3,1c,e,8,4,,2,2k,c,6,,2,,4d,c,l,4,1j,2,,7,2,2,2,3,9,,a,2,2,7,3,5,1v,9,,,2,,,4,,5,,,e,2,2a,i,n,,29,k,6j,7,2,9,r,2,2a,h,2y,d,2t,3,2,a,74,f,6t,6,,2,2,4,,,,2,3x,7,2,7,3,,s,a,14,7,,4,8,,9,b,1a,g,5i,8,5j,8,,8,2a,m,,e,3e,6,3,,,2,,7,,,1u,5,,2,,5,9n,4,9,2,,,1c,7,3,5,n,,44l,,6,f,8ug,i,1xc,5,1n,7,t4,,,1j,7,4,29,,b,2,f57,2,3mp,1a,2,n,f2,5,3,6,8,8,2,7,u,4,44,3,1iz,1j,4,1e,8,,e,,m,5,,f,11s,7,,h,2,7,,2,,5,2s,,4g,7,af,,1p,4,e4,4,72,2,6r,,2,,7,2,5,,d6,7,31,7,240,5`.split(`,`).map(e=>e?parseInt(e,36):1);for(let t=0,n=0;t<e.length;t++)(t%2?oe:T).push(n+=e[t])})();function se(e){if(e<768)return!1;for(let t=0,n=T.length;;){let r=t+n>>1;if(e<T[r])n=r;else if(e>=oe[r])t=r+1;else return!0;if(t==n)return!1}}function ce(e){return e>=127462&&e<=127487}var le=8205;function E(e,t,n=!0,r=!0){return(n?ue:de)(e,t,r)}function ue(e,t,n){if(t==e.length)return t;t&&pe(e.charCodeAt(t))&&me(e.charCodeAt(t-1))&&t--;let r=fe(e,t);for(t+=he(r);t<e.length;){let i=fe(e,t);if(r==le||i==le||n&&se(i))t+=he(i),r=i;else if(ce(i)){let n=0,r=t-2;for(;r>=0&&ce(fe(e,r));)n++,r-=2;if(n%2==0)break;t+=2}else break}return t}function de(e,t,n){for(;t>1;){let r=ue(e,t-2,n);if(r<t)return r;t--}return 0}function fe(e,t){let n=e.charCodeAt(t);if(!me(n)||t+1==e.length)return n;let r=e.charCodeAt(t+1);return pe(r)?(n-55296<<10)+(r-56320)+65536:n}function pe(e){return e>=56320&&e<57344}function me(e){return e>=55296&&e<56320}function he(e){return e<65536?1:2}var D=class e{lineAt(e){if(e<0||e>this.length)throw RangeError(`Invalid position ${e} in document of length ${this.length}`);return this.lineInner(e,!1,1,0)}line(e){if(e<1||e>this.lines)throw RangeError(`Invalid line number ${e} in ${this.lines}-line document`);return this.lineInner(e,!0,1,0)}replace(e,t,n){[e,t]=we(this,e,t);let r=[];return this.decompose(0,e,r,2),n.length&&n.decompose(0,n.length,r,3),this.decompose(t,this.length,r,1),_e.from(r,this.length-(t-e)+n.length)}append(e){return this.replace(this.length,this.length,e)}slice(e,t=this.length){[e,t]=we(this,e,t);let n=[];return this.decompose(e,t,n,0),_e.from(n,t-e)}eq(e){if(e==this)return!0;if(e.length!=this.length||e.lines!=this.lines)return!1;let t=this.scanIdentical(e,1),n=this.length-this.scanIdentical(e,-1),r=new xe(this),i=new xe(e);for(let e=t,a=t;;){if(r.next(e),i.next(e),e=0,r.lineBreak!=i.lineBreak||r.done!=i.done||r.value!=i.value)return!1;if(a+=r.value.length,r.done||a>=n)return!0}}iter(e=1){return new xe(this,e)}iterRange(e,t=this.length){return new O(this,e,t)}iterLines(e,t){let n;if(e==null)n=this.iter();else{t??=this.lines+1;let r=this.line(e).from;n=this.iterRange(r,Math.max(r,t==this.lines+1?this.length:t<=1?0:this.line(t-1).to))}return new Se(n)}toString(){return this.sliceString(0)}toJSON(){let e=[];return this.flatten(e),e}constructor(){}static of(t){if(t.length==0)throw RangeError(`A document must have at least one line`);return t.length==1&&!t[0]?e.empty:t.length<=32?new ge(t):_e.from(ge.split(t,[]))}},ge=class e extends D{constructor(e,t=ve(e)){super(),this.text=e,this.length=t}get lines(){return this.text.length}get children(){return null}lineInner(e,t,n,r){for(let i=0;;i++){let a=this.text[i],o=r+a.length;if((t?n:o)>=e)return new Ce(r,o,n,a);r=o+1,n++}}decompose(t,n,r,i){let a=t<=0&&n>=this.length?this:new e(be(this.text,t,n),Math.min(n,this.length)-Math.max(0,t));if(i&1){let t=r.pop(),n=ye(a.text,t.text.slice(),0,a.length);if(n.length<=32)r.push(new e(n,t.length+a.length));else{let t=n.length>>1;r.push(new e(n.slice(0,t)),new e(n.slice(t)))}}else r.push(a)}replace(t,n,r){if(!(r instanceof e))return super.replace(t,n,r);[t,n]=we(this,t,n);let i=ye(this.text,ye(r.text,be(this.text,0,t)),n),a=this.length+r.length-(n-t);return i.length<=32?new e(i,a):_e.from(e.split(i,[]),a)}sliceString(e,t=this.length,n=`
`){[e,t]=we(this,e,t);let r=``;for(let i=0,a=0;i<=t&&a<this.text.length;a++){let o=this.text[a],s=i+o.length;i>e&&a&&(r+=n),e<s&&t>i&&(r+=o.slice(Math.max(0,e-i),t-i)),i=s+1}return r}flatten(e){for(let t of this.text)e.push(t)}scanIdentical(){return 0}static split(t,n){let r=[],i=-1;for(let a of t)r.push(a),i+=a.length+1,r.length==32&&(n.push(new e(r,i)),r=[],i=-1);return i>-1&&n.push(new e(r,i)),n}},_e=class e extends D{constructor(e,t){super(),this.children=e,this.length=t,this.lines=0;for(let t of e)this.lines+=t.lines}lineInner(e,t,n,r){for(let i=0;;i++){let a=this.children[i],o=r+a.length,s=n+a.lines-1;if((t?s:o)>=e)return a.lineInner(e,t,n,r);r=o+1,n=s+1}}decompose(e,t,n,r){for(let i=0,a=0;a<=t&&i<this.children.length;i++){let o=this.children[i],s=a+o.length;if(e<=s&&t>=a){let i=r&(a<=e|(s>=t?2:0));a>=e&&s<=t&&!i?n.push(o):o.decompose(e-a,t-a,n,i)}a=s+1}}replace(t,n,r){if([t,n]=we(this,t,n),r.lines<this.lines)for(let i=0,a=0;i<this.children.length;i++){let o=this.children[i],s=a+o.length;if(t>=a&&n<=s){let c=o.replace(t-a,n-a,r),l=this.lines-o.lines+c.lines;if(c.lines<l>>4&&c.lines>l>>6){let a=this.children.slice();return a[i]=c,new e(a,this.length-(n-t)+r.length)}return super.replace(a,s,c)}a=s+1}return super.replace(t,n,r)}sliceString(e,t=this.length,n=`
`){[e,t]=we(this,e,t);let r=``;for(let i=0,a=0;i<this.children.length&&a<=t;i++){let o=this.children[i],s=a+o.length;a>e&&i&&(r+=n),e<s&&t>a&&(r+=o.sliceString(e-a,t-a,n)),a=s+1}return r}flatten(e){for(let t of this.children)t.flatten(e)}scanIdentical(t,n){if(!(t instanceof e))return 0;let r=0,[i,a,o,s]=n>0?[0,0,this.children.length,t.children.length]:[this.children.length-1,t.children.length-1,-1,-1];for(;;i+=n,a+=n){if(i==o||a==s)return r;let e=this.children[i],c=t.children[a];if(e!=c)return r+e.scanIdentical(c,n);r+=e.length+1}}static from(t,n=t.reduce((e,t)=>e+t.length+1,-1)){let r=0;for(let e of t)r+=e.lines;if(r<32){let e=[];for(let n of t)n.flatten(e);return new ge(e,n)}let i=Math.max(32,r>>5),a=i<<1,o=i>>1,s=[],c=0,l=-1,u=[];function d(t){let n;if(t.lines>a&&t instanceof e)for(let e of t.children)d(e);else t.lines>o&&(c>o||!c)?(f(),s.push(t)):t instanceof ge&&c&&(n=u[u.length-1])instanceof ge&&t.lines+n.lines<=32?(c+=t.lines,l+=t.length+1,u[u.length-1]=new ge(n.text.concat(t.text),n.length+1+t.length)):(c+t.lines>i&&f(),c+=t.lines,l+=t.length+1,u.push(t))}function f(){c!=0&&(s.push(u.length==1?u[0]:e.from(u,l)),l=-1,c=u.length=0)}for(let e of t)d(e);return f(),s.length==1?s[0]:new e(s,n)}};D.empty=new ge([``],0);function ve(e){let t=-1;for(let n of e)t+=n.length+1;return t}function ye(e,t,n=0,r=1e9){for(let i=0,a=0,o=!0;a<e.length&&i<=r;a++){let s=e[a],c=i+s.length;c>=n&&(c>r&&(s=s.slice(0,r-i)),i<n&&(s=s.slice(n-i)),o?(t[t.length-1]+=s,o=!1):t.push(s)),i=c+1}return t}function be(e,t,n){return ye(e,[``],t,n)}var xe=class{constructor(e,t=1){this.dir=t,this.done=!1,this.lineBreak=!1,this.value=``,this.nodes=[e],this.offsets=[t>0?1:(e instanceof ge?e.text.length:e.children.length)<<1]}nextInner(e,t){for(this.done=this.lineBreak=!1;;){let n=this.nodes.length-1,r=this.nodes[n],i=this.offsets[n],a=i>>1,o=r instanceof ge?r.text.length:r.children.length;if(a==(t>0?o:0)){if(n==0)return this.done=!0,this.value=``,this;t>0&&this.offsets[n-1]++,this.nodes.pop(),this.offsets.pop()}else if((i&1)==(t>0?0:1)){if(this.offsets[n]+=t,e==0)return this.lineBreak=!0,this.value=`
`,this;e--}else if(r instanceof ge){let i=r.text[a+(t<0?-1:0)];if(this.offsets[n]+=t,i.length>Math.max(0,e))return this.value=e==0?i:t>0?i.slice(e):i.slice(0,i.length-e),this;e-=i.length}else{let i=r.children[a+(t<0?-1:0)];e>i.length?(e-=i.length,this.offsets[n]+=t):(t<0&&this.offsets[n]--,this.nodes.push(i),this.offsets.push(t>0?1:(i instanceof ge?i.text.length:i.children.length)<<1))}}}next(e=0){return e<0&&(this.nextInner(-e,-this.dir),e=this.value.length),this.nextInner(e,this.dir)}},O=class{constructor(e,t,n){this.value=``,this.done=!1,this.cursor=new xe(e,t>n?-1:1),this.pos=t>n?e.length:0,this.from=Math.min(t,n),this.to=Math.max(t,n)}nextInner(e,t){if(t<0?this.pos<=this.from:this.pos>=this.to)return this.value=``,this.done=!0,this;e+=Math.max(0,t<0?this.pos-this.to:this.from-this.pos);let n=t<0?this.pos-this.from:this.to-this.pos;e>n&&(e=n),n-=e;let{value:r}=this.cursor.next(e);return this.pos+=(r.length+e)*t,this.value=r.length<=n?r:t<0?r.slice(r.length-n):r.slice(0,n),this.done=!this.value,this}next(e=0){return e<0?e=Math.max(e,this.from-this.pos):e>0&&(e=Math.min(e,this.to-this.pos)),this.nextInner(e,this.cursor.dir)}get lineBreak(){return this.cursor.lineBreak&&this.value!=``}},Se=class{constructor(e){this.inner=e,this.afterBreak=!0,this.value=``,this.done=!1}next(e=0){let{done:t,lineBreak:n,value:r}=this.inner.next(e);return t&&this.afterBreak?(this.value=``,this.afterBreak=!1):t?(this.done=!0,this.value=``):n?this.afterBreak?this.value=``:(this.afterBreak=!0,this.next()):(this.value=r,this.afterBreak=!1),this}get lineBreak(){return!1}};typeof Symbol<`u`&&(D.prototype[Symbol.iterator]=function(){return this.iter()},xe.prototype[Symbol.iterator]=O.prototype[Symbol.iterator]=Se.prototype[Symbol.iterator]=function(){return this});var Ce=class{constructor(e,t,n,r){this.from=e,this.to=t,this.number=n,this.text=r}get length(){return this.to-this.from}};function we(e,t,n){return t=Math.max(0,Math.min(e.length,t)),[t,Math.max(t,Math.min(e.length,n))]}function k(e,t,n=!0,r=!0){return E(e,t,n,r)}function Te(e){return e>=56320&&e<57344}function Ee(e){return e>=55296&&e<56320}function De(e,t){let n=e.charCodeAt(t);if(!Ee(n)||t+1==e.length)return n;let r=e.charCodeAt(t+1);return Te(r)?(n-55296<<10)+(r-56320)+65536:n}function Oe(e){return e<=65535?String.fromCharCode(e):(e-=65536,String.fromCharCode((e>>10)+55296,(e&1023)+56320))}function ke(e){return e<65536?1:2}var Ae=/\r\n?|\n/,je=(function(e){return e[e.Simple=0]=`Simple`,e[e.TrackDel=1]=`TrackDel`,e[e.TrackBefore=2]=`TrackBefore`,e[e.TrackAfter=3]=`TrackAfter`,e})(je||={}),Me=class e{constructor(e){this.sections=e}get length(){let e=0;for(let t=0;t<this.sections.length;t+=2)e+=this.sections[t];return e}get newLength(){let e=0;for(let t=0;t<this.sections.length;t+=2){let n=this.sections[t+1];e+=n<0?this.sections[t]:n}return e}get empty(){return this.sections.length==0||this.sections.length==2&&this.sections[1]<0}iterGaps(e){for(let t=0,n=0,r=0;t<this.sections.length;){let i=this.sections[t++],a=this.sections[t++];a<0?(e(n,r,i),r+=i):r+=a,n+=i}}iterChangedRanges(e,t=!1){Fe(this,e,t)}get invertedDesc(){let t=[];for(let e=0;e<this.sections.length;){let n=this.sections[e++],r=this.sections[e++];r<0?t.push(n,r):t.push(r,n)}return new e(t)}composeDesc(e){return this.empty?e:e.empty?this:Le(this,e)}mapDesc(e,t=!1){return e.empty?this:Ie(this,e,t)}mapPos(e,t=-1,n=je.Simple){let r=0,i=0;for(let a=0;a<this.sections.length;){let o=this.sections[a++],s=this.sections[a++],c=r+o;if(s<0){if(c>e)return i+(e-r);i+=o}else{if(n!=je.Simple&&c>=e&&(n==je.TrackDel&&r<e&&c>e||n==je.TrackBefore&&r<e||n==je.TrackAfter&&c>e))return null;if(c>e||c==e&&t<0&&!o)return e==r||t<0?i:i+s;i+=s}r=c}if(e>r)throw RangeError(`Position ${e} is out of range for changeset of length ${r}`);return i}touchesRange(e,t=e){for(let n=0,r=0;n<this.sections.length&&r<=t;){let i=this.sections[n++],a=this.sections[n++],o=r+i;if(a>=0&&r<=t&&o>=e)return r<e&&o>t?`cover`:!0;r=o}return!1}toString(){let e=``;for(let t=0;t<this.sections.length;){let n=this.sections[t++],r=this.sections[t++];e+=(e?` `:``)+n+(r>=0?`:`+r:``)}return e}toJSON(){return this.sections}static fromJSON(t){if(!Array.isArray(t)||t.length%2||t.some(e=>typeof e!=`number`))throw RangeError(`Invalid JSON representation of ChangeDesc`);return new e(t)}static create(t){return new e(t)}},Ne=class e extends Me{constructor(e,t){super(e),this.inserted=t}apply(e){if(this.length!=e.length)throw RangeError(`Applying change set to a document with the wrong length`);return Fe(this,(t,n,r,i,a)=>e=e.replace(r,r+(n-t),a),!1),e}mapDesc(e,t=!1){return Ie(this,e,t,!0)}invert(t){let n=this.sections.slice(),r=[];for(let e=0,i=0;e<n.length;e+=2){let a=n[e],o=n[e+1];if(o>=0){n[e]=o,n[e+1]=a;let s=e>>1;for(;r.length<s;)r.push(D.empty);r.push(a?t.slice(i,i+a):D.empty)}i+=a}return new e(n,r)}compose(e){return this.empty?e:e.empty?this:Le(this,e,!0)}map(e,t=!1){return e.empty?this:Ie(this,e,t,!0)}iterChanges(e,t=!1){Fe(this,e,t)}get desc(){return Me.create(this.sections)}filter(t){let n=[],r=[],i=[],a=new Re(this);done:for(let e=0,o=0;;){let s=e==t.length?1e9:t[e++];for(;o<s||o==s&&a.len==0;){if(a.done)break done;let e=Math.min(a.len,s-o);A(i,e,-1);let t=a.ins==-1?-1:a.off==0?a.ins:0;A(n,e,t),t>0&&Pe(r,n,a.text),a.forward(e),o+=e}let c=t[e++];for(;o<c;){if(a.done)break done;let e=Math.min(a.len,c-o);A(n,e,-1),A(i,e,a.ins==-1?-1:a.off==0?a.ins:0),a.forward(e),o+=e}}return{changes:new e(n,r),filtered:Me.create(i)}}toJSON(){let e=[];for(let t=0;t<this.sections.length;t+=2){let n=this.sections[t],r=this.sections[t+1];r<0?e.push(n):r==0?e.push([n]):e.push([n].concat(this.inserted[t>>1].toJSON()))}return e}static of(t,n,r){let i=[],a=[],o=0,s=null;function c(t=!1){if(!t&&!i.length)return;o<n&&A(i,n-o,-1);let r=new e(i,a);s=s?s.compose(r.map(s)):r,i=[],a=[],o=0}function l(t){if(Array.isArray(t))for(let e of t)l(e);else if(t instanceof e){if(t.length!=n)throw RangeError(`Mismatched change set length (got ${t.length}, expected ${n})`);c(),s=s?s.compose(t.map(s)):t}else{let{from:e,to:s=e,insert:l}=t;if(e>s||e<0||s>n)throw RangeError(`Invalid change range ${e} to ${s} (in doc of length ${n})`);let u=l?typeof l==`string`?D.of(l.split(r||Ae)):l:D.empty,d=u.length;if(e==s&&d==0)return;e<o&&c(),e>o&&A(i,e-o,-1),A(i,s-e,d),Pe(a,i,u),o=s}}return l(t),c(!s),s}static empty(t){return new e(t?[t,-1]:[],[])}static fromJSON(t){if(!Array.isArray(t))throw RangeError(`Invalid JSON representation of ChangeSet`);let n=[],r=[];for(let e=0;e<t.length;e++){let i=t[e];if(typeof i==`number`)n.push(i,-1);else if(!Array.isArray(i)||typeof i[0]!=`number`||i.some((e,t)=>t&&typeof e!=`string`))throw RangeError(`Invalid JSON representation of ChangeSet`);else if(i.length==1)n.push(i[0],0);else{for(;r.length<e;)r.push(D.empty);r[e]=D.of(i.slice(1)),n.push(i[0],r[e].length)}}return new e(n,r)}static createSet(t,n){return new e(t,n)}};function A(e,t,n,r=!1){if(t==0&&n<=0)return;let i=e.length-2;i>=0&&n<=0&&n==e[i+1]?e[i]+=t:i>=0&&t==0&&e[i]==0?e[i+1]+=n:r?(e[i]+=t,e[i+1]+=n):e.push(t,n)}function Pe(e,t,n){if(n.length==0)return;let r=t.length-2>>1;if(r<e.length)e[e.length-1]=e[e.length-1].append(n);else{for(;e.length<r;)e.push(D.empty);e.push(n)}}function Fe(e,t,n){let r=e.inserted;for(let i=0,a=0,o=0;o<e.sections.length;){let s=e.sections[o++],c=e.sections[o++];if(c<0)i+=s,a+=s;else{let l=i,u=a,d=D.empty;for(;l+=s,u+=c,c&&r&&(d=d.append(r[o-2>>1])),!(n||o==e.sections.length||e.sections[o+1]<0);)s=e.sections[o++],c=e.sections[o++];t(i,l,a,u,d),i=l,a=u}}}function Ie(e,t,n,r=!1){let i=[],a=r?[]:null,o=new Re(e),s=new Re(t);for(let e=-1;;)if(o.done&&s.len||s.done&&o.len)throw Error(`Mismatched change set lengths`);else if(o.ins==-1&&s.ins==-1){let e=Math.min(o.len,s.len);A(i,e,-1),o.forward(e),s.forward(e)}else if(s.ins>=0&&(o.ins<0||e==o.i||o.off==0&&(s.len<o.len||s.len==o.len&&!n))){let t=s.len;for(A(i,s.ins,-1);t;){let n=Math.min(o.len,t);o.ins>=0&&e<o.i&&o.len<=n&&(A(i,0,o.ins),a&&Pe(a,i,o.text),e=o.i),o.forward(n),t-=n}s.next()}else if(o.ins>=0){let t=0,n=o.len;for(;n;)if(s.ins==-1){let e=Math.min(n,s.len);t+=e,n-=e,s.forward(e)}else if(s.ins==0&&s.len<n)n-=s.len,s.next();else break;A(i,t,e<o.i?o.ins:0),a&&e<o.i&&Pe(a,i,o.text),e=o.i,o.forward(o.len-n)}else if(o.done&&s.done)return a?Ne.createSet(i,a):Me.create(i);else throw Error(`Mismatched change set lengths`)}function Le(e,t,n=!1){let r=[],i=n?[]:null,a=new Re(e),o=new Re(t);for(let e=!1;;)if(a.done&&o.done)return i?Ne.createSet(r,i):Me.create(r);else if(a.ins==0)A(r,a.len,0,e),a.next();else if(o.len==0&&!o.done)A(r,0,o.ins,e),i&&Pe(i,r,o.text),o.next();else if(a.done||o.done)throw Error(`Mismatched change set lengths`);else{let t=Math.min(a.len2,o.len),n=r.length;if(a.ins==-1){let n=o.ins==-1?-1:o.off?0:o.ins;A(r,t,n,e),i&&n&&Pe(i,r,o.text)}else o.ins==-1?(A(r,a.off?0:a.len,t,e),i&&Pe(i,r,a.textBit(t))):(A(r,a.off?0:a.len,o.off?0:o.ins,e),i&&!o.off&&Pe(i,r,o.text));e=(a.ins>t||o.ins>=0&&o.len>t)&&(e||r.length>n),a.forward2(t),o.forward(t)}}var Re=class{constructor(e){this.set=e,this.i=0,this.next()}next(){let{sections:e}=this.set;this.i<e.length?(this.len=e[this.i++],this.ins=e[this.i++]):(this.len=0,this.ins=-2),this.off=0}get done(){return this.ins==-2}get len2(){return this.ins<0?this.len:this.ins}get text(){let{inserted:e}=this.set,t=this.i-2>>1;return t>=e.length?D.empty:e[t]}textBit(e){let{inserted:t}=this.set,n=this.i-2>>1;return n>=t.length&&!e?D.empty:t[n].slice(this.off,e==null?void 0:this.off+e)}forward(e){e==this.len?this.next():(this.len-=e,this.off+=e)}forward2(e){this.ins==-1?this.forward(e):e==this.ins?this.next():(this.ins-=e,this.off+=e)}},ze=class e{constructor(e,t,n,r){this.from=e,this.to=t,this.flags=n,this.goalColumn=r}get anchor(){return this.flags&32?this.to:this.from}get head(){return this.flags&32?this.from:this.to}get empty(){return this.from==this.to}get assoc(){return this.flags&8?-1:this.flags&16?1:0}get undirectional(){return(this.flags&64)>0}get bidiLevel(){let e=this.flags&7;return e==7?null:e}map(t,n=-1){let r,i;return this.empty?r=i=t.mapPos(this.from,n):(r=t.mapPos(this.from,1),i=t.mapPos(this.to,-1)),r==this.from&&i==this.to?this:new e(r,i,this.flags,this.goalColumn)}extend(e,t=e,n=0){if(e<=this.anchor&&t>=this.anchor)return j.range(e,t,void 0,void 0,n);let r=Math.abs(e-this.anchor)>Math.abs(t-this.anchor)?e:t;return j.range(this.anchor,r,void 0,void 0,n)}eq(e,t=!1){return this.anchor==e.anchor&&this.head==e.head&&this.goalColumn==e.goalColumn&&(!t||!this.empty||this.assoc==e.assoc)}toJSON(){return{anchor:this.anchor,head:this.head}}static fromJSON(e){if(!e||typeof e.anchor!=`number`||typeof e.head!=`number`)throw RangeError(`Invalid JSON representation for SelectionRange`);return j.range(e.anchor,e.head)}static create(t,n,r,i){return new e(t,n,r,i)}},j=class e{constructor(e,t){this.ranges=e,this.mainIndex=t}map(t,n=-1){return t.empty?this:e.create(this.ranges.map(e=>e.map(t,n)),this.mainIndex)}eq(e,t=!1){if(this.ranges.length!=e.ranges.length||this.mainIndex!=e.mainIndex)return!1;for(let n=0;n<this.ranges.length;n++)if(!this.ranges[n].eq(e.ranges[n],t))return!1;return!0}get main(){return this.ranges[this.mainIndex]}asSingle(){return this.ranges.length==1?this:new e([this.main],0)}addRange(t,n=!0){return e.create([t].concat(this.ranges),n?0:this.mainIndex+1)}replaceRange(t,n=this.mainIndex){let r=this.ranges.slice();return r[n]=t,e.create(r,this.mainIndex)}toJSON(){return{ranges:this.ranges.map(e=>e.toJSON()),main:this.mainIndex}}static fromJSON(t){if(!t||!Array.isArray(t.ranges)||typeof t.main!=`number`||t.main>=t.ranges.length)throw RangeError(`Invalid JSON representation for EditorSelection`);return new e(t.ranges.map(e=>ze.fromJSON(e)),t.main)}static single(t,n=t){return new e([e.range(t,n)],0)}static create(t,n=0){if(t.length==0)throw RangeError(`A selection needs at least one range`);for(let r=0,i=0;i<t.length;i++){let a=t[i];if(a.empty?a.from<=r:a.from<r)return e.normalized(t.slice(),n);r=a.to}return new e(t,n)}static cursor(e,t=0,n,r){return ze.create(e,e,(t==0?0:t<0?8:16)|(n==null?7:Math.min(6,n)),r)}static range(e,t,n,r,i){let a=r==null?7:Math.min(6,r);return!i&&e!=t&&(i=t<e?1:-1),i&&(a|=i<0?8:16),t<e?ze.create(t,e,a|32,n):ze.create(e,t,a,n)}static undirectionalRange(e,t){return ze.create(e,t,64,void 0)}static normalized(t,n=0){let r=t[n];t.sort((e,t)=>e.from-t.from),n=t.indexOf(r);for(let r=1;r<t.length;r++){let i=t[r],a=t[r-1];if(i.empty?i.from<=a.to:i.from<a.to){let o=a.from,s=Math.max(i.to,a.to);r<=n&&n--,t.splice(--r,2,i.anchor>i.head?e.range(s,o):e.range(o,s))}}return new e(t,n)}};function Be(e,t){for(let n of e.ranges)if(n.to>t)throw RangeError(`Selection points outside of document`)}var Ve=0,M=class e{constructor(e,t,n,r,i){this.combine=e,this.compareInput=t,this.compare=n,this.isStatic=r,this.id=Ve++,this.default=e([]),this.extensions=typeof i==`function`?i(this):i}get reader(){return this}static define(t={}){return new e(t.combine||(e=>e),t.compareInput||((e,t)=>e===t),t.compare||(t.combine?(e,t)=>e===t:He),!!t.static,t.enables)}of(e){return new Ue([],this,0,e)}compute(e,t){if(this.isStatic)throw Error(`Can't compute a static facet`);return new Ue(e,this,1,t)}computeN(e,t){if(this.isStatic)throw Error(`Can't compute a static facet`);return new Ue(e,this,2,t)}from(e,t){return t||=e=>e,this.compute([e],n=>t(n.field(e)))}};function He(e,t){return e==t||e.length==t.length&&e.every((e,n)=>e===t[n])}var Ue=class{constructor(e,t,n,r){this.dependencies=e,this.facet=t,this.type=n,this.value=r,this.id=Ve++}dynamicSlot(e){let t=this.value,n=this.facet.compareInput,r=this.id,i=e[r]>>1,a=this.type==2,o=!1,s=!1,c=[];for(let t of this.dependencies)t==`doc`?o=!0:t==`selection`?s=!0:(e[t.id]??1)&1||c.push(e[t.id]);return{create(e){return e.values[i]=t(e),1},update(e,r){if(o&&r.docChanged||s&&(r.docChanged||r.selection)||Ge(e,c)){let r=t(e);if(a?!We(r,e.values[i],n):!n(r,e.values[i]))return e.values[i]=r,1}return 0},reconfigure:(e,o)=>{let s,c=o.config.address[r];if(c!=null){let r=it(o,c);if(this.dependencies.every(t=>t instanceof M?o.facet(t)===e.facet(t):t instanceof Je?o.field(t,!1)==e.field(t,!1):!0)||(a?We(s=t(e),r,n):n(s=t(e),r)))return e.values[i]=r,0}else s=t(e);return e.values[i]=s,1}}}get extension(){return this}};function We(e,t,n){if(e.length!=t.length)return!1;for(let r=0;r<e.length;r++)if(!n(e[r],t[r]))return!1;return!0}function Ge(e,t){let n=!1;for(let r of t)rt(e,r)&1&&(n=!0);return n}function Ke(e,t,n){let r=n.map(t=>e[t.id]),i=n.map(e=>e.type),a=r.filter(e=>!(e&1)),o=e[t.id]>>1;function s(e){let n=[];for(let t=0;t<r.length;t++){let a=it(e,r[t]);if(i[t]==2)for(let e of a)n.push(e);else n.push(a)}return t.combine(n)}return{create(e){for(let t of r)rt(e,t);return e.values[o]=s(e),1},update(e,n){if(!Ge(e,a))return 0;let r=s(e);return t.compare(r,e.values[o])?0:(e.values[o]=r,1)},reconfigure(e,i){let a=Ge(e,r),c=i.config.facets[t.id],l=i.facet(t);if(c&&!a&&He(n,c))return e.values[o]=l,0;let u=s(e);return t.compare(u,l)?(e.values[o]=l,0):(e.values[o]=u,1)}}}var qe=M.define({static:!0}),Je=class e{constructor(e,t,n,r,i){this.id=e,this.createF=t,this.updateF=n,this.compareF=r,this.spec=i,this.provides=void 0}static define(t){let n=new e(Ve++,t.create,t.update,t.compare||((e,t)=>e===t),t);return t.provide&&(n.provides=t.provide(n)),n}create(e){return(e.facet(qe).find(e=>e.field==this)?.create||this.createF)(e)}slot(e){let t=e[this.id]>>1;return{create:e=>(e.values[t]=this.create(e),1),update:(e,n)=>{let r=e.values[t],i=this.updateF(r,n);return this.compareF(r,i)?0:(e.values[t]=i,1)},reconfigure:(e,n)=>{let r=e.facet(qe),i=n.facet(qe),a;return(a=r.find(e=>e.field==this))&&a!=i.find(e=>e.field==this)?(e.values[t]=a.create(e),1):n.config.address[this.id]==null?(e.values[t]=this.create(e),1):(e.values[t]=n.field(this),0)}}}init(e){return[this,qe.of({field:this,create:e})]}get extension(){return this}},Ye={lowest:4,low:3,default:2,high:1,highest:0};function Xe(e){return t=>new Qe(t,e)}var Ze={highest:Xe(Ye.highest),high:Xe(Ye.high),default:Xe(Ye.default),low:Xe(Ye.low),lowest:Xe(Ye.lowest)},Qe=class{constructor(e,t){this.inner=e,this.prec=t}get extension(){return this}},$e=class e{of(e){return new et(this,e)}reconfigure(t){return e.reconfigure.of({compartment:this,extension:t})}get(e){return e.config.compartments.get(this)}},et=class{constructor(e,t){this.compartment=e,this.inner=t}get extension(){return this}},tt=class e{constructor(e,t,n,r,i,a){for(this.base=e,this.compartments=t,this.dynamicSlots=n,this.address=r,this.staticValues=i,this.facets=a,this.statusTemplate=[];this.statusTemplate.length<n.length;)this.statusTemplate.push(0)}staticFacet(e){let t=this.address[e.id];return t==null?e.default:this.staticValues[t>>1]}static resolve(t,n,r){let i=[],a=Object.create(null),o=new Map;for(let e of nt(t,n,o))e instanceof Je?i.push(e):(a[e.facet.id]||(a[e.facet.id]=[])).push(e);let s=Object.create(null),c=[],l=[];for(let e of i)s[e.id]=l.length<<1,l.push(t=>e.slot(t));let u=r?.config.facets;for(let e in a){let t=a[e],n=t[0].facet,i=u&&u[e]||[];if(t.every(e=>e.type==0)){if(s[n.id]=c.length<<1|1,He(i,t))c.push(r.facet(n));else{let e=n.combine(t.map(e=>e.value));c.push(r&&n.compare(e,r.facet(n))?r.facet(n):e)}}else{for(let e of t)e.type==0?(s[e.id]=c.length<<1|1,c.push(e.value)):(s[e.id]=l.length<<1,l.push(t=>e.dynamicSlot(t)));s[n.id]=l.length<<1,l.push(e=>Ke(e,n,t))}}let d=l.map(e=>e(s));return new e(t,o,d,s,c,a)}};function nt(e,t,n){let r=[[],[],[],[],[]],i=new Map;function a(e,o){let s=i.get(e);if(s!=null){if(s<=o)return;let t=r[s].indexOf(e);t>-1&&r[s].splice(t,1),e instanceof et&&n.delete(e.compartment)}if(i.set(e,o),Array.isArray(e))for(let t of e)a(t,o);else if(e instanceof et){if(n.has(e.compartment))throw RangeError(`Duplicate use of compartment in extensions`);let r=t.get(e.compartment)||e.inner;n.set(e.compartment,r),a(r,o)}else if(e instanceof Qe)a(e.inner,e.prec);else if(e instanceof Je)r[o].push(e),e.provides&&a(e.provides,o);else if(e instanceof Ue)r[o].push(e),e.facet.extensions&&a(e.facet.extensions,Ye.default);else{let t=e.extension;if(!t)throw Error(`Unrecognized extension value in extension set (${e}).`);if(t==e)throw Error(`Unrecognized extension value in extension set (${e}). This sometimes happens because multiple instances of @codemirror/state are loaded, breaking instanceof checks.`);a(t,o)}}return a(e,Ye.default),r.reduce((e,t)=>e.concat(t))}function rt(e,t){if(t&1)return 2;let n=t>>1,r=e.status[n];if(r==4)throw Error(`Cyclic dependency between fields and/or facets`);if(r&2)return r;e.status[n]=4;let i=e.computeSlot(e,e.config.dynamicSlots[n]);return e.status[n]=2|i}function it(e,t){return t&1?e.config.staticValues[t>>1]:e.values[t>>1]}var at=M.define(),ot=M.define({combine:e=>e.some(e=>e),static:!0}),st=M.define({combine:e=>e.length?e[0]:void 0,static:!0}),ct=M.define(),lt=M.define(),ut=M.define(),dt=M.define({combine:e=>e.length?e[0]:!1}),ft=class{constructor(e,t){this.type=e,this.value=t}static define(){return new pt}},pt=class{of(e){return new ft(this,e)}},mt=class{constructor(e){this.map=e}of(e){return new N(this,e)}},N=class e{constructor(e,t){this.type=e,this.value=t}map(t){let n=this.type.map(this.value,t);return n===void 0?void 0:n==this.value?this:new e(this.type,n)}is(e){return this.type==e}static define(e={}){return new mt(e.map||(e=>e))}static mapEffects(e,t){if(!e.length)return e;let n=[];for(let r of e){let e=r.map(t);e&&n.push(e)}return n}};N.reconfigure=N.define(),N.appendConfig=N.define();var ht=class e{constructor(t,n,r,i,a,o){this.startState=t,this.changes=n,this.selection=r,this.effects=i,this.annotations=a,this.scrollIntoView=o,this._doc=null,this._state=null,r&&Be(r,n.newLength),a.some(t=>t.type==e.time)||(this.annotations=a.concat(e.time.of(Date.now())))}static create(t,n,r,i,a,o){return new e(t,n,r,i,a,o)}get newDoc(){return this._doc||=this.changes.apply(this.startState.doc)}get newSelection(){return this.selection||this.startState.selection.map(this.changes)}get state(){return this._state||this.startState.applyTransaction(this),this._state}annotation(e){for(let t of this.annotations)if(t.type==e)return t.value}get docChanged(){return!this.changes.empty}get reconfigured(){return this.startState.config!=this.state.config}isUserEvent(t){let n=this.annotation(e.userEvent);return!!(n&&(n==t||n.length>t.length&&n.slice(0,t.length)==t&&n[t.length]==`.`))}};ht.time=ft.define(),ht.userEvent=ft.define(),ht.addToHistory=ft.define(),ht.remote=ft.define();function gt(e,t){let n=[];for(let r=0,i=0;;){let a,o;if(r<e.length&&(i==t.length||t[i]>=e[r]))a=e[r++],o=e[r++];else if(i<t.length)a=t[i++],o=t[i++];else return n;!n.length||n[n.length-1]<a?n.push(a,o):n[n.length-1]<o&&(n[n.length-1]=o)}}function _t(e,t,n){let r,i,a;return n?(r=t.changes,i=Ne.empty(t.changes.length),a=e.changes.compose(t.changes)):(r=t.changes.map(e.changes),i=e.changes.mapDesc(t.changes,!0),a=e.changes.compose(r)),{changes:a,selection:t.selection?t.selection.map(i):e.selection?.map(r),effects:N.mapEffects(e.effects,r).concat(N.mapEffects(t.effects,i)),annotations:e.annotations.length?e.annotations.concat(t.annotations):t.annotations,scrollIntoView:e.scrollIntoView||t.scrollIntoView}}function vt(e,t,n){let r=t.selection,i=Ct(t.annotations);return t.userEvent&&(i=i.concat(ht.userEvent.of(t.userEvent))),{changes:t.changes instanceof Ne?t.changes:Ne.of(t.changes||[],n,e.facet(st)),selection:r&&(r instanceof j?r:j.single(r.anchor,r.head)),effects:Ct(t.effects),annotations:i,scrollIntoView:!!t.scrollIntoView}}function yt(e,t,n){let r=vt(e,t.length?t[0]:{},e.doc.length);t.length&&t[0].filter===!1&&(n=!1);for(let i=1;i<t.length;i++){t[i].filter===!1&&(n=!1);let a=!!t[i].sequential;r=_t(r,vt(e,t[i],a?r.changes.newLength:e.doc.length),a)}let i=ht.create(e,r.changes,r.selection,r.effects,r.annotations,r.scrollIntoView);return xt(n?bt(i):i)}function bt(e){let t=e.startState,n=!0;for(let r of t.facet(ct)){let t=r(e);if(t===!1){n=!1;break}Array.isArray(t)&&(n=n===!0?t:gt(n,t))}if(n!==!0){let r,i;if(n===!1)i=e.changes.invertedDesc,r=Ne.empty(t.doc.length);else{let t=e.changes.filter(n);r=t.changes,i=t.filtered.mapDesc(t.changes).invertedDesc}e=ht.create(t,r,e.selection&&e.selection.map(i),N.mapEffects(e.effects,i),e.annotations,e.scrollIntoView)}let r=t.facet(lt);for(let n=r.length-1;n>=0;n--){let i=r[n](e);e=i instanceof ht?i:Array.isArray(i)&&i.length==1&&i[0]instanceof ht?i[0]:yt(t,Ct(i),!1)}return e}function xt(e){let t=e.startState,n=t.facet(ut),r=e;for(let i=n.length-1;i>=0;i--){let a=n[i](e);a&&Object.keys(a).length&&(r=_t(r,vt(t,a,e.changes.newLength),!0))}return r==e?e:ht.create(t,e.changes,e.selection,r.effects,r.annotations,r.scrollIntoView)}var St=[];function Ct(e){return e==null?St:Array.isArray(e)?e:[e]}var P=(function(e){return e[e.Word=0]=`Word`,e[e.Space=1]=`Space`,e[e.Other=2]=`Other`,e})(P||={}),wt=/[\u00df\u0587\u0590-\u05f4\u0600-\u06ff\u3040-\u309f\u30a0-\u30ff\u3400-\u4db5\u4e00-\u9fcc\uac00-\ud7af]/,Tt;try{Tt=RegExp(`[\\p{Alphabetic}\\p{Number}_]`,`u`)}catch{}function Et(e){if(Tt)return Tt.test(e);for(let t=0;t<e.length;t++){let n=e[t];if(/\w/.test(n)||n>``&&(n.toUpperCase()!=n.toLowerCase()||wt.test(n)))return!0}return!1}function Dt(e){return t=>{if(!/\S/.test(t))return P.Space;if(Et(t))return P.Word;for(let n=0;n<e.length;n++)if(t.indexOf(e[n])>-1)return P.Word;return P.Other}}var F=class e{constructor(e,t,n,r,i,a){this.config=e,this.doc=t,this.selection=n,this.values=r,this.status=e.statusTemplate.slice(),this.computeSlot=i,a&&(a._state=this);for(let e=0;e<this.config.dynamicSlots.length;e++)rt(this,e<<1);this.computeSlot=null}field(e,t=!0){let n=this.config.address[e.id];if(n==null){if(t)throw RangeError(`Field is not present in this state`)}else return rt(this,n),it(this,n)}update(...e){return yt(this,e,!0)}applyTransaction(t){let n=this.config,{base:r,compartments:i}=n;for(let e of t.effects)e.is($e.reconfigure)?(n&&=(i=new Map,n.compartments.forEach((e,t)=>i.set(t,e)),null),i.set(e.value.compartment,e.value.extension)):e.is(N.reconfigure)?(n=null,r=e.value):e.is(N.appendConfig)&&(n=null,r=Ct(r).concat(e.value));let a;n?a=t.startState.values.slice():(n=tt.resolve(r,i,this),a=new e(n,this.doc,this.selection,n.dynamicSlots.map(()=>null),(e,t)=>t.reconfigure(e,this),null).values);let o=t.startState.facet(ot)?t.newSelection:t.newSelection.asSingle();new e(n,t.newDoc,o,a,(e,n)=>n.update(e,t),t)}replaceSelection(e){return typeof e==`string`&&(e=this.toText(e)),this.changeByRange(t=>({changes:{from:t.from,to:t.to,insert:e},range:j.cursor(t.from+e.length,-1)}))}changeByRange(e){let t=this.selection,n=e(t.ranges[0]),r=this.changes(n.changes),i=[n.range],a=Ct(n.effects);for(let n=1;n<t.ranges.length;n++){let o=e(t.ranges[n]),s=this.changes(o.changes),c=s.map(r);for(let e=0;e<n;e++)i[e]=i[e].map(c);let l=r.mapDesc(s,!0);i.push(o.range.map(l)),r=r.compose(c),a=N.mapEffects(a,c).concat(N.mapEffects(Ct(o.effects),l))}return{changes:r,selection:j.create(i,t.mainIndex),effects:a}}changes(t=[]){return t instanceof Ne?t:Ne.of(t,this.doc.length,this.facet(e.lineSeparator))}toText(t){return D.of(t.split(this.facet(e.lineSeparator)||Ae))}sliceDoc(e=0,t=this.doc.length){return this.doc.sliceString(e,t,this.lineBreak)}facet(e){let t=this.config.address[e.id];return t==null?e.default:(rt(this,t),it(this,t))}toJSON(e){let t={doc:this.sliceDoc(),selection:this.selection.toJSON()};if(e)for(let n in e){let r=e[n];r instanceof Je&&this.config.address[r.id]!=null&&(t[n]=r.spec.toJSON(this.field(e[n]),this))}return t}static fromJSON(t,n={},r){if(!t||typeof t.doc!=`string`)throw RangeError(`Invalid JSON representation for EditorState`);let i=[];if(r){for(let e in r)if(Object.prototype.hasOwnProperty.call(t,e)){let n=r[e],a=t[e];i.push(n.init(e=>n.spec.fromJSON(a,e)))}}return e.create({doc:t.doc,selection:j.fromJSON(t.selection),extensions:n.extensions?i.concat([n.extensions]):i})}static create(t={}){let n=tt.resolve(t.extensions||[],new Map),r=t.doc instanceof D?t.doc:D.of((t.doc||``).split(n.staticFacet(e.lineSeparator)||Ae)),i=t.selection?t.selection instanceof j?t.selection:j.single(t.selection.anchor,t.selection.head):j.single(0);return Be(i,r.length),n.staticFacet(ot)||(i=i.asSingle()),new e(n,r,i,n.dynamicSlots.map(()=>null),(e,t)=>t.create(e),null)}get tabSize(){return this.facet(e.tabSize)}get lineBreak(){return this.facet(e.lineSeparator)||`
`}get readOnly(){return this.facet(dt)}phrase(t,...n){for(let n of this.facet(e.phrases))if(Object.prototype.hasOwnProperty.call(n,t)){t=n[t];break}return n.length&&(t=t.replace(/\$(\$|\d*)/g,(e,t)=>{if(t==`$`)return`$`;let r=+(t||1);return!r||r>n.length?e:n[r-1]})),t}languageDataAt(e,t,n=-1){let r=[];for(let i of this.facet(at))for(let a of i(this,t,n))Object.prototype.hasOwnProperty.call(a,e)&&r.push(a[e]);return r}charCategorizer(e){let t=this.languageDataAt(`wordChars`,e);return Dt(t.length?t[0]:``)}wordAt(e){let{text:t,from:n,length:r}=this.doc.lineAt(e),i=this.charCategorizer(e),a=e-n,o=e-n;for(;a>0;){let e=k(t,a,!1);if(i(t.slice(e,a))!=P.Word)break;a=e}for(;o<r;){let e=k(t,o);if(i(t.slice(o,e))!=P.Word)break;o=e}return a==o?null:j.range(a+n,o+n)}};F.allowMultipleSelections=ot,F.tabSize=M.define({combine:e=>e.length?e[0]:4}),F.lineSeparator=st,F.readOnly=dt,F.phrases=M.define({compare(e,t){let n=Object.keys(e),r=Object.keys(t);return n.length==r.length&&n.every(n=>e[n]==t[n])}}),F.languageData=at,F.changeFilter=ct,F.transactionFilter=lt,F.transactionExtender=ut,$e.reconfigure=N.define();function Ot(e,t,n={}){let r={};for(let t of e)for(let e of Object.keys(t)){let i=t[e],a=r[e];if(a===void 0)r[e]=i;else if(a!==i&&i!==void 0){if(Object.hasOwnProperty.call(n,e))r[e]=n[e](a,i);else throw Error(`Config merge conflict for field `+e)}}for(let e in t)r[e]===void 0&&(r[e]=t[e]);return r}var kt=class{eq(e){return this==e}range(e,t=e){return jt.create(e,t,this)}};kt.prototype.startSide=kt.prototype.endSide=0,kt.prototype.point=!1,kt.prototype.mapMode=je.TrackDel;function At(e,t){return e==t||e.constructor==t.constructor&&e.eq(t)}var jt=class e{constructor(e,t,n){this.from=e,this.to=t,this.value=n}static create(t,n,r){return new e(t,n,r)}};function Mt(e,t){return e.from-t.from||e.value.startSide-t.value.startSide}var Nt=class e{constructor(e,t,n,r){this.from=e,this.to=t,this.value=n,this.maxPoint=r}get length(){return Pt(this.to)}findIndex(e,t,n,r=0){let i=n?this.to:this.from;for(let a=r,o=i.length;;){if(a==o)return a;let r=a+o>>1,s=i[r]-e||(n?this.value[r].endSide:this.value[r].startSide)-t;if(r==a)return s>=0?a:o;s>=0?o=r:a=r+1}}between(e,t,n,r){for(let i=this.findIndex(t,-1e9,!0),a=this.findIndex(n,1e9,!1,i);i<a;i++)if(r(this.from[i]+e,this.to[i]+e,this.value[i])===!1)return!1}map(t,n,r,i,a){let o=[],s=[],c=[],l=-1,u=-1;iter:for(let e=0;e<this.value.length;e++){let d=this.value[e],f=this.from[e]+t,p=this.to[e]+t,m,h;if(f==p){let e=n.mapPos(f,d.startSide,d.mapMode);if(e==null||(m=h=e,d.startSide!=d.endSide&&(h=n.mapPos(f,d.endSide),h<m)))continue}else if(m=n.mapPos(f,d.startSide),h=n.mapPos(p,d.endSide),m>h||m==h&&d.startSide>0&&d.endSide<=0)continue;if(!((h-m||d.endSide-d.startSide)<0)){if(l<0&&(l=m),d.point&&(u=Math.max(u,h-m)),(m-r||d.startSide-i)>=0)o.push(d),s.push(m-l),c.push(h-l),r=h,i=d.endSide;else{if(m==h)for(let e=o.length;e>0;e--){if((m-(c[e-1]+l)||d.startSide-o[e-1].endSide)>=0){o.splice(e,0,d),s.splice(e,0,m-l),c.splice(e,0,h-l);continue iter}if((m-(s[e-1]+l)||d.endSide-o[e-1].startSide)>0)break}a(m,h,d)}}}return{mapped:o.length?new e(s,c,o,u):null,pos:l}}},I=class e{constructor(e,t,n,r){this.chunkPos=e,this.chunk=t,this.nextLayer=n,this.maxPoint=r}static create(t,n,r,i){return new e(t,n,r,i)}get length(){let e=this.chunk.length-1;return e<0?0:Math.max(this.chunkEnd(e),this.nextLayer.length)}get size(){if(this.isEmpty)return 0;let e=this.nextLayer.size;for(let t of this.chunk)e+=t.value.length;return e}chunkEnd(e){return this.chunkPos[e]+this.chunk[e].length}update(t){let{add:n=[],sort:r=!1,filterFrom:i=0,filterTo:a=this.length}=t,o=t.filter;if(n.length==0&&!o)return this;if(r&&(n=n.slice().sort(Mt)),this.isEmpty)return n.length?e.of(n):this;let s=new Rt(this,null,-1).goto(0),c=0,l=[],u=new It;for(;s.value||c<n.length;)if(c<n.length&&(s.from-n[c].from||s.startSide-n[c].value.startSide)>=0){let e=n[c++];u.addInner(e.from,e.to,e.value,!1)||l.push(e)}else s.rangeIndex==1&&s.chunkIndex<this.chunk.length&&(c==n.length||this.chunkEnd(s.chunkIndex)<n[c].from)&&(!o||i>this.chunkEnd(s.chunkIndex)||a<this.chunkPos[s.chunkIndex])&&u.addChunk(this.chunkPos[s.chunkIndex],this.chunk[s.chunkIndex])?s.nextChunk():((!o||i>s.to||a<s.from||o(s.from,s.to,s.value))&&(u.addInner(s.from,s.to,s.value,!1)||l.push(jt.create(s.from,s.to,s.value))),s.next());return u.finishInner(this.nextLayer.isEmpty&&!l.length?e.empty:this.nextLayer.update({add:l,filter:o,filterFrom:i,filterTo:a}))}map(t){if(t.empty||this.isEmpty)return this;let n=[],r=[],i=-1,a,o=(e,t,n)=>{a||=new It,a.addRange(e,t,n,!1)};for(let e=0;e<this.chunk.length;e++){let a=this.chunkPos[e],s=this.chunk[e],c=t.touchesRange(a,a+s.length);if(c===!1)i=Math.max(i,s.maxPoint),n.push(s),r.push(t.mapPos(a));else if(c===!0){let[e,c]=n.length?[Pt(r)+Pt(n).length,Pt(Pt(n).value).endSide]:[-1,-1],{mapped:l,pos:u}=s.map(a,t,e,c,o);l&&(i=Math.max(i,l.maxPoint),n.push(l),r.push(u))}}let s=this.nextLayer.map(t);return a&&(s=a.finishInner(s)),n.length==0?s:new e(r,n,s||e.empty,i)}between(e,t,n){if(!this.isEmpty){for(let r=0;r<this.chunk.length;r++){let i=this.chunkPos[r],a=this.chunk[r];if(t>=i&&e<=i+a.length&&a.between(i,e-i,t-i,n)===!1)return}this.nextLayer.between(e,t,n)}}iter(e=0){return zt.from([this]).goto(e)}get isEmpty(){return this.nextLayer==this}static iter(e,t=0){return zt.from(e).goto(t)}static compare(e,t,n,r,i=-1){let a=e.filter(e=>e.maxPoint>0||!e.isEmpty&&e.maxPoint>=i),o=t.filter(e=>e.maxPoint>0||!e.isEmpty&&e.maxPoint>=i),s=Lt(a,o,n),c=new Vt(a,s,i),l=new Vt(o,s,i);n.iterGaps((e,t,n)=>Ht(c,e,l,t,n,r)),n.empty&&n.length==0&&Ht(c,0,l,0,0,r)}static eq(e,t,n=0,r){r??=1e9-1;let i=e.filter(e=>!e.isEmpty&&t.indexOf(e)<0),a=t.filter(t=>!t.isEmpty&&e.indexOf(t)<0);if(i.length!=a.length)return!1;if(!i.length)return!0;let o=Lt(i,a),s=new Vt(i,o,0).goto(n),c=new Vt(a,o,0).goto(n);for(;;){if(s.to!=c.to||!Ut(s.active,c.active)||s.point&&(!c.point||!At(s.point,c.point)))return!1;if(s.to>r)return!0;s.next(),c.next()}}static spans(e,t,n,r,i=-1){let a=new Vt(e,null,i).goto(t),o=t,s=a.openStart;for(;;){let e=Math.min(a.to,n);if(a.point){let n=a.activeForPoint(a.to),i=a.pointFrom<t?n.length+1:a.point.startSide<0?n.length:Math.min(n.length,s);r.point(o,e,a.point,n,i,a.pointRank),s=Math.min(a.openEnd(e),n.length)}else e>o&&(r.span(o,e,a.active,s),s=a.openEnd(e));if(a.to>n)return s+(a.point&&a.to>n?1:0);o=a.to,a.next()}}static of(e,t=!1){let n=new It;for(let r of e instanceof jt?[e]:t?Ft(e):e)n.add(r.from,r.to,r.value);return n.finish()}static join(t){if(!t.length)return e.empty;let n=Pt(t);for(let r=t.length-2;r>=0;r--)for(let i=t[r];i!=e.empty;i=i.nextLayer)n=new e(i.chunkPos,i.chunk,n,Math.max(i.maxPoint,n.maxPoint));return n}};I.empty=new I([],[],null,-1);function Pt(e){return e[e.length-1]}function Ft(e){if(e.length>1)for(let t=e[0],n=1;n<e.length;n++){let r=e[n];if(Mt(t,r)>0)return e.slice().sort(Mt);t=r}return e}I.empty.nextLayer=I.empty;var It=class e{finishChunk(e){this.chunks.push(new Nt(this.from,this.to,this.value,this.maxPoint)),this.chunkPos.push(this.chunkStart),this.chunkStart=-1,this.setMaxPoint=Math.max(this.setMaxPoint,this.maxPoint),this.maxPoint=-1,e&&(this.from=[],this.to=[],this.value=[])}constructor(){this.chunks=[],this.chunkPos=[],this.chunkStart=-1,this.last=null,this.lastFrom=-1e9,this.lastTo=-1e9,this.from=[],this.to=[],this.value=[],this.maxPoint=-1,this.setMaxPoint=-1,this.nextLayer=null}add(e,t,n){this.addRange(e,t,n,!0)}addRange(t,n,r,i){this.addInner(t,n,r,i)||(this.nextLayer||=new e).addRange(t,n,r,i)}addInner(e,t,n,r){let i=e-this.lastTo||n.startSide-this.last.endSide;if(r&&i<=0&&(e-this.lastFrom||n.startSide-this.last.startSide)<0)throw Error("Ranges must be added sorted by `from` position and `startSide`");return i<0?!1:(this.from.length==250&&this.finishChunk(!0),this.chunkStart<0&&(this.chunkStart=e),this.from.push(e-this.chunkStart),this.to.push(t-this.chunkStart),this.last=n,this.lastFrom=e,this.lastTo=t,this.value.push(n),n.point&&(this.maxPoint=Math.max(this.maxPoint,t-e)),!0)}addChunk(e,t){if((e-this.lastTo||t.value[0].startSide-this.last.endSide)<0)return!1;this.from.length&&this.finishChunk(!0),this.setMaxPoint=Math.max(this.setMaxPoint,t.maxPoint),this.chunks.push(t),this.chunkPos.push(e);let n=t.value.length-1;return this.last=t.value[n],this.lastFrom=t.from[n]+e,this.lastTo=t.to[n]+e,!0}finish(){return this.finishInner(I.empty)}finishInner(e){if(this.from.length&&this.finishChunk(!1),this.chunks.length==0)return e;let t=I.create(this.chunkPos,this.chunks,this.nextLayer?this.nextLayer.finishInner(e):e,this.setMaxPoint);return this.from=null,t}};function Lt(e,t,n){let r=new Map;for(let t of e)for(let e=0;e<t.chunk.length;e++)t.chunk[e].maxPoint<=0&&r.set(t.chunk[e],t.chunkPos[e]);let i=new Set;for(let e of t)for(let t=0;t<e.chunk.length;t++){let a=r.get(e.chunk[t]);a!=null&&(n?n.mapPos(a):a)==e.chunkPos[t]&&!n?.touchesRange(a,a+e.chunk[t].length)&&i.add(e.chunk[t])}return i}var Rt=class{constructor(e,t,n,r=0){this.layer=e,this.skip=t,this.minPoint=n,this.rank=r}get startSide(){return this.value?this.value.startSide:0}get endSide(){return this.value?this.value.endSide:0}goto(e,t=-1e9){return this.chunkIndex=this.rangeIndex=0,this.gotoInner(e,t,!1),this}gotoInner(e,t,n){for(;this.chunkIndex<this.layer.chunk.length;){let t=this.layer.chunk[this.chunkIndex];if(!(this.skip&&this.skip.has(t)||this.layer.chunkEnd(this.chunkIndex)<e||t.maxPoint<this.minPoint))break;this.chunkIndex++,n=!1}if(this.chunkIndex<this.layer.chunk.length){let r=this.layer.chunk[this.chunkIndex].findIndex(e-this.layer.chunkPos[this.chunkIndex],t,!0);(!n||this.rangeIndex<r)&&this.setRangeIndex(r)}this.next()}forward(e,t){(this.to-e||this.endSide-t)<0&&this.gotoInner(e,t,!0)}next(){for(;;)if(this.chunkIndex==this.layer.chunk.length){this.from=this.to=1e9,this.value=null;break}else{let e=this.layer.chunkPos[this.chunkIndex],t=this.layer.chunk[this.chunkIndex],n=e+t.from[this.rangeIndex];if(this.from=n,this.to=e+t.to[this.rangeIndex],this.value=t.value[this.rangeIndex],this.setRangeIndex(this.rangeIndex+1),this.minPoint<0||this.value.point&&this.to-this.from>=this.minPoint)break}}setRangeIndex(e){if(e==this.layer.chunk[this.chunkIndex].value.length){if(this.chunkIndex++,this.skip)for(;this.chunkIndex<this.layer.chunk.length&&this.skip.has(this.layer.chunk[this.chunkIndex]);)this.chunkIndex++;this.rangeIndex=0}else this.rangeIndex=e}nextChunk(){this.chunkIndex++,this.rangeIndex=0,this.next()}compare(e){return this.from-e.from||this.startSide-e.startSide||this.rank-e.rank||this.to-e.to||this.endSide-e.endSide}},zt=class e{constructor(e){this.heap=e}static from(t,n=null,r=-1){let i=[];for(let e=0;e<t.length;e++)for(let a=t[e];!a.isEmpty;a=a.nextLayer)a.maxPoint>=r&&i.push(new Rt(a,n,r,e));return i.length==1?i[0]:new e(i)}get startSide(){return this.value?this.value.startSide:0}goto(e,t=-1e9){for(let n of this.heap)n.goto(e,t);for(let e=this.heap.length>>1;e>=0;e--)Bt(this.heap,e);return this.next(),this}forward(e,t){for(let n of this.heap)n.forward(e,t);for(let e=this.heap.length>>1;e>=0;e--)Bt(this.heap,e);(this.to-e||this.value.endSide-t)<0&&this.next()}next(){if(this.heap.length==0)this.from=this.to=1e9,this.value=null,this.rank=-1;else{let e=this.heap[0];this.from=e.from,this.to=e.to,this.value=e.value,this.rank=e.rank,e.value&&e.next(),Bt(this.heap,0)}}};function Bt(e,t){for(let n=e[t];;){let r=(t<<1)+1;if(r>=e.length)break;let i=e[r];if(r+1<e.length&&i.compare(e[r+1])>=0&&(i=e[r+1],r++),n.compare(i)<0)break;e[r]=n,e[t]=i,t=r}}var Vt=class{constructor(e,t,n){this.minPoint=n,this.active=[],this.activeTo=[],this.activeRank=[],this.minActive=-1,this.point=null,this.pointFrom=0,this.pointRank=0,this.to=-1e9,this.endSide=0,this.openStart=-1,this.cursor=zt.from(e,t,n)}goto(e,t=-1e9){return this.cursor.goto(e,t),this.active.length=this.activeTo.length=this.activeRank.length=0,this.minActive=-1,this.to=e,this.endSide=t,this.openStart=-1,this.next(),this}forward(e,t){for(;this.minActive>-1&&(this.activeTo[this.minActive]-e||this.active[this.minActive].endSide-t)<0;)this.removeActive(this.minActive);this.cursor.forward(e,t)}removeActive(e){Wt(this.active,e),Wt(this.activeTo,e),Wt(this.activeRank,e),this.minActive=Kt(this.active,this.activeTo)}addActive(e){let t=0,{value:n,to:r,rank:i}=this.cursor;for(;t<this.activeRank.length&&(i-this.activeRank[t]||r-this.activeTo[t])>0;)t++;Gt(this.active,t,n),Gt(this.activeTo,t,r),Gt(this.activeRank,t,i),e&&Gt(e,t,this.cursor.from),this.minActive=Kt(this.active,this.activeTo)}next(){let e=this.to,t=this.point;this.point=null;let n=this.openStart<0?[]:null;for(;;){let r=this.minActive;if(r>-1&&(this.activeTo[r]-this.cursor.from||this.active[r].endSide-this.cursor.startSide)<0){if(this.activeTo[r]>e){this.to=this.activeTo[r],this.endSide=this.active[r].endSide;break}this.removeActive(r),n&&Wt(n,r)}else if(!this.cursor.value){this.to=this.endSide=1e9;break}else if(this.cursor.from>e){this.to=this.cursor.from,this.endSide=this.cursor.startSide;break}else{let e=this.cursor.value;if(!e.point)this.addActive(n),this.cursor.next();else if(t&&this.cursor.to==this.to&&this.cursor.from<this.cursor.to)this.cursor.next();else{this.point=e,this.pointFrom=this.cursor.from,this.pointRank=this.cursor.rank,this.to=this.cursor.to,this.endSide=e.endSide,this.cursor.next(),this.forward(this.to,this.endSide);break}}}if(n){this.openStart=0;for(let t=n.length-1;t>=0&&n[t]<e;t--)this.openStart++}}activeForPoint(e){if(!this.active.length)return this.active;let t=[];for(let n=this.active.length-1;n>=0&&!(this.activeRank[n]<this.pointRank);n--)(this.activeTo[n]>e||this.activeTo[n]==e&&this.active[n].endSide>=this.point.endSide)&&t.push(this.active[n]);return t.reverse()}openEnd(e){let t=0;for(let n=this.activeTo.length-1;n>=0&&this.activeTo[n]>e;n--)t++;return t}};function Ht(e,t,n,r,i,a){e.goto(t),n.goto(r);let o=r+i,s=r,c=r-t,l=!!a.boundChange;for(let t=!1;;){let r=e.to+c-n.to,i=r||e.endSide-n.endSide,u=i<0?e.to+c:n.to,d=Math.min(u,o);if(e.point||n.point?(e.point&&n.point&&At(e.point,n.point)&&Ut(e.activeForPoint(e.to),n.activeForPoint(n.to))||a.comparePoint(s,d,e.point,n.point),t=!1):(t&&=(a.boundChange(s),!1),d>s&&!Ut(e.active,n.active)&&a.compareRange(s,d,e.active,n.active),l&&d<o&&(r||e.openEnd(u)!=n.openEnd(u))&&(t=!0)),u>o)break;s=u,i<=0&&e.next(),i>=0&&n.next()}}function Ut(e,t){if(e.length!=t.length)return!1;for(let n=0;n<e.length;n++)if(e[n]!=t[n]&&!At(e[n],t[n]))return!1;return!0}function Wt(e,t){for(let n=t,r=e.length-1;n<r;n++)e[n]=e[n+1];e.pop()}function Gt(e,t,n){for(let n=e.length-1;n>=t;n--)e[n+1]=e[n];e[t]=n}function Kt(e,t){let n=-1,r=1e9;for(let i=0;i<t.length;i++)(t[i]-r||e[i].endSide-e[n].endSide)<0&&(n=i,r=t[i]);return n}function qt(e,t,n=e.length){let r=0;for(let i=0;i<n&&i<e.length;)e.charCodeAt(i)==9?(r+=t-r%t,i++):(r++,i=k(e,i));return r}function Jt(e,t,n,r){for(let r=0,i=0;;){if(i>=t)return r;if(r==e.length)break;i+=e.charCodeAt(r)==9?n-i%n:1,r=k(e,r)}return r===!0?-1:e.length}for(var Yt=`ͼ`,Xt=typeof Symbol>`u`?`__ͼ`:Symbol.for(Yt),Zt=typeof Symbol>`u`?`__styleSet`+Math.floor(Math.random()*1e8):Symbol(`styleSet`),Qt=typeof globalThis<`u`?globalThis:typeof window<`u`?window:{},$t=class{constructor(e,t){this.rules=[];let{finish:n}=t||{};function r(e){return/^@/.test(e)?[e]:e.split(/,\s*/)}function i(e,t,a,o){let s=[],c=/^@(\w+)\b/.exec(e[0]),l=c&&c[1]==`keyframes`;if(c&&t==null)return a.push(e[0]+`;`);for(let n in t){let o=t[n];if(/&/.test(n))i(n.split(/,\s*/).map(t=>e.map(e=>t.replace(/&/,e))).reduce((e,t)=>e.concat(t)),o,a);else if(o&&typeof o==`object`){if(!c)throw RangeError(`The value of a property (`+n+`) should be a primitive value.`);i(r(n),o,s,l)}else o!=null&&s.push(n.replace(/_.*/,``).replace(/[A-Z]/g,e=>`-`+e.toLowerCase())+`: `+o+`;`)}(s.length||l)&&a.push((n&&!c&&!o?e.map(n):e).join(`, `)+` {`+s.join(` `)+`}`)}for(let t in e)i(r(t),e[t],this.rules)}getRules(){return this.rules.join(`
`)}static newName(){let e=Qt[Xt]||1;return Qt[Xt]=e+1,Yt+e.toString(36)}static mount(e,t,n){let r=e[Zt],i=n&&n.nonce;r?i&&r.setNonce(i):r=new tn(e,i),r.mount(Array.isArray(t)?t:[t],e)}},en=new Map,tn=class{constructor(e,t){let n=e.ownerDocument||e,r=n.defaultView;if(!e.head&&e.adoptedStyleSheets&&r.CSSStyleSheet){let t=en.get(n);if(t)return e[Zt]=t;this.sheet=new r.CSSStyleSheet,en.set(n,this)}else this.styleTag=n.createElement(`style`),t&&this.styleTag.setAttribute(`nonce`,t);this.modules=[],e[Zt]=this}mount(e,t){let n=this.sheet,r=0,i=0,a=!1;for(let t=0;t<e.length;t++){let o=e[t],s=this.modules.indexOf(o);if(s<i&&s>-1&&(this.modules.splice(s,1),a=!0,i--,s=-1),s==-1){if(this.modules.splice(i++,0,o),a=!0,n)for(let e=0;e<o.rules.length;e++)n.insertRule(o.rules[e],r++)}else{for(;i<s;)r+=this.modules[i++].rules.length;r+=o.rules.length,i++}}if(n)t.adoptedStyleSheets.indexOf(this.sheet)<0&&(t.adoptedStyleSheets=[this.sheet,...t.adoptedStyleSheets]);else{if(a){let e=``;for(let t=0;t<this.modules.length;t++)e+=this.modules[t].getRules()+`
`;this.styleTag.textContent=e}let e=t.head||t;this.styleTag.parentNode!=e&&e.insertBefore(this.styleTag,e.firstChild)}}setNonce(e){this.styleTag&&this.styleTag.getAttribute(`nonce`)!=e&&this.styleTag.setAttribute(`nonce`,e)}},nn={8:`Backspace`,9:`Tab`,10:`Enter`,12:`NumLock`,13:`Enter`,16:`Shift`,17:`Control`,18:`Alt`,20:`CapsLock`,27:`Escape`,32:` `,33:`PageUp`,34:`PageDown`,35:`End`,36:`Home`,37:`ArrowLeft`,38:`ArrowUp`,39:`ArrowRight`,40:`ArrowDown`,44:`PrintScreen`,45:`Insert`,46:`Delete`,59:`;`,61:`=`,91:`Meta`,92:`Meta`,106:`*`,107:`+`,108:`,`,109:`-`,110:`.`,111:`/`,144:`NumLock`,145:`ScrollLock`,160:`Shift`,161:`Shift`,162:`Control`,163:`Control`,164:`Alt`,165:`Alt`,173:`-`,186:`;`,187:`=`,188:`,`,189:`-`,190:`.`,191:`/`,192:"`",219:`[`,220:`\\`,221:`]`,222:`'`},rn={48:`)`,49:`!`,50:`@`,51:`#`,52:`$`,53:`%`,54:`^`,55:`&`,56:`*`,57:`(`,59:`:`,61:`+`,173:`_`,186:`:`,187:`+`,188:`<`,189:`_`,190:`>`,191:`?`,192:`~`,219:`{`,220:`|`,221:`}`,222:`"`},an=typeof navigator<`u`&&/Mac/.test(navigator.platform),on=typeof navigator<`u`&&/MSIE \d|Trident\/(?:[7-9]|\d{2,})\..*rv:(\d+)/.exec(navigator.userAgent),sn=0;sn<10;sn++)nn[48+sn]=nn[96+sn]=String(sn);for(var sn=1;sn<=24;sn++)nn[sn+111]=`F`+sn;for(var sn=65;sn<=90;sn++)nn[sn]=String.fromCharCode(sn+32),rn[sn]=String.fromCharCode(sn);for(var cn in nn)rn.hasOwnProperty(cn)||(rn[cn]=nn[cn]);function ln(e){var t=!(an&&e.metaKey&&e.shiftKey&&!e.ctrlKey&&!e.altKey||on&&e.shiftKey&&e.key&&e.key.length==1||e.key==`Unidentified`)&&e.key||(e.shiftKey?rn:nn)[e.keyCode]||e.key||`Unidentified`;return t==`Esc`&&(t=`Escape`),t==`Del`&&(t=`Delete`),t==`Left`&&(t=`ArrowLeft`),t==`Up`&&(t=`ArrowUp`),t==`Right`&&(t=`ArrowRight`),t==`Down`&&(t=`ArrowDown`),t}function L(){var e=arguments[0];typeof e==`string`&&(e=document.createElement(e));var t=1,n=arguments[1];if(n&&typeof n==`object`&&n.nodeType==null&&!Array.isArray(n)){for(var r in n)if(Object.prototype.hasOwnProperty.call(n,r)){var i=n[r];typeof i==`string`?e.setAttribute(r,i):i!=null&&(e[r]=i)}t++}for(;t<arguments.length;t++)un(e,arguments[t]);return e}function un(e,t){if(typeof t==`string`)e.appendChild(document.createTextNode(t));else if(t!=null){if(t.nodeType!=null)e.appendChild(t);else if(Array.isArray(t))for(var n=0;n<t.length;n++)un(e,t[n]);else throw RangeError(`Unsupported child node: `+t)}}var dn=typeof navigator<`u`?navigator:{userAgent:``,vendor:``,platform:``},fn=typeof document<`u`?document:{documentElement:{style:{}}},pn=/Edge\/(\d+)/.exec(dn.userAgent),mn=/MSIE \d/.test(dn.userAgent),hn=/Trident\/(?:[7-9]|\d{2,})\..*rv:(\d+)/.exec(dn.userAgent),gn=!!(mn||hn||pn),_n=!gn&&/gecko\/(\d+)/i.test(dn.userAgent),vn=!gn&&/Chrome\/(\d+)/.exec(dn.userAgent),yn=`webkitFontSmoothing`in fn.documentElement.style,bn=!gn&&/Apple Computer/.test(dn.vendor),xn=bn&&(/Mobile\/\w+/.test(dn.userAgent)||dn.maxTouchPoints>2),R={mac:xn||/Mac/.test(dn.platform),windows:/Win/.test(dn.platform),linux:/Linux|X11/.test(dn.platform),ie:gn,ie_version:mn?fn.documentMode||6:hn?+hn[1]:pn?+pn[1]:0,gecko:_n,gecko_version:_n?+(/Firefox\/(\d+)/.exec(dn.userAgent)||[0,0])[1]:0,chrome:!!vn,chrome_version:vn?+vn[1]:0,ios:xn,android:/Android\b/.test(dn.userAgent),webkit:yn,webkit_version:yn?+(/\bAppleWebKit\/(\d+)/.exec(dn.userAgent)||[0,0])[1]:0,safari:bn,safari_version:bn?+(/\bVersion\/(\d+(\.\d+)?)/.exec(dn.userAgent)||[0,0])[1]:0,tabSize:fn.documentElement.style.tabSize==null?`-moz-tab-size`:`tab-size`};function Sn(e,t){for(let n in e)n==`class`&&t.class?t.class+=` `+e.class:n==`style`&&t.style?t.style+=`;`+e.style:t[n]=e[n];return t}var Cn=Object.create(null);function wn(e,t,n){if(e==t)return!0;e||=Cn,t||=Cn;let r=Object.keys(e),i=Object.keys(t);if(r.length-0!=i.length-0)return!1;for(let a of r)if(a!=n&&(i.indexOf(a)==-1||e[a]!==t[a]))return!1;return!0}function Tn(e,t){for(let n=e.attributes.length-1;n>=0;n--){let r=e.attributes[n].name;t[r]??e.removeAttribute(r)}for(let n in t){let r=t[n];n==`style`?e.style.cssText=r:e.getAttribute(n)!=r&&e.setAttribute(n,r)}}function En(e,t,n){let r=!1;if(t)for(let i in t)n&&i in n||(r=!0,i==`style`?e.style.cssText=``:e.removeAttribute(i));if(n)for(let i in n)t&&t[i]==n[i]||(r=!0,i==`style`?e.style.cssText=n[i]:e.setAttribute(i,n[i]));return r}function Dn(e){let t=Object.create(null);for(let n=0;n<e.attributes.length;n++){let r=e.attributes[n];t[r.name]=r.value}return t}var On=class{eq(e){return!1}updateDOM(e,t,n){return!1}compare(e){return this==e||this.constructor==e.constructor&&this.eq(e)}get estimatedHeight(){return-1}get lineBreaks(){return 0}ignoreEvent(e){return!0}coordsAt(e,t,n){return null}get isHidden(){return!1}get editable(){return!1}destroy(e){}},kn=(function(e){return e[e.Text=0]=`Text`,e[e.WidgetBefore=1]=`WidgetBefore`,e[e.WidgetAfter=2]=`WidgetAfter`,e[e.WidgetRange=3]=`WidgetRange`,e})(kn||={}),z=class extends kt{constructor(e,t,n,r){super(),this.startSide=e,this.endSide=t,this.widget=n,this.spec=r}get heightRelevant(){return!1}static mark(e){return new An(e)}static widget(e){let t=Math.max(-1e4,Math.min(1e4,e.side||0)),n=!!e.block;return t+=n&&!e.inlineOrder?t>0?3e8:-4e8:t>0?1e8:-1e8,new Mn(e,t,t,n,e.widget||null,!1)}static replace(e){let t=!!e.block,n,r;if(e.isBlockGap)n=-5e8,r=4e8;else{let{start:i,end:a}=Nn(e,t);n=(i?t?-3e8:-1:5e8)-1,r=(a?t?2e8:1:-6e8)+1}return new Mn(e,n,r,t,e.widget||null,!0)}static line(e){return new jn(e)}static set(e,t=!1){return I.of(e,t)}hasHeight(){return this.widget?this.widget.estimatedHeight>-1:!1}};z.none=I.empty;var An=class e extends z{constructor(e){let{start:t,end:n}=Nn(e);super(t?-1:5e8,n?1:-6e8,null,e),this.tagName=e.tagName||`span`,this.attrs=e.class&&e.attributes?Sn(e.attributes,{class:e.class}):e.class?{class:e.class}:e.attributes||Cn}eq(t){return this==t||t instanceof e&&this.tagName==t.tagName&&wn(this.attrs,t.attrs)}range(e,t=e){if(e>=t)throw RangeError(`Mark decorations may not be empty`);return super.range(e,t)}};An.prototype.point=!1;var jn=class e extends z{constructor(e){super(-2e8,-2e8,null,e)}eq(t){return t instanceof e&&this.spec.class==t.spec.class&&wn(this.spec.attributes,t.spec.attributes)}range(e,t=e){if(t!=e)throw RangeError(`Line decoration ranges must be zero-length`);return super.range(e,t)}};jn.prototype.mapMode=je.TrackBefore,jn.prototype.point=!0;var Mn=class e extends z{constructor(e,t,n,r,i,a){super(t,n,i,e),this.block=r,this.isReplace=a,this.mapMode=r?t<=0?je.TrackBefore:je.TrackAfter:je.TrackDel}get type(){return this.startSide==this.endSide?this.startSide<=0?kn.WidgetBefore:kn.WidgetAfter:kn.WidgetRange}get heightRelevant(){return this.block||!!this.widget&&(this.widget.estimatedHeight>=5||this.widget.lineBreaks>0)}eq(t){return t instanceof e&&Pn(this.widget,t.widget)&&this.block==t.block&&this.startSide==t.startSide&&this.endSide==t.endSide}range(e,t=e){if(this.isReplace&&(e>t||e==t&&this.startSide>0&&this.endSide<=0))throw RangeError(`Invalid range for replacement decoration`);if(!this.isReplace&&t!=e)throw RangeError(`Widget decorations can only have zero-length ranges`);return super.range(e,t)}};Mn.prototype.point=!0;function Nn(e,t=!1){let{inclusiveStart:n,inclusiveEnd:r}=e;return n??=e.inclusive,r??=e.inclusive,{start:n??t,end:r??t}}function Pn(e,t){return e==t||!!(e&&t&&e.compare(t))}function Fn(e,t,n,r=0){let i=n.length-1;i>=0&&n[i]+r>=e?n[i]=Math.max(n[i],t):n.push(e,t)}var In=class e extends kt{constructor(e,t,n){super(),this.tagName=e,this.attributes=t,this.rank=n}eq(t){return t==this||t instanceof e&&this.tagName==t.tagName&&wn(this.attributes,t.attributes)}static create(t){return new e(t.tagName,t.attributes||Cn,t.rank==null?50:Math.max(0,Math.min(t.rank,100)))}static set(e,t=!1){return I.of(e,t)}};In.prototype.startSide=In.prototype.endSide=-1;var B=e=>e.nodeType==1,Ln=e=>e.nodeType==3;function Rn(e){let t;return t=e.nodeType==11?e.getSelection?e:e.ownerDocument:e,t.getSelection()}function zn(e,t){return t?e==t||e.contains(t.nodeType==1?t:t.parentNode):!1}function Bn(e,t){if(!t.anchorNode)return!1;try{return zn(e,t.anchorNode)}catch{return!1}}function Vn(e){return Ln(e)?ir(e,0,e.nodeValue.length).getClientRects():B(e)?e.getClientRects():[]}function Hn(e,t,n,r){return n?Gn(e,t,n,r,-1)||Gn(e,t,n,r,1):!1}function Un(e){for(var t=0;;t++)if(e=e.previousSibling,!e)return t}function Wn(e){return B(e)&&/^(DIV|P|LI|UL|OL|BLOCKQUOTE|DD|DT|H\d|SECTION|PRE)$/.test(e.nodeName)}function Gn(e,t,n,r,i){for(;;){if(e==n&&t==r)return!0;if(t==(i<0?0:Kn(e))){if(e.nodeName==`DIV`)return!1;let n=e.parentNode;if(!n||!B(n))return!1;t=Un(e)+(i<0?0:1),e=n}else if(B(e)){if(e=e.childNodes[t+(i<0?-1:0)],B(e)&&(e.nodeName==`IMG`||e.nodeName==`BR`||e.contentEditable==`false`))return!1;t=i<0?Kn(e):0}else return!1}}function Kn(e){return Ln(e)?e.nodeValue.length:e.childNodes.length}function qn(e,t){let{left:n,right:r}=e;if(n==r)return e;let i=t?n:r;return{left:i,right:i,top:e.top,bottom:e.bottom}}function Jn(e){let t=e.visualViewport;return t?{left:0,right:t.width,top:0,bottom:t.height}:{left:0,right:e.innerWidth,top:0,bottom:e.innerHeight}}function Yn(e,t){let n=t.width/e.offsetWidth,r=t.height/e.offsetHeight;return(n>.995&&n<1.005||!isFinite(n)||Math.abs(t.width-e.offsetWidth)<1)&&(n=1),(r>.995&&r<1.005||!isFinite(r)||Math.abs(t.height-e.offsetHeight)<1)&&(r=1),{scaleX:n,scaleY:r}}function Xn(e,t,n,r,i,a,o,s){let c=e.ownerDocument,l=c.defaultView||window;for(let u=e,d=!1;u&&!d;)if(B(u)){let e,f=u==c.body,p=1,m=1,h=!0,g=!0;if(f)e=Jn(l);else{let t=getComputedStyle(u);if(/^(fixed|sticky)$/.test(t.position)&&(d=!0),(u.scrollHeight<=u.clientHeight||!/auto|scroll/.test(t.overflowY))&&(g=!1),(u.scrollWidth<=u.clientWidth||!/auto|scroll/.test(t.overflowX))&&(h=!1),!h&&!g){u=u.assignedSlot||u.parentNode;continue}let n=u.getBoundingClientRect();({scaleX:p,scaleY:m}=Yn(u,n)),e={left:n.left,right:n.left+u.clientWidth*p,top:n.top,bottom:n.top+u.clientHeight*m}}let _=0,v=0;if(g){if(i==`nearest`)t.top<e.top+o?(v=t.top-(e.top+o),n>0&&t.bottom>e.bottom+v&&(v=t.bottom-e.bottom+o)):t.bottom>e.bottom-o&&(v=t.bottom-e.bottom+o,n<0&&t.top-v<e.top&&(v=t.top-(e.top+o)));else{let r=t.bottom-t.top,a=e.bottom-e.top;v=(i==`center`&&r<=a?t.top+r/2-a/2:i==`start`||i==`center`&&n<0?t.top-o:t.bottom-a+o)-e.top}}if(h&&(r==`nearest`?t.left<e.left+a?(_=t.left-(e.left+a),n>0&&t.right>e.right+_&&(_=t.right-e.right+a)):t.right>e.right-a&&(_=t.right-e.right+a,n<0&&t.left<e.left+_&&(_=t.left-(e.left+a))):_=(r==`center`?t.left+(t.right-t.left)/2-(e.right-e.left)/2:r==`start`==s?t.left-a:t.right-(e.right-e.left)+a)-e.left),_||v){if(f)l.scrollBy(_,v);else{let e=0,n=0;if(v){let e=u.scrollTop;u.scrollTop+=v/m,n=(u.scrollTop-e)*m}if(_){let t=u.scrollLeft;u.scrollLeft+=_/p,e=(u.scrollLeft-t)*p}t={left:t.left-e,top:t.top-n,right:t.right-e,bottom:t.bottom-n},e&&Math.abs(e-_)<1&&(r=`nearest`),n&&Math.abs(n-v)<1&&(i=`nearest`)}}if(f)break;(t.top<e.top||t.bottom>e.bottom||t.left<e.left||t.right>e.right)&&(t={left:Math.max(t.left,e.left),right:Math.min(t.right,e.right),top:Math.max(t.top,e.top),bottom:Math.min(t.bottom,e.bottom)}),u=u.assignedSlot||u.parentNode}else if(u.nodeType==11)u=u.host;else break}function Zn(e,t=!0){let n=e.ownerDocument,r=null,i=null;for(let a=e.parentNode;a&&!(a==n.body||(!t||r)&&i);)if(B(a))!i&&a.scrollHeight>a.clientHeight&&/scroll|auto/.test(getComputedStyle(a).overflowY)&&(i=a),t&&!r&&a.scrollWidth>a.clientWidth&&/scroll|auto/.test(getComputedStyle(a).overflowX)&&(r=a),a=a.assignedSlot||a.parentNode;else if(a.nodeType==11)a=a.host;else break;return{x:r,y:i}}var Qn=class{constructor(){this.anchorNode=null,this.anchorOffset=0,this.focusNode=null,this.focusOffset=0}eq(e){return this.anchorNode==e.anchorNode&&this.anchorOffset==e.anchorOffset&&this.focusNode==e.focusNode&&this.focusOffset==e.focusOffset}setRange(e){let{anchorNode:t,focusNode:n}=e;this.set(t,Math.min(e.anchorOffset,t?Kn(t):0),n,Math.min(e.focusOffset,n?Kn(n):0))}set(e,t,n,r){this.anchorNode=e,this.anchorOffset=t,this.focusNode=n,this.focusOffset=r}};function $n(e){let t=[];for(let n=e;n;n=n.nodeType==11?n.host:n.parentNode)B(n)&&t.push({node:n,left:n.scrollLeft,top:n.scrollTop});return t}function er(e,t=!0){for(let{node:n,left:r,top:i}of e)t&&n.scrollTop!=i&&(n.scrollTop=i),n.scrollLeft!=r&&(n.scrollLeft=r)}var tr=null;R.safari&&R.safari_version>=26&&(tr=!1);function nr(e){if(e.setActive)return e.setActive();if(tr)return e.focus(tr);let t=$n(e);e.focus(tr==null?{get preventScroll(){return tr={preventScroll:!0},!0}}:void 0),tr||(tr=!1,er(t))}var rr;function ir(e,t,n=t){let r=rr||=document.createRange();return r.setEnd(e,n),r.setStart(e,t),r}function ar(e,t,n,r){let i={key:t,code:t,keyCode:n,which:n,cancelable:!0};r&&({altKey:i.altKey,ctrlKey:i.ctrlKey,shiftKey:i.shiftKey,metaKey:i.metaKey}=r);let a=new KeyboardEvent(`keydown`,i);a.synthetic=!0,e.dispatchEvent(a);let o=new KeyboardEvent(`keyup`,i);return o.synthetic=!0,e.dispatchEvent(o),a.defaultPrevented||o.defaultPrevented}function or(e){for(;e;){if(e&&(e.nodeType==9||e.nodeType==11&&e.host))return e;e=e.assignedSlot||e.parentNode}return null}function sr(e,t){let n=t.focusNode,r=t.focusOffset;if(!n||t.anchorNode!=n||t.anchorOffset!=r)return!1;for(r=Math.min(r,Kn(n));;)if(r){if(!B(n))return!1;let e=n.childNodes[r-1];e.contentEditable==`false`?r--:(n=e,r=Kn(n))}else if(n==e)return!0;else r=Un(n),n=n.parentNode}function cr(e){return e instanceof Window?e.pageYOffset>Math.max(0,e.document.documentElement.scrollHeight-e.innerHeight-4):e.scrollTop>Math.max(1,e.scrollHeight-e.clientHeight-4)}function lr(e,t){for(let n=e,r=t;;)if(Ln(n)&&r>0)return{node:n,offset:r};else if(B(n)&&r>0){if(n.contentEditable==`false`)return null;n=n.childNodes[r-1],r=Kn(n)}else if(n.parentNode&&!Wn(n))r=Un(n),n=n.parentNode;else return null}function ur(e,t){for(let n=e,r=t;;)if(Ln(n)&&r<n.nodeValue.length)return{node:n,offset:r};else if(B(n)&&r<n.childNodes.length){if(n.contentEditable==`false`)return null;n=n.childNodes[r],r=0}else if(n.parentNode&&!Wn(n))r=Un(n)+1,n=n.parentNode;else return null}var dr=class e{constructor(e,t,n=!0){this.node=e,this.offset=t,this.precise=n}static before(t,n){return new e(t.parentNode,Un(t),n)}static after(t,n){return new e(t.parentNode,Un(t)+1,n)}},V=(function(e){return e[e.LTR=0]=`LTR`,e[e.RTL=1]=`RTL`,e})(V||={}),fr=V.LTR,pr=V.RTL;function mr(e){let t=[];for(let n=0;n<e.length;n++)t.push(1<<e[n]);return t}var hr=mr(`88888888888888888888888888888888888666888888787833333333337888888000000000000000000000000008888880000000000000000000000000088888888888888888888888888888888888887866668888088888663380888308888800000000000000000000000800000000000000000000000000000008`),gr=mr(`4444448826627288999999999992222222222222222222222222222222222222222222222229999999999999999999994444444444644222822222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222999999949999999229989999223333333333`),_r=Object.create(null),vr=[];for(let e of[`()`,`[]`,`{}`]){let t=e.charCodeAt(0),n=e.charCodeAt(1);_r[t]=n,_r[n]=-t}function yr(e){return e<=247?hr[e]:1424<=e&&e<=1524?2:1536<=e&&e<=1785?gr[e-1536]:1774<=e&&e<=2220?4:8192<=e&&e<=8204?256:64336<=e&&e<=65023?4:1}var br=/[\u0590-\u05f4\u0600-\u06ff\u0700-\u08ac\ufb50-\ufdff]/,xr=class{get dir(){return this.level%2?pr:fr}constructor(e,t,n){this.from=e,this.to=t,this.level=n}side(e,t){return this.dir==t==e?this.to:this.from}forward(e,t){return e==(this.dir==t)}static find(e,t,n,r){let i=-1;for(let a=0;a<e.length;a++){let o=e[a];if(o.from<=t&&o.to>=t){if(o.level==n)return a;(i<0||(r==0?e[i].level>o.level:r<0?o.from<t:o.to>t))&&(i=a)}}if(i<0)throw RangeError(`Index out of range`);return i}};function Sr(e,t){if(e.length!=t.length)return!1;for(let n=0;n<e.length;n++){let r=e[n],i=t[n];if(r.from!=i.from||r.to!=i.to||r.direction!=i.direction||!Sr(r.inner,i.inner))return!1}return!0}var H=[];function Cr(e,t,n,r,i){for(let a=0;a<=r.length;a++){let o=a?r[a-1].to:t,s=a<r.length?r[a].from:n,c=a?256:i;for(let t=o,n=c,r=c;t<s;t++){let i=yr(e.charCodeAt(t));i==512?i=n:i==8&&r==4&&(i=16),H[t]=i==4?2:i,i&7&&(r=i),n=i}for(let e=o,t=c,r=c;e<s;e++){let i=H[e];if(i==128)e<s-1&&t==H[e+1]&&t&24?i=H[e]=t:H[e]=256;else if(i==64){let i=e+1;for(;i<s&&H[i]==64;)i++;let a=e&&t==8||i<n&&H[i]==8?r==1?1:8:256;for(let t=e;t<i;t++)H[t]=a;e=i-1}else i==8&&r==1&&(H[e]=1);t=i,i&7&&(r=i)}}}function wr(e,t,n,r,i){let a=i==1?2:1;for(let o=0,s=0,c=0;o<=r.length;o++){let l=o?r[o-1].to:t,u=o<r.length?r[o].from:n;for(let t=l,n,r,o;t<u;t++)if(r=_r[n=e.charCodeAt(t)]){if(r<0){for(let e=s-3;e>=0;e-=3)if(vr[e+1]==-r){let n=vr[e+2],r=n&2?i:n&4?n&1?a:i:0;r&&(H[t]=H[vr[e]]=r),s=e;break}}else if(vr.length==189)break;else vr[s++]=t,vr[s++]=n,vr[s++]=c}else if((o=H[t])==2||o==1){let e=o==i;c=+!e;for(let t=s-3;t>=0;t-=3){let n=vr[t+2];if(n&2)break;if(e)vr[t+2]|=2;else{if(n&4)break;vr[t+2]|=4}}}}}function Tr(e,t,n,r){for(let i=0,a=r;i<=n.length;i++){let o=i?n[i-1].to:e,s=i<n.length?n[i].from:t;for(let c=o;c<s;){let o=H[c];if(o==256){let o=c+1;for(;;)if(o==s){if(i==n.length)break;o=n[i++].to,s=i<n.length?n[i].from:t}else if(H[o]==256)o++;else break;let l=a==1,u=l==((o<t?H[o]:r)==1)?l?1:2:r;for(let t=o,r=i,a=r?n[r-1].to:e;t>c;)t==a&&(t=n[--r].from,a=r?n[r-1].to:e),H[--t]=u;c=o}else a=o,c++}}}function Er(e,t,n,r,i,a,o){let s=r%2?2:1;if(r%2==i%2)for(let c=t,l=0;c<n;){let t=!0,u=!1;if(l==a.length||c<a[l].from){let e=H[c];e!=s&&(t=!1,u=e==16)}let d=!t&&s==1?[]:null,f=t?r:r+1,p=c;run:for(;;)if(l<a.length&&p==a[l].from){if(u)break run;let m=a[l];if(!t)for(let e=m.to,t=l+1;;){if(e==n)break run;if(t<a.length&&a[t].from==e)e=a[t++].to;else if(H[e]==s)break run;else break}l++,d?d.push(m):(m.from>c&&o.push(new xr(c,m.from,f)),Dr(e,m.direction==fr==!(f%2)?r:r+1,i,m.inner,m.from,m.to,o),c=m.to),p=m.to}else if(p==n||(t?H[p]!=s:H[p]==s))break;else p++;d?Er(e,c,p,r+1,i,d,o):c<p&&o.push(new xr(c,p,f)),c=p}else for(let c=n,l=a.length;c>t;){let n=!0,u=!1;if(!l||c>a[l-1].to){let e=H[c-1];e!=s&&(n=!1,u=e==16)}let d=!n&&s==1?[]:null,f=n?r:r+1,p=c;run:for(;;)if(l&&p==a[l-1].to){if(u)break run;let m=a[--l];if(!n)for(let e=m.from,n=l;;){if(e==t)break run;if(n&&a[n-1].to==e)e=a[--n].from;else if(H[e-1]==s)break run;else break}d?d.push(m):(m.to<c&&o.push(new xr(m.to,c,f)),Dr(e,m.direction==fr==!(f%2)?r:r+1,i,m.inner,m.from,m.to,o),c=m.from),p=m.from}else if(p==t||(n?H[p-1]!=s:H[p-1]==s))break;else p--;d?Er(e,p,c,r+1,i,d,o):p<c&&o.push(new xr(p,c,f)),c=p}}function Dr(e,t,n,r,i,a,o){let s=t%2?2:1;Cr(e,i,a,r,s),wr(e,i,a,r,s),Tr(i,a,r,s),Er(e,i,a,t,n,r,o)}function Or(e,t,n){if(!e)return[new xr(0,0,+(t==pr))];if(t==fr&&!n.length&&!br.test(e))return kr(e.length);if(n.length)for(;e.length>H.length;)H[H.length]=256;let r=[],i=t==fr?0:1;return Dr(e,i,i,n,0,e.length,r),r}function kr(e){return[new xr(0,e,0)]}var Ar=``;function jr(e,t,n,r,i){if(!e.length)return null;let a=r.head-e.from,o;if(r.head==e.from&&r.assoc<0){if(!i)return null;a=t[o=0].side(!1,n)}else if(r.head==e.to&&r.assoc>0){if(i)return null;a=t[o=t.length-1].side(!0,n)}else o=xr.find(t,a,r.bidiLevel??-1,r.assoc);let s=t[o],c=s.side(i,n);if(a==c){let e=o+=i?1:-1;if(e<0||e>=t.length)return null;s=t[o=e],a=s.side(!i,n),c=s.side(i,n)}let l=k(e.text,a,s.forward(i,n));(l<s.from||l>s.to)&&(l=c),Ar=e.text.slice(Math.min(a,l),Math.max(a,l));let u=o==(i?t.length-1:0)?null:t[o+(i?1:-1)];if(l==c){if(!u)return i?j.cursor(e.to,1):j.cursor(e.from,-1);if(u.level+ +!i<s.level)return j.cursor(u.side(!i,n)+e.from,u.forward(i,n)?1:-1,u.level)}return j.cursor(l+e.from,s.forward(i,n)?-1:1,s.level)}function Mr(e,t,n){for(let r=t;r<n;r++){let t=yr(e.charCodeAt(r));if(t==1)return fr;if(t==2||t==4)return pr}}var Nr=M.define(),Pr=M.define(),Fr=M.define(),Ir=M.define(),Lr=M.define(),Rr=M.define(),zr=M.define(),Br=M.define(),Vr=M.define(),Hr=M.define({combine:e=>e.some(e=>e)}),Ur=M.define({combine:e=>e.some(e=>e)}),Wr=M.define(),Gr=class e{constructor(e,t,n,r,i,a=!1){this.range=e,this.y=t,this.x=n,this.yMargin=r,this.xMargin=i,this.isSnapshot=a}map(t){return t.empty?this:new e(this.range.map(t),this.y,this.x,this.yMargin,this.xMargin,this.isSnapshot)}clip(t){return this.range.to<=t.doc.length?this:new e(j.cursor(t.doc.length),this.y,this.x,this.yMargin,this.xMargin,this.isSnapshot)}},Kr=N.define({map:(e,t)=>e.map(t)}),qr=N.define();function Jr(e,t,n){let r=e.facet(Ir);r.length?r[0](t):window.onerror&&window.onerror(String(t),n,void 0,void 0,t)||(n?console.error(n+`:`,t):console.error(t))}var Yr=M.define({combine:e=>!e.length||e[0]}),Xr=0,Zr=M.define({combine(e){return e.filter((t,n)=>{for(let r=0;r<n;r++)if(e[r].plugin==t.plugin)return!1;return!0})}}),U=class e{constructor(e,t,n,r,i){this.id=e,this.create=t,this.domEventHandlers=n,this.domEventObservers=r,this.baseExtensions=i(this),this.extension=this.baseExtensions.concat(Zr.of({plugin:this,arg:void 0}))}of(e){return this.baseExtensions.concat(Zr.of({plugin:this,arg:e}))}static define(t,n){let{eventHandlers:r,eventObservers:i,provide:a,decorations:o}=n||{};return new e(Xr++,t,r,i,e=>{let t=[];return o&&t.push(ti.of(t=>{let n=t.plugin(e);return n?o(n):z.none})),a&&t.push(a(e)),t})}static fromClass(t,n){return e.define((e,n)=>new t(e,n),n)}},Qr=class{constructor(e){this.spec=e,this.mustUpdate=null,this.value=null}get plugin(){return this.spec&&this.spec.plugin}update(e){if(!this.value){if(this.spec)try{this.value=this.spec.plugin.create(e,this.spec.arg)}catch(t){Jr(e.state,t,`CodeMirror plugin crashed`),this.deactivate()}}else if(this.mustUpdate){let e=this.mustUpdate;if(this.mustUpdate=null,this.value.update)try{this.value.update(e)}catch(t){if(Jr(e.state,t,`CodeMirror plugin crashed`),this.value.destroy)try{this.value.destroy()}catch{}this.deactivate()}}return this}destroy(e){if(this.value?.destroy)try{this.value.destroy()}catch(t){Jr(e.state,t,`CodeMirror plugin crashed`)}}deactivate(){this.spec=this.value=null}},$r=M.define(),ei=M.define(),ti=M.define(),ni=M.define(),ri=M.define(),ii=M.define(),ai=M.define();function oi(e,t){let n=e.state.facet(ai);if(!n.length)return n;let r=n.map(t=>t instanceof Function?t(e):t),i=[];return I.spans(r,t.from,t.to,{point(){},span(e,n,r,a){let o=e-t.from,s=n-t.from,c=i;for(let e=r.length-1;e>=0;e--,a--){let t=r[e].spec.bidiIsolate,n;if(a>0&&c.length&&(n=c[c.length-1]).to==o&&n.direction==t)n.to=s,c=n.inner;else{let e={from:o,to:s,direction:t,inner:[]};c.push(e),c=e.inner}}}}),si(t.text,i)}function si(e,t){for(let n of t){if(n.direction==null){for(let t=n.from,r=0;t<n.to;r++){let i=r<n.inner.length?n.inner[r]:null,a=Mr(e,t,i?i.from:n.to);if(a){n.direction=a;break}if(!i)break;t=i.to}n.direction??=V.LTR}si(e,n.inner)}return t}var ci=M.define();function li(e){let t=0,n=0,r=0,i=0;for(let a of e.state.facet(ci)){let o=a(e);o&&(o.left!=null&&(t=Math.max(t,o.left)),o.right!=null&&(n=Math.max(n,o.right)),o.top!=null&&(r=Math.max(r,o.top)),o.bottom!=null&&(i=Math.max(i,o.bottom)))}return{left:t,right:n,top:r,bottom:i}}var ui=M.define(),di=class e{constructor(e,t,n,r){this.fromA=e,this.toA=t,this.fromB=n,this.toB=r}join(t){return new e(Math.min(this.fromA,t.fromA),Math.max(this.toA,t.toA),Math.min(this.fromB,t.fromB),Math.max(this.toB,t.toB))}addToSet(e){let t=e.length,n=this;for(;t>0;t--){let r=e[t-1];if(!(r.fromA>n.toA)){if(r.toA<n.fromA)break;n=n.join(r),e.splice(t-1,1)}}return e.splice(t,0,n),e}static extendWithRanges(t,n){if(n.length==0)return t;let r=[];for(let i=0,a=0,o=0;;){let s=i<t.length?t[i].fromB:1e9,c=a<n.length?n[a]:1e9,l=Math.min(s,c);if(l==1e9)break;let u=l+o,d=l,f=u;for(;;)if(a<n.length&&n[a]<=d){let e=n[a+1];a+=2,d=Math.max(d,e);for(let e=i;e<t.length&&t[e].fromB<=d;e++)o=t[e].toA-t[e].toB;f=Math.max(f,e+o)}else if(i<t.length&&t[i].fromB<=d){let e=t[i++];d=Math.max(d,e.toB),f=Math.max(f,e.toA),o=e.toA-e.toB}else break;r.push(new e(u,f,l,d))}return r}},fi=class e{constructor(e,t,n){this.view=e,this.state=t,this.transactions=n,this.flags=0,this.startState=e.state,this.changes=Ne.empty(this.startState.doc.length);for(let e of n)this.changes=this.changes.compose(e.changes);let r=[];this.changes.iterChangedRanges((e,t,n,i)=>r.push(new di(e,t,n,i))),this.changedRanges=r}static create(t,n,r){return new e(t,n,r)}get viewportChanged(){return(this.flags&4)>0}get viewportMoved(){return(this.flags&8)>0}get heightChanged(){return(this.flags&2)>0}get geometryChanged(){return this.docChanged||(this.flags&18)>0}get focusChanged(){return(this.flags&1)>0}get docChanged(){return!this.changes.empty}get selectionSet(){return this.transactions.some(e=>e.selection)}get empty(){return this.flags==0&&this.transactions.length==0}},pi=[],W=class{constructor(e,t,n=0){this.dom=e,this.length=t,this.flags=n,this.parent=null,e.cmTile=this}get breakAfter(){return this.flags&1}get children(){return pi}isWidget(){return!1}get isHidden(){return!1}isComposite(){return!1}isLine(){return!1}isText(){return!1}isBlock(){return!1}get domAttrs(){return null}sync(e){if(this.flags|=2,this.flags&4){this.flags&=-5;let e=this.domAttrs;e&&Tn(this.dom,e)}}toString(){return this.constructor.name+(this.children.length?`(${this.children})`:``)+(this.breakAfter?`#`:``)}destroy(){this.parent=null}setDOM(e){this.dom=e,e.cmTile=this}get posAtStart(){return this.parent?this.parent.posBefore(this):0}get posAtEnd(){return this.posAtStart+this.length}posBefore(e,t=this.posAtStart){let n=t;for(let t of this.children){if(t==e)return n;n+=t.length+t.breakAfter}throw RangeError(`Invalid child in posBefore`)}posAfter(e){return this.posBefore(e)+e.length}covers(e){return!0}coordsIn(e,t,n){return null}domPosFor(e,t){let n=Un(this.dom),r=this.length?e>0:t>0;return new dr(this.parent.dom,n+ +!!r,e==0||e==this.length)}markDirty(e){this.flags&=-3,e&&(this.flags|=4),this.parent&&this.parent.flags&2&&this.parent.markDirty(!1)}get overrideDOMText(){return null}get root(){for(let e=this;e;e=e.parent)if(e instanceof gi)return e;return null}static get(e){return e.cmTile}},mi=class extends W{constructor(e){super(e,0),this._children=[]}isComposite(){return!0}get children(){return this._children}get lastChild(){return this.children.length?this.children[this.children.length-1]:null}append(e){this.children.push(e),e.parent=this}sync(e){if(this.flags&2)return;super.sync(e);let t=this.dom,n=null,r,i=e?.node==t?e:null,a=0;for(let o of this.children){if(o.sync(e),a+=o.length+o.breakAfter,r=n?n.nextSibling:t.firstChild,i&&r!=o.dom&&(i.written=!0),o.dom.parentNode==t)for(;r&&r!=o.dom;)r=hi(r);else t.insertBefore(o.dom,r);n=o.dom}for(r=n?n.nextSibling:t.firstChild,i&&r&&(i.written=!0);r;)r=hi(r);this.length=a}};function hi(e){let t=e.nextSibling;return e.parentNode.removeChild(e),t}var gi=class extends mi{constructor(e,t){super(t),this.view=e}owns(e){for(;e;e=e.parent)if(e==this)return!0;return!1}isBlock(){return!0}nearest(e){for(;;){if(!e)return null;let t=W.get(e);if(t&&this.owns(t))return t;e=e.parentNode}}blockTiles(e){for(let t=[],n=this,r=0,i=0;;)if(r==n.children.length){if(!t.length)return;n=n.parent,n.breakAfter&&i++,r=t.pop()}else{let a=n.children[r++];if(a instanceof _i)t.push(r),n=a,r=0;else{let t=i+a.length,n=e(a,i);if(n!==void 0)return n;i=t+a.breakAfter}}}resolveBlock(e,t){let n,r=-1,i,a=-1;if(this.blockTiles((o,s)=>{let c=s+o.length;if(e>=s&&e<=c){if(o.isWidget()&&t>=-1&&t<=1){if(o.flags&32)return!0;o.flags&16&&(n=void 0)}(s<e||e==c&&(t<-1?o.length:o.covers(1)))&&(!n||!o.isWidget()&&n.isWidget())&&(n=o,r=e-s),(c>e||e==s&&(t>1?o.length:o.covers(-1)))&&(!i||!o.isWidget()&&i.isWidget())&&(i=o,a=e-s)}}),!n&&!i)throw Error(`No tile at position `+e);return n&&t<0||!i?{tile:n,offset:r}:{tile:i,offset:a}}},_i=class e extends mi{constructor(e,t){super(e),this.wrapper=t}isBlock(){return!0}covers(e){return this.children.length?e<0?this.children[0].covers(-1):this.lastChild.covers(1):!1}get domAttrs(){return this.wrapper.attributes}static of(t,n){let r=new e(n||document.createElement(t.tagName),t);return n||(r.flags|=4),r}},vi=class e extends mi{constructor(e,t){super(e),this.attrs=t}isLine(){return!0}static start(t,n,r){let i=new e(n||document.createElement(`div`),t);return n&&r||(i.flags|=4),i}get domAttrs(){return this.attrs}resolveInline(e,t,n=!1){let r=null,i=-1,a=null,o=-1;function s(e,c){for(let l=0,u=0;l<e.children.length&&u<=c;l++){let d=e.children[l],f=u+d.length;f>=c&&(d.isComposite()?s(d,c-u):(!a||n&&a.isHidden&&(t>0&&!(a.flags&32)||n&&bi(a,d)))&&(f>c||d.flags&32&&t<=1)?(a=d,o=c-u):(u<c||d.flags&16&&!(n&&d.isHidden)&&t>=-1)&&(r=d,i=c-u)),u=f}}s(this,e);let c=(t<0?r:a)||r||a;return c?{tile:c,offset:c==r?i:o}:null}coordsIn(e,t,n){let r=this.resolveInline(e,t,!0);return r?r.tile.coordsIn(Math.max(0,r.offset),t,n):yi(this)}domIn(e,t){let n=this.resolveInline(e,t);if(n){let{tile:e,offset:r}=n;if(this.dom.contains(e.dom))return e.isText()?new dr(e.dom,Math.min(e.dom.nodeValue.length,r)):e.domPosFor(r,e.flags&16?1:e.flags&32?-1:t);let i=n.tile.parent,a=!1;for(let e of i.children){if(a)return new dr(e.dom,0);e==n.tile&&(a=!0)}}return new dr(this.dom,0)}};function yi(e){let t=e.dom.lastChild;if(!t)return e.dom.getBoundingClientRect();let n=Vn(t);return n[n.length-1]||null}function bi(e,t){let n=e.coordsIn(0,1),r=t.coordsIn(0,1);return n&&r&&r.top<n.bottom}var xi=class e extends mi{constructor(e,t){super(e),this.mark=t}get domAttrs(){return this.mark.attrs}static of(t,n){let r=new e(n||document.createElement(t.tagName),t);return n||(r.flags|=4),r}},Si=class e extends W{constructor(e,t){super(e,t.length),this.text=t}sync(e){this.flags&2||(super.sync(e),this.dom.nodeValue!=this.text&&(e&&e.node==this.dom&&(e.written=!0),this.dom.nodeValue=this.text))}isText(){return!0}toString(){return JSON.stringify(this.text)}coordsIn(e,t,n){let r=this.dom.nodeValue.length;e>r&&(e=r);let i=e,a=e,o=0;e==0&&t<0||e==r&&t>=0?R.chrome||R.gecko||(e?(i--,o=1):a<r&&(a++,o=-1)):t<0?i--:a<r&&a++;let s=ir(this.dom,i,a).getClientRects();if(!s.length)return null;let c=s[(o?o<0:t>=0)?0:s.length-1];return R.safari&&!o&&c.width==0&&(c=Array.prototype.find.call(s,e=>e.width)||c),n==null?c:qn(c,(o?o>0:t<0)==n)}static of(t,n){let r=new e(n||document.createTextNode(t),t);return n||(r.flags|=2),r}},Ci=class e extends W{constructor(e,t,n,r){super(e,t,r),this.widget=n}isWidget(){return!0}get isHidden(){return this.widget.isHidden}covers(e){return this.flags&48?!1:(this.flags&(e<0?64:128))>0}coordsIn(e,t){return this.coordsInWidget(e,t,!1)}coordsInWidget(e,t,n){let r=this.widget.coordsAt(this.dom,e,t);if(r)return r;if(n)return qn(this.dom.getBoundingClientRect(),this.length?e==0:t<=0);{let t=this.dom.getClientRects(),n=null;if(!t.length)return null;let r=this.flags&16?!0:this.flags&32?!1:e>0;for(let i=r?t.length-1:0;n=t[i],!(e>0?i==0:i==t.length-1||n.top<n.bottom);i+=r?-1:1);return qn(n,!r)}}get overrideDOMText(){if(!this.length)return D.empty;let{root:e}=this;if(!e)return D.empty;let t=this.posAtStart;return e.view.state.doc.slice(t,t+this.length)}destroy(){super.destroy(),this.widget.destroy(this.dom)}static of(t,n,r,i,a){return a||(a=t.toDOM(n),t.editable||(a.contentEditable=`false`)),new e(a,r,t,i)}},wi=class extends W{constructor(e){let t=document.createElement(`img`);t.className=`cm-widgetBuffer`,t.setAttribute(`aria-hidden`,`true`),super(t,0,e)}get isHidden(){return!0}get overrideDOMText(){return D.empty}coordsIn(e,t,n){let r=this.dom.getBoundingClientRect();return n==null?r:qn(r,t>0==n)}},Ti=class{constructor(e){this.index=0,this.beforeBreak=!1,this.parents=[],this.tile=e}advance(e,t,n){let{tile:r,index:i,beforeBreak:a,parents:o}=this;for(;e||t>0;)if(!r.isComposite()){let t=r.length;if(i<t&&e){let a=Math.min(e,t-i);n&&n.skip(r,i,i+a),e-=a,i+=a}if(i==t)a=!!r.breakAfter,{tile:r,index:i}=o.pop(),i++;else if(!e)break}else if(a){if(!e)break;n&&n.break(),e--,a=!1}else if(i==r.children.length){if(!e&&!o.length)break;n&&n.leave(r),a=!!r.breakAfter,{tile:r,index:i}=o.pop(),i++}else{let s=r.children[i],c=s.breakAfter;(t>0?s.length<=e:s.length<e)&&(!n||n.skip(s,0,s.length)!==!1||!s.isComposite)?(a=!!c,i++,e-=s.length):(o.push({tile:r,index:i}),r=s,i=0,n&&s.isComposite()&&n.enter(s))}return this.tile=r,this.index=i,this.beforeBreak=a,this}get root(){return this.parents.length?this.parents[0].tile:this.tile}},Ei=class{constructor(e,t,n,r){this.from=e,this.to=t,this.wrapper=n,this.rank=r}},Di=class{constructor(e,t,n){this.cache=e,this.root=t,this.blockWrappers=n,this.curLine=null,this.lastBlock=null,this.afterWidget=null,this.pos=0,this.wrappers=[],this.wrapperPos=0}addText(e,t,n,r){this.flushBuffer();let i=this.ensureMarks(t,n),a=i.lastChild;if(a&&a.isText()&&!(a.flags&8)&&a.length+e.length<512){this.cache.reused.set(a,2);let t=i.children[i.children.length-1]=new Si(a.dom,a.text+e);t.parent=i}else i.append(r||Si.of(e,this.cache.find(Si)?.dom));this.pos+=e.length,this.afterWidget=null}addComposition(e,t){let n=this.curLine;n.dom!=t.line.dom&&(n.setDOM(this.cache.reused.has(t.line)?Li(t.line.dom):t.line.dom),this.cache.reused.set(t.line,2));let r=n;for(let e=t.marks.length-1;e>=0;e--){let n=t.marks[e],i=r.lastChild;if(i instanceof xi&&i.mark.eq(n.mark))i.dom!=n.dom&&i.setDOM(Li(n.dom)),r=i;else{let{dom:e}=n;this.cache.reused.get(n)&&W.get(n.dom)&&(e=Li(n.dom));let t=xi.of(n.mark,e);r.append(t),r=t}this.cache.reused.set(n,2)}let i=W.get(e.text);i&&this.cache.reused.set(i,2);let a=new Si(e.text,e.text.nodeValue);a.flags|=8,this.pos=e.range.toB,r.append(a)}addInlineWidget(e,t,n){let r=this.afterWidget&&e.flags&48&&(this.afterWidget.flags&48)==(e.flags&48);r||this.flushBuffer();let i=this.ensureMarks(t,n);!r&&!(e.flags&16)&&i.append(this.getBuffer(1)),i.append(e),this.pos+=e.length,this.afterWidget=e}addMark(e,t,n){this.flushBuffer(),this.ensureMarks(t,n).append(e),this.pos+=e.length,this.afterWidget=null}addBlockWidget(e){this.getBlockPos().append(e),this.pos+=e.length,this.lastBlock=e,this.endLine()}continueWidget(e){let t=this.afterWidget||this.lastBlock;t.length+=e,this.pos+=e}addLineStart(e,t){e||=Pi;let n=vi.start(e,t||this.cache.find(vi)?.dom,!!t);this.getBlockPos().append(this.lastBlock=this.curLine=n)}addLine(e){this.getBlockPos().append(e),this.pos+=e.length,this.lastBlock=e,this.endLine()}addBreak(){this.lastBlock.flags|=1,this.endLine(),this.pos++}addLineStartIfNotCovered(e){this.blockPosCovered()||this.addLineStart(e)}ensureLine(e){this.curLine||this.addLineStart(e)}ensureMarks(e,t){let n=this.curLine;for(let r=e.length-1;r>=0;r--){let i=e[r],a;if(t>0&&(a=n.lastChild)&&a instanceof xi&&a.mark.eq(i))n=a,t--;else{let e=xi.of(i,this.cache.find(xi,e=>e.mark.eq(i))?.dom);n.append(e),n=e,t=0}}return n}endLine(){if(this.curLine){this.flushBuffer();let e=this.curLine.lastChild;(!e||!Mi(this.curLine,!1)||e.dom.nodeName!=`BR`&&e.isWidget()&&!(R.ios&&Mi(this.curLine,!0)))&&this.curLine.append(this.cache.findWidget(zi,0,32)||new Ci(zi.toDOM(),0,zi,32)),this.curLine=this.afterWidget=null}}updateBlockWrappers(){this.wrapperPos>this.pos+1e4&&(this.blockWrappers.goto(this.pos),this.wrappers.length=0);for(let e=this.wrappers.length-1;e>=0;e--)this.wrappers[e].to<this.pos&&this.wrappers.splice(e,1);for(let e=this.blockWrappers;e.value&&e.from<=this.pos;e.next())if(e.to>=this.pos){let t=e.rank*102+e.value.rank,n=new Ei(e.from,e.to,e.value,t),r=this.wrappers.length;for(;r>0&&(this.wrappers[r-1].rank-n.rank||this.wrappers[r-1].to-n.to)<0;)r--;this.wrappers.splice(r,0,n)}this.wrapperPos=this.pos}getBlockPos(){this.updateBlockWrappers();let e=this.root;for(let t of this.wrappers){let n=e.lastChild;if(t.from<this.pos&&n instanceof _i&&n.wrapper.eq(t.wrapper))e=n;else{let n=_i.of(t.wrapper,this.cache.find(_i,e=>e.wrapper.eq(t.wrapper))?.dom);e.append(n),e=n}}return e}blockPosCovered(){let e=this.lastBlock;return e!=null&&!e.breakAfter&&(!e.isWidget()||(e.flags&160)>0)}getBuffer(e){let t=2|(e<0?16:32),n=this.cache.find(wi,void 0,1);return n&&(n.flags=t),n||new wi(t)}flushBuffer(){this.afterWidget&&!(this.afterWidget.flags&32)&&(this.afterWidget.parent.append(this.getBuffer(-1)),this.afterWidget=null)}},Oi=class{constructor(e){this.skipCount=0,this.text=``,this.textOff=0,this.cursor=e.iter()}skip(e){this.textOff+e<=this.text.length?this.textOff+=e:(this.skipCount+=e-(this.text.length-this.textOff),this.text=``,this.textOff=0)}next(e){if(this.textOff==this.text.length){let{value:t,lineBreak:n,done:r}=this.cursor.next(this.skipCount);if(this.skipCount=0,r)throw Error(`Ran out of text content when drawing inline views`);this.text=t;let i=this.textOff=Math.min(e,t.length);return n?null:t.slice(0,i)}let t=Math.min(this.text.length,this.textOff+e),n=this.text.slice(this.textOff,t);return this.textOff=t,n}},ki=[Ci,vi,Si,xi,wi,_i,gi];for(let e=0;e<ki.length;e++)ki[e].bucket=e;var Ai=class{constructor(e){this.view=e,this.buckets=ki.map(()=>[]),this.index=ki.map(()=>0),this.reused=new Map}add(e){let t=e.constructor.bucket,n=this.buckets[t];n.length<6?n.push(e):n[this.index[t]=(this.index[t]+1)%6]=e}find(e,t,n=2){let r=e.bucket,i=this.buckets[r],a=this.index[r];for(let e=0;e<i.length;e++){let o=(e+a)%i.length,s=i[o];if((!t||t(s))&&!this.reused.has(s))return i.splice(o,1),o<a&&this.index[r]--,this.reused.set(s,n),s}return null}findWidget(e,t,n){let r=this.buckets[0];if(r.length)for(let i=0,a=0;;i++){if(i==r.length){if(a)return null;a=1,i=0}let o=r[i];if(!this.reused.has(o)&&(a==0?o.widget.compare(e):o.widget.constructor==e.constructor&&e.updateDOM(o.dom,this.view,o.widget)))return r.splice(i,1),i<this.index[0]&&this.index[0]--,o.widget==e&&o.length==t&&(o.flags&497)==n?(this.reused.set(o,1),o):(this.reused.set(o,2),new Ci(o.dom,t,e,o.flags&-498|n))}}reuse(e){return this.reused.set(e,1),e}maybeReuse(e,t=2){if(!this.reused.has(e))return this.reused.set(e,t),e.dom}clear(){for(let e=0;e<this.buckets.length;e++)this.buckets[e].length=this.index[e]=0}},ji=class{constructor(e,t,n,r,i){this.view=e,this.decorations=r,this.disallowBlockEffectsFor=i,this.openWidget=!1,this.openMarks=0,this.cache=new Ai(e),this.text=new Oi(e.state.doc),this.builder=new Di(this.cache,new gi(e,e.contentDOM),I.iter(n)),this.cache.reused.set(t,2),this.old=new Ti(t),this.reuseWalker={skip:(e,t,n)=>{if(this.cache.add(e),e.isComposite())return!1},enter:e=>this.cache.add(e),leave:()=>{},break:()=>{}}}run(e,t){let n=t&&this.getCompositionContext(t.text);for(let r=0,i=0,a=0;;){let o=a<e.length?e[a++]:null,s=o?o.fromA:this.old.root.length;if(s>r){let e=s-r;this.preserve(e,!a,!o),r=s,i+=e}if(!o)break;t&&o.fromA<=t.range.fromA&&o.toA>=t.range.toA?(this.forward(o.fromA,t.range.fromA,t.range.fromA<t.range.toA?1:-1),this.emit(i,t.range.fromB),this.builder.flushBuffer(),this.cache.clear(),this.builder.addComposition(t,n),this.text.skip(t.range.toB-t.range.fromB),this.forward(t.range.fromA,o.toA),this.emit(t.range.toB,o.toB)):(this.forward(o.fromA,o.toA),this.emit(i,o.toB)),i=o.toB,r=o.toA}return this.builder.curLine&&this.builder.endLine(),this.builder.root}preserve(e,t,n){let r=Ii(this.old),i=this.openMarks;this.old.advance(e,n?1:-1,{skip:(e,t,n)=>{if(e.isWidget()){if(this.openWidget)this.builder.continueWidget(n-t);else{let a=n>0||t<e.length?Ci.of(e.widget,this.view,n-t,e.flags&496,this.cache.maybeReuse(e)):this.cache.reuse(e);a.flags&256?(a.flags&=-2,this.builder.addBlockWidget(a)):(this.builder.ensureLine(null),this.builder.addInlineWidget(a,r,i),i=r.length)}}else if(e.isText())this.builder.ensureLine(null),!t&&n==e.length&&!this.cache.reused.has(e)?this.builder.addText(e.text,r,i,this.cache.reuse(e)):(this.cache.add(e),this.builder.addText(e.text.slice(t,n),r,i)),i=r.length;else if(e.isLine())e.flags&=-2,this.cache.reused.set(e,1),this.builder.addLine(e);else if(e instanceof wi)this.cache.add(e);else if(e instanceof xi)this.builder.ensureLine(null),this.builder.addMark(e,r,i),this.cache.reused.set(e,1),i=r.length;else return!1;this.openWidget=!1},enter:e=>{e.isLine()?this.builder.addLineStart(e.attrs,this.cache.maybeReuse(e)):(this.cache.add(e),e instanceof xi&&r.unshift(e.mark)),this.openWidget=!1},leave:e=>{e.isLine()?r.length&&=i=0:e instanceof xi&&(r.shift(),i=Math.min(i,r.length))},break:()=>{this.builder.addBreak(),this.openWidget=!1}}),this.text.skip(e)}emit(e,t){let n=null,r=this.builder,i=-1,a=I.spans(this.decorations,e,t,{point:(e,t,a,o,s,c)=>{if(a instanceof Mn){if(this.disallowBlockEffectsFor[c]){if(a.block)throw RangeError(`Block decorations may not be specified via plugins`);if(t>this.view.state.doc.lineAt(e).to)throw RangeError(`Decorations that replace line breaks may not be specified via plugins`)}if(i=o.length,s>o.length)r.continueWidget(t-e);else{let i=a.widget||(a.block?Ri.block:Ri.inline),c=Ni(a),l=this.cache.findWidget(i,t-e,c)||Ci.of(i,this.view,t-e,c);a.block?(a.startSide>0&&r.addLineStartIfNotCovered(n),r.addBlockWidget(l)):(r.ensureLine(n),r.addInlineWidget(l,o,s))}n=null}else n=Fi(n,a);t>e&&this.text.skip(t-e)},span:(e,t,a,o)=>{for(let i=e;i<t;){let s=this.text.next(Math.min(512,t-i));s==null?(r.addLineStartIfNotCovered(n),r.addBreak(),i++):(r.ensureLine(n),r.addText(s,a,i==e?o:a.length),i+=s.length),n=null}i=a.length}});i>-1&&(this.openWidget=a>i),this.openWidget||r.addLineStartIfNotCovered(n),this.openMarks=a}forward(e,t,n=1){t-e<=10?this.old.advance(t-e,n,this.reuseWalker):(this.old.advance(5,-1,this.reuseWalker),this.old.advance(t-e-10,-1),this.old.advance(5,n,this.reuseWalker))}getCompositionContext(e){let t=[],n=null;for(let r=e.parentNode;;r=r.parentNode){let e=W.get(r);if(r==this.view.contentDOM)break;e instanceof xi?t.push(e):e?.isLine()?n=e:e instanceof _i||(r.nodeName==`DIV`&&!n?n=new vi(r,Pi):n||t.push(xi.of(new An({tagName:r.nodeName.toLowerCase(),attributes:Dn(r)}),r)))}return n?{line:n,marks:t}:null}};function Mi(e,t){let n=e=>{for(let r of e.children)if((t?r.isText():r.length)||n(r))return!0;return!1};return n(e)}function Ni(e){let t=e.isReplace?(e.startSide<0?64:0)|(e.endSide>0?128:0):e.startSide>0?32:16;return e.block&&(t|=256),t}var Pi={class:`cm-line`};function Fi(e,t){let n=t.spec.attributes,r=t.spec.class;return!n&&!r||(e||={class:`cm-line`},n&&Sn(n,e),r&&(e.class+=` `+r)),e}function Ii(e){let t=[];for(let n=e.parents.length;n>1;n--){let r=n==e.parents.length?e.tile:e.parents[n].tile;r instanceof xi&&t.push(r.mark)}return t}function Li(e){let t=W.get(e);return t&&t.setDOM(e.cloneNode()),e}var Ri=class extends On{constructor(e){super(),this.tag=e}eq(e){return e.tag==this.tag}toDOM(){return document.createElement(this.tag)}updateDOM(e){return e.nodeName.toLowerCase()==this.tag}get isHidden(){return!0}};Ri.inline=new Ri(`span`),Ri.block=new Ri(`div`);var zi=new class extends On{toDOM(){return document.createElement(`br`)}get isHidden(){return!0}get editable(){return!0}},Bi=class{constructor(e){this.view=e,this.decorations=[],this.blockWrappers=[],this.dynamicDecorationMap=[!1],this.domChanged=null,this.hasComposition=null,this.editContextFormatting=z.none,this.lastCompositionAfterCursor=!1,this.minWidth=0,this.minWidthFrom=0,this.minWidthTo=0,this.impreciseAnchor=null,this.impreciseHead=null,this.forceSelection=!1,this.lastUpdate=Date.now(),this.updateDeco(),this.tile=new gi(e,e.contentDOM),this.updateInner([new di(0,0,0,e.state.doc.length)],null)}update(e){let t=e.changedRanges;this.minWidth>0&&t.length&&(t.every(({fromA:e,toA:t})=>t<this.minWidthFrom||e>this.minWidthTo)?(this.minWidthFrom=e.changes.mapPos(this.minWidthFrom,1),this.minWidthTo=e.changes.mapPos(this.minWidthTo,1)):this.minWidth=this.minWidthFrom=this.minWidthTo=0),this.updateEditContextFormatting(e);let n=-1;this.view.inputState.composing>=0&&!this.view.observer.editContext&&(this.domChanged?.newSel?n=this.domChanged.newSel.head:!Zi(e.changes,this.hasComposition)&&!e.selectionSet&&(n=e.state.selection.main.head));let r=n>-1?Wi(this.view,e.changes,n):null;if(this.domChanged=null,this.hasComposition){let{from:n,to:r}=this.hasComposition;t=new di(n,r,e.changes.mapPos(n,-1),e.changes.mapPos(r,1)).addToSet(t.slice())}this.hasComposition=r?{from:r.range.fromB,to:r.range.toB}:null,(R.ie||R.chrome)&&!r&&e&&e.state.doc.lines!=e.startState.doc.lines&&(this.forceSelection=!0);let i=this.decorations,a=this.blockWrappers;this.updateDeco();let o=qi(i,this.decorations,e.changes);o.length&&(t=di.extendWithRanges(t,o));let s=Yi(a,this.blockWrappers,e.changes);return s.length&&(t=di.extendWithRanges(t,s)),r&&!t.some(e=>e.fromA<=r.range.fromA&&e.toA>=r.range.toA)&&(t=r.range.addToSet(t.slice())),this.tile.flags&2&&t.length==0?!1:(this.updateInner(t,r),e.transactions.length&&(this.lastUpdate=Date.now()),!0)}updateInner(e,t){this.view.viewState.mustMeasureContent=!0;let{observer:n}=this.view;n.ignore(()=>{if(t||e.length){let n=this.tile,r=new ji(this.view,n,this.blockWrappers,this.decorations,this.dynamicDecorationMap);t&&W.get(t.text)&&r.cache.reused.set(W.get(t.text),2),this.tile=r.run(e,t),Vi(n,r.cache.reused)}this.tile.dom.style.height=this.view.viewState.contentHeight/this.view.scaleY+`px`,this.tile.dom.style.flexBasis=this.minWidth?this.minWidth+`px`:``;let r=R.chrome||R.ios?{node:n.selectionRange.focusNode,written:!1}:void 0;this.tile.sync(r),r&&(r.written||n.selectionRange.focusNode!=r.node||!this.tile.dom.contains(r.node))&&(this.forceSelection=!0),this.tile.dom.style.height=``});let r=[];if(this.view.viewport.from||this.view.viewport.to<this.view.state.doc.length)for(let e of this.tile.children)e.isWidget()&&e.widget instanceof Qi&&r.push(e.dom);n.updateGaps(r)}updateEditContextFormatting(e){this.editContextFormatting=this.editContextFormatting.map(e.changes);for(let t of e.transactions)for(let e of t.effects)e.is(qr)&&(this.editContextFormatting=e.value)}updateSelection(e=!1,t=!1){(e||!this.view.observer.selectionRange.focusNode)&&this.view.observer.readSelectionRange();let{dom:n}=this.tile,r=this.view.root.activeElement,i=r==n,a=!i&&!(this.view.state.facet(Yr)||n.tabIndex>-1)&&Bn(n,this.view.observer.selectionRange)&&!(r&&n.contains(r));if(!(i||t||a))return;let o=this.forceSelection;this.forceSelection=!1;let s=this.view.state.selection.main,c,l;if(s.empty?l=c=this.inlineDOMNearPos(s.anchor,s.assoc||1):(l=this.inlineDOMNearPos(s.head,s.head==s.from?1:-1),c=this.inlineDOMNearPos(s.anchor,s.anchor==s.from?1:-1)),R.gecko&&s.empty&&!this.hasComposition&&Hi(c)){let e=document.createTextNode(``);this.view.observer.ignore(()=>c.node.insertBefore(e,c.node.childNodes[c.offset]||null)),c=l=new dr(e,0),o=!0}let u=this.view.observer.selectionRange;!o&&u.focusNode&&(Hn(c.node,c.offset,u.anchorNode,u.anchorOffset)&&Hn(l.node,l.offset,u.focusNode,u.focusOffset)||this.suppressWidgetCursorChange(u,s))||(this.view.observer.ignore(()=>{R.android&&R.chrome&&n.contains(u.focusNode)&&Xi(u.focusNode,n)&&(n.blur(),n.focus({preventScroll:!0}));let e=Rn(this.view.root);if(e){if(s.empty){if(R.gecko){let e=Gi(c.node,c.offset);if(e&&e!=3){let t=(e==1?lr:ur)(c.node,c.offset);t&&(c=new dr(t.node,t.offset))}}e.collapse(c.node,c.offset),s.bidiLevel!=null&&e.caretBidiLevel!==void 0&&(e.caretBidiLevel=s.bidiLevel)}else if(e.extend){e.collapse(c.node,c.offset);try{e.extend(l.node,l.offset)}catch{}}else{let t=document.createRange();s.anchor>s.head&&([c,l]=[l,c]),t.setEnd(l.node,l.offset),t.setStart(c.node,c.offset),e.removeAllRanges(),e.addRange(t)}}a&&this.view.root.activeElement==n&&(n.blur(),r&&r.focus())}),this.view.observer.setSelectionRange(c,l)),this.impreciseAnchor=c.precise?null:new dr(u.anchorNode,u.anchorOffset),this.impreciseHead=l.precise?null:new dr(u.focusNode,u.focusOffset)}suppressWidgetCursorChange(e,t){return this.hasComposition&&t.empty&&Hn(e.focusNode,e.focusOffset,e.anchorNode,e.anchorOffset)&&this.posFromDOM(e.focusNode,e.focusOffset)==t.head}enforceCursorAssoc(){if(this.hasComposition)return;let{view:e}=this,t=e.state.selection.main,n=Rn(e.root),{anchorNode:r,anchorOffset:i}=e.observer.selectionRange;if(!n||!t.empty||!t.assoc||!n.modify)return;let a=this.lineAt(t.head,t.assoc);if(!a)return;let o=a.posAtStart;if(t.head==o||t.head==o+a.length)return;let s=this.coordsAt(t.head,-1),c=this.coordsAt(t.head,1);if(!s||!c||s.bottom>c.top)return;let l=this.domAtPos(t.head+t.assoc,t.assoc);n.collapse(l.node,l.offset),n.modify(`move`,t.assoc<0?`forward`:`backward`,`lineboundary`),e.observer.readSelectionRange();let u=e.observer.selectionRange;e.docView.posFromDOM(u.anchorNode,u.anchorOffset)!=t.from&&n.collapse(r,i)}posFromDOM(e,t){let n=this.tile.nearest(e);if(!n)return this.tile.dom.compareDocumentPosition(e)&2?0:this.view.state.doc.length;let r=n.posAtStart;if(n.isComposite()){let i;if(e==n.dom)i=n.dom.childNodes[t];else{let r=Kn(e)==0?0:t==0?-1:1;for(;;){let t=e.parentNode;if(t==n.dom)break;r==0&&t.firstChild!=t.lastChild&&(r=e==t.firstChild?-1:1),e=t}i=r<0?e:e.nextSibling}if(i==n.dom.firstChild)return r;for(;i&&!W.get(i);)i=i.nextSibling;if(!i)return r+n.length;for(let e=0,t=r;;e++){let r=n.children[e];if(r.dom==i)return t;t+=r.length+r.breakAfter}}else if(n.isText())return e==n.dom?r+t:r+(t?n.length:0);else return r}domAtPos(e,t){let{tile:n,offset:r}=this.tile.resolveBlock(e,t);return n.isWidget()?n.domPosFor(r,t):n.domIn(r,t)}inlineDOMNearPos(e,t){let n,r=-1,i=!1,a,o=-1,s=!1;return this.tile.blockTiles((t,c)=>{if(t.isWidget()){if(t.flags&32&&c>=e)return!0;t.flags&16&&(i=!0)}else{let l=c+t.length;if(c<=e&&(n=t,r=e-c,i=l<e),l>=e&&!a&&(a=t,o=e-c,s=c>e),c>e&&a)return!0}}),!n&&!a?this.domAtPos(e,t):(i&&a?n=null:s&&n&&(a=null),n&&t<0||!a?n.domIn(r,t):a.domIn(o,t))}coordsAt(e,t,n){let{tile:r,offset:i}=this.tile.resolveBlock(e,t);return r.isWidget()?r.widget instanceof Qi?null:r.coordsInWidget(i,t,!0):r.coordsIn(i,t,n)}lineAt(e,t){let{tile:n}=this.tile.resolveBlock(e,t);return n.isLine()?n:null}coordsForChar(e){let{tile:t,offset:n}=this.tile.resolveBlock(e,1);if(!t.isLine())return null;function r(e,t){if(e.isComposite())for(let n of e.children){if(n.length>=t){let e=r(n,t);if(e)return e}if(t-=n.length,t<0)break}else if(e.isText()&&t<e.length){let n=k(e.text,t);if(n==t)return null;let r=ir(e.dom,t,n).getClientRects();for(let e=0;e<r.length;e++){let t=r[e];if(e==r.length-1||t.top<t.bottom&&t.left<t.right)return t}}return null}return r(t,n)}measureVisibleLineHeights(e){let t=[],{from:n,to:r}=e,i=this.view.contentDOM.clientWidth,a=i>Math.max(this.view.scrollDOM.clientWidth,this.minWidth)+1,o=-1,s=this.view.textDirection==V.LTR,c=0,l=(e,u,d)=>{for(let f=0;f<e.children.length&&!(u>r);f++){let r=e.children[f],p=u+r.length,m=r.dom.getBoundingClientRect(),{height:h}=m;if(d&&!f&&(c+=m.top-d.top),r instanceof _i)p>n&&l(r,u,m);else if(u>=n&&(c>0&&t.push(-c),t.push(h+c),c=0,a)){let e=r.dom.lastChild,t=e?Vn(e):[];if(t.length){let e=t[t.length-1],n=s?e.right-m.left:m.right-e.left;n>o&&(o=n,this.minWidth=i,this.minWidthFrom=u,this.minWidthTo=p)}}d&&f==e.children.length-1&&(c+=d.bottom-m.bottom),u=p+r.breakAfter}};return l(this.tile,0,null),t}textDirectionAt(e){let{tile:t}=this.tile.resolveBlock(e,1);return getComputedStyle(t.dom).direction==`rtl`?V.RTL:V.LTR}measureTextSize(){let e=this.tile.blockTiles(e=>{if(e.isLine()&&e.children.length&&e.length<=20){let t=0,n;for(let r of e.children){if(!r.isText()||/[^ -~]/.test(r.text))return;let e=Vn(r.dom);if(e.length!=1)return;t+=e[0].width,n=e[0].height}if(t)return{lineHeight:e.dom.getBoundingClientRect().height,charWidth:t/e.length,textHeight:n}}});if(e)return e;let t=document.createElement(`div`),n,r,i;return t.className=`cm-line`,t.style.width=`99999px`,t.style.position=`absolute`,t.textContent=`abc def ghi jkl mno pqr stu`,this.view.observer.ignore(()=>{this.tile.dom.appendChild(t);let e=Vn(t.firstChild)[0];n=t.getBoundingClientRect().height,r=e&&e.width?e.width/27:7,i=e&&e.height?e.height:n,t.remove()}),{lineHeight:n,charWidth:r,textHeight:i}}computeBlockGapDeco(){let e=[],t=this.view.viewState;for(let n=0,r=0;;r++){let i=r==t.viewports.length?null:t.viewports[r],a=i?i.from-1:this.view.state.doc.length;if(a>n){let r=(t.lineBlockAt(a).bottom-t.lineBlockAt(n).top)/this.view.scaleY;e.push(z.replace({widget:new Qi(r),block:!0,inclusive:!0,isBlockGap:!0}).range(n,a))}if(!i)break;n=i.to+1}return z.set(e)}updateDeco(){let e=1,t=this.view.state.facet(ti).map(t=>(this.dynamicDecorationMap[e++]=typeof t==`function`)?t(this.view):t),n=!1,r=this.view.state.facet(ri).map((e,t)=>{let r=typeof e==`function`;return r&&(n=!0),r?e(this.view):e});for(r.length&&(this.dynamicDecorationMap[e++]=n,t.push(I.join(r))),this.decorations=[this.editContextFormatting,...t,this.computeBlockGapDeco(),this.view.viewState.lineGapDeco];e<this.decorations.length;)this.dynamicDecorationMap[e++]=!1;this.blockWrappers=this.view.state.facet(ni).map(e=>typeof e==`function`?e(this.view):e)}scrollIntoView(e){if(e.isSnapshot){let t=this.view.viewState.lineBlockAt(e.range.head);this.view.scrollDOM.scrollTop=t.top-e.yMargin,this.view.scrollDOM.scrollLeft=e.xMargin;return}for(let t of this.view.state.facet(Wr))try{if(t(this.view,e.range,e))return!0}catch(e){Jr(this.view.state,e,`scroll handler`)}let{range:t}=e,n=this.coordsAt(t.head,t.assoc||(t.head>t.anchor?-1:1)),r;if(!n)return;!t.empty&&(r=this.coordsAt(t.anchor,t.anchor>t.head?-1:1))&&(n={left:Math.min(n.left,r.left),top:Math.min(n.top,r.top),right:Math.max(n.right,r.right),bottom:Math.max(n.bottom,r.bottom)});let i=li(this.view),a={left:n.left-i.left,top:n.top-i.top,right:n.right+i.right,bottom:n.bottom+i.bottom},{offsetWidth:o,offsetHeight:s}=this.view.scrollDOM;if(Xn(this.view.scrollDOM,a,t.head<t.anchor?-1:1,e.x,e.y,Math.max(Math.min(e.xMargin,o),-o),Math.max(Math.min(e.yMargin,s),-s),this.view.textDirection==V.LTR),window.visualViewport&&window.innerHeight-window.visualViewport.height>1&&(n.top>window.visualViewport.offsetTop+window.visualViewport.height||n.bottom<window.visualViewport.offsetTop)){let e=this.view.docView.lineAt(t.head,1);if(e){let t=$n(e.dom);e.dom.scrollIntoView({block:`nearest`}),er(t,!1)}}}lineHasWidget(e){let t=e=>e.isWidget()||e.children.some(t);return t(this.tile.resolveBlock(e,1).tile)}destroy(){Vi(this.tile)}};function Vi(e,t){let n=t?.get(e);if(n!=1){n??e.destroy();for(let n of e.children)Vi(n,t)}}function Hi(e){return B(e.node)&&e.node.firstChild&&(e.offset==0||e.node.childNodes[e.offset-1].contentEditable==`false`)&&(e.offset==e.node.childNodes.length||e.node.childNodes[e.offset].contentEditable==`false`)}function Ui(e,t){let n=e.observer.selectionRange;if(!n.focusNode)return null;let r=lr(n.focusNode,n.focusOffset),i=ur(n.focusNode,n.focusOffset),a=r||i;if(i&&r&&i.node!=r.node){let t=W.get(i.node);if(!t||t.isText()&&t.text!=i.node.nodeValue)a=i;else if(e.docView.lastCompositionAfterCursor){let e=W.get(r.node);!e||e.isText()&&e.text!=r.node.nodeValue||(a=i)}}if(e.docView.lastCompositionAfterCursor=a!=r,!a)return null;let o=t-a.offset;return{from:o,to:o+a.node.nodeValue.length,node:a.node}}function Wi(e,t,n){let r=Ui(e,n);if(!r)return null;let{node:i,from:a,to:o}=r,s=i.nodeValue;if(/[\n\r]/.test(s)||e.state.doc.sliceString(r.from,r.to)!=s)return null;let c=t.invertedDesc;return{range:new di(c.mapPos(a),c.mapPos(o),a,o),text:i}}function Gi(e,t){return B(e)?(t&&e.childNodes[t-1].contentEditable==`false`?1:0)|(t<e.childNodes.length&&e.childNodes[t].contentEditable==`false`?2:0):0}var Ki=class{constructor(){this.changes=[]}compareRange(e,t){Fn(e,t,this.changes)}comparePoint(e,t){Fn(e,t,this.changes)}boundChange(e){Fn(e,e,this.changes)}};function qi(e,t,n){let r=new Ki;return I.compare(e,t,n,r),r.changes}var Ji=class{constructor(){this.changes=[]}compareRange(e,t){Fn(e,t,this.changes)}comparePoint(){}boundChange(e){Fn(e,e,this.changes)}};function Yi(e,t,n){let r=new Ji;return I.compare(e,t,n,r),r.changes}function Xi(e,t){for(let n=e;n&&n!=t;n=n.assignedSlot||n.parentNode)if(B(n)&&n.contentEditable==`false`)return!0;return!1}function Zi(e,t){let n=!1;return t&&e.iterChangedRanges((e,r)=>{e<t.to&&r>t.from&&(n=!0)}),n}var Qi=class extends On{constructor(e){super(),this.height=e}toDOM(){let e=document.createElement(`div`);return e.className=`cm-gap`,this.updateDOM(e),e}eq(e){return e.height==this.height}updateDOM(e){return e.style.height=this.height+`px`,!0}get editable(){return!0}get estimatedHeight(){return this.height}ignoreEvent(){return!1}};function $i(e,t,n=1){let r=e.charCategorizer(t),i=e.doc.lineAt(t),a=t-i.from;if(i.length==0)return j.cursor(t);a==0?n=1:a==i.length&&(n=-1);let o=a,s=a;n<0?o=k(i.text,a,!1):s=k(i.text,a);let c=r(i.text.slice(o,s));for(;o>0;){let e=k(i.text,o,!1);if(r(i.text.slice(e,o))!=c)break;o=e}for(;s<i.length;){let e=k(i.text,s);if(r(i.text.slice(s,e))!=c)break;s=e}return j.undirectionalRange(o+i.from,s+i.from)}function ea(e,t,n,r,i){let a=Math.round((r-t.left)*e.defaultCharacterWidth);if(e.lineWrapping&&n.height>e.defaultLineHeight*1.5){let t=e.viewState.heightOracle.textHeight,r=Math.floor((i-n.top-(e.defaultLineHeight-t)*.5)/t);a+=r*e.viewState.heightOracle.lineLength}let o=e.state.sliceDoc(n.from,n.to);return n.from+Jt(o,a,e.state.tabSize)}function ta(e,t,n){let r=e.lineBlockAt(t);if(Array.isArray(r.type)){let e;for(let i of r.type){if(i.from>t)break;if(!(i.to<t)){if(i.from<t&&i.to>t)return i;(!e||i.type==kn.Text&&(e.type!=i.type||(n<0?i.from<t:i.to>t)))&&(e=i)}}return e||r}return r}function na(e,t,n,r){let i=ta(e,t.head,t.assoc||-1),a=!r||i.type!=kn.Text||!(e.lineWrapping||i.widgetLineBreaks)?null:e.coordsAtPos(t.assoc<0&&t.head>i.from?t.head-1:t.head);if(a){let t=e.dom.getBoundingClientRect(),r=e.textDirectionAt(i.from),o=e.posAndSideAtCoords({x:n==(r==V.LTR)?t.right-1:t.left+1,y:(a.top+a.bottom)/2});if(o)return j.cursor(o.pos,o.assoc)}return j.cursor(n?i.to:i.from,n?1:-1)}function ra(e,t,n,r){let i=e.state.doc.lineAt(t.head),a=e.bidiSpans(i),o=e.textDirectionAt(i.from);for(let s=t,c=null;;){let t=jr(i,a,o,s,n),l=Ar;if(!t){if(i.number==(n?e.state.doc.lines:1))return s;l=`
`,i=e.state.doc.line(i.number+(n?1:-1)),a=e.bidiSpans(i),t=n?j.cursor(i.from,-1):j.cursor(i.to,1)}if(!c){if(!r)return t;c=r(l)}else if(!c(l))return s;s=t}}function ia(e,t,n){let r=e.state.charCategorizer(t),i=r(n);return e=>{let t=r(e);return i==P.Space&&(i=t),i==t}}function aa(e,t,n,r){let i=t.head,a=n?1:-1;if(i==(n?e.state.doc.length:0))return j.cursor(i,t.assoc);let o=t.goalColumn,s,c=e.contentDOM.getBoundingClientRect(),l=e.coordsAtPos(i,t.assoc||((t.empty?n:t.head==t.from)?1:-1)),u=e.documentTop;if(l)o??=l.left-c.left,s=a<0?l.top:l.bottom;else{let t=e.viewState.lineBlockAt(i);o??=Math.min(c.right-c.left,e.defaultCharacterWidth*(i-t.from)),s=(a<0?t.top:t.bottom)+u}let d=c.left+o,f=e.viewState.heightOracle.textHeight>>1,p=r??f;for(let t=0;;t+=f){let r=s+(p+t)*a,i=ua(e,{x:d,y:r},!1,a);if(n?r>c.bottom:r<c.top)return j.cursor(i.pos,i.assoc);let l=e.coordsAtPos(i.pos,i.assoc),u=l?(l.top+l.bottom)/2:0;if(!l||(n?u>s:u<s))return j.cursor(i.pos,i.assoc,void 0,o)}}function oa(e,t,n){for(;;){let r=0;for(let i of e)i.between(t-1,t+1,(e,i,a)=>{if(t>e&&t<i){let a=r||n||(t-e<i-t?-1:1);t=a<0?e:i,r=a}});if(!r)return t}}function sa(e,t){let n=null;for(let r=0;r<t.ranges.length;r++){let i=t.ranges[r],a=null;if(i.empty){let t=oa(e,i.from,0);t!=i.from&&(a=j.cursor(t,-1))}else{let t=oa(e,i.from,-1),n=oa(e,i.to,1);(t!=i.from||n!=i.to)&&(a=i.undirectional?j.undirectionalRange(i.from,i.to):j.range(i.from==i.anchor?t:n,i.from==i.head?t:n))}a&&(n||=t.ranges.slice(),n[r]=a)}return n?j.create(n,t.mainIndex):t}function ca(e,t,n){let r=oa(e.state.facet(ii).map(t=>t(e)),n.from,t.head>n.from?-1:1);return r==n.from?n:j.cursor(r,r<n.from?1:-1)}var la=class{constructor(e,t){this.pos=e,this.assoc=t}};function ua(e,t,n,r){let i=e.contentDOM.getBoundingClientRect(),a=i.top+e.viewState.paddingTop,{x:o,y:s}=t,c=s-a,l;for(;;){if(c<0)return new la(0,1);if(c>e.viewState.docHeight)return new la(e.state.doc.length,-1);if(l=e.elementAtHeight(c),r==null)break;if(l.type==kn.Text){if(r<0?l.to<e.viewport.from:l.from>e.viewport.to)break;let t=e.docView.coordsAt(r<0?l.from:l.to,r>0?-1:1);if(t&&(r<0?t.top<=c+a:t.bottom>=c+a))break}let t=e.viewState.heightOracle.textHeight/2;c=r>0?l.bottom+t:l.top-t}if(e.viewport.from>=l.to||e.viewport.to<=l.from){if(n)return null;if(l.type==kn.Text){let t=ea(e,i,l,o,s);return new la(t,t==l.from?1:-1)}}if(l.type!=kn.Text)return c<(l.top+l.bottom)/2?new la(l.from,1):new la(l.to,-1);let u=e.docView.lineAt(l.from,2);return u&&u.length==l.length||(u=e.docView.lineAt(l.from,-2)),new da(e,o,s,e.textDirectionAt(l.from)).scanTile(u,l.from)}var da=class{constructor(e,t,n,r){this.view=e,this.x=t,this.y=n,this.baseDir=r,this.line=null,this.spans=null}bidiSpansAt(e){return(!this.line||this.line.from>e||this.line.to<e)&&(this.line=this.view.state.doc.lineAt(e),this.spans=this.view.bidiSpans(this.line)),this}baseDirAt(e,t){let{line:n,spans:r}=this.bidiSpansAt(e);return r[xr.find(r,e-n.from,-1,t)].level==this.baseDir}dirAt(e,t){let{line:n,spans:r}=this.bidiSpansAt(e);return r[xr.find(r,e-n.from,-1,t)].dir}bidiIn(e,t){let{spans:n,line:r}=this.bidiSpansAt(e);return n.length>1||n.length&&(n[0].level!=this.baseDir||n[0].to+r.from<t)}scan(e,t,n=!1){let r=0,i=e.length-1,a=new Set,o=this.bidiIn(e[0],e[i]),s,c,l=-1,u=1e9,d;search:for(;r<i;){let n=i-r,f=r+i>>1;adjust:if(a.has(f)){for(let e=1;e<n;e++){let t=f+e;if(t>=i&&(t-=n),!a.has(t)){f=t;break adjust}}break search}a.add(f);let p=t(f),m=0;if(p)for(let e=0;e<p.length;e++){let t=p[e];if(!(t.width==0&&p.length>1)){if(t.bottom<this.y)(!s||s.bottom<t.bottom)&&(s=t),m=1;else if(t.top>this.y)(!c||c.top>t.top)&&(c=t),m=-1;else{let e=t.left>this.x?this.x-t.left:t.right<this.x?this.x-t.right:0,n=Math.abs(e);n<u&&(l=f,u=n,d=t),e&&(m=e<0==(this.baseDir==V.LTR)?-1:1)}}}m==-1&&(!o||this.baseDirAt(e[f],1))?i=f:m==1&&(!o||this.baseDirAt(e[f+1],-1))&&(r=f+1)}if(!d){if(!c&&!s)return{i:0,after:!1};let n=s&&(!c||this.y-s.bottom<c.top-this.y)?s:c;return this.y=(n.top+n.bottom)/2,this.scan(e,t,!0)}if(u&&!n){let{top:n,bottom:r}=d;if(s&&s.bottom>(n+n+r)/3)return this.y=s.bottom-1,this.scan(e,t,!0);if(c&&c.top<(n+r+r)/3)return this.y=c.top+1,this.scan(e,t,!0)}let f=(o?this.dirAt(e[l],1):this.baseDir)==V.LTR;return{i:l,after:this.x>(d.left+d.right)/2==f}}scanText(e,t){let n=[];for(let r=0;r<e.length;r=k(e.text,r))n.push(t+r);n.push(t+e.length);let r=this.scan(n,r=>{let i=n[r]-t,a=n[r+1]-t;return ir(e.dom,i,a).getClientRects()});return r.after?new la(n[r.i+1],-1):new la(n[r.i],1)}scanTile(e,t){if(!e.length)return new la(t,1);if(e.children.length==1){let n=e.children[0];if(n.isText())return this.scanText(n,t);if(n.isComposite())return this.scanTile(n,t)}let n=[t];for(let r=0,i=t;r<e.children.length;r++)n.push(i+=e.children[r].length);let r=this.scan(n,t=>{let n=e.children[t];return n.flags&48?null:(B(n.dom)?n.dom:ir(n.dom,0,n.length)).getClientRects()}),i=e.children[r.i],a=n[r.i];return i.isText()?this.scanText(i,a):i.isComposite()?this.scanTile(i,a):r.after?new la(n[r.i+1],-1):new la(a,1)}},fa=`￿`,pa=class{constructor(e,t){this.points=e,this.view=t,this.text=``,this.lineSeparator=t.state.facet(F.lineSeparator)}append(e){this.text+=e}lineBreak(){this.text+=fa}readRange(e,t){if(!e)return this;let n=e.parentNode;for(let r=e;;){this.findPointBefore(n,r);let e=this.text.length;this.readNode(r);let i=W.get(r),a=r.nextSibling;if(a==t){i?.breakAfter&&!a&&n!=this.view.contentDOM&&this.lineBreak();break}let o=W.get(a);(i&&o?i.breakAfter:(i?i.breakAfter:Wn(r))||Wn(a)&&(r.nodeName!=`BR`||i?.isWidget())&&this.text.length>e)&&!ha(a,t)&&this.lineBreak(),r=a}return this.findPointBefore(n,t),this}readTextNode(e){let t=e.nodeValue;for(let n of this.points)n.node==e&&(n.pos=this.text.length+Math.min(n.offset,t.length));for(let n=0,r=this.lineSeparator?null:/\r\n?|\n/g;;){let i=-1,a=1,o;if(this.lineSeparator?(i=t.indexOf(this.lineSeparator,n),a=this.lineSeparator.length):(o=r.exec(t))&&(i=o.index,a=o[0].length),this.append(t.slice(n,i<0?t.length:i)),i<0)break;if(this.lineBreak(),a>1)for(let t of this.points)t.node==e&&t.pos>this.text.length&&(t.pos-=a-1);n=i+a}}readNode(e){let t=W.get(e),n=t&&t.overrideDOMText;if(n!=null){this.findPointInside(e,n.length);for(let e=n.iter();!e.next().done;)e.lineBreak?this.lineBreak():this.append(e.value)}else Ln(e)?this.readTextNode(e):e.nodeName==`BR`?e.nextSibling&&this.lineBreak():B(e)&&this.readRange(e.firstChild,null)}findPointBefore(e,t){for(let n of this.points)n.node==e&&e.childNodes[n.offset]==t&&(n.pos=this.text.length)}findPointInside(e,t){for(let n of this.points)(Ln(e)?n.node==e:e.contains(n.node))&&(n.pos=this.text.length+(ma(e,n.node,n.offset)?t:0))}};function ma(e,t,n){for(;;){if(!t||n<Kn(t))return!1;if(t==e)return!0;n=Un(t)+1,t=t.parentNode}}function ha(e,t){let n;for(;e!=t&&e;e=e.nextSibling){let t=W.get(e);if(!t?.isWidget())return!1;t&&(n||=[]).push(t)}if(n){for(let e of n)if(e.overrideDOMText?.length)return!1}return!0}var ga=class{constructor(e,t){this.node=e,this.offset=t,this.pos=-1}},_a=class{constructor(e,t,n,r){this.typeOver=r,this.bounds=null,this.text=``,this.domChanged=t>-1;let{impreciseHead:i,impreciseAnchor:a}=e.docView,o=e.state.selection;if(e.state.readOnly&&t>-1)this.newSel=null;else if(t>-1&&(this.bounds=va(e.docView.tile,t,n,0))){let t=i||a?[]:Ca(e),n=new pa(t,e);n.readRange(this.bounds.startDOM,this.bounds.endDOM),this.text=n.text,this.newSel=wa(t,this.bounds.from)}else{let t=e.observer.selectionRange,n=i&&i.node==t.focusNode&&i.offset==t.focusOffset||!zn(e.contentDOM,t.focusNode)?o.main.head:e.docView.posFromDOM(t.focusNode,t.focusOffset),r=a&&a.node==t.anchorNode&&a.offset==t.anchorOffset||!zn(e.contentDOM,t.anchorNode)?o.main.anchor:e.docView.posFromDOM(t.anchorNode,t.anchorOffset),s=e.viewport;if((R.ios||R.chrome)&&n!=r&&Math.min(n,r)<=o.main.from&&Math.max(n,r)>=o.main.to&&(s.from>0||s.to<e.state.doc.length)){let t=Math.min(n,r),i=Math.max(n,r),a=s.from-t,o=s.to-i;a!=0&&a!=1&&t!=0||o!=0&&o!=-1&&i!=e.state.doc.length||(n=0,r=e.state.doc.length)}if(e.inputState.composing>-1&&o.ranges.length>1)this.newSel=o.replaceRange(j.range(r,n));else if(e.lineWrapping&&r==n&&!(o.main.empty&&o.main.head==n)&&e.inputState.lastTouchTime>Date.now()-100){let t=e.coordsAtPos(n,-1),r=0;t&&(r=e.inputState.lastTouchY<=t.bottom?-1:1),this.newSel=j.create([j.cursor(n,r)])}else this.newSel=j.single(r,n)}}};function va(e,t,n,r){if(e.isComposite()){let i=-1,a=-1,o=-1,s=-1;for(let c=0,l=r,u=r;c<e.children.length;c++){let r=e.children[c],d=l+r.length;if(l<t&&d>n)return va(r,t,n,l);if(d>=t&&i==-1&&(i=c,a=l),l>n&&r.dom.parentNode==e.dom){o=c,s=u;break}u=d,l=d+r.breakAfter}return{from:a,to:s<0?r+e.length:s,startDOM:(i?e.children[i-1].dom.nextSibling:null)||e.dom.firstChild,endDOM:o<e.children.length&&o>=0?e.children[o].dom:null}}return e.isText()?{from:r,to:r+e.length,startDOM:e.dom,endDOM:e.dom.nextSibling}:null}function ya(e,t){let n,{newSel:r}=t,{state:i}=e,a=i.selection.main,o=e.inputState.lastKeyTime>Date.now()-100?e.inputState.lastKeyCode:-1;if(t.bounds){let{from:e,to:r}=t.bounds,s=a.from,c=null;(o===8||R.android&&t.text.length<r-e)&&(s=a.to,c=`end`);let l=i.doc.sliceString(e,r,fa),u,d;!a.empty&&a.from>=e&&a.to<=r&&(t.typeOver||l!=t.text)&&l.slice(0,a.from-e)==t.text.slice(0,a.from-e)&&l.slice(a.to-e)==t.text.slice(u=t.text.length-(l.length-(a.to-e)))?n={from:a.from,to:a.to,insert:D.of(t.text.slice(a.from-e,u).split(fa))}:(d=Sa(l,t.text,s-e,c))&&(R.chrome&&o==13&&d.toB==d.from+2&&t.text.slice(d.from,d.toB)==`￿￿`&&d.toB--,n={from:e+d.from,to:e+d.toA,insert:D.of(t.text.slice(d.from,d.toB).split(fa))})}else r&&(!e.hasFocus&&i.facet(Yr)||Ta(r,a))&&(r=null);if(!n&&!r)return!1;if((R.mac||R.android)&&n&&n.from==n.to&&n.from==a.head-1&&/^\. ?$/.test(n.insert.toString())&&e.contentDOM.getAttribute(`autocorrect`)==`off`?(r&&n.insert.length==2&&(r=j.single(r.main.anchor-1,r.main.head-1)),n={from:n.from,to:n.to,insert:D.of([n.insert.toString().replace(`.`,` `)])}):i.doc.lineAt(a.from).to<a.to&&e.docView.lineHasWidget(a.to)&&e.inputState.insertingTextAt>Date.now()-50?n={from:a.from,to:a.to,insert:i.toText(e.inputState.insertingText)}:R.chrome&&n&&n.from==n.to&&n.from==a.head&&n.insert.toString()==`
 `&&e.lineWrapping&&(r&&=j.single(r.main.anchor-1,r.main.head-1),n={from:a.from,to:a.to,insert:D.of([` `])}),n)return ba(e,n,r,o);if(r&&!Ta(r,a)){let t=!1,n=`select`;return e.inputState.lastSelectionTime>Date.now()-50&&(e.inputState.lastSelectionOrigin==`select`&&(t=!0),n=e.inputState.lastSelectionOrigin,n==`select.pointer`&&(r=sa(i.facet(ii).map(t=>t(e)),r))),e.dispatch({selection:r,scrollIntoView:t,userEvent:n}),!0}return!1}function ba(e,t,n,r=-1){if(R.ios&&e.inputState.flushIOSKey(t))return!0;let i=e.state.selection.main;if(R.android&&(t.to==i.to&&(t.from==i.from||t.from==i.from-1&&e.state.sliceDoc(t.from,i.from)==` `)&&t.insert.length==1&&t.insert.lines==2&&ar(e.contentDOM,`Enter`,13)||(t.from==i.from-1&&t.to==i.to&&t.insert.length==0||r==8&&t.insert.length<t.to-t.from&&t.to>i.head)&&ar(e.contentDOM,`Backspace`,8)||t.from==i.from&&t.to==i.to+1&&t.insert.length==0&&ar(e.contentDOM,`Delete`,46)))return!0;let a=t.insert.toString();e.inputState.composing>=0&&e.inputState.composing++;let o,s=()=>o||=xa(e,t,n);return e.state.facet(Rr).some(n=>n(e,t.from,t.to,a,s))||e.dispatch(s()),!0}function xa(e,t,n){let r,i=e.state,a=i.selection.main,o=-1;if(t.from==t.to&&t.from<a.from||t.from>a.to){let n=t.from<a.from?-1:1,r=n<0?a.from:a.to,s=oa(i.facet(ii).map(t=>t(e)),r,n);t.from==s&&(o=s)}if(o>-1)r={changes:t,selection:j.cursor(t.from+t.insert.length,-1)};else if(t.from>=a.from&&t.to<=a.to&&t.to-t.from>=(a.to-a.from)/3&&(!n||n.main.empty&&n.main.from==t.from+t.insert.length)&&e.inputState.composing<0){let n=a.from<t.from?i.sliceDoc(a.from,t.from):``,o=a.to>t.to?i.sliceDoc(t.to,a.to):``;r=i.replaceSelection(e.state.toText(n+t.insert.sliceString(0,void 0,e.state.lineBreak)+o))}else{let o=i.changes(t),s=n&&n.main.to<=o.newLength?n.main:void 0;if(i.selection.ranges.length>1&&(e.inputState.composing>=0||e.inputState.compositionPendingChange)&&t.to<=a.to+10&&t.to>=a.to-10){let c=e.state.sliceDoc(t.from,t.to),l,u=n&&Ui(e,n.main.head);if(u){let e=t.insert.length-(t.to-t.from);l={from:u.from,to:u.to-e}}else l=e.state.doc.lineAt(a.head);let d=a.to-t.to;r=i.changeByRange(n=>{if(n.from==a.from&&n.to==a.to)return{changes:o,range:s||n.map(o)};let r=n.to-d,u=r-c.length;if(e.state.sliceDoc(u,r)!=c||r>=l.from&&u<=l.to)return{range:n};let f=i.changes({from:u,to:r,insert:t.insert}),p=n.to-a.to;return{changes:f,range:s?j.range(Math.max(0,s.anchor+p),Math.max(0,s.head+p)):n.map(f)}})}else r={changes:o,selection:s&&i.selection.replaceRange(s)}}let s=`input.type`;return(e.composing||e.inputState.compositionPendingChange&&e.inputState.compositionEndedAt>Date.now()-50)&&(e.inputState.compositionPendingChange=!1,s+=`.compose`,e.inputState.compositionFirstChange&&(s+=`.start`,e.inputState.compositionFirstChange=!1)),i.update(r,{userEvent:s,scrollIntoView:!0})}function Sa(e,t,n,r){let i=Math.min(e.length,t.length),a=0;for(;a<i&&e.charCodeAt(a)==t.charCodeAt(a);)a++;if(a==i&&e.length==t.length)return null;let o=e.length,s=t.length;for(;o>0&&s>0&&e.charCodeAt(o-1)==t.charCodeAt(s-1);)o--,s--;if(r==`end`){let e=Math.max(0,a-Math.min(o,s));n-=o+e-a}if(o<a&&e.length<t.length){let e=n<=a&&n>=o?a-n:0;a-=e,s=a+(s-o),o=a}else if(s<a){let e=n<=a&&n>=s?a-n:0;a-=e,o=a+(o-s),s=a}return{from:a,toA:o,toB:s}}function Ca(e){let t=[];if(e.root.activeElement!=e.contentDOM)return t;let{anchorNode:n,anchorOffset:r,focusNode:i,focusOffset:a}=e.observer.selectionRange;return n&&(t.push(new ga(n,r)),(i!=n||a!=r)&&t.push(new ga(i,a))),t}function wa(e,t){if(e.length==0)return null;let n=e[0].pos,r=e.length==2?e[1].pos:n;return n<0||r<0?null:n==r?j.create([j.cursor(r+t,-1)]):j.single(n+t,r+t)}function Ta(e,t){return t.head==e.main.head&&t.anchor==e.main.anchor}var Ea=class{setSelectionOrigin(e){this.lastSelectionOrigin=e,this.lastSelectionTime=Date.now()}constructor(e){this.view=e,this.lastKeyCode=0,this.lastKeyTime=0,this.touchActive=!1,this.lastTouchTime=0,this.lastTouchX=0,this.lastTouchY=0,this.lastFocusTime=0,this.lastScrollTop=0,this.lastScrollLeft=0,this.lastWheelEvent=0,this.pendingIOSKey=void 0,this.lastIOSMomentumScroll=0,this.tabFocusMode=-1,this.lastSelectionOrigin=null,this.lastSelectionTime=0,this.lastContextMenu=0,this.scrollHandlers=[],this.handlers=Object.create(null),this.composing=-1,this.compositionFirstChange=null,this.compositionEndedAt=0,this.compositionPendingKey=!1,this.compositionPendingChange=!1,this.insertingText=``,this.insertingTextAt=0,this.mouseSelection=null,this.draggedContent=null,this.handleEvent=this.handleEvent.bind(this),this.notifiedFocused=e.hasFocus,R.safari&&e.contentDOM.addEventListener(`input`,()=>null),R.gecko&&lo(e.contentDOM.ownerDocument)}handleEvent(e){Ba(this.view,e)&&!this.ignoreDuringComposition(e)&&(e.type==`keydown`&&this.keydown(e)||(this.view.updateState==0?this.runHandlers(e.type,e):Promise.resolve().then(()=>this.runHandlers(e.type,e))))}runHandlers(e,t){let n=this.handlers[e];if(n){for(let e of n.observers)e(this.view,t);for(let e of n.handlers){if(t.defaultPrevented)break;if(e(this.view,t)){t.preventDefault();break}}}}ensureHandlers(e){let t=ka(e),n=this.handlers,r=this.view.contentDOM;for(let e in t)if(e!=`scroll`){let i=!t[e].handlers.length,a=n[e];a&&i!=!a.handlers.length&&(r.removeEventListener(e,this.handleEvent),a=null),a||r.addEventListener(e,this.handleEvent,{passive:i})}for(let e in n)e!=`scroll`&&!t[e]&&r.removeEventListener(e,this.handleEvent);this.handlers=t}keydown(e){if(this.lastKeyCode=e.keyCode,this.lastKeyTime=Date.now(),e.keyCode==9&&this.tabFocusMode>-1&&(!this.tabFocusMode||Date.now()<=this.tabFocusMode))return!0;if(this.tabFocusMode>0&&e.keyCode!=27&&Ma.indexOf(e.keyCode)<0&&(this.tabFocusMode=-1),R.android&&R.chrome&&!e.synthetic&&(e.keyCode==13||e.keyCode==8))return this.view.observer.delayAndroidKey(e.key,e.keyCode),!0;if(R.ios&&!e.synthetic&&!e.altKey&&!e.metaKey&&(Aa.some(t=>t.keyCode==e.keyCode)&&!e.ctrlKey||ja.indexOf(e.key)>-1&&e.ctrlKey)){let t={ctrlKey:e.ctrlKey,altKey:e.altKey,metaKey:e.metaKey,shiftKey:e.shiftKey};t.shiftKey&&R.ios&&!/^(off|none)$/.test(this.view.contentDOM.autocapitalize)&&Da(this.view.win)&&(t.shiftKey=!1);let n=this.pendingIOSKey={key:e.key,keyCode:e.keyCode,mods:t};return setTimeout(()=>{this.pendingIOSKey==n&&this.flushIOSKey()},50),!0}return e.keyCode!=229&&this.view.observer.forceFlush(),!1}flushIOSKey(e){let t=this.pendingIOSKey;return!t||this.view.observer.pendingRecords().length||t.key==`Enter`&&e&&e.from<e.to&&/^\S+$/.test(e.insert.toString())?!1:(this.pendingIOSKey=void 0,ar(this.view.contentDOM,t.key,t.keyCode,t.mods))}ignoreDuringComposition(e){return!/^key/.test(e.type)||e.synthetic?!1:this.composing>0?!0:R.safari&&!R.ios&&this.compositionPendingKey&&Date.now()-this.compositionEndedAt<100?(this.compositionPendingKey=!1,!0):!1}startMouseSelection(e){this.mouseSelection&&this.mouseSelection.destroy(),this.mouseSelection=e}update(e){this.view.observer.update(e),this.mouseSelection&&this.mouseSelection.update(e),this.draggedContent&&e.docChanged&&(this.draggedContent=this.draggedContent.map(e.changes)),e.transactions.length&&(this.lastKeyCode=this.lastSelectionTime=0)}destroy(){this.mouseSelection&&this.mouseSelection.destroy()}};function Da(e){return e.visualViewport?e.visualViewport.height*e.visualViewport.scale/e.document.documentElement.clientHeight<.85:!1}function Oa(e,t){return(n,r)=>{try{return t.call(e,r,n)}catch(e){Jr(n.state,e)}}}function ka(e){let t=Object.create(null);function n(e){return t[e]||(t[e]={observers:[],handlers:[]})}for(let t of e){let e=t.spec,r=e&&e.plugin.domEventHandlers,i=e&&e.plugin.domEventObservers;if(r)for(let e in r){let i=r[e];i&&n(e).handlers.push(Oa(t.value,i))}if(i)for(let e in i){let r=i[e];r&&n(e).observers.push(Oa(t.value,r))}}for(let e in Va)n(e).handlers.push(Va[e]);for(let e in Ha)n(e).observers.push(Ha[e]);return t}var Aa=[{key:`Backspace`,keyCode:8,inputType:`deleteContentBackward`},{key:`Enter`,keyCode:13,inputType:`insertParagraph`},{key:`Enter`,keyCode:13,inputType:`insertLineBreak`},{key:`Delete`,keyCode:46,inputType:`deleteContentForward`}],ja=`dthko`,Ma=[16,17,18,20,91,92,224,225],Na=6;function Pa(e){return Math.max(0,e)*.7+8}function Fa(e,t){return Math.max(Math.abs(e.clientX-t.clientX),Math.abs(e.clientY-t.clientY))}var Ia=class{constructor(e,t,n,r){this.view=e,this.startEvent=t,this.style=n,this.mustSelect=r,this.scrollSpeed={x:0,y:0},this.scrolling=-1,this.lastEvent=t,this.scrollParents=Zn(e.contentDOM),this.atoms=e.state.facet(ii).map(t=>t(e));let i=e.contentDOM.ownerDocument;i.addEventListener(`mousemove`,this.move=this.move.bind(this)),i.addEventListener(`mouseup`,this.up=this.up.bind(this)),this.extend=t.shiftKey,this.multiple=e.state.facet(F.allowMultipleSelections)&&La(e,t),this.dragging=za(e,t)&&Qa(t)==1?null:!1}start(e){this.dragging===!1&&this.select(e)}move(e){if(e.buttons==0)return this.destroy();if(this.dragging||this.dragging==null&&Fa(this.startEvent,e)<10)return;this.select(this.lastEvent=e);let t=0,n=0,r=0,i=0,a=this.view.win.innerWidth,o=this.view.win.innerHeight;this.scrollParents.x&&({left:r,right:a}=this.scrollParents.x.getBoundingClientRect()),this.scrollParents.y&&({top:i,bottom:o}=this.scrollParents.y.getBoundingClientRect());let s=li(this.view);e.clientX-s.left<=r+Na?t=-Pa(r-e.clientX):e.clientX+s.right>=a-Na&&(t=Pa(e.clientX-a)),e.clientY-s.top<=i+Na?n=-Pa(i-e.clientY):e.clientY+s.bottom>=o-Na&&(n=Pa(e.clientY-o)),this.setScrollSpeed(t,n)}up(e){this.dragging??this.select(this.lastEvent),this.dragging||e.preventDefault(),this.destroy()}destroy(){this.setScrollSpeed(0,0);let e=this.view.contentDOM.ownerDocument;e.removeEventListener(`mousemove`,this.move),e.removeEventListener(`mouseup`,this.up),this.view.inputState.mouseSelection=this.view.inputState.draggedContent=null}setScrollSpeed(e,t){this.scrollSpeed={x:e,y:t},e||t?this.scrolling<0&&(this.scrolling=setInterval(()=>this.scroll(),50)):this.scrolling>-1&&(clearInterval(this.scrolling),this.scrolling=-1)}scroll(){let{x:e,y:t}=this.scrollSpeed;e&&this.scrollParents.x&&(this.scrollParents.x.scrollLeft+=e,e=0),t&&this.scrollParents.y&&(this.scrollParents.y.scrollTop+=t,t=0),(e||t)&&this.view.win.scrollBy(e,t),this.dragging===!1&&this.select(this.lastEvent)}select(e){let{view:t}=this,n=sa(this.atoms,this.style.get(e,this.extend,this.multiple));(this.mustSelect||!n.eq(t.state.selection,this.dragging===!1))&&this.view.dispatch({selection:n,userEvent:`select.pointer`}),this.mustSelect=!1}update(e){e.transactions.some(e=>e.isUserEvent(`input.type`))?this.destroy():this.style.update(e)&&setTimeout(()=>this.select(this.lastEvent),20)}};function La(e,t){let n=e.state.facet(Nr);return n.length?n[0](t):R.mac?t.metaKey:t.ctrlKey}function Ra(e,t){let n=e.state.facet(Pr);return n.length?n[0](t):R.mac?!t.altKey:!t.ctrlKey}function za(e,t){let{main:n}=e.state.selection;if(n.empty)return!1;let r=Rn(e.root);if(!r||r.rangeCount==0)return!0;let i=r.getRangeAt(0).getClientRects();for(let e=0;e<i.length;e++){let n=i[e];if(n.left<=t.clientX&&n.right>=t.clientX&&n.top<=t.clientY&&n.bottom>=t.clientY)return!0}return!1}function Ba(e,t){if(!t.bubbles)return!0;if(t.defaultPrevented)return!1;for(let n=t.target,r;n!=e.contentDOM;n=n.parentNode)if(!n||n.nodeType==11||(r=W.get(n))&&r.isWidget()&&!r.isHidden&&r.widget.ignoreEvent(t))return!1;return!0}var Va=Object.create(null),Ha=Object.create(null),Ua=R.ie&&R.ie_version<15||R.ios&&R.webkit_version<604;function Wa(e){let t=e.dom.parentNode;if(!t)return;let n=t.appendChild(document.createElement(`textarea`));n.style.cssText=`position: fixed; left: -10000px; top: 10px`,n.focus(),setTimeout(()=>{e.focus(),n.remove(),Ka(e,n.value)},50)}function Ga(e,t,n){for(let r of e.facet(t))n=r(n,e);return n}function Ka(e,t){t=Ga(e.state,Br,t);let{state:n}=e,r,i=1,a=n.toText(t),o=a.lines==n.selection.ranges.length;if(io!=null&&n.selection.ranges.every(e=>e.empty)&&io==a.toString()){let e=-1;r=n.changeByRange(r=>{let s=n.doc.lineAt(r.from);if(s.from==e)return{range:r};e=s.from;let c=n.toText((o?a.line(i++).text:t)+n.lineBreak);return{changes:{from:s.from,insert:c},range:j.cursor(r.from+c.length,-1)}})}else r=o?n.changeByRange(e=>{let t=a.line(i++);return{changes:{from:e.from,to:e.to,insert:t.text},range:j.cursor(e.from+t.length,-1)}}):n.replaceSelection(a);e.dispatch(r,{userEvent:`input.paste`,scrollIntoView:!0})}Ha.scroll=e=>{let t=e.inputState;t.lastScrollTop=e.scrollDOM.scrollTop,t.lastScrollLeft=e.scrollDOM.scrollLeft,R.ios&&!t.touchActive&&(t.lastIOSMomentumScroll=Date.now())},Ha.wheel=Ha.mousewheel=e=>{e.inputState.lastWheelEvent=Date.now()},Va.keydown=(e,t)=>(e.inputState.setSelectionOrigin(`select`),t.keyCode==27&&e.inputState.tabFocusMode!=0&&(e.inputState.tabFocusMode=Date.now()+2e3),!1),Ha.touchstart=(e,t)=>{let n=e.inputState,r=t.targetTouches[0];n.touchActive=!0,n.lastTouchTime=Date.now(),r&&(n.lastTouchX=r.clientX,n.lastTouchY=r.clientY),n.setSelectionOrigin(`select.pointer`)},Ha.touchmove=e=>{e.inputState.setSelectionOrigin(`select.pointer`)},Ha.touchend=(e,t)=>{e.inputState.touchActive=!1},Va.mousedown=(e,t)=>{if(e.observer.flush(),e.inputState.lastTouchTime>Date.now()-2e3)return!1;let n=null;for(let r of e.state.facet(Fr))if(n=r(e,t),n)break;if(!n&&t.button==0&&(n=$a(e,t)),n){let r=!e.hasFocus;e.inputState.startMouseSelection(new Ia(e,t,n,r)),r&&e.observer.ignore(()=>{nr(e.contentDOM);let t=e.root.activeElement;t&&!t.contains(e.contentDOM)&&t.blur()});let i=e.inputState.mouseSelection;if(i)return i.start(t),i.dragging===!1}else e.inputState.setSelectionOrigin(`select.pointer`);return!1};function qa(e,t,n,r){if(r==1)return j.cursor(t,n);if(r==2)return $i(e.state,t,n);{let r=e.docView.lineAt(t,n),i=e.state.doc.lineAt(r?r.posAtEnd:t),a=r?r.posAtStart:i.from,o=r?r.posAtEnd:i.to;return o<e.state.doc.length&&o==i.to&&o++,j.undirectionalRange(a,o)}}var Ja=R.ie&&R.ie_version<=11,Ya=null,Xa=0,Za=0;function Qa(e){if(!Ja)return e.detail;let t=Ya,n=Za;return Ya=e,Za=Date.now(),Xa=!t||n>Date.now()-400&&Math.abs(t.clientX-e.clientX)<2&&Math.abs(t.clientY-e.clientY)<2?(Xa+1)%3:1}function $a(e,t){let n=e.posAndSideAtCoords({x:t.clientX,y:t.clientY},!1),r=Qa(t),i=e.state.selection;return{update(e){e.docChanged&&(n.pos=e.changes.mapPos(n.pos),i=i.map(e.changes))},get(t,a,o){let s=e.posAndSideAtCoords({x:t.clientX,y:t.clientY},!1),c,l=qa(e,s.pos,s.assoc,r);if(n.pos!=s.pos&&!a){let t=qa(e,n.pos,n.assoc,r),i=Math.min(t.from,l.from),a=Math.max(t.to,l.to);l=i<l.from?j.range(i,a,l.assoc):j.range(a,i,l.assoc)}return a?i.replaceRange(i.main.extend(l.from,l.to,l.assoc)):o&&r==1&&i.ranges.length>1&&(c=eo(i,s.pos))?c:o?i.addRange(l):j.create([l])}}}function eo(e,t){for(let n=0;n<e.ranges.length;n++){let{from:r,to:i}=e.ranges[n];if(r<=t&&i>=t)return j.create(e.ranges.slice(0,n).concat(e.ranges.slice(n+1)),e.mainIndex==n?0:e.mainIndex-+(e.mainIndex>n))}return null}Va.dragstart=(e,t)=>{let{selection:{main:n}}=e.state;if(t.target.draggable){let r=e.docView.tile.nearest(t.target);if(r&&r.isWidget()){let e=r.posAtStart,t=e+r.length;(e>=n.to||t<=n.from)&&(n=j.undirectionalRange(e,t))}}let{inputState:r}=e;return r.mouseSelection&&(r.mouseSelection.dragging=!0),r.draggedContent=n,t.dataTransfer&&(t.dataTransfer.setData(`Text`,Ga(e.state,Vr,e.state.sliceDoc(n.from,n.to))),t.dataTransfer.effectAllowed=`copyMove`),!1},Va.dragend=e=>(e.inputState.draggedContent=null,!1);function to(e,t,n,r){if(n=Ga(e.state,Br,n),!n)return;let i=e.posAtCoords({x:t.clientX,y:t.clientY},!1),{draggedContent:a}=e.inputState,o=r&&a&&Ra(e,t)?{from:a.from,to:a.to}:null,s={from:i,insert:n},c=e.state.changes(o?[o,s]:s);e.focus(),e.dispatch({changes:c,selection:{anchor:c.mapPos(i,-1),head:c.mapPos(i,1)},userEvent:o?`move.drop`:`input.drop`}),e.inputState.draggedContent=null}Va.drop=(e,t)=>{if(!t.dataTransfer)return!1;if(e.state.readOnly)return!0;let n=t.dataTransfer.files;if(n&&n.length){let r=Array(n.length),i=0,a=()=>{++i==n.length&&to(e,t,r.filter(e=>e!=null).join(e.state.lineBreak),!1)};for(let e=0;e<n.length;e++){let t=new FileReader;t.onerror=a,t.onload=()=>{/[\x00-\x08\x0e-\x1f]{2}/.test(t.result)||(r[e]=t.result),a()},t.readAsText(n[e])}return!0}{let n=t.dataTransfer.getData(`Text`);if(n)return to(e,t,n,!0),!0}return!1},Va.paste=(e,t)=>{if(e.state.readOnly)return!0;e.observer.flush();let n=Ua?null:t.clipboardData;return n?(Ka(e,n.getData(`text/plain`)||n.getData(`text/uri-list`)),!0):(Wa(e),!1)};function no(e,t){let n=e.dom.parentNode;if(!n)return;let r=n.appendChild(document.createElement(`textarea`));r.style.cssText=`position: fixed; left: -10000px; top: 10px`,r.value=t,r.focus(),r.selectionEnd=t.length,r.selectionStart=0,setTimeout(()=>{r.remove(),e.focus()},50)}function ro(e){let t=[],n=[],r=!1;for(let r of e.selection.ranges)r.empty||(t.push(e.sliceDoc(r.from,r.to)),n.push(r));if(!t.length){let i=-1;for(let{from:r}of e.selection.ranges){let a=e.doc.lineAt(r);a.number>i&&(t.push(a.text),n.push({from:a.from,to:Math.min(e.doc.length,a.to+1)})),i=a.number}r=!0}return{text:Ga(e,Vr,t.join(e.lineBreak)),ranges:n,linewise:r}}var io=null;Va.copy=Va.cut=(e,t)=>{if(!Bn(e.contentDOM,e.observer.selectionRange))return!1;let{text:n,ranges:r,linewise:i}=ro(e.state);if(!n&&!i)return!1;io=i?n:null,t.type==`cut`&&!e.state.readOnly&&e.dispatch({changes:r,scrollIntoView:!0,userEvent:`delete.cut`});let a=Ua?null:t.clipboardData;return a?(a.clearData(),a.setData(`text/plain`,n),!0):(no(e,n),!1)};var ao=ft.define();function oo(e,t){let n=[];for(let r of e.facet(zr)){let i=r(e,t);i&&n.push(i)}return n.length?e.update({effects:n,annotations:ao.of(!0)}):null}function so(e){setTimeout(()=>{let t=e.hasFocus;if(t!=e.inputState.notifiedFocused){let n=oo(e.state,t);n?e.dispatch(n):e.update([])}},10)}Ha.focus=e=>{e.inputState.lastFocusTime=Date.now(),!e.scrollDOM.scrollTop&&(e.inputState.lastScrollTop||e.inputState.lastScrollLeft)&&(e.scrollDOM.scrollTop=e.inputState.lastScrollTop,e.scrollDOM.scrollLeft=e.inputState.lastScrollLeft),so(e)},Ha.blur=e=>{e.observer.clearSelectionRange(),so(e)},Ha.compositionstart=Ha.compositionupdate=(e,t)=>{if(!e.observer.editContext&&(e.inputState.compositionFirstChange??(e.inputState.compositionFirstChange=!0),e.inputState.composing<0)){let{main:t}=e.state.selection;!t.empty&&e.lineBlockAt(t.from).from!=e.lineBlockAt(t.to).from&&!e.state.readOnly&&e.dispatch({changes:e.state.selection.ranges.filter(e=>!e.empty).map(e=>({from:e.from,to:e.to})),userEvent:`input`}),e.inputState.composing=0}},Ha.compositionend=e=>{e.observer.editContext||(e.inputState.composing=-1,e.inputState.compositionEndedAt=Date.now(),e.inputState.compositionPendingKey=!0,e.inputState.compositionPendingChange=e.observer.pendingRecords().length>0,e.inputState.compositionFirstChange=null,R.chrome&&R.android?e.observer.flushSoon():e.inputState.compositionPendingChange?Promise.resolve().then(()=>e.observer.flush()):setTimeout(()=>{e.inputState.composing<0&&e.docView.hasComposition&&e.update([])},50))},Ha.contextmenu=e=>{e.inputState.lastContextMenu=Date.now()},Va.beforeinput=(e,t)=>{if((t.inputType==`insertText`||t.inputType==`insertCompositionText`)&&(e.inputState.insertingText=t.data,e.inputState.insertingTextAt=Date.now()),t.inputType==`insertReplacementText`&&e.observer.editContext){let n=t.dataTransfer?.getData(`text/plain`),r=t.getTargetRanges();if(n&&r.length){let t=r[0];return ba(e,{from:e.posAtDOM(t.startContainer,t.startOffset),to:e.posAtDOM(t.endContainer,t.endOffset),insert:e.state.toText(n)},null),!0}}let n;if(R.chrome&&R.android&&(n=Aa.find(e=>e.inputType==t.inputType))&&(e.observer.delayAndroidKey(n.key,n.keyCode),n.key==`Backspace`||n.key==`Delete`)){let t=window.visualViewport?.height||0;setTimeout(()=>{(window.visualViewport?.height||0)>t+10&&e.hasFocus&&(e.contentDOM.blur(),e.focus())},100)}return R.ios&&t.inputType==`deleteContentForward`&&e.observer.flushSoon(),R.safari&&t.inputType==`insertText`&&e.inputState.composing>=0&&setTimeout(()=>Ha.compositionend(e,t),20),!1};var co=new Set;function lo(e){co.has(e)||(co.add(e),e.addEventListener(`copy`,()=>{}),e.addEventListener(`cut`,()=>{}))}var uo=[`pre-wrap`,`normal`,`pre-line`,`break-spaces`],fo=!1;function po(){fo=!1}var mo=class{constructor(e){this.lineWrapping=e,this.doc=D.empty,this.heightSamples={},this.lineHeight=14,this.charWidth=7,this.textHeight=14,this.lineLength=30}heightForGap(e,t){let n=this.doc.lineAt(t).number-this.doc.lineAt(e).number+1;return this.lineWrapping&&(n+=Math.max(0,Math.ceil((t-e-n*this.lineLength*.5)/this.lineLength))),this.lineHeight*n}heightForLine(e){return this.lineWrapping?(1+Math.max(0,Math.ceil((e-this.lineLength)/Math.max(1,this.lineLength-5))))*this.lineHeight:this.lineHeight}setDoc(e){return this.doc=e,this}mustRefreshForWrapping(e){return uo.indexOf(e)>-1!=this.lineWrapping}mustRefreshForHeights(e){let t=!1;for(let n=0;n<e.length;n++){let r=e[n];r<0?n++:this.heightSamples[Math.floor(r*10)]||(t=!0,this.heightSamples[Math.floor(r*10)]=!0)}return t}refresh(e,t,n,r,i,a){let o=uo.indexOf(e)>-1,s=Math.abs(t-this.lineHeight)>.3||this.lineWrapping!=o;if(this.lineWrapping=o,this.lineHeight=t,this.charWidth=n,this.textHeight=r,this.lineLength=i,s){this.heightSamples={};for(let e=0;e<a.length;e++){let t=a[e];t<0?e++:this.heightSamples[Math.floor(t*10)]=!0}}return s}},ho=class{constructor(e,t){this.from=e,this.heights=t,this.index=0}get more(){return this.index<this.heights.length}},go=class e{constructor(e,t,n,r,i){this.from=e,this.length=t,this.top=n,this.height=r,this._content=i}get type(){return typeof this._content==`number`?kn.Text:Array.isArray(this._content)?this._content:this._content.type}get to(){return this.from+this.length}get bottom(){return this.top+this.height}get widget(){return this._content instanceof Mn?this._content.widget:null}get widgetLineBreaks(){return typeof this._content==`number`?this._content:0}join(t){let n=(Array.isArray(this._content)?this._content:[this]).concat(Array.isArray(t._content)?t._content:[t]);return new e(this.from,this.length+t.length,this.top,this.height+t.height,n)}},G=(function(e){return e[e.ByPos=0]=`ByPos`,e[e.ByHeight=1]=`ByHeight`,e[e.ByPosNoHeight=2]=`ByPosNoHeight`,e})(G||={}),_o=.001,vo=class e{constructor(e,t,n=2){this.length=e,this.height=t,this.flags=n}get outdated(){return(this.flags&2)>0}set outdated(e){this.flags=(e?2:0)|this.flags&-3}setHeight(e){this.height!=e&&(Math.abs(this.height-e)>_o&&(fo=!0),this.height=e)}replace(t,n,r){return e.of(r)}decomposeLeft(e,t){t.push(this)}decomposeRight(e,t){t.push(this)}applyChanges(e,t,n,r){let i=this,a=n.doc;for(let o=r.length-1;o>=0;o--){let{fromA:s,toA:c,fromB:l,toB:u}=r[o],d=i.lineAt(s,G.ByPosNoHeight,n.setDoc(t),0,0),f=d.to>=c?d:i.lineAt(c,G.ByPosNoHeight,n,0,0);for(u+=f.to-c,c=f.to;o>0&&d.from<=r[o-1].toA;)s=r[o-1].fromA,l=r[o-1].fromB,o--,s<d.from&&(d=i.lineAt(s,G.ByPosNoHeight,n,0,0));l+=d.from-s,s=d.from;let p=Do.build(n.setDoc(a),e,l,u);i=yo(i,i.replace(s,c,p))}return i.updateHeight(n,0)}static empty(){return new So(0,0,0)}static of(t){if(t.length==1)return t[0];let n=0,r=t.length,i=0,a=0;for(;;)if(n==r){if(i>a*2){let e=t[n-1];e.break?t.splice(--n,1,e.left,null,e.right):t.splice(--n,1,e.left,e.right),r+=1+e.break,i-=e.size}else if(a>i*2){let e=t[r];e.break?t.splice(r,1,e.left,null,e.right):t.splice(r,1,e.left,e.right),r+=2+e.break,a-=e.size}else break}else if(i<a){let e=t[n++];e&&(i+=e.size)}else{let e=t[--r];e&&(a+=e.size)}let o=!1;return t[n-1]==null?(o=!0,n--):t[n]??(o=!0,r++),new wo(e.of(t.slice(0,n)),o,e.of(t.slice(r)))}};function yo(e,t){return e==t?e:(e.constructor!=t.constructor&&(fo=!0),t)}vo.prototype.size=1;var bo=z.replace({}),xo=class extends vo{constructor(e,t,n){super(e,t),this.deco=n,this.spaceAbove=0}mainBlock(e,t){return new go(t,this.length,e+this.spaceAbove,this.height-this.spaceAbove,this.deco||0)}blockAt(e,t,n,r){return this.spaceAbove&&e<n+this.spaceAbove?new go(r,0,n,this.spaceAbove,bo):this.mainBlock(n,r)}lineAt(e,t,n,r,i){let a=this.mainBlock(r,i);return this.spaceAbove?this.blockAt(0,n,r,i).join(a):a}forEachLine(e,t,n,r,i,a){e<=i+this.length&&t>=i&&a(this.lineAt(0,G.ByPos,n,r,i))}setMeasuredHeight(e){let t=e.heights[e.index++];t<0?(this.spaceAbove=-t,t=e.heights[e.index++]):this.spaceAbove=0,this.setHeight(t)}updateHeight(e,t=0,n=!1,r){return r&&r.from<=t&&r.more&&this.setMeasuredHeight(r),this.outdated=!1,this}toString(){return`block(${this.length})`}},So=class e extends xo{constructor(e,t,n){super(e,t,null),this.collapsed=0,this.widgetHeight=0,this.breaks=0,this.spaceAbove=n}mainBlock(e,t){return new go(t,this.length,e+this.spaceAbove,this.height-this.spaceAbove,this.breaks)}replace(t,n,r){let i=r[0];return r.length==1&&(i instanceof e||i instanceof Co&&i.flags&4)&&Math.abs(this.length-i.length)<10?(i instanceof Co?i=new e(i.length,this.height,this.spaceAbove):i.height=this.height,this.outdated||(i.outdated=!1),i):vo.of(r)}updateHeight(e,t=0,n=!1,r){return r&&r.from<=t&&r.more?this.setMeasuredHeight(r):(n||this.outdated)&&(this.spaceAbove=0,this.setHeight(Math.max(this.widgetHeight,e.heightForLine(this.length-this.collapsed))+this.breaks*e.lineHeight)),this.outdated=!1,this}toString(){return`line(${this.length}${this.collapsed?-this.collapsed:``}${this.widgetHeight?`:`+this.widgetHeight:``})`}},Co=class e extends vo{constructor(e){super(e,0)}heightMetrics(e,t){let n=e.doc.lineAt(t).number,r=e.doc.lineAt(t+this.length).number,i=r-n+1,a,o=0;if(e.lineWrapping){let t=Math.min(this.height,e.lineHeight*i);a=t/i,this.length>i+1&&(o=(this.height-t)/(this.length-i-1))}else a=this.height/i;return{firstLine:n,lastLine:r,perLine:a,perChar:o}}blockAt(e,t,n,r){let{firstLine:i,lastLine:a,perLine:o,perChar:s}=this.heightMetrics(t,r);if(t.lineWrapping){let i=r+(e<t.lineHeight?0:Math.round(Math.max(0,Math.min(1,(e-n)/this.height))*this.length)),a=t.doc.lineAt(i),c=o+a.length*s,l=Math.max(n,e-c/2);return new go(a.from,a.length,l,c,0)}{let r=Math.max(0,Math.min(a-i,Math.floor((e-n)/o))),{from:s,length:c}=t.doc.line(i+r);return new go(s,c,n+o*r,o,0)}}lineAt(e,t,n,r,i){if(t==G.ByHeight)return this.blockAt(e,n,r,i);if(t==G.ByPosNoHeight){let{from:t,to:r}=n.doc.lineAt(e);return new go(t,r-t,0,0,0)}let{firstLine:a,perLine:o,perChar:s}=this.heightMetrics(n,i),c=n.doc.lineAt(e),l=o+c.length*s,u=c.number-a,d=r+o*u+s*(c.from-i-u);return new go(c.from,c.length,Math.max(r,Math.min(d,r+this.height-l)),l,0)}forEachLine(e,t,n,r,i,a){e=Math.max(e,i),t=Math.min(t,i+this.length);let{firstLine:o,perLine:s,perChar:c}=this.heightMetrics(n,i);for(let l=e,u=r;l<=t;){let t=n.doc.lineAt(l);if(l==e){let n=t.number-o;u+=s*n+c*(e-i-n)}let r=s+c*t.length;a(new go(t.from,t.length,u,r,0)),u+=r,l=t.to+1}}replace(t,n,r){let i=this.length-n;if(i>0){let t=r[r.length-1];t instanceof e?r[r.length-1]=new e(t.length+i):r.push(null,new e(i-1))}if(t>0){let n=r[0];n instanceof e?r[0]=new e(t+n.length):r.unshift(new e(t-1),null)}return vo.of(r)}decomposeLeft(t,n){n.push(new e(t-1),null)}decomposeRight(t,n){n.push(null,new e(this.length-t-1))}updateHeight(t,n=0,r=!1,i){let a=n+this.length;if(i&&i.from<=n+this.length&&i.more){let r=[],o=Math.max(n,i.from),s=-1;for(i.from>n&&r.push(new e(i.from-n-1).updateHeight(t,n));o<=a&&i.more;){let e=t.doc.lineAt(o).length;r.length&&r.push(null);let n=i.heights[i.index++],a=0;n<0&&(a=-n,n=i.heights[i.index++]),s==-1?s=n:Math.abs(n-s)>=_o&&(s=-2);let c=new So(e,n,a);c.outdated=!1,r.push(c),o+=e+1}o<=a&&r.push(null,new e(a-o).updateHeight(t,o));let c=vo.of(r);return(s<0||Math.abs(c.height-this.height)>=_o||Math.abs(s-this.heightMetrics(t,n).perLine)>=_o)&&(fo=!0),yo(this,c)}return(r||this.outdated)&&(this.setHeight(t.heightForGap(n,n+this.length)),this.outdated=!1),this}toString(){return`gap(${this.length})`}},wo=class extends vo{constructor(e,t,n){super(e.length+ +!!t+n.length,e.height+n.height,+!!t|(e.outdated||n.outdated?2:0)),this.left=e,this.right=n,this.size=e.size+n.size}get break(){return this.flags&1}blockAt(e,t,n,r){let i=n+this.left.height;return e<i?this.left.blockAt(e,t,n,r):this.right.blockAt(e,t,i,r+this.left.length+this.break)}lineAt(e,t,n,r,i){let a=r+this.left.height,o=i+this.left.length+this.break,s=t==G.ByHeight?e<a:e<o,c=s?this.left.lineAt(e,t,n,r,i):this.right.lineAt(e,t,n,a,o);if(this.break||(s?c.to<o:c.from>o))return c;let l=t==G.ByPosNoHeight?G.ByPosNoHeight:G.ByPos;return s?c.join(this.right.lineAt(o,l,n,a,o)):this.left.lineAt(o,l,n,r,i).join(c)}forEachLine(e,t,n,r,i,a){let o=r+this.left.height,s=i+this.left.length+this.break;if(this.break)e<s&&this.left.forEachLine(e,t,n,r,i,a),t>=s&&this.right.forEachLine(e,t,n,o,s,a);else{let c=this.lineAt(s,G.ByPos,n,r,i);e<c.from&&this.left.forEachLine(e,Math.min(t,c.from-1),n,r,i,a),c.to>=e&&c.from<=t&&a(c),t>c.to&&this.right.forEachLine(Math.max(e,c.to+1),t,n,o,s,a)}}replace(e,t,n){let r=this.left.length+this.break;if(t<r)return this.balanced(this.left.replace(e,t,n),this.right);if(e>this.left.length)return this.balanced(this.left,this.right.replace(e-r,t-r,n));let i=[];e>0&&this.decomposeLeft(e,i);let a=i.length;for(let e of n)i.push(e);if(e>0&&To(i,a-1),t<this.length){let e=i.length;this.decomposeRight(t,i),To(i,e)}return vo.of(i)}decomposeLeft(e,t){let n=this.left.length;if(e<=n)return this.left.decomposeLeft(e,t);t.push(this.left),this.break&&(n++,e>=n&&t.push(null)),e>n&&this.right.decomposeLeft(e-n,t)}decomposeRight(e,t){let n=this.left.length,r=n+this.break;if(e>=r)return this.right.decomposeRight(e-r,t);e<n&&this.left.decomposeRight(e,t),this.break&&e<r&&t.push(null),t.push(this.right)}balanced(e,t){return e.size>2*t.size||t.size>2*e.size?vo.of(this.break?[e,null,t]:[e,t]):(this.left=yo(this.left,e),this.right=yo(this.right,t),this.setHeight(e.height+t.height),this.outdated=e.outdated||t.outdated,this.size=e.size+t.size,this.length=e.length+this.break+t.length,this)}updateHeight(e,t=0,n=!1,r){let{left:i,right:a}=this,o=t+i.length+this.break,s=null;return r&&r.from<=t+i.length&&r.more?s=i=i.updateHeight(e,t,n,r):i.updateHeight(e,t,n),r&&r.from<=o+a.length&&r.more?s=a=a.updateHeight(e,o,n,r):a.updateHeight(e,o,n),s?this.balanced(i,a):(this.height=this.left.height+this.right.height,this.outdated=!1,this)}toString(){return this.left+(this.break?` `:`-`)+this.right}};function To(e,t){let n,r;e[t]==null&&(n=e[t-1])instanceof Co&&(r=e[t+1])instanceof Co&&e.splice(t-1,3,new Co(n.length+1+r.length))}var Eo=5,Do=class e{constructor(e,t){this.pos=e,this.oracle=t,this.nodes=[],this.lineStart=-1,this.lineEnd=-1,this.covering=null,this.writtenTo=e}get isCovered(){return this.covering&&this.nodes[this.nodes.length-1]==this.covering}span(e,t){if(this.lineStart>-1){let e=Math.min(t,this.lineEnd),n=this.nodes[this.nodes.length-1];n instanceof So?n.length+=e-this.pos:(e>this.pos||!this.isCovered)&&this.nodes.push(new So(e-this.pos,-1,0)),this.writtenTo=e,t>e&&(this.nodes.push(null),this.writtenTo++,this.lineStart=-1)}this.pos=t}point(e,t,n){if(e<t||n.heightRelevant){let r=n.widget?n.widget.estimatedHeight:0,i=n.widget?n.widget.lineBreaks:0;r<0&&(r=this.oracle.lineHeight);let a=t-e;n.block?this.addBlock(new xo(a,r,n)):(a||i||r>=Eo)&&this.addLineDeco(r,i,a)}else t>e&&this.span(e,t);this.lineEnd>-1&&this.lineEnd<this.pos&&(this.lineEnd=this.oracle.doc.lineAt(this.pos).to)}enterLine(){if(this.lineStart>-1)return;let{from:e,to:t}=this.oracle.doc.lineAt(this.pos);this.lineStart=e,this.lineEnd=t,this.writtenTo<e&&((this.writtenTo<e-1||this.nodes[this.nodes.length-1]==null)&&this.nodes.push(this.blankContent(this.writtenTo,e-1)),this.nodes.push(null)),this.pos>e&&this.nodes.push(new So(this.pos-e,-1,0)),this.writtenTo=this.pos}blankContent(e,t){let n=new Co(t-e);return this.oracle.doc.lineAt(e).to==t&&(n.flags|=4),n}ensureLine(){this.enterLine();let e=this.nodes.length?this.nodes[this.nodes.length-1]:null;if(e instanceof So)return e;let t=new So(0,-1,0);return this.nodes.push(t),t}addBlock(e){this.enterLine();let t=e.deco;t&&t.startSide>0&&!this.isCovered&&this.ensureLine(),this.nodes.push(e),this.writtenTo=this.pos+=e.length,t&&t.endSide>0&&(this.covering=e)}addLineDeco(e,t,n){let r=this.ensureLine();r.length+=n,r.collapsed+=n,r.widgetHeight=Math.max(r.widgetHeight,e),r.breaks+=t,this.writtenTo=this.pos+=n}finish(e){let t=this.nodes.length==0?null:this.nodes[this.nodes.length-1];this.lineStart>-1&&!(t instanceof So)&&!this.isCovered?this.nodes.push(new So(0,-1,0)):(this.writtenTo<this.pos||t==null)&&this.nodes.push(this.blankContent(this.writtenTo,this.pos));let n=e;for(let e of this.nodes)e instanceof So&&e.updateHeight(this.oracle,n),n+=e?e.length:1;return this.nodes}static build(t,n,r,i){let a=new e(r,t);return I.spans(n,r,i,a,0),a.finish(r)}};function Oo(e,t,n){let r=new ko;return I.compare(e,t,n,r,0),r.changes}var ko=class{constructor(){this.changes=[]}compareRange(){}comparePoint(e,t,n,r){(e<t||n&&n.heightRelevant||r&&r.heightRelevant)&&Fn(e,t,this.changes,5)}};function Ao(e,t){let n=e.getBoundingClientRect(),r=e.ownerDocument,i=r.defaultView||window,a=Math.max(0,n.left),o=Math.min(i.innerWidth,n.right),s=Math.max(0,n.top),c=Math.min(i.innerHeight,n.bottom);for(let t=e.parentNode;t&&t!=r.body;)if(B(t)){let n=window.getComputedStyle(t);if((t.scrollHeight>t.clientHeight||t.scrollWidth>t.clientWidth)&&n.overflow!=`visible`){let n=t.getBoundingClientRect();a=Math.max(a,n.left),o=Math.min(o,n.right),s=Math.max(s,n.top),c=Math.min(t==e.parentNode?i.innerHeight:c,n.bottom)}t=n.position==`absolute`||n.position==`fixed`?t.offsetParent:t.parentNode}else if(t.nodeType==11)t=t.host;else break;return{left:a-n.left,right:Math.max(a,o)-n.left,top:s-(n.top+t),bottom:Math.max(s,c)-(n.top+t)}}function jo(e){let t=e.getBoundingClientRect(),n=e.ownerDocument.defaultView||window;return t.left<n.innerWidth&&t.right>0&&t.top<n.innerHeight&&t.bottom>0}function Mo(e,t){let n=e.getBoundingClientRect();return{left:0,right:n.right-n.left,top:t,bottom:n.bottom-(n.top+t)}}var No=class{constructor(e,t,n,r){this.from=e,this.to=t,this.size=n,this.displaySize=r}static same(e,t){if(e.length!=t.length)return!1;for(let n=0;n<e.length;n++){let r=e[n],i=t[n];if(r.from!=i.from||r.to!=i.to||r.size!=i.size)return!1}return!0}draw(e,t){return z.replace({widget:new Po(this.displaySize*(t?e.scaleY:e.scaleX),t)}).range(this.from,this.to)}},Po=class extends On{constructor(e,t){super(),this.size=e,this.vertical=t}eq(e){return e.size==this.size&&e.vertical==this.vertical}toDOM(){let e=document.createElement(`div`);return this.vertical?e.style.height=this.size+`px`:(e.style.width=this.size+`px`,e.style.height=`2px`,e.style.display=`inline-block`),e}get estimatedHeight(){return this.vertical?this.size:-1}},Fo=class{constructor(e,t){this.view=e,this.state=t,this.pixelViewport={left:0,right:window.innerWidth,top:0,bottom:0},this.inView=!0,this.paddingTop=0,this.paddingBottom=0,this.contentDOMWidth=0,this.contentDOMHeight=0,this.editorHeight=0,this.editorWidth=0,this.scaleX=1,this.scaleY=1,this.scrollOffset=0,this.scrolledToBottom=!1,this.scrollAnchorPos=0,this.scrollAnchorHeight=-1,this.scaler=Vo,this.scrollTarget=null,this.printing=!1,this.mustMeasureContent=!0,this.defaultTextDirection=V.LTR,this.visibleRanges=[],this.mustEnforceCursorAssoc=!1;let n=t.facet(ei).some(e=>typeof e!=`function`&&e.class==`cm-lineWrapping`);this.heightOracle=new mo(n),this.stateDeco=Ho(t),this.heightMap=vo.empty().applyChanges(this.stateDeco,D.empty,this.heightOracle.setDoc(t.doc),[new di(0,0,0,t.doc.length)]);for(let e=0;e<2&&(this.viewport=this.getViewport(0,null),this.updateForViewport());e++);this.updateViewportLines(),this.lineGaps=this.ensureLineGaps([]),this.lineGapDeco=z.set(this.lineGaps.map(e=>e.draw(this,!1))),this.scrollParent=e.scrollDOM,this.computeVisibleRanges()}updateForViewport(){let e=[this.viewport],{main:t}=this.state.selection;for(let n=0;n<=1;n++){let r=n?t.head:t.anchor;if(!e.some(({from:e,to:t})=>r>=e&&r<=t)){let{from:t,to:n}=this.lineBlockAt(r);e.push(new Io(t,n))}}return this.viewports=e.sort((e,t)=>e.from-t.from),this.updateScaler()}updateScaler(){let e=this.scaler;return this.scaler=this.heightMap.height<=7e6?Vo:Uo.get(this.heightOracle,this.heightMap,this.viewports),e.eq(this.scaler)?0:2}updateViewportLines(){this.viewportLines=[],this.heightMap.forEachLine(this.viewport.from,this.viewport.to,this.heightOracle.setDoc(this.state.doc),0,0,e=>{this.viewportLines.push(Wo(e,this.scaler))})}update(e,t=null){this.state=e.state;let n=this.stateDeco;this.stateDeco=Ho(this.state);let r=e.changedRanges,i=di.extendWithRanges(r,Oo(n,this.stateDeco,e?e.changes:Ne.empty(this.state.doc.length))),a=this.heightMap.height,o=this.scrolledToBottom?null:this.scrollAnchorAt(this.scrollOffset);po(),this.heightMap=this.heightMap.applyChanges(this.stateDeco,e.startState.doc,this.heightOracle.setDoc(this.state.doc),i),(this.heightMap.height!=a||fo)&&(e.flags|=2),o?(this.scrollAnchorPos=e.changes.mapPos(o.from,-1),this.scrollAnchorHeight=o.top):(this.scrollAnchorPos=-1,this.scrollAnchorHeight=a);let s=i.length?this.mapViewport(this.viewport,e.changes):this.viewport;(t&&(t.range.head<s.from||t.range.head>s.to)||!this.viewportIsAppropriate(s))&&(s=this.getViewport(0,t));let c=s.from!=this.viewport.from||s.to!=this.viewport.to;this.viewport=s,e.flags|=this.updateForViewport(),(c||!e.changes.empty||e.flags&2)&&this.updateViewportLines(),(this.lineGaps.length||this.viewport.to-this.viewport.from>4e3)&&this.updateLineGaps(this.ensureLineGaps(this.mapLineGaps(this.lineGaps,e.changes))),e.flags|=this.computeVisibleRanges(e.changes),t&&(this.scrollTarget=t),!this.mustEnforceCursorAssoc&&(e.selectionSet||e.focusChanged)&&e.view.lineWrapping&&e.state.selection.main.empty&&e.state.selection.main.assoc&&!e.state.facet(Ur)&&(this.mustEnforceCursorAssoc=!0)}measure(){let{view:e}=this,t=e.contentDOM,n=window.getComputedStyle(t),r=this.heightOracle,i=n.whiteSpace;this.defaultTextDirection=n.direction==`rtl`?V.RTL:V.LTR;let a=this.heightOracle.mustRefreshForWrapping(i)||this.mustMeasureContent===`refresh`,o=t.getBoundingClientRect(),s=a||this.mustMeasureContent||this.contentDOMHeight!=o.height;this.contentDOMHeight=o.height,this.mustMeasureContent=!1;let c=0,l=0;if(o.width&&o.height){let{scaleX:e,scaleY:n}=Yn(t,o);(e>.005&&Math.abs(this.scaleX-e)>.005||n>.005&&Math.abs(this.scaleY-n)>.005)&&(this.scaleX=e,this.scaleY=n,c|=16,a=s=!0)}let u=(parseInt(n.paddingTop)||0)*this.scaleY,d=(parseInt(n.paddingBottom)||0)*this.scaleY;(this.paddingTop!=u||this.paddingBottom!=d)&&(this.paddingTop=u,this.paddingBottom=d,c|=18),this.editorWidth!=e.scrollDOM.clientWidth&&(r.lineWrapping&&(s=!0),this.editorWidth=e.scrollDOM.clientWidth,c|=16);let f=Zn(this.view.contentDOM,!1).y;f!=this.scrollParent&&(this.scrollParent=f,this.scrollAnchorHeight=-1,this.scrollOffset=0);let p=this.getScrollOffset();this.scrollOffset!=p&&(this.scrollAnchorHeight=-1,this.scrollOffset=p),this.scrolledToBottom=cr(this.scrollParent||e.win);let m=(this.printing?Mo:Ao)(t,this.paddingTop),h=m.top-this.pixelViewport.top,g=m.bottom-this.pixelViewport.bottom;this.pixelViewport=m;let _=this.pixelViewport.bottom>this.pixelViewport.top&&this.pixelViewport.right>this.pixelViewport.left;if(_!=this.inView&&(this.inView=_,_&&(s=!0)),!this.inView&&!this.scrollTarget&&!jo(e.dom))return 0;let v=o.width;if((this.contentDOMWidth!=v||this.editorHeight!=e.scrollDOM.clientHeight)&&(this.contentDOMWidth=o.width,this.editorHeight=e.scrollDOM.clientHeight,c|=16),s){let t=e.docView.measureVisibleLineHeights(this.viewport);if(r.mustRefreshForHeights(t)&&(a=!0),a||r.lineWrapping&&Math.abs(v-this.contentDOMWidth)>r.charWidth){let{lineHeight:n,charWidth:o,textHeight:s}=e.docView.measureTextSize();a=n>0&&r.refresh(i,n,o,s,Math.max(5,v/o),t),a&&(e.docView.minWidth=0,c|=16)}h>0&&g>0?l=Math.max(h,g):h<0&&g<0&&(l=Math.min(h,g)),po();for(let n of this.viewports){let i=n.from==this.viewport.from?t:e.docView.measureVisibleLineHeights(n);this.heightMap=(a?vo.empty().applyChanges(this.stateDeco,D.empty,this.heightOracle,[new di(0,0,0,e.state.doc.length)]):this.heightMap).updateHeight(r,0,a,new ho(n.from,i))}fo&&(c|=2)}let y=!this.viewportIsAppropriate(this.viewport,l)||this.scrollTarget&&(this.scrollTarget.range.head<this.viewport.from||this.scrollTarget.range.head>this.viewport.to);return y&&(c&2&&(c|=this.updateScaler()),this.viewport=this.getViewport(l,this.scrollTarget),c|=this.updateForViewport()),(c&2||y)&&this.updateViewportLines(),(this.lineGaps.length||this.viewport.to-this.viewport.from>4e3)&&this.updateLineGaps(this.ensureLineGaps(a?[]:this.lineGaps,e)),c|=this.computeVisibleRanges(),this.mustEnforceCursorAssoc&&(this.mustEnforceCursorAssoc=!1,e.docView.enforceCursorAssoc()),c}get visibleTop(){return this.scaler.fromDOM(this.pixelViewport.top)}get visibleBottom(){return this.scaler.fromDOM(this.pixelViewport.bottom)}getViewport(e,t){let n=.5-Math.max(-.5,Math.min(.5,e/1e3/2)),r=this.heightMap,i=this.heightOracle,{visibleTop:a,visibleBottom:o}=this,s=new Io(r.lineAt(a-n*1e3,G.ByHeight,i,0,0).from,r.lineAt(o+(1-n)*1e3,G.ByHeight,i,0,0).to);if(t){let{head:e}=t.range;if(e<s.from||e>s.to){let n=Math.min(this.editorHeight,this.pixelViewport.bottom-this.pixelViewport.top),a=r.lineAt(e,G.ByPos,i,0,0),o;o=t.y==`center`?(a.top+a.bottom)/2-n/2:t.y==`start`||t.y==`nearest`&&e<s.from?a.top:a.bottom-n,s=new Io(r.lineAt(o-500,G.ByHeight,i,0,0).from,r.lineAt(o+n+500,G.ByHeight,i,0,0).to)}}return s}mapViewport(e,t){let n=t.mapPos(e.from,-1),r=t.mapPos(e.to,1);return new Io(this.heightMap.lineAt(n,G.ByPos,this.heightOracle,0,0).from,this.heightMap.lineAt(r,G.ByPos,this.heightOracle,0,0).to)}viewportIsAppropriate({from:e,to:t},n=0){if(!this.inView)return!0;let{top:r}=this.heightMap.lineAt(e,G.ByPos,this.heightOracle,0,0),{bottom:i}=this.heightMap.lineAt(t,G.ByPos,this.heightOracle,0,0),{visibleTop:a,visibleBottom:o}=this;return(e==0||r<=a-Math.max(10,Math.min(-n,250)))&&(t==this.state.doc.length||i>=o+Math.max(10,Math.min(n,250)))&&r>a-2e3&&i<o+2e3}mapLineGaps(e,t){if(!e.length||t.empty)return e;let n=[];for(let r of e)t.touchesRange(r.from,r.to)||n.push(new No(t.mapPos(r.from),t.mapPos(r.to),r.size,r.displaySize));return n}ensureLineGaps(e,t){let n=this.heightOracle.lineWrapping,r=n?1e4:2e3,i=r>>1,a=r<<1;if(this.defaultTextDirection!=V.LTR&&!n)return[];let o=[],s=(r,a,c,l)=>{if(a-r<i)return;let u=this.state.selection.main,d=[u.from];u.empty||d.push(u.to);for(let e of d)if(e>r&&e<a){s(r,e-10,c,l),s(e+10,a,c,l);return}let f=Bo(e,e=>e.from>=c.from&&e.to<=c.to&&Math.abs(e.from-r)<i&&Math.abs(e.to-a)<i&&!d.some(t=>e.from<t&&e.to>t));if(!f){if(a<c.to&&t&&n&&t.visibleRanges.some(e=>e.from<=a&&e.to>=a)){let e=t.moveToLineBoundary(j.cursor(a),!1,!0).head;e>r&&(a=e)}let e=this.gapSize(c,r,a,l);f=new No(r,a,e,n||e<2e6?e:2e6)}o.push(f)},c=t=>{if(t.length<a||t.type!=kn.Text)return;let i=Lo(t.from,t.to,this.stateDeco);if(i.total<a)return;let o=this.scrollTarget?this.scrollTarget.range.head:null,c,l;if(n){let e=r/this.heightOracle.lineLength*this.heightOracle.lineHeight,n,a;if(o!=null){let r=zo(i,o),s=((this.visibleBottom-this.visibleTop)/2+e)/t.height;n=r-s,a=r+s}else n=(this.visibleTop-t.top-e)/t.height,a=(this.visibleBottom-t.top+e)/t.height;c=Ro(i,n),l=Ro(i,a)}else{let n=i.total*this.heightOracle.charWidth,a=r*this.heightOracle.charWidth,s=0;if(n>2e6)for(let n of e)n.from>=t.from&&n.from<t.to&&n.size!=n.displaySize&&n.from*this.heightOracle.charWidth+s<this.pixelViewport.left&&(s=n.size-n.displaySize);let u=this.pixelViewport.left+s,d=this.pixelViewport.right+s,f,p;if(o!=null){let e=zo(i,o),t=((d-u)/2+a)/n;f=e-t,p=e+t}else f=(u-a)/n,p=(d+a)/n;c=Ro(i,f),l=Ro(i,p)}c>t.from&&s(t.from,c,t,i),l<t.to&&s(l,t.to,t,i)};for(let e of this.viewportLines)Array.isArray(e.type)?e.type.forEach(c):c(e);return o}gapSize(e,t,n,r){let i=zo(r,n)-zo(r,t);return this.heightOracle.lineWrapping?e.height*i:r.total*this.heightOracle.charWidth*i}updateLineGaps(e){No.same(e,this.lineGaps)||(this.lineGaps=e,this.lineGapDeco=z.set(e.map(e=>e.draw(this,this.heightOracle.lineWrapping))))}computeVisibleRanges(e){let t=this.stateDeco;this.lineGaps.length&&(t=t.concat(this.lineGapDeco));let n=[];I.spans(t,this.viewport.from,this.viewport.to,{span(e,t){n.push({from:e,to:t})},point(){}},20);let r=0;if(n.length!=this.visibleRanges.length)r=12;else for(let t=0;t<n.length&&!(r&8);t++){let i=this.visibleRanges[t],a=n[t];(i.from!=a.from||i.to!=a.to)&&(r|=4,e&&e.mapPos(i.from,-1)==a.from&&e.mapPos(i.to,1)==a.to||(r|=8))}return this.visibleRanges=n,r}lineBlockAt(e){return e>=this.viewport.from&&e<=this.viewport.to&&this.viewportLines.find(t=>t.from<=e&&t.to>=e)||Wo(this.heightMap.lineAt(e,G.ByPos,this.heightOracle,0,0),this.scaler)}lineBlockAtHeight(e){return e>=this.viewportLines[0].top&&e<=this.viewportLines[this.viewportLines.length-1].bottom&&this.viewportLines.find(t=>t.top<=e&&t.bottom>=e)||Wo(this.heightMap.lineAt(this.scaler.fromDOM(e),G.ByHeight,this.heightOracle,0,0),this.scaler)}getScrollOffset(){return this.scrollParent==this.view.scrollDOM?this.scrollParent.scrollTop*this.scaleY:(this.scrollParent?this.scrollParent.getBoundingClientRect().top:0)-this.view.contentDOM.getBoundingClientRect().top}scrollAnchorAt(e){let t=this.lineBlockAtHeight(e+12-this.paddingTop);return t.from>=this.viewport.from||this.viewportLines[0].top-e>200?t:this.viewportLines[0]}elementAtHeight(e){return Wo(this.heightMap.blockAt(this.scaler.fromDOM(e),this.heightOracle,0,0),this.scaler)}get docHeight(){return this.scaler.toDOM(this.heightMap.height)}get contentHeight(){return this.docHeight+this.paddingTop+this.paddingBottom}},Io=class{constructor(e,t){this.from=e,this.to=t}};function Lo(e,t,n){let r=[],i=e,a=0;return I.spans(n,e,t,{span(){},point(e,t){e>i&&(r.push({from:i,to:e}),a+=e-i),i=t}},20),i<t&&(r.push({from:i,to:t}),a+=t-i),{total:a,ranges:r}}function Ro({total:e,ranges:t},n){if(n<=0)return t[0].from;if(n>=1)return t[t.length-1].to;let r=Math.floor(e*n);for(let e=0;;e++){let{from:n,to:i}=t[e],a=i-n;if(r<=a)return n+r;r-=a}}function zo(e,t){let n=0;for(let{from:r,to:i}of e.ranges){if(t<=i){n+=t-r;break}n+=i-r}return n/e.total}function Bo(e,t){for(let n of e)if(t(n))return n}var Vo={toDOM(e){return e},fromDOM(e){return e},scale:1,eq(e){return e==this}};function Ho(e){let t=e.facet(ti).filter(e=>typeof e!=`function`),n=e.facet(ri).filter(e=>typeof e!=`function`);return n.length&&t.push(I.join(n)),t}var Uo=class e{constructor(e,t){this.scale=e,this.viewports=t}static get(t,n,r){let i=0,a=0,o=0,s=r.map(({from:e,to:r})=>{let a=n.lineAt(e,G.ByPos,t,0,0).top,o=n.lineAt(r,G.ByPos,t,0,0).bottom;return i+=o-a,{from:e,to:r,top:a,bottom:o,domTop:0,domBottom:0}});if(n.height-i<10)return Vo;let c=(7e6-i)/(n.height-i);for(let e of s)e.domTop=o+(e.top-a)*c,o=e.domBottom=e.domTop+(e.bottom-e.top),a=e.bottom;return new e(c,s)}toDOM(e){for(let t=0,n=0,r=0;;t++){let i=t<this.viewports.length?this.viewports[t]:null;if(!i||e<i.top)return r+(e-n)*this.scale;if(e<=i.bottom)return i.domTop+(e-i.top);n=i.bottom,r=i.domBottom}}fromDOM(e){for(let t=0,n=0,r=0;;t++){let i=t<this.viewports.length?this.viewports[t]:null;if(!i||e<i.domTop)return n+(e-r)/this.scale;if(e<=i.domBottom)return i.top+(e-i.domTop);n=i.bottom,r=i.domBottom}}eq(t){return t instanceof e&&this.scale==t.scale&&this.viewports.length==t.viewports.length&&this.viewports.every((e,n)=>e.from==t.viewports[n].from&&e.to==t.viewports[n].to)}};function Wo(e,t){if(t.scale==1)return e;let n=t.toDOM(e.top),r=t.toDOM(e.bottom);return new go(e.from,e.length,n,r-n,Array.isArray(e._content)?e._content.map(e=>Wo(e,t)):e._content)}var Go=M.define({combine:e=>e.join(` `)}),Ko=M.define({combine:e=>e.indexOf(!0)>-1}),qo=$t.newName(),Jo=$t.newName(),Yo=$t.newName(),Xo={"&light":`.`+Jo,"&dark":`.`+Yo};function Zo(e,t,n){return new $t(t,{finish(t){return/&/.test(t)?t.replace(/&\w*/,t=>{if(t==`&`)return e;if(!n||!n[t])throw RangeError(`Unsupported selector: ${t}`);return n[t]}):e+` `+t}})}var Qo=Zo(`.`+qo,{"&":{position:`relative !important`,boxSizing:`border-box`,"&.cm-focused":{outline:`1px dotted #212121`},display:`flex !important`,flexDirection:`column`},".cm-scroller":{display:`flex !important`,alignItems:`flex-start !important`,fontFamily:`monospace`,lineHeight:1.4,height:`100%`,overflowX:`auto`,position:`relative`,zIndex:0,overflowAnchor:`none`},".cm-content":{margin:0,flexGrow:2,flexShrink:0,display:`block`,whiteSpace:`pre`,wordWrap:`normal`,boxSizing:`border-box`,minHeight:`100%`,padding:`4px 0`,outline:`none`,"&[contenteditable=true]":{WebkitUserModify:`read-write-plaintext-only`}},".cm-lineWrapping":{whiteSpace_fallback:`pre-wrap`,whiteSpace:`break-spaces`,wordBreak:`break-word`,overflowWrap:`anywhere`,flexShrink:1},"&light .cm-content":{caretColor:`black`},"&dark .cm-content":{caretColor:`white`},".cm-line":{display:`block`,padding:`0 2px 0 6px`},".cm-layer":{userSelect:`none`,position:`absolute`,left:0,top:0,contain:`size style`,"& > *":{position:`absolute`}},"&light .cm-selectionBackground":{background:`#d9d9d9`},"&dark .cm-selectionBackground":{background:`#222`},"&light.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground":{background:`#d7d4f0`},"&dark.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground":{background:`#233`},".cm-cursorLayer":{pointerEvents:`none`},"&.cm-focused > .cm-scroller > .cm-cursorLayer":{animation:`steps(1) cm-blink 1.2s infinite`},"@keyframes cm-blink":{"0%":{},"50%":{opacity:0},"100%":{}},"@keyframes cm-blink2":{"0%":{},"50%":{opacity:0},"100%":{}},".cm-cursor, .cm-dropCursor":{borderLeft:`1.2px solid black`,marginLeft:`-0.6px`,pointerEvents:`none`},".cm-cursor":{display:`none`},"&dark .cm-cursor":{borderLeftColor:`#ddd`},".cm-selectionHandle":{backgroundColor:`currentColor`,width:`1.5px`},".cm-selectionHandle-start::before, .cm-selectionHandle-end::before":{content:`""`,backgroundColor:`inherit`,borderRadius:`50%`,width:`8px`,height:`8px`,position:`absolute`,left:`-3.25px`},".cm-selectionHandle-start::before":{top:`-8px`},".cm-selectionHandle-end::before":{bottom:`-8px`},".cm-dropCursor":{position:`absolute`},"&.cm-focused > .cm-scroller > .cm-cursorLayer .cm-cursor":{display:`block`},".cm-iso":{unicodeBidi:`isolate`},".cm-announced":{position:`fixed`,top:`-10000px`},"@media print":{".cm-announced":{display:`none`}},"&light .cm-activeLine":{backgroundColor:`#cceeff44`},"&dark .cm-activeLine":{backgroundColor:`#99eeff33`},"&light .cm-specialChar":{color:`red`},"&dark .cm-specialChar":{color:`#f78`},".cm-gutters":{flexShrink:0,display:`flex`,height:`100%`,boxSizing:`border-box`,zIndex:200},".cm-gutters-before":{insetInlineStart:0},".cm-gutters-after":{insetInlineEnd:0},"&light .cm-gutters":{backgroundColor:`#f5f5f5`,color:`#6c6c6c`,border:`0px solid #ddd`,"&.cm-gutters-before":{borderRightWidth:`1px`},"&.cm-gutters-after":{borderLeftWidth:`1px`}},"&dark .cm-gutters":{backgroundColor:`#333338`,color:`#ccc`},".cm-gutter":{display:`flex !important`,flexDirection:`column`,flexShrink:0,boxSizing:`border-box`,minHeight:`100%`,overflow:`hidden`},".cm-gutterElement":{boxSizing:`border-box`},".cm-lineNumbers .cm-gutterElement":{padding:`0 3px 0 5px`,minWidth:`20px`,textAlign:`right`,whiteSpace:`nowrap`},"&light .cm-activeLineGutter":{backgroundColor:`#e2f2ff`},"&dark .cm-activeLineGutter":{backgroundColor:`#222227`},".cm-panels":{boxSizing:`border-box`,position:`sticky`,left:0,right:0,zIndex:300},"&light .cm-panels":{backgroundColor:`#f5f5f5`,color:`black`},".cm-panels-top":{top:`0`},".cm-panels-bottom":{bottom:`0`},"&light .cm-panels-top":{borderBottom:`1px solid #ddd`},"&light .cm-panels-bottom":{borderTop:`1px solid #ddd`},"&dark .cm-panels":{backgroundColor:`#333338`,color:`white`},".cm-dialog":{padding:`2px 19px 4px 6px`,position:`relative`,"& label":{fontSize:`80%`}},".cm-dialog-close":{position:`absolute`,top:`3px`,right:`4px`,backgroundColor:`inherit`,border:`none`,font:`inherit`,fontSize:`14px`,padding:`0`},".cm-tab":{display:`inline-block`,overflow:`hidden`,verticalAlign:`bottom`},".cm-widgetBuffer":{verticalAlign:`baseline`,height:`1em`,width:0,display:`inline`},".cm-placeholder":{color:`#888`,display:`inline-block`,verticalAlign:`top`,userSelect:`none`},".cm-highlightSpace":{background:`radial-gradient(circle at 50% 55%, #aaa 20%, transparent 0) no-repeat`,backgroundSize:`.4em`,backgroundPosition:`calc(min(50%, 0px)) center`},".cm-highlightTab":{backgroundImage:`url('data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="200" height="20"><path stroke="%23888" stroke-width="1" fill="none" d="M1 10H196L190 5M190 15L196 10M197 4L197 16"/></svg>')`,backgroundSize:`auto 100%`,backgroundPosition:`right 90%`,backgroundRepeat:`no-repeat`},".cm-trailingSpace":{backgroundColor:`#ff332255`},".cm-button":{verticalAlign:`middle`,color:`inherit`,fontSize:`70%`,padding:`.2em 1em`,borderRadius:`1px`},"&light .cm-button":{backgroundImage:`linear-gradient(#eff1f5, #d9d9df)`,border:`1px solid #888`,"&:active":{backgroundImage:`linear-gradient(#b4b4b4, #d0d3d6)`}},"&dark .cm-button":{backgroundImage:`linear-gradient(#393939, #111)`,border:`1px solid #888`,"&:active":{backgroundImage:`linear-gradient(#111, #333)`}},".cm-textfield":{verticalAlign:`middle`,color:`inherit`,fontSize:`70%`,border:`1px solid silver`,padding:`.2em .5em`},"&light .cm-textfield":{backgroundColor:`white`},"&dark .cm-textfield":{border:`1px solid #555`,backgroundColor:`inherit`}},Xo),$o={childList:!0,characterData:!0,subtree:!0,attributes:!0,characterDataOldValue:!0},es=R.ie&&R.ie_version<=11,ts=class{constructor(e){this.view=e,this.active=!1,this.editContext=null,this.selectionRange=new Qn,this.selectionChanged=!1,this.delayedFlush=-1,this.resizeTimeout=-1,this.queue=[],this.delayedAndroidKey=null,this.flushingAndroidKey=-1,this.lastChange=0,this.scrollTargets=[],this.intersection=null,this.resizeScroll=null,this.intersecting=!1,this.gapIntersection=null,this.gaps=[],this.printQuery=null,this.parentCheck=-1,this.dom=e.contentDOM,this.observer=new MutationObserver(t=>{for(let e of t)this.queue.push(e);(R.ie&&R.ie_version<=11||R.ios&&e.composing)&&t.some(e=>e.type==`childList`&&e.removedNodes.length||e.type==`characterData`&&e.oldValue.length>e.target.nodeValue.length)?this.flushSoon():this.flush()}),window.EditContext&&R.android&&e.constructor.EDIT_CONTEXT!==!1&&!(R.chrome&&R.chrome_version<126)&&(this.editContext=new as(e),e.state.facet(Yr)&&(e.contentDOM.editContext=this.editContext.editContext)),es&&(this.onCharData=e=>{this.queue.push({target:e.target,type:`characterData`,oldValue:e.prevValue}),this.flushSoon()}),this.onSelectionChange=this.onSelectionChange.bind(this),this.onResize=this.onResize.bind(this),this.onPrint=this.onPrint.bind(this),this.onScroll=this.onScroll.bind(this),window.matchMedia&&(this.printQuery=window.matchMedia(`print`)),typeof ResizeObserver==`function`&&(this.resizeScroll=new ResizeObserver(()=>{this.view.docView?.lastUpdate<Date.now()-75&&this.onResize()}),this.resizeScroll.observe(e.scrollDOM)),this.addWindowListeners(this.win=e.win),this.start(),typeof IntersectionObserver==`function`&&(this.intersection=new IntersectionObserver(e=>{this.parentCheck<0&&(this.parentCheck=setTimeout(this.listenForScroll.bind(this),1e3)),e.length>0&&e[e.length-1].intersectionRatio>0!=this.intersecting&&(this.intersecting=!this.intersecting,this.intersecting!=this.view.inView&&this.onScrollChanged(document.createEvent(`Event`)))},{threshold:[0,.001]}),this.intersection.observe(this.dom),this.gapIntersection=new IntersectionObserver(e=>{e.length>0&&e[e.length-1].intersectionRatio>0&&this.onScrollChanged(document.createEvent(`Event`))},{})),this.listenForScroll(),this.readSelectionRange()}onScrollChanged(e){this.view.inputState.runHandlers(`scroll`,e),this.intersecting&&this.view.measure()}onScroll(e){this.intersecting&&this.flush(this.pendingRecords().length>0),this.editContext&&this.view.requestMeasure(this.editContext.measureReq),this.onScrollChanged(e)}onResize(){this.resizeTimeout<0&&(this.resizeTimeout=setTimeout(()=>{this.resizeTimeout=-1,this.view.requestMeasure()},50))}onPrint(e){(e.type!=`change`&&e.type||e.matches)&&(this.view.viewState.printing=!0,this.view.measure(),setTimeout(()=>{this.view.viewState.printing=!1,this.view.requestMeasure()},500))}updateGaps(e){if(this.gapIntersection&&(e.length!=this.gaps.length||this.gaps.some((t,n)=>t!=e[n]))){this.gapIntersection.disconnect();for(let t of e)this.gapIntersection.observe(t);this.gaps=e}}onSelectionChange(e){let t=this.selectionChanged;if(!this.readSelectionRange()||this.delayedAndroidKey)return;let{view:n}=this,r=this.selectionRange;if(n.state.facet(Yr)?n.root.activeElement!=this.dom:!Bn(this.dom,r))return;let i=r.anchorNode&&n.docView.tile.nearest(r.anchorNode);i&&i.isWidget()&&i.widget.ignoreEvent(e)?t||(this.selectionChanged=!1):(R.ie&&R.ie_version<=11||R.android&&R.chrome)&&!n.state.selection.main.empty&&r.focusNode&&Hn(r.focusNode,r.focusOffset,r.anchorNode,r.anchorOffset)?this.flushSoon():this.flush(!1)}readSelectionRange(){let{view:e}=this,t=Rn(e.root);if(!t)return!1;let n=R.safari&&e.root.nodeType==11&&e.root.activeElement==this.dom&&is(this.view,t)||t;if(!n||this.selectionRange.eq(n))return!1;let r=Bn(this.dom,n);return r&&!this.selectionChanged&&e.inputState.lastFocusTime>Date.now()-200&&e.inputState.lastTouchTime<Date.now()-300&&sr(this.dom,n)?(this.view.inputState.lastFocusTime=0,e.docView.updateSelection(),!1):(this.selectionRange.setRange(n),r&&(this.selectionChanged=!0),!0)}setSelectionRange(e,t){this.selectionRange.set(e.node,e.offset,t.node,t.offset),this.selectionChanged=!1}clearSelectionRange(){this.selectionRange.set(null,0,null,0)}listenForScroll(){this.parentCheck=-1;let e=0,t=null;for(let n=this.dom;n;)if(B(n))!t&&e<this.scrollTargets.length&&this.scrollTargets[e]==n?e++:t||=this.scrollTargets.slice(0,e),t&&t.push(n),n=n.assignedSlot||n.parentNode;else if(n.nodeType==11)n=n.host;else break;if(e<this.scrollTargets.length&&!t&&(t=this.scrollTargets.slice(0,e)),t){for(let e of this.scrollTargets)e.removeEventListener(`scroll`,this.onScroll);for(let e of this.scrollTargets=t)e.addEventListener(`scroll`,this.onScroll)}}ignore(e){if(!this.active)return e();try{return this.stop(),e()}finally{this.start(),this.clear()}}start(){this.active||=(this.observer.observe(this.dom,$o),es&&this.dom.addEventListener(`DOMCharacterDataModified`,this.onCharData),!0)}stop(){this.active&&(this.active=!1,this.observer.disconnect(),es&&this.dom.removeEventListener(`DOMCharacterDataModified`,this.onCharData))}clear(){this.processRecords(),this.queue.length=0,this.selectionChanged=!1}delayAndroidKey(e,t){if(!this.delayedAndroidKey){let e=()=>{let e=this.delayedAndroidKey;e&&(this.clearDelayedAndroidKey(),this.view.inputState.lastKeyCode=e.keyCode,this.view.inputState.lastKeyTime=Date.now(),!this.flush()&&e.force&&ar(this.dom,e.key,e.keyCode))};this.flushingAndroidKey=this.view.win.requestAnimationFrame(e)}this.delayedAndroidKey&&e!=`Enter`||(this.delayedAndroidKey={key:e,keyCode:t,force:this.lastChange<Date.now()-50||!!this.delayedAndroidKey?.force})}clearDelayedAndroidKey(){this.win.cancelAnimationFrame(this.flushingAndroidKey),this.delayedAndroidKey=null,this.flushingAndroidKey=-1}flushSoon(){this.delayedFlush<0&&(this.delayedFlush=this.view.win.requestAnimationFrame(()=>{this.delayedFlush=-1,this.flush()}))}forceFlush(){this.delayedFlush>=0&&(this.view.win.cancelAnimationFrame(this.delayedFlush),this.delayedFlush=-1),this.flush()}pendingRecords(){for(let e of this.observer.takeRecords())this.queue.push(e);return this.queue}processRecords(){let e=this.pendingRecords();e.length&&(this.queue=[]);let t=-1,n=-1,r=!1;for(let i of e){let e=this.readMutation(i);e&&(e.typeOver&&(r=!0),t==-1?{from:t,to:n}=e:(t=Math.min(e.from,t),n=Math.max(e.to,n)))}return{from:t,to:n,typeOver:r}}readChange(){let{from:e,to:t,typeOver:n}=this.processRecords(),r=this.selectionChanged&&Bn(this.dom,this.selectionRange);if(e<0&&!r)return null;e>-1&&(this.lastChange=Date.now()),this.view.inputState.lastFocusTime=0,this.selectionChanged=!1;let i=new _a(this.view,e,t,n);return this.view.docView.domChanged={newSel:i.newSel?i.newSel.main:null},i}flush(e=!0){if(this.delayedFlush>=0||this.delayedAndroidKey)return!1;e&&this.readSelectionRange();let t=this.selectionChanged,n=this.readChange();if(!n)return this.view.requestMeasure(),!1;let r=this.view.state,i=ya(this.view,n);return this.view.state==r&&(n.domChanged||t)&&this.view.update([]),i}readMutation(e){let t=this.view.docView.tile.nearest(e.target);if(!t||t.isWidget())return null;if(t.markDirty(e.type==`attributes`),e.type==`childList`){let n=ns(t,e.previousSibling||e.target.previousSibling,-1),r=ns(t,e.nextSibling||e.target.nextSibling,1);return{from:n?t.posAfter(n):t.posAtStart,to:r?t.posBefore(r):t.posAtEnd,typeOver:!1}}return e.type==`characterData`?{from:t.posAtStart,to:t.posAtEnd,typeOver:e.target.nodeValue==e.oldValue}:null}setWindow(e){e!=this.win&&(this.removeWindowListeners(this.win),this.win=e,this.addWindowListeners(this.win))}addWindowListeners(e){e.addEventListener(`resize`,this.onResize),this.printQuery?this.printQuery.addEventListener?this.printQuery.addEventListener(`change`,this.onPrint):this.printQuery.addListener(this.onPrint):e.addEventListener(`beforeprint`,this.onPrint),e.addEventListener(`scroll`,this.onScroll),e.document.addEventListener(`selectionchange`,this.onSelectionChange)}removeWindowListeners(e){e.removeEventListener(`scroll`,this.onScroll),e.removeEventListener(`resize`,this.onResize),this.printQuery?this.printQuery.removeEventListener?this.printQuery.removeEventListener(`change`,this.onPrint):this.printQuery.removeListener(this.onPrint):e.removeEventListener(`beforeprint`,this.onPrint),e.document.removeEventListener(`selectionchange`,this.onSelectionChange)}update(e){this.editContext&&(this.editContext.update(e),e.startState.facet(Yr)!=e.state.facet(Yr)&&(e.view.contentDOM.editContext=e.state.facet(Yr)?this.editContext.editContext:null))}destroy(){var e,t,n;this.stop(),(e=this.intersection)==null||e.disconnect(),(t=this.gapIntersection)==null||t.disconnect(),(n=this.resizeScroll)==null||n.disconnect();for(let e of this.scrollTargets)e.removeEventListener(`scroll`,this.onScroll);this.removeWindowListeners(this.win),clearTimeout(this.parentCheck),clearTimeout(this.resizeTimeout),this.win.cancelAnimationFrame(this.delayedFlush),this.win.cancelAnimationFrame(this.flushingAndroidKey),this.editContext&&(this.view.contentDOM.editContext=null,this.editContext.destroy())}};function ns(e,t,n){for(;t;){let r=W.get(t);if(r&&r.parent==e)return r;let i=t.parentNode;t=i==e.dom?n>0?t.nextSibling:t.previousSibling:i}return null}function rs(e,t){let n=t.startContainer,r=t.startOffset,i=t.endContainer,a=t.endOffset,o=e.docView.domAtPos(e.state.selection.main.anchor,1);return Hn(o.node,o.offset,i,a)&&([n,r,i,a]=[i,a,n,r]),{anchorNode:n,anchorOffset:r,focusNode:i,focusOffset:a}}function is(e,t){if(t.getComposedRanges){let n=t.getComposedRanges(e.root)[0];if(n)return rs(e,n)}let n=null;function r(e){e.preventDefault(),e.stopImmediatePropagation(),n=e.getTargetRanges()[0]}return e.contentDOM.addEventListener(`beforeinput`,r,!0),e.dom.ownerDocument.execCommand(`indent`),e.contentDOM.removeEventListener(`beforeinput`,r,!0),n?rs(e,n):null}var as=class{constructor(e){this.from=0,this.to=0,this.pendingContextChange=null,this.handlers=Object.create(null),this.composing=null,this.resetRange(e.state);let t=this.editContext=new window.EditContext({text:e.state.doc.sliceString(this.from,this.to),selectionStart:this.toContextPos(Math.max(this.from,Math.min(this.to,e.state.selection.main.anchor))),selectionEnd:this.toContextPos(e.state.selection.main.head)});this.handlers.textupdate=n=>{let r=e.state.selection.main,{anchor:i,head:a}=r,o=this.toEditorPos(n.updateRangeStart),s=this.toEditorPos(n.updateRangeEnd);e.inputState.composing>=0&&!this.composing&&(this.composing={contextBase:n.updateRangeStart,editorBase:o,drifted:!1});let c=s-o>n.text.length;o==this.from&&i<this.from?o=i:s==this.to&&i>this.to&&(s=i);let l=Sa(e.state.sliceDoc(o,s),n.text,(c?r.from:r.to)-o,c?`end`:null);if(!l){let t=j.single(this.toEditorPos(n.selectionStart),this.toEditorPos(n.selectionEnd));Ta(t,r)||e.dispatch({selection:t,userEvent:`select`});return}let u={from:l.from+o,to:l.toA+o,insert:D.of(n.text.slice(l.from,l.toB).split(`
`))};if((R.mac||R.android)&&u.from==a-1&&/^\. ?$/.test(n.text)&&e.contentDOM.getAttribute(`autocorrect`)==`off`&&(u={from:o,to:s,insert:D.of([n.text.replace(`.`,` `)])}),this.pendingContextChange=u,!e.state.readOnly){let t=this.to-this.from+(u.to-u.from+u.insert.length);ba(e,u,j.single(this.toEditorPos(n.selectionStart,t),this.toEditorPos(n.selectionEnd,t)))}this.pendingContextChange&&(this.revertPending(e.state),this.setSelection(e.state)),u.from<u.to&&!u.insert.length&&e.inputState.composing>=0&&!/[\\p{Alphabetic}\\p{Number}_]/.test(t.text.slice(Math.max(0,n.updateRangeStart-1),Math.min(t.text.length,n.updateRangeStart+1)))&&this.handlers.compositionend(n)},this.handlers.characterboundsupdate=n=>{let r=[],i=null;for(let t=this.toEditorPos(n.rangeStart),a=this.toEditorPos(n.rangeEnd);t<a;t++){let n=e.coordsForChar(t);i=n&&new DOMRect(n.left,n.top,n.right-n.left,n.bottom-n.top)||i||new DOMRect,r.push(i)}t.updateCharacterBounds(n.rangeStart,r)},this.handlers.textformatupdate=t=>{let n=[];for(let e of t.getTextFormats()){let t=e.underlineStyle,r=e.underlineThickness;if(!/none/i.test(t)&&!/none/i.test(r)){let i=this.toEditorPos(e.rangeStart),a=this.toEditorPos(e.rangeEnd);if(i<a){let e=`text-decoration: underline ${/^[a-z]/.test(t)?t+` `:t==`Dashed`?`dashed `:t==`Squiggle`?`wavy `:``}${/thin/i.test(r)?1:2}px`;n.push(z.mark({attributes:{style:e}}).range(i,a))}}}e.dispatch({effects:qr.of(z.set(n))})},this.handlers.compositionstart=()=>{e.inputState.composing<0&&(e.inputState.composing=0,e.inputState.compositionFirstChange=!0)},this.handlers.compositionend=()=>{if(e.inputState.composing=-1,e.inputState.compositionFirstChange=null,this.composing){let{drifted:t}=this.composing;this.composing=null,t&&this.reset(e.state)}};for(let e in this.handlers)t.addEventListener(e,this.handlers[e]);this.measureReq={read:e=>{let t=Rn(e.root);t&&t.rangeCount&&this.editContext.updateSelectionBounds(t.getRangeAt(0).getBoundingClientRect())}}}applyEdits(e){let t=0,n=!1,r=this.pendingContextChange;return e.changes.iterChanges((i,a,o,s,c)=>{if(n)return;let l=c.length-(a-i);if(r&&a>=r.to){if(r.from==i&&r.to==a&&r.insert.eq(c)){r=this.pendingContextChange=null,t+=l,this.to+=l;return}r=null,this.revertPending(e.state)}if(i+=t,a+=t,a<=this.from)this.from+=l,this.to+=l;else if(i<this.to){if(i<this.from||a>this.to||this.to-this.from+c.length>3e4){n=!0;return}this.editContext.updateText(this.toContextPos(i),this.toContextPos(a),c.toString()),this.to+=l}t+=l}),r&&!n&&this.revertPending(e.state),!n}update(e){let t=this.pendingContextChange,n=e.startState.selection.main;this.composing&&(this.composing.drifted||!e.changes.touchesRange(n.from,n.to)&&e.transactions.some(e=>!e.isUserEvent(`input.type`)&&e.changes.touchesRange(this.from,this.to)))?(this.composing.drifted=!0,this.composing.editorBase=e.changes.mapPos(this.composing.editorBase)):!this.applyEdits(e)||!this.rangeIsValid(e.state)?(this.pendingContextChange=null,this.reset(e.state)):(e.docChanged||e.selectionSet||t)&&this.setSelection(e.state),(e.geometryChanged||e.docChanged||e.selectionSet)&&e.view.requestMeasure(this.measureReq)}resetRange(e){let{head:t}=e.selection.main;this.from=Math.max(0,t-1e4),this.to=Math.min(e.doc.length,t+1e4)}reset(e){this.resetRange(e),this.editContext.updateText(0,this.editContext.text.length,e.doc.sliceString(this.from,this.to)),this.setSelection(e)}revertPending(e){let t=this.pendingContextChange;this.pendingContextChange=null,this.editContext.updateText(this.toContextPos(t.from),this.toContextPos(t.from+t.insert.length),e.doc.sliceString(t.from,t.to))}setSelection(e){let{main:t}=e.selection,n=this.toContextPos(Math.max(this.from,Math.min(this.to,t.anchor))),r=this.toContextPos(t.head);(this.editContext.selectionStart!=n||this.editContext.selectionEnd!=r)&&this.editContext.updateSelection(n,r)}rangeIsValid(e){let{head:t}=e.selection.main;return!(this.from>0&&t-this.from<500||this.to<e.doc.length&&this.to-t<500||this.to-this.from>3e4)}toEditorPos(e,t=this.to-this.from){e=Math.min(e,t);let n=this.composing;return n&&n.drifted?n.editorBase+(e-n.contextBase):e+this.from}toContextPos(e){let t=this.composing;return t&&t.drifted?t.contextBase+(e-t.editorBase):e-this.from}destroy(){for(let e in this.handlers)this.editContext.removeEventListener(e,this.handlers[e])}},K=class e{get state(){return this.viewState.state}get viewport(){return this.viewState.viewport}get visibleRanges(){return this.viewState.visibleRanges}get inView(){return this.viewState.inView}get composing(){return!!this.inputState&&this.inputState.composing>0}get compositionStarted(){return!!this.inputState&&this.inputState.composing>=0}get root(){return this._root}get win(){return this.dom.ownerDocument.defaultView||window}constructor(e={}){this.plugins=[],this.pluginMap=new Map,this.editorAttrs={},this.contentAttrs={},this.bidiCache=[],this.destroyed=!1,this.updateState=2,this.measureScheduled=-1,this.measureRequests=[],this.clearAnnouncement=-1,this.contentDOM=document.createElement(`div`),this.scrollDOM=document.createElement(`div`),this.scrollDOM.tabIndex=-1,this.scrollDOM.className=`cm-scroller`,this.scrollDOM.appendChild(this.contentDOM),this.announceDOM=document.createElement(`div`),this.announceDOM.className=`cm-announced`,this.announceDOM.setAttribute(`aria-live`,`polite`),this.dom=document.createElement(`div`),this.dom.appendChild(this.announceDOM),this.dom.appendChild(this.scrollDOM),e.parent&&e.parent.appendChild(this.dom);let{dispatch:t}=e;this.dispatchTransactions=e.dispatchTransactions||t&&(e=>e.forEach(e=>t(e,this)))||(e=>this.update(e)),this.dispatch=this.dispatch.bind(this),this._root=e.root||or(e.parent)||document,this.viewState=new Fo(this,e.state||F.create(e)),e.scrollTo&&e.scrollTo.is(Kr)&&(this.viewState.scrollTarget=e.scrollTo.value.clip(this.viewState.state)),this.plugins=this.state.facet(Zr).map(e=>new Qr(e));for(let e of this.plugins)e.update(this);this.observer=new ts(this),this.inputState=new Ea(this),this.inputState.ensureHandlers(this.plugins),this.docView=new Bi(this),this.mountStyles(),this.updateAttrs(),this.updateState=0,this.requestMeasure(),document.fonts?.ready&&document.fonts.ready.then(()=>{this.viewState.mustMeasureContent=`refresh`,this.requestMeasure()})}dispatch(...e){let t=e.length==1&&e[0]instanceof ht?e:e.length==1&&Array.isArray(e[0])?e[0]:[this.state.update(...e)];this.dispatchTransactions(t,this)}update(t){if(this.updateState!=0)throw Error(`Calls to EditorView.update are not allowed while an update is in progress`);let n=!1,r=!1,i,a=this.state;for(let e of t){if(e.startState!=a)throw RangeError(`Trying to update state with a transaction that doesn't start from the previous state.`);a=e.state}if(this.destroyed){this.viewState.state=a;return}let o=this.hasFocus,s=0,c=null;t.some(e=>e.annotation(ao))?(this.inputState.notifiedFocused=o,s=1):o!=this.inputState.notifiedFocused&&(this.inputState.notifiedFocused=o,c=oo(a,o),c||(s=1));let l=this.observer.delayedAndroidKey,u=null;if(l?(this.observer.clearDelayedAndroidKey(),u=this.observer.readChange(),(u&&!this.state.doc.eq(a.doc)||!this.state.selection.eq(a.selection))&&(u=null)):this.observer.clear(),a.facet(F.phrases)!=this.state.facet(F.phrases))return this.setState(a);i=fi.create(this,a,t),i.flags|=s;let d=this.viewState.scrollTarget;try{this.updateState=2;for(let n of t){if(d&&=d.map(n.changes),n.scrollIntoView){let{main:t}=n.state.selection,{x:r,y:i}=this.state.facet(e.cursorScrollMargin);d=new Gr(t.empty?t:j.cursor(t.head,t.head>t.anchor?-1:1),`nearest`,`nearest`,i,r)}for(let e of n.effects)e.is(Kr)&&(d=e.value.clip(this.state))}this.viewState.update(i,d),this.bidiCache=cs.update(this.bidiCache,i.changes),i.empty||(this.updatePlugins(i),this.inputState.update(i)),n=this.docView.update(i),this.state.facet(ui)!=this.styleModules&&this.mountStyles(),r=this.updateAttrs(),this.showAnnouncements(t),this.docView.updateSelection(n,t.some(e=>e.isUserEvent(`select.pointer`)))}finally{this.updateState=0}if(i.startState.facet(Go)!=i.state.facet(Go)&&(this.viewState.mustMeasureContent=!0),(n||r||d||this.viewState.mustEnforceCursorAssoc||this.viewState.mustMeasureContent)&&this.requestMeasure(),n&&this.docViewUpdate(),!i.empty)for(let e of this.state.facet(Lr))try{e(i)}catch(e){Jr(this.state,e,`update listener`)}(c||u)&&Promise.resolve().then(()=>{c&&this.state==c.startState&&this.dispatch(c),u&&!ya(this,u)&&l.force&&ar(this.contentDOM,l.key,l.keyCode)})}setState(e){if(this.updateState!=0)throw Error(`Calls to EditorView.setState are not allowed while an update is in progress`);if(this.destroyed){this.viewState.state=e;return}this.updateState=2;let t=this.hasFocus;try{for(let e of this.plugins)e.destroy(this);this.viewState=new Fo(this,e),this.plugins=e.facet(Zr).map(e=>new Qr(e)),this.pluginMap.clear();for(let e of this.plugins)e.update(this);this.docView.destroy(),this.docView=new Bi(this),this.inputState.ensureHandlers(this.plugins),this.mountStyles(),this.updateAttrs(),this.bidiCache=[]}finally{this.updateState=0}t&&this.focus(),this.requestMeasure()}updatePlugins(e){let t=e.startState.facet(Zr),n=e.state.facet(Zr);if(t!=n){let r=[];for(let i of n){let n=t.indexOf(i);if(n<0)r.push(new Qr(i));else{let t=this.plugins[n];t.mustUpdate=e,r.push(t)}}for(let t of this.plugins)t.mustUpdate!=e&&t.destroy(this);this.plugins=r,this.pluginMap.clear()}else for(let t of this.plugins)t.mustUpdate=e;for(let e=0;e<this.plugins.length;e++)this.plugins[e].update(this);t!=n&&this.inputState.ensureHandlers(this.plugins)}docViewUpdate(){for(let e of this.plugins){let t=e.value;if(t&&t.docViewUpdate)try{t.docViewUpdate(this)}catch(e){Jr(this.state,e,`doc view update listener`)}}}measure(e=!0){if(this.destroyed)return;if(this.measureScheduled>-1&&this.win.cancelAnimationFrame(this.measureScheduled),this.observer.delayedAndroidKey){this.measureScheduled=-1,this.requestMeasure();return}this.measureScheduled=0,e&&this.observer.forceFlush();let t=null,n=this.viewState.scrollParent,r=this.viewState.getScrollOffset(),{scrollAnchorPos:i,scrollAnchorHeight:a,scaleY:o}=this.viewState;Math.abs(r-this.viewState.scrollOffset)>1&&(a=-1),this.viewState.scrollAnchorHeight=-1;try{for(let e=0;;e++){if(a<0){if(cr(n||this.win))i=-1,a=this.viewState.heightMap.height/this.viewState.scaleY;else{let e=this.viewState.scrollAnchorAt(r);i=e.from,a=e.top}o=this.viewState.scaleY}this.updateState=1;let s=this.viewState.measure();if(!s&&!this.measureRequests.length&&this.viewState.scrollTarget==null)break;if(e>5){console.warn(this.measureRequests.length?`Measure loop restarted more than 5 times`:`Viewport failed to stabilize`);break}let c=[];s&4||([this.measureRequests,c]=[c,this.measureRequests]);let l=c.map(e=>{try{return e.read(this)}catch(e){return Jr(this.state,e),ss}}),u=fi.create(this,this.state,[]),d=!1;u.flags|=s,t?t.flags|=s:t=u,this.updateState=2,u.empty||(this.updatePlugins(u),this.inputState.update(u),this.updateAttrs(),d=this.docView.update(u),d&&this.docViewUpdate());for(let e=0;e<c.length;e++)if(l[e]!=ss)try{let t=c[e];t.write&&t.write(l[e],this)}catch(e){Jr(this.state,e)}if(d&&this.docView.updateSelection(!0),!u.viewportChanged&&this.measureRequests.length==0){if(this.viewState.editorHeight){if(this.viewState.scrollTarget){this.docView.scrollIntoView(this.viewState.scrollTarget),this.viewState.scrollTarget=null,a=-1;continue}{let e=(i<0?this.viewState.heightMap.height:this.viewState.lineBlockAt(i).top)/this.viewState.scaleY-a/o;if((e>1||e<-1)&&!(R.ios&&this.inputState.lastIOSMomentumScroll>Date.now()-100)&&(n==this.scrollDOM||this.hasFocus||Math.max(this.inputState.lastWheelEvent,this.inputState.lastTouchTime)>Date.now()-100)){r+=e,n?i<0?n.scrollTop=n.scrollHeight:n.scrollTop+=e:this.win.scrollBy(0,e),a=-1;continue}}}break}}}finally{this.updateState=0,this.measureScheduled=-1}if(t&&!t.empty)for(let e of this.state.facet(Lr))e(t)}get themeClasses(){return qo+` `+(this.state.facet(Ko)?Yo:Jo)+` `+this.state.facet(Go)}updateAttrs(){let e=ls(this,$r,{class:`cm-editor`+(this.hasFocus?` cm-focused `:` `)+this.themeClasses}),t={spellcheck:`false`,autocorrect:`off`,autocapitalize:`off`,writingsuggestions:`false`,translate:`no`,contenteditable:this.state.facet(Yr)?`true`:`false`,class:`cm-content`,style:`${R.tabSize}: ${this.state.tabSize}`,role:`textbox`,"aria-multiline":`true`};this.state.readOnly&&(t[`aria-readonly`]=`true`),ls(this,ei,t);let n=this.observer.ignore(()=>{let n=En(this.contentDOM,this.contentAttrs,t),r=En(this.dom,this.editorAttrs,e);return n||r});return this.editorAttrs=e,this.contentAttrs=t,n}showAnnouncements(t){let n=!0;for(let r of t)for(let t of r.effects)if(t.is(e.announce)){n&&=(this.announceDOM.textContent=``,this.win.clearTimeout(this.clearAnnouncement),this.clearAnnouncement=this.win.setTimeout(()=>{this.announceDOM.textContent=`\xA0`},200),!1);let e=this.announceDOM.appendChild(document.createElement(`div`));e.textContent=t.value}}mountStyles(){this.styleModules=this.state.facet(ui);let t=this.state.facet(e.cspNonce);$t.mount(this.root,this.styleModules.concat(Qo).reverse(),t?{nonce:t}:void 0)}readMeasured(){if(this.updateState==2)throw Error(`Reading the editor layout isn't allowed during an update`);this.updateState==0&&this.measureScheduled>-1&&this.measure(!1)}requestMeasure(e){if(this.measureScheduled<0&&(this.measureScheduled=this.win.requestAnimationFrame(()=>this.measure())),e){if(this.measureRequests.indexOf(e)>-1)return;if(e.key!=null){for(let t=0;t<this.measureRequests.length;t++)if(this.measureRequests[t].key===e.key){this.measureRequests[t]=e;return}}this.measureRequests.push(e)}}plugin(e){let t=this.pluginMap.get(e);return(t===void 0||t&&t.plugin!=e)&&this.pluginMap.set(e,t=this.plugins.find(t=>t.plugin==e)||null),t&&t.update(this).value}get documentTop(){return this.contentDOM.getBoundingClientRect().top+this.viewState.paddingTop}get documentPadding(){return{top:this.viewState.paddingTop,bottom:this.viewState.paddingBottom}}get scaleX(){return this.viewState.scaleX}get scaleY(){return this.viewState.scaleY}elementAtHeight(e){return this.readMeasured(),this.viewState.elementAtHeight(e)}lineBlockAtHeight(e){return this.readMeasured(),this.viewState.lineBlockAtHeight(e)}get viewportLineBlocks(){return this.viewState.viewportLines}lineBlockAt(e){return this.viewState.lineBlockAt(e)}get contentHeight(){return this.viewState.contentHeight}moveByChar(e,t,n){return ca(this,e,ra(this,e,t,n))}moveByGroup(e,t){return ca(this,e,ra(this,e,t,t=>ia(this,e.head,t)))}visualLineSide(e,t){return t?j.cursor(e.to,1):j.cursor(e.from,-1)}moveToLineBoundary(e,t,n=!0){return na(this,e,t,n)}moveVertically(e,t,n){return ca(this,e,aa(this,e,t,n))}domAtPos(e,t=1){return this.docView.domAtPos(e,t)}posAtDOM(e,t=0){return this.docView.posFromDOM(e,t)}posAtCoords(e,t=!0){this.readMeasured();let n=ua(this,e,t);return n&&n.pos}posAndSideAtCoords(e,t=!0){return this.readMeasured(),ua(this,e,t)}coordsAtPos(e,t=1){this.readMeasured();let n=this.state.doc.lineAt(e),r=this.bidiSpans(n),i=r[xr.find(r,e-n.from,-1,t)];return n.length&&(e==n.from&&t<0||e==n.to&&t>0)&&i.dir!=this.textDirectionAt(n.from)&&(e==n.to?(e=n.from+i.from,t=1):(e=n.from+i.to,t=-1)),this.docView.coordsAt(e,t,i.dir==V.RTL)}coordsForChar(e){return this.readMeasured(),this.docView.coordsForChar(e)}get defaultCharacterWidth(){return this.viewState.heightOracle.charWidth}get defaultLineHeight(){return this.viewState.heightOracle.lineHeight}get textDirection(){return this.viewState.defaultTextDirection}textDirectionAt(e){return!this.state.facet(Hr)||e<this.viewport.from||e>this.viewport.to?this.textDirection:(this.readMeasured(),this.docView.textDirectionAt(e))}get lineWrapping(){return this.viewState.heightOracle.lineWrapping}bidiSpans(e){if(e.length>os)return kr(e.length);let t=this.textDirectionAt(e.from),n;for(let r of this.bidiCache)if(r.from==e.from&&r.dir==t&&(r.fresh||Sr(r.isolates,n=oi(this,e))))return r.order;n||=oi(this,e);let r=Or(e.text,t,n);return this.bidiCache.push(new cs(e.from,e.to,t,n,!0,r)),r}get hasFocus(){return(this.dom.ownerDocument.hasFocus()||R.safari&&this.inputState?.lastContextMenu>Date.now()-3e4)&&this.root.activeElement==this.contentDOM}focus(){this.observer.ignore(()=>{nr(this.contentDOM),this.docView.updateSelection()})}setRoot(e){this._root!=e&&(this._root=e,this.observer.setWindow((e.nodeType==9?e:e.ownerDocument).defaultView||window),this.mountStyles())}destroy(){this.root.activeElement==this.contentDOM&&this.contentDOM.blur();for(let e of this.plugins)e.destroy(this);this.plugins=[],this.inputState.destroy(),this.docView.destroy(),this.dom.remove(),this.observer.destroy(),this.win.clearTimeout(this.clearAnnouncement),this.measureScheduled>-1&&this.win.cancelAnimationFrame(this.measureScheduled),this.destroyed=!0}static scrollIntoView(e,t={}){return Kr.of(new Gr(typeof e==`number`?j.cursor(e):e,t.y??`nearest`,t.x??`nearest`,t.yMargin??5,t.xMargin??5))}scrollSnapshot(){let{scrollTop:e,scrollLeft:t}=this.scrollDOM,n=this.viewState.scrollAnchorAt(e);return Kr.of(new Gr(j.cursor(n.from),`start`,`start`,n.top-e,t,!0))}setTabFocusMode(e){e==null?this.inputState.tabFocusMode=this.inputState.tabFocusMode<0?0:-1:typeof e==`boolean`?this.inputState.tabFocusMode=e?0:-1:this.inputState.tabFocusMode!=0&&(this.inputState.tabFocusMode=Date.now()+e)}static domEventHandlers(e){return U.define(()=>({}),{eventHandlers:e})}static domEventObservers(e){return U.define(()=>({}),{eventObservers:e})}static theme(e,t){let n=$t.newName(),r=[Go.of(n),ui.of(Zo(`.${n}`,e))];return t&&t.dark&&r.push(Ko.of(!0)),r}static baseTheme(e){return Ze.lowest(ui.of(Zo(`.`+qo,e,Xo)))}static findFromDOM(e){let t=e.querySelector(`.cm-content`);return(t&&W.get(t)||W.get(e))?.root?.view||null}};K.styleModule=ui,K.inputHandler=Rr,K.clipboardInputFilter=Br,K.clipboardOutputFilter=Vr,K.scrollHandler=Wr,K.focusChangeEffect=zr,K.perLineTextDirection=Hr,K.exceptionSink=Ir,K.updateListener=Lr,K.editable=Yr,K.mouseSelectionStyle=Fr,K.dragMovesSelection=Pr,K.clickAddsSelectionRange=Nr,K.decorations=ti,K.blockWrappers=ni,K.outerDecorations=ri,K.atomicRanges=ii,K.bidiIsolatedRanges=ai,K.cursorScrollMargin=M.define({combine:e=>{let t=5,n=5;for(let r of e)typeof r==`number`?t=n=r:{x:t,y:n}=r;return{x:t,y:n}}}),K.scrollMargins=ci,K.darkTheme=Ko,K.cspNonce=M.define({combine:e=>e.length?e[0]:``}),K.contentAttributes=ei,K.editorAttributes=$r,K.lineWrapping=K.contentAttributes.of({class:`cm-lineWrapping`}),K.announce=N.define();var os=4096,ss={},cs=class e{constructor(e,t,n,r,i,a){this.from=e,this.to=t,this.dir=n,this.isolates=r,this.fresh=i,this.order=a}static update(t,n){if(n.empty&&!t.some(e=>e.fresh))return t;let r=[],i=t.length?t[t.length-1].dir:V.LTR;for(let a=Math.max(0,t.length-10);a<t.length;a++){let o=t[a];o.dir==i&&!n.touchesRange(o.from,o.to)&&r.push(new e(n.mapPos(o.from,1),n.mapPos(o.to,-1),o.dir,o.isolates,!1,o.order))}return r}};function ls(e,t,n){for(let r=e.state.facet(t),i=r.length-1;i>=0;i--){let t=r[i],a=typeof t==`function`?t(e):t;a&&Sn(a,n)}return n}var us=R.mac?`mac`:R.windows?`win`:R.linux?`linux`:`key`;function ds(e,t){let n=e.split(/-(?!$)/),r=n[n.length-1];r==`Space`&&(r=` `);let i,a,o,s;for(let e=0;e<n.length-1;++e){let r=n[e];if(/^(cmd|meta|m)$/i.test(r))s=!0;else if(/^a(lt)?$/i.test(r))i=!0;else if(/^(c|ctrl|control)$/i.test(r))a=!0;else if(/^s(hift)?$/i.test(r))o=!0;else if(/^mod$/i.test(r))t==`mac`?s=!0:a=!0;else throw Error(`Unrecognized modifier name: `+r)}return i&&(r=`Alt-`+r),a&&(r=`Ctrl-`+r),s&&(r=`Meta-`+r),o&&(r=`Shift-`+r),r}function fs(e,t,n){return t.altKey&&(e=`Alt-`+e),t.ctrlKey&&(e=`Ctrl-`+e),t.metaKey&&(e=`Meta-`+e),n!==!1&&t.shiftKey&&(e=`Shift-`+e),e}var ps=Ze.default(K.domEventHandlers({keydown(e,t){return Ss(gs(t.state),e,t,`editor`)}})),ms=M.define({enables:ps}),hs=new WeakMap;function gs(e){let t=e.facet(ms),n=hs.get(t);return n||hs.set(t,n=bs(t.reduce((e,t)=>e.concat(t),[]))),n}function _s(e,t,n){return Ss(gs(e.state),t,e,n)}var vs=null,ys=4e3;function bs(e,t=us){let n=Object.create(null),r=Object.create(null),i=(e,t)=>{let n=r[e];if(n==null)r[e]=t;else if(n!=t)throw Error(`Key binding `+e+` is used both as a regular binding and as a multi-stroke prefix`)},a=(e,r,a,o,s)=>{let c=n[e]||(n[e]=Object.create(null)),l=r.split(/ (?!$)/).map(e=>ds(e,t));for(let t=1;t<l.length;t++){let n=l.slice(0,t).join(` `);i(n,!0),c[n]||(c[n]={preventDefault:!0,stopPropagation:!1,run:[t=>{let r=vs={view:t,prefix:n,scope:e};return setTimeout(()=>{vs==r&&(vs=null)},ys),!0}]})}let u=l.join(` `);i(u,!1);let d=c[u]||(c[u]={preventDefault:!1,stopPropagation:!1,run:(c._any?.run)?.slice()||[]});a&&d.run.push(a),o&&(d.preventDefault=!0),s&&(d.stopPropagation=!0)};for(let r of e){let e=r.scope?r.scope.split(` `):[`editor`];if(r.any)for(let t of e){let e=n[t]||(n[t]=Object.create(null));e._any||={preventDefault:!1,stopPropagation:!1,run:[]};let{any:i}=r;for(let t in e)e[t].run.push(e=>i(e,xs))}let i=r[t]||r.key;if(i)for(let t of e)a(t,i,r.run,r.preventDefault,r.stopPropagation),r.shift&&a(t,`Shift-`+i,r.shift,r.preventDefault,r.stopPropagation)}return n}var xs=null;function Ss(e,t,n,r){xs=t;let i=ln(t),a=ke(De(i,0))==i.length&&i!=` `,o=``,s=!1,c=!1,l=!1;vs&&vs.view==n&&vs.scope==r&&(o=vs.prefix+` `,Ma.indexOf(t.keyCode)<0&&(c=!0,vs=null));let u=new Set,d=e=>{if(e){for(let t of e.run)if(!u.has(t)&&(u.add(t),t(n)))return e.stopPropagation&&(l=!0),!0;e.preventDefault&&(e.stopPropagation&&(l=!0),c=!0)}return!1},f=e[r],p,m;return f&&(d(f[o+fs(i,t,!a)])?s=!0:a&&(t.altKey||t.metaKey||t.ctrlKey)&&!(R.windows&&t.ctrlKey&&t.altKey)&&!(R.mac&&t.altKey&&!(t.ctrlKey||t.metaKey))&&(p=nn[t.keyCode])&&p!=i?(d(f[o+fs(p,t,!0)])||t.shiftKey&&(m=rn[t.keyCode])!=i&&m!=p&&d(f[o+fs(m,t,!1)]))&&(s=!0):a&&t.shiftKey&&d(f[o+fs(i,t,!0)])&&(s=!0),!s&&d(f._any)&&(s=!0)),c&&(s=!0),s&&l&&t.stopPropagation(),xs=null,s}var Cs=class e{constructor(e,t,n,r,i){this.className=e,this.left=t,this.top=n,this.width=r,this.height=i}draw(){let e=document.createElement(`div`);return e.className=this.className,this.adjust(e),e}update(e,t){return t.className==this.className&&(this.adjust(e),!0)}adjust(e){e.style.left=this.left+`px`,e.style.top=this.top+`px`,this.width!=null&&(e.style.width=this.width+`px`),e.style.height=this.height+`px`}eq(e){return this.left==e.left&&this.top==e.top&&this.width==e.width&&this.height==e.height&&this.className==e.className}static forRange(t,n,r){if(r.empty){let i=t.coordsAtPos(r.head,r.assoc||1);if(!i)return[];let a=ws(t);return[new e(n,i.left-a.left,i.top-a.top,null,i.bottom-i.top)]}return Es(t,n,r)}};function ws(e){let t=e.scrollDOM.getBoundingClientRect();return{left:(e.textDirection==V.LTR?t.left:t.right-e.scrollDOM.clientWidth*e.scaleX)-e.scrollDOM.scrollLeft*e.scaleX,top:t.top-e.scrollDOM.scrollTop*e.scaleY}}function Ts(e,t,n,r){let i=e.coordsAtPos(t,n*2);if(!i)return r;let a=e.dom.getBoundingClientRect(),o=(i.top+i.bottom)/2,s=e.posAtCoords({x:a.left+1,y:o}),c=e.posAtCoords({x:a.right-1,y:o});return s==null||c==null?r:{from:Math.max(r.from,Math.min(s,c)),to:Math.min(r.to,Math.max(s,c))}}function Es(e,t,n){if(n.to<=e.viewport.from||n.from>=e.viewport.to)return[];let r=Math.max(n.from,e.viewport.from),i=Math.min(n.to,e.viewport.to),a=e.textDirection==V.LTR,o=e.contentDOM,s=o.getBoundingClientRect(),c=ws(e),l=o.querySelector(`.cm-line`),u=l&&window.getComputedStyle(l),d=s.left+(u?parseInt(u.paddingLeft)+Math.min(0,parseInt(u.textIndent)):0),f=s.right-(u?parseInt(u.paddingRight):0),p=ta(e,r,1),m=ta(e,i,-1),h=p.type==kn.Text?p:null,g=m.type==kn.Text?m:null;if(h&&(e.lineWrapping||p.widgetLineBreaks)&&(h=Ts(e,r,1,h)),g&&(e.lineWrapping||m.widgetLineBreaks)&&(g=Ts(e,i,-1,g)),h&&g&&h.from==g.from&&h.to==g.to)return v(y(n.from,n.to,h));{let t=h?y(n.from,null,h):ee(p,!1),r=g?y(null,n.to,g):ee(m,!0),i=[];return(h||p).to<(g||m).from-(h&&g?1:0)||p.widgetLineBreaks>1&&t.bottom+e.defaultLineHeight/2<r.top?i.push(_(d,t.bottom,f,r.top)):t.bottom<r.top&&e.elementAtHeight((t.bottom+r.top)/2).type==kn.Text&&(t.bottom=r.top=(t.bottom+r.top)/2),v(t).concat(i).concat(v(r))}function _(e,n,r,i){return new Cs(t,e-c.left,n-c.top,Math.max(0,r-e),i-n)}function v({top:e,bottom:t,horizontal:n}){let r=[];for(let i=0;i<n.length;i+=2)r.push(_(n[i],e,n[i+1],t));return r}function y(t,n,r){let i=1e9,o=-1e9,s=[];function c(t,n,c,l,u){let p=e.coordsAtPos(t,t==r.to?-2:2),m=e.coordsAtPos(c,c==r.from?2:-2);p&&m&&(i=Math.min(p.top,m.top,i),o=Math.max(p.bottom,m.bottom,o),u==V.LTR?s.push(a&&n?d:p.left,a&&l?f:m.right):s.push(!a&&l?d:m.left,!a&&n?f:p.right))}let l=t??r.from,u=n??r.to;for(let r of e.visibleRanges)if(r.to>l&&r.from<u)for(let i=Math.max(r.from,l),a=Math.min(r.to,u);;){let r=e.state.doc.lineAt(i);for(let o of e.bidiSpans(r)){let e=o.from+r.from,s=o.to+r.from;if(e>=a)break;s>i&&c(Math.max(e,i),t==null&&e<=l,Math.min(s,a),n==null&&s>=u,o.dir)}if(i=r.to+1,i>=a)break}return s.length==0&&c(l,t==null,u,n==null,e.textDirection),{top:i,bottom:o,horizontal:s}}function ee(e,t){let n=s.top+(t?e.top:e.bottom);return{top:n,bottom:n,horizontal:[]}}}function Ds(e,t){return e.constructor==t.constructor&&e.eq(t)}var Os=class{constructor(e,t){this.view=e,this.layer=t,this.drawn=[],this.scaleX=1,this.scaleY=1,this.measureReq={read:this.measure.bind(this),write:this.draw.bind(this)},this.dom=e.scrollDOM.appendChild(document.createElement(`div`)),this.dom.classList.add(`cm-layer`),t.above&&this.dom.classList.add(`cm-layer-above`),t.class&&this.dom.classList.add(t.class),this.scale(),this.dom.setAttribute(`aria-hidden`,`true`),this.setOrder(e.state),e.requestMeasure(this.measureReq),t.mount&&t.mount(this.dom,e)}update(e){e.startState.facet(ks)!=e.state.facet(ks)&&this.setOrder(e.state),(this.layer.update(e,this.dom)||e.geometryChanged)&&(this.scale(),e.view.requestMeasure(this.measureReq))}docViewUpdate(e){this.layer.updateOnDocViewUpdate!==!1&&e.requestMeasure(this.measureReq)}setOrder(e){let t=0,n=e.facet(ks);for(;t<n.length&&n[t]!=this.layer;)t++;this.dom.style.zIndex=String((this.layer.above?150:-1)-t)}measure(){return this.layer.markers(this.view)}scale(){let{scaleX:e,scaleY:t}=this.view;(e!=this.scaleX||t!=this.scaleY)&&(this.scaleX=e,this.scaleY=t,this.dom.style.transform=`scale(${1/e}, ${1/t})`)}draw(e){if(e.length!=this.drawn.length||e.some((e,t)=>!Ds(e,this.drawn[t]))){let t=this.dom.firstChild,n=0;for(let r of e)r.update&&t&&r.constructor&&this.drawn[n].constructor&&r.update(t,this.drawn[n])?(t=t.nextSibling,n++):this.dom.insertBefore(r.draw(),t);for(;t;){let e=t.nextSibling;t.remove(),t=e}this.drawn=e,R.webkit&&(this.dom.style.display=this.dom.firstChild?``:`none`)}}destroy(){this.layer.destroy&&this.layer.destroy(this.dom,this.view),this.dom.remove()}},ks=M.define();function As(e){return[U.define(t=>new Os(t,e)),ks.of(e)]}var js=!R.ios,Ms=M.define({combine(e){return Ot(e,{cursorBlinkRate:1200,drawRangeCursor:!0,iosSelectionHandles:!0},{cursorBlinkRate:(e,t)=>Math.min(e,t),drawRangeCursor:(e,t)=>e||t})}});function Ns(e={}){return[Ms.of(e),Fs,Ls,zs,Ur.of(!0)]}function Ps(e){return e.startState.facet(Ms)!=e.state.facet(Ms)}var Fs=As({above:!0,markers(e){let{state:t}=e,n=t.facet(Ms),r=[];for(let i of t.selection.ranges){let a=i==t.selection.main;if(i.empty||n.drawRangeCursor&&!(a&&R.ios&&n.iosSelectionHandles)){let t=a?`cm-cursor cm-cursor-primary`:`cm-cursor cm-cursor-secondary`,n=i.empty?i:j.cursor(i.head,i.assoc);for(let i of Cs.forRange(e,t,n))r.push(i)}}return r},update(e,t){e.transactions.some(e=>e.selection)&&(t.style.animationName=t.style.animationName==`cm-blink`?`cm-blink2`:`cm-blink`);let n=Ps(e);return n&&Is(e.state,t),e.docChanged||e.selectionSet||n},mount(e,t){Is(t.state,e)},class:`cm-cursorLayer`});function Is(e,t){t.style.animationDuration=e.facet(Ms).cursorBlinkRate+`ms`}var Ls=As({above:!1,markers(e){let t=[],{main:n,ranges:r}=e.state.selection;for(let i of r)if(!i.empty&&(js||i!=n))for(let n of Cs.forRange(e,`cm-selectionBackground`,i))t.push(n);if(R.ios&&!n.empty&&e.state.facet(Ms).iosSelectionHandles){for(let r of Cs.forRange(e,`cm-selectionHandle cm-selectionHandle-start`,j.cursor(n.from,1)))t.push(r);for(let r of Cs.forRange(e,`cm-selectionHandle cm-selectionHandle-end`,j.cursor(n.to,1)))t.push(r)}return t},update(e,t){return e.docChanged||e.selectionSet||e.viewportChanged||Ps(e)},class:`cm-selectionLayer`}),Rs=R.gecko&&R.gecko_version==153?`#ffffff01`:`transparent`,zs=Ze.highest(K.theme({".cm-line":{...js?{"& ::selection, &::selection":{backgroundColor:`${Rs} !important`}}:{},caretColor:`transparent !important`},".cm-content":{caretColor:`transparent !important`,"& :focus":{caretColor:`initial !important`,...js?{"&::selection, & ::selection":{backgroundColor:`Highlight !important`}}:{}}}})),Bs=N.define({map(e,t){return e==null?null:t.mapPos(e)}}),Vs=Je.define({create(){return null},update(e,t){return e!=null&&(e=t.changes.mapPos(e)),t.effects.reduce((e,t)=>t.is(Bs)?t.value:e,e)}}),Hs=U.fromClass(class{constructor(e){this.view=e,this.cursor=null,this.measureReq={read:this.readPos.bind(this),write:this.drawCursor.bind(this)}}update(e){var t;let n=e.state.field(Vs);n==null?this.cursor!=null&&((t=this.cursor)==null||t.remove(),this.cursor=null):(this.cursor||(this.cursor=this.view.scrollDOM.appendChild(document.createElement(`div`)),this.cursor.className=`cm-dropCursor`),(e.startState.field(Vs)!=n||e.docChanged||e.geometryChanged)&&this.view.requestMeasure(this.measureReq))}readPos(){let{view:e}=this,t=e.state.field(Vs),n=t!=null&&e.coordsAtPos(t);if(!n)return null;let r=e.scrollDOM.getBoundingClientRect();return{left:n.left-r.left+e.scrollDOM.scrollLeft*e.scaleX,top:n.top-r.top+e.scrollDOM.scrollTop*e.scaleY,height:n.bottom-n.top}}drawCursor(e){if(this.cursor){let{scaleX:t,scaleY:n}=this.view;e?(this.cursor.style.left=e.left/t+`px`,this.cursor.style.top=e.top/n+`px`,this.cursor.style.height=e.height/n+`px`):this.cursor.style.left=`-100000px`}}destroy(){this.cursor&&this.cursor.remove()}setDropPos(e){this.view.state.field(Vs)!=e&&this.view.dispatch({effects:Bs.of(e)})}},{eventObservers:{dragover(e){this.setDropPos(this.view.posAtCoords({x:e.clientX,y:e.clientY}))},dragleave(e){e.target!=this.view.contentDOM&&this.view.contentDOM.contains(e.relatedTarget)||this.setDropPos(null)},dragend(){this.setDropPos(null)},drop(){this.setDropPos(null)}}});function Us(){return[Vs,Hs]}function Ws(e,t,n,r,i){t.lastIndex=0;for(let a=e.iterRange(n,r),o=n,s;!a.next().done;o+=a.value.length)if(!a.lineBreak)for(;s=t.exec(a.value);)i(o+s.index,s)}function Gs(e,t){let n=e.visibleRanges;if(n.length==1&&n[0].from==e.viewport.from&&n[0].to==e.viewport.to)return n;let r=[];for(let{from:i,to:a}of n)i=Math.max(e.state.doc.lineAt(i).from,i-t),a=Math.min(e.state.doc.lineAt(a).to,a+t),r.length&&r[r.length-1].to>=i?r[r.length-1].to=a:r.push({from:i,to:a});return r}var Ks=class{constructor(e){let{regexp:t,decoration:n,decorate:r,boundary:i,maxLength:a=1e3}=e;if(!t.global)throw RangeError(`The regular expression given to MatchDecorator should have its 'g' flag set`);if(this.regexp=t,r)this.addMatch=(e,t,n,i)=>r(i,n,n+e[0].length,e,t);else if(typeof n==`function`)this.addMatch=(e,t,r,i)=>{let a=n(e,t,r);a&&i(r,r+e[0].length,a)};else if(n)this.addMatch=(e,t,r,i)=>i(r,r+e[0].length,n);else throw RangeError(`Either 'decorate' or 'decoration' should be provided to MatchDecorator`);this.boundary=i,this.maxLength=a}createDeco(e){let t=new It,n=t.add.bind(t);for(let{from:t,to:r}of Gs(e,this.maxLength))Ws(e.state.doc,this.regexp,t,r,(t,r)=>this.addMatch(r,e,t,n));return t.finish()}updateDeco(e,t){let n=1e9,r=-1;return e.docChanged&&e.changes.iterChanges((t,i,a,o)=>{o>=e.view.viewport.from&&a<=e.view.viewport.to&&(n=Math.min(a,n),r=Math.max(o,r))}),e.viewportMoved||r-n>1e3?this.createDeco(e.view):r>-1?this.updateRange(e.view,t.map(e.changes),n,r):t}updateRange(e,t,n,r){for(let i of e.visibleRanges){let a=Math.max(i.from,n),o=Math.min(i.to,r);if(o>=a){let n=e.state.doc.lineAt(a),r=n.to<o?e.state.doc.lineAt(o):n,s=Math.max(i.from,n.from),c=Math.min(i.to,r.to);if(this.boundary){for(;a>n.from;a--)if(this.boundary.test(n.text[a-1-n.from])){s=a;break}for(;o<r.to;o++)if(this.boundary.test(r.text[o-r.from])){c=o;break}}let l=[],u,d=(e,t,n)=>l.push(n.range(e,t));if(n==r)for(this.regexp.lastIndex=s-n.from;(u=this.regexp.exec(n.text))&&u.index<c-n.from;)this.addMatch(u,e,u.index+n.from,d);else Ws(e.state.doc,this.regexp,s,c,(t,n)=>this.addMatch(n,e,t,d));t=t.update({filterFrom:s,filterTo:c,filter:(e,t)=>e<s||t>c,add:l})}}return t}},qs=/x/.unicode==null?`g`:`gu`,Js=RegExp(`[\0-\b
--­؜​‎‏\u2028\u2029‭‮⁦⁧⁩﻿￹-￼]`,qs),Ys={0:`null`,7:`bell`,8:`backspace`,10:`newline`,11:`vertical tab`,13:`carriage return`,27:`escape`,8203:`zero width space`,8204:`zero width non-joiner`,8205:`zero width joiner`,8206:`left-to-right mark`,8207:`right-to-left mark`,8232:`line separator`,8237:`left-to-right override`,8238:`right-to-left override`,8294:`left-to-right isolate`,8295:`right-to-left isolate`,8297:`pop directional isolate`,8233:`paragraph separator`,65279:`zero width no-break space`,65532:`object replacement`},Xs=null;function Zs(){if(Xs==null&&typeof document<`u`&&document.body){let e=document.body.style;Xs=(e.tabSize??e.MozTabSize)!=null}return Xs||!1}var Qs=M.define({combine(e){let t=Ot(e,{render:null,specialChars:Js,addSpecialChars:null});return(t.replaceTabs=!Zs())&&(t.specialChars=RegExp(`	|`+t.specialChars.source,qs)),t.addSpecialChars&&(t.specialChars=RegExp(t.specialChars.source+`|`+t.addSpecialChars.source,qs)),t}});function $s(e={}){return[Qs.of(e),tc()]}var ec=null;function tc(){return ec||=U.fromClass(class{constructor(e){this.view=e,this.decorations=z.none,this.decorationCache=Object.create(null),this.decorator=this.makeDecorator(e.state.facet(Qs)),this.decorations=this.decorator.createDeco(e)}makeDecorator(e){return new Ks({regexp:e.specialChars,decoration:(t,n,r)=>{let{doc:i}=n.state,a=De(t[0],0);if(a==9){let e=i.lineAt(r),t=n.state.tabSize,a=qt(e.text,t,r-e.from);return z.replace({widget:new ac((t-a%t)*this.view.defaultCharacterWidth/this.view.scaleX)})}return this.decorationCache[a]||(this.decorationCache[a]=z.replace({widget:new ic(e,a)}))},boundary:e.replaceTabs?void 0:/[^]/})}update(e){let t=e.state.facet(Qs);e.startState.facet(Qs)==t?this.decorations=this.decorator.updateDeco(e,this.decorations):(this.decorator=this.makeDecorator(t),this.decorations=this.decorator.createDeco(e.view))}},{decorations:e=>e.decorations})}var nc=`•`;function rc(e){return e>=32?nc:e==10?`␤`:String.fromCharCode(9216+e)}var ic=class extends On{constructor(e,t){super(),this.options=e,this.code=t}eq(e){return e.code==this.code}toDOM(e){let t=rc(this.code),n=e.state.phrase(`Control character`)+` `+(Ys[this.code]||`0x`+this.code.toString(16)),r=this.options.render&&this.options.render(this.code,n,t);if(r)return r;let i=document.createElement(`span`);return i.textContent=t,i.title=n,i.setAttribute(`aria-label`,n),i.className=`cm-specialChar`,i}ignoreEvent(){return!1}},ac=class extends On{constructor(e){super(),this.width=e}eq(e){return e.width==this.width}toDOM(){let e=document.createElement(`span`);return e.textContent=`	`,e.className=`cm-tab`,e.style.width=this.width+`px`,e}ignoreEvent(){return!1}};function oc(){return cc}var sc=z.line({class:`cm-activeLine`}),cc=U.fromClass(class{constructor(e){this.decorations=this.getDeco(e)}update(e){(e.docChanged||e.selectionSet)&&(this.decorations=this.getDeco(e.view))}getDeco(e){let t=-1,n=[];for(let r of e.state.selection.ranges){let i=e.lineBlockAt(r.head);i.from>t&&(n.push(sc.range(i.from)),t=i.from)}return z.set(n)}},{decorations:e=>e.decorations}),lc=2e3;function uc(e,t,n){let r=Math.min(t.line,n.line),i=Math.max(t.line,n.line),a=[];if(t.off>lc||n.off>lc||t.col<0||n.col<0){let o=Math.min(t.off,n.off),s=Math.max(t.off,n.off);for(let t=r;t<=i;t++){let n=e.doc.line(t);n.length<=s&&a.push(j.range(n.from+o,n.to+s))}}else{let o=Math.min(t.col,n.col),s=Math.max(t.col,n.col);for(let t=r;t<=i;t++){let n=e.doc.line(t),r=Jt(n.text,o,e.tabSize,!0);if(r<0)a.push(j.cursor(n.to));else{let t=Jt(n.text,s,e.tabSize);a.push(j.range(n.from+r,n.from+t))}}}return a}function dc(e,t){let n=e.coordsAtPos(e.viewport.from);return n?Math.round(Math.abs((n.left-t)/e.defaultCharacterWidth)):-1}function fc(e,t){let n=e.posAtCoords({x:t.clientX,y:t.clientY},!1),r=e.state.doc.lineAt(n),i=n-r.from,a=i>lc?-1:i==r.length?dc(e,t.clientX):qt(r.text,e.state.tabSize,n-r.from);return{line:r.number,col:a,off:i}}function pc(e,t){let n=fc(e,t),r=e.state.selection;return n?{update(e){if(e.docChanged){let t=e.changes.mapPos(e.startState.doc.line(n.line).from),i=e.state.doc.lineAt(t);n={line:i.number,col:n.col,off:Math.min(n.off,i.length)},r=r.map(e.changes)}},get(t,i,a){let o=fc(e,t);if(!o)return r;let s=uc(e.state,n,o);return s.length?a?j.create(s.concat(r.ranges)):j.create(s):r}}:null}function mc(e){let t=e?.eventFilter||(e=>e.altKey&&e.button==0);return K.mouseSelectionStyle.of((e,n)=>t(n)?pc(e,n):null)}var hc={Alt:[18,e=>!!e.altKey],Control:[17,e=>!!e.ctrlKey],Shift:[16,e=>!!e.shiftKey],Meta:[91,e=>!!e.metaKey]},gc={style:`cursor: crosshair`};function _c(e={}){let[t,n]=hc[e.key||`Alt`],r=U.fromClass(class{constructor(e){this.view=e,this.isDown=!1}set(e){this.isDown!=e&&(this.isDown=e,this.view.update([]))}},{eventObservers:{keydown(e){this.set(e.keyCode==t||n(e))},keyup(e){e.keyCode!=t&&n(e)||this.set(!1)},mousemove(e){this.set(n(e))}}});return[r,K.contentAttributes.of(e=>e.plugin(r)?.isDown?gc:null)]}var vc=`-10000px`,yc=class{constructor(e,t,n,r){this.facet=t,this.createTooltipView=n,this.removeTooltipView=r,this.input=e.state.facet(t),this.tooltips=this.input.filter(e=>e);let i=null;this.tooltipViews=this.tooltips.map(e=>i=n(e,i))}update(e,t){var n;let r=e.state.facet(this.facet),i=r.filter(e=>e);if(r===this.input){for(let t of this.tooltipViews)t.update&&t.update(e);return!1}let a=[],o=t?[]:null;for(let n=0;n<i.length;n++){let r=i[n],s=-1;if(r){for(let e=0;e<this.tooltips.length;e++){let t=this.tooltips[e];t&&t.create==r.create&&(s=e)}if(s<0)a[n]=this.createTooltipView(r,n?a[n-1]:null),o&&(o[n]=!!r.above);else{let r=a[n]=this.tooltipViews[s];o&&(o[n]=t[s]),r.update&&r.update(e)}}}for(let e of this.tooltipViews)a.indexOf(e)<0&&(this.removeTooltipView(e),(n=e.destroy)==null||n.call(e));return t&&(o.forEach((e,n)=>t[n]=e),t.length=o.length),this.input=r,this.tooltips=i,this.tooltipViews=a,!0}};function bc(e){let t=e.dom.ownerDocument.documentElement,n=t.clientWidth,r=getComputedStyle(t);if(!/stable/.test(r.scrollbarGutter||``))return{left:0,right:n};let i=t.getBoundingClientRect(),a=e.win.scrollX;return{left:Math.max(0,i.left+a-parseFloat(r.marginLeft)),right:Math.min(n,i.right+a+parseFloat(r.marginRight))}}function xc(e){let{left:t,right:n}=bc(e);return{top:0,left:t,right:n,bottom:e.dom.ownerDocument.documentElement.clientHeight}}var Sc=M.define({combine:e=>({position:R.ios?`absolute`:e.find(e=>e.position)?.position||`fixed`,parent:e.find(e=>e.parent)?.parent||null,tooltipSpace:e.find(e=>e.tooltipSpace)?.tooltipSpace||xc})}),Cc=new WeakMap,wc=U.fromClass(class{constructor(e){this.view=e,this.above=[],this.inView=!0,this.madeAbsolute=!1,this.lastTransaction=0,this.measureTimeout=-1;let t=e.state.facet(Sc);this.position=t.position,this.parent=t.parent,this.classes=e.themeClasses,this.createContainer(),this.measureReq={read:this.readMeasure.bind(this),write:this.writeMeasure.bind(this),key:this},this.resizeObserver=typeof ResizeObserver==`function`?new ResizeObserver(()=>this.measureSoon()):null,this.manager=new yc(e,Oc,(e,t)=>this.createTooltip(e,t),e=>{this.resizeObserver&&this.resizeObserver.unobserve(e.dom),e.dom.remove()}),this.above=this.manager.tooltips.map(e=>!!e.above),this.intersectionObserver=typeof IntersectionObserver==`function`?new IntersectionObserver(e=>{Date.now()>this.lastTransaction-50&&e.length>0&&e[e.length-1].intersectionRatio<1&&this.measureSoon()},{threshold:[1]}):null,this.observeIntersection(),e.win.addEventListener(`resize`,this.measureSoon=this.measureSoon.bind(this)),this.maybeMeasure()}createContainer(){this.parent?(this.container=document.createElement(`div`),this.container.style.position=`relative`,this.container.className=this.view.themeClasses,this.parent.appendChild(this.container)):this.container=this.view.dom}observeIntersection(){if(this.intersectionObserver){this.intersectionObserver.disconnect();for(let e of this.manager.tooltipViews)this.intersectionObserver.observe(e.dom)}}measureSoon(){this.measureTimeout<0&&(this.measureTimeout=setTimeout(()=>{this.measureTimeout=-1,this.maybeMeasure()},50))}update(e){e.transactions.length&&(this.lastTransaction=Date.now());let t=this.manager.update(e,this.above);t&&this.observeIntersection();let n=t||e.geometryChanged,r=e.state.facet(Sc);if(r.position!=this.position&&!this.madeAbsolute){this.position=r.position;for(let e of this.manager.tooltipViews)e.dom.style.position=this.position;n=!0}if(r.parent!=this.parent){this.parent&&this.container.remove(),this.parent=r.parent,this.createContainer();for(let e of this.manager.tooltipViews)this.container.appendChild(e.dom);n=!0}else this.parent&&this.view.themeClasses!=this.classes&&(this.classes=this.container.className=this.view.themeClasses);n&&this.maybeMeasure()}createTooltip(e,t){let n=e.create(this.view),r=t?t.dom:null;if(n.dom.classList.add(`cm-tooltip`),e.arrow&&!n.dom.querySelector(`.cm-tooltip > .cm-tooltip-arrow`)){let e=document.createElement(`div`);e.className=`cm-tooltip-arrow`,n.dom.appendChild(e)}return n.dom.style.position=this.position,n.dom.style.top=vc,n.dom.style.left=`0px`,this.container.insertBefore(n.dom,r),n.mount&&n.mount(this.view),this.resizeObserver&&this.resizeObserver.observe(n.dom),n}destroy(){var e,t,n;this.view.win.removeEventListener(`resize`,this.measureSoon);for(let t of this.manager.tooltipViews)t.dom.remove(),(e=t.destroy)==null||e.call(t);this.parent&&this.container.remove(),(t=this.resizeObserver)==null||t.disconnect(),(n=this.intersectionObserver)==null||n.disconnect(),clearTimeout(this.measureTimeout)}readMeasure(){let e=1,t=1,n=!1,r=0;if(this.position==`fixed`&&this.manager.tooltipViews.length){let{dom:e}=this.manager.tooltipViews[0];if(R.safari){let t=e.getBoundingClientRect();n=Math.abs(t.top+1e4)>1||Math.abs(t.left)>1}else n=!!e.offsetParent&&e.offsetParent!=this.container.ownerDocument.body}if(n||this.position==`absolute`){if(this.parent){let n=this.parent.getBoundingClientRect();n.width&&n.height&&(e=n.width/this.parent.offsetWidth,t=n.height/this.parent.offsetHeight)}else({scaleX:e,scaleY:t}=this.view.viewState)}else r=bc(this.view).left;let i=this.view.scrollDOM.getBoundingClientRect(),a=li(this.view);return{visible:{left:i.left+a.left,top:i.top+a.top,right:i.right-a.right,bottom:i.bottom-a.bottom},parent:this.parent?this.container.getBoundingClientRect():this.view.dom.getBoundingClientRect(),pos:this.manager.tooltips.map((e,t)=>{let n=this.manager.tooltipViews[t];return n.getCoords?n.getCoords(e.pos):this.view.coordsAtPos(e.pos)}),size:this.manager.tooltipViews.map(({dom:e})=>e.getBoundingClientRect()),space:this.view.state.facet(Sc).tooltipSpace(this.view),scaleX:e,scaleY:t,makeAbsolute:n,fixedLeft:r}}writeMeasure(e){if(e.makeAbsolute){this.madeAbsolute=!0,this.position=`absolute`;for(let e of this.manager.tooltipViews)e.dom.style.position=`absolute`}let{visible:t,space:n,scaleX:r,scaleY:i}=e,a=[];for(let o=0;o<this.manager.tooltips.length;o++){let s=this.manager.tooltips[o],c=this.manager.tooltipViews[o],{dom:l}=c,u=e.pos[o],d=e.size[o];if(!u||s.clip!==!1&&(u.bottom<=Math.max(t.top,n.top)||u.top>=Math.min(t.bottom,n.bottom)||u.right<Math.max(t.left,n.left)-.1||u.left>Math.min(t.right,n.right)+.1)){l.style.top=vc;continue}let f=s.arrow?c.dom.querySelector(`.cm-tooltip-arrow`):null,p=f?7:0,m=d.right-d.left,h=Cc.get(c)??d.bottom-d.top,g=c.offset||Dc,_=this.view.textDirection==V.LTR,v=d.width>n.right-n.left?_?n.left:n.right-d.width:_?Math.max(n.left,Math.min(u.left-(f?14:0)+g.x,n.right-m)):Math.min(Math.max(n.left,u.left-m+(f?14:0)-g.x),n.right-m),y=this.above[o];!s.strictSide&&(y?u.top-h-p-g.y<n.top:u.bottom+h+p+g.y>n.bottom)&&y==n.bottom-u.bottom>u.top-n.top&&(y=this.above[o]=!y);let ee=(y?u.top-n.top:n.bottom-u.bottom)-p;if(ee<h&&c.resize!==!1){if(ee<this.view.defaultLineHeight){l.style.top=vc;continue}Cc.set(c,h),l.style.height=(h=ee)/i+`px`}else l.style.height&&(l.style.height=``);let b=y?u.top-h-p-g.y:u.bottom+p+g.y,x=v+m;if(c.overlap!==!0)for(let e of a)e.left<x&&e.right>v&&e.top<b+h&&e.bottom>b&&(b=y?e.top-h-2-p:e.bottom+p+2);if(this.position==`absolute`?(l.style.top=(b-e.parent.top)/i+`px`,Tc(l,(v-e.parent.left)/r)):(l.style.top=b/i+`px`,Tc(l,(v-e.fixedLeft)/r)),f){let e=u.left+(_?g.x:-g.x)-(v+14-7);f.style.left=e/r+`px`}c.overlap!==!0&&a.push({left:v,top:b,right:x,bottom:b+h}),l.classList.toggle(`cm-tooltip-above`,y),l.classList.toggle(`cm-tooltip-below`,!y),c.positioned&&c.positioned(e.space)}}maybeMeasure(){if(this.manager.tooltips.length&&(this.view.inView&&this.view.requestMeasure(this.measureReq),this.inView!=this.view.inView&&(this.inView=this.view.inView,!this.inView)))for(let e of this.manager.tooltipViews)e.dom.style.top=vc}},{eventObservers:{scroll(){this.maybeMeasure()}}});function Tc(e,t){let n=parseInt(e.style.left,10);(isNaN(n)||Math.abs(t-n)>1)&&(e.style.left=t+`px`)}var Ec=K.baseTheme({".cm-tooltip":{zIndex:500,boxSizing:`border-box`},"&light .cm-tooltip":{border:`1px solid #bbb`,backgroundColor:`#f5f5f5`},"&light .cm-tooltip-section:not(:first-child)":{borderTop:`1px solid #bbb`},"&dark .cm-tooltip":{backgroundColor:`#333338`,color:`white`},".cm-tooltip-arrow":{height:`7px`,width:`14px`,position:`absolute`,zIndex:-1,overflow:`hidden`,"&:before, &:after":{content:`''`,position:`absolute`,width:0,height:0,borderLeft:`7px solid transparent`,borderRight:`7px solid transparent`},".cm-tooltip-above &":{bottom:`-7px`,"&:before":{borderTop:`7px solid #bbb`},"&:after":{borderTop:`7px solid #f5f5f5`,bottom:`1px`}},".cm-tooltip-below &":{top:`-7px`,"&:before":{borderBottom:`7px solid #bbb`},"&:after":{borderBottom:`7px solid #f5f5f5`,top:`1px`}}},"&dark .cm-tooltip .cm-tooltip-arrow":{"&:before":{borderTopColor:`#333338`,borderBottomColor:`#333338`},"&:after":{borderTopColor:`transparent`,borderBottomColor:`transparent`}}}),Dc={x:0,y:0},Oc=M.define({enables:[wc,Ec]}),kc=M.define({combine:e=>e.reduce((e,t)=>e.concat(t),[])}),Ac=class e{static create(t){return new e(t)}constructor(e){this.view=e,this.mounted=!1,this.dom=document.createElement(`div`),this.dom.classList.add(`cm-tooltip-hover`),this.manager=new yc(e,kc,(e,t)=>this.createHostedView(e,t),e=>e.dom.remove())}createHostedView(e,t){let n=e.create(this.view);return n.dom.classList.add(`cm-tooltip-section`),this.dom.insertBefore(n.dom,t?t.dom.nextSibling:this.dom.firstChild),this.mounted&&n.mount&&n.mount(this.view),n}mount(e){for(let t of this.manager.tooltipViews)t.mount&&t.mount(e);this.mounted=!0}positioned(e){for(let t of this.manager.tooltipViews)t.positioned&&t.positioned(e)}update(e){this.manager.update(e)}destroy(){var e;for(let t of this.manager.tooltipViews)(e=t.destroy)==null||e.call(t)}passProp(e){let t;for(let n of this.manager.tooltipViews){let r=n[e];if(r!==void 0){if(t===void 0)t=r;else if(t!==r)return}}return t}get offset(){return this.passProp(`offset`)}get getCoords(){return this.passProp(`getCoords`)}get overlap(){return this.passProp(`overlap`)}get resize(){return this.passProp(`resize`)}},jc=Oc.compute([kc],e=>{let t=e.facet(kc);return t.length===0?null:{pos:Math.min(...t.map(e=>e.pos)),end:Math.max(...t.map(e=>e.end??e.pos)),create:Ac.create,above:t[0].above,arrow:t.some(e=>e.arrow)}}),Mc=M.define(),Nc=class{constructor(e,t,n,r,i,a){this.view=e,this.source=t,this.field=n,this.locked=r,this.setHover=i,this.hoverTime=a,this.hoverTimeout=-1,this.restartTimeout=-1,this.pending=null,this.lastMove={x:0,y:0,target:e.dom,time:0},this.checkHover=this.checkHover.bind(this),e.dom.addEventListener(`mouseleave`,this.mouseleave=this.mouseleave.bind(this)),e.dom.addEventListener(`mousemove`,this.mousemove=this.mousemove.bind(this))}update(e){this.pending&&(this.pending=null,clearTimeout(this.restartTimeout),this.restartTimeout=setTimeout(()=>this.startHover(),20))}get active(){return this.view.state.field(this.field)}checkHover(){if(this.hoverTimeout=-1,this.active.length)return;let e=Date.now()-this.lastMove.time;e<this.hoverTime?this.hoverTimeout=setTimeout(this.checkHover,this.hoverTime-e):this.startHover()}startHover(){clearTimeout(this.restartTimeout);let{view:e,lastMove:t}=this,n=e.docView.tile.nearest(t.target);if(!n)return;let r,i=1;if(n.isWidget())r=n.posAtStart;else{if(r=e.posAtCoords(t),r==null)return;let n=e.coordsAtPos(r);if(!n||t.y<n.top||t.y>n.bottom||t.x<n.left-e.defaultCharacterWidth||t.x>n.right+e.defaultCharacterWidth)return;let a=e.bidiSpans(e.state.doc.lineAt(r)).find(e=>e.from<=r&&e.to>=r),o=a&&a.dir==V.RTL?-1:1;i=t.x<n.left?-o:o}this.activateHover(e,r,i)}activateHover(e,t,n,r){let i=this.source(e,t,n),a=t=>{if(t&&(!Array.isArray(t)||t.length)){let n=Array.isArray(t)?t:[t];r&&this.locked.set(n,r),e.dispatch({effects:this.setHover.of(n)})}};if(i&&`then`in i){let n=this.pending={pos:t};i.then(e=>{this.pending==n&&(this.pending=null,a(e))},t=>Jr(e.state,t,`hover tooltip`))}else a(i)}get tooltip(){let e=this.view.plugin(wc),t=e?e.manager.tooltips.findIndex(e=>e.create==Ac.create):-1;return t>-1?e.manager.tooltipViews[t]:null}mousemove(e){this.lastMove={x:e.clientX,y:e.clientY,target:e.target,time:Date.now()},this.hoverTimeout<0&&(this.hoverTimeout=setTimeout(this.checkHover,this.hoverTime));let{active:t,tooltip:n}=this;if(t.length&&!this.locked.has(t)&&n&&!Fc(n.dom,e)||this.pending){let{pos:n}=t[0]||this.pending,r=t[0]?.end??n;(n==r?this.view.posAtCoords(this.lastMove)!=n:!Ic(this.view,n,r,e.clientX,e.clientY))&&(this.view.dispatch({effects:this.setHover.of([])}),this.pending=null)}}mouseleave(e){clearTimeout(this.hoverTimeout),this.hoverTimeout=-1;let{active:t}=this;if(t.length&&!this.locked.has(t)){let{tooltip:t}=this;t&&t.dom.contains(e.relatedTarget)?this.watchTooltipLeave(t.dom):this.view.dispatch({effects:this.setHover.of([])})}}watchTooltipLeave(e){let t=n=>{e.removeEventListener(`mouseleave`,t);let{active:r}=this;r.length&&!this.locked.has(r)&&!this.view.dom.contains(n.relatedTarget)&&this.view.dispatch({effects:this.setHover.of([])})};e.addEventListener(`mouseleave`,t)}destroy(){clearTimeout(this.hoverTimeout),clearTimeout(this.restartTimeout),this.view.dom.removeEventListener(`mouseleave`,this.mouseleave),this.view.dom.removeEventListener(`mousemove`,this.mousemove)}},Pc=4;function Fc(e,t){let{left:n,right:r,top:i,bottom:a}=e.getBoundingClientRect(),o;if(o=e.querySelector(`.cm-tooltip-arrow`)){let e=o.getBoundingClientRect();i=Math.min(e.top,i),a=Math.max(e.bottom,a)}return t.clientX>=n-Pc&&t.clientX<=r+Pc&&t.clientY>=i-Pc&&t.clientY<=a+Pc}function Ic(e,t,n,r,i,a){let o=e.scrollDOM.getBoundingClientRect(),s=e.documentTop+e.documentPadding.top+e.contentHeight;if(o.left>r||o.right<r||o.top>i||Math.min(o.bottom,s)<i)return!1;let c=e.posAtCoords({x:r,y:i},!1);return c>=t&&c<=n}function Lc(e,t={}){let n=N.define(),r=new WeakMap,i=Je.define({create(){return[]},update(e,a){let o=r.get(e);if(e.length&&(t.hideOnChange&&(a.docChanged||a.selection)||o&&o(a)?e=[]:t.hideOn&&(e=e.filter(e=>!t.hideOn(a,e)))),a.docChanged&&e.length){let t=[];for(let n of e){let e=a.changes.mapPos(n.pos,-1,je.TrackDel);if(e!=null){let r=Object.assign(Object.create(null),n);r.pos=e,r.end!=null&&(r.end=a.changes.mapPos(r.end)),t.push(r)}}e=t}for(let t of a.effects)t.is(n)&&(e=t.value,o=void 0),(t.is(Bc)&&!t.value||t.value==i)&&(e=[]);return e.length&&o&&r.set(e,o),e},provide:e=>kc.from(e)}),a=U.define(a=>new Nc(a,e,i,r,n,t.hoverTime||300));return{active:i,extension:[i,a,Mc.of(a),jc]}}function Rc(e,t,n,r={}){let i=e.state.facet(Mc).map(t=>e.plugin(t)).filter(e=>!!e);if(r.tooltip&&r.tooltip.active){let e=i.find(e=>e.field==r.tooltip.active);e&&(i=[e])}for(let a of i)a.activateHover(e,t,n,r.until??(()=>!1))}function zc(e,t){let n=e.plugin(wc);if(!n)return null;let r=n.manager.tooltips.indexOf(t);return r<0?null:n.manager.tooltipViews[r]}var Bc=N.define(),Vc=M.define({combine(e){let t,n;for(let r of e)t||=r.topContainer,n||=r.bottomContainer;return{topContainer:t,bottomContainer:n}}});function Hc(e,t){let n=e.plugin(Uc),r=n?n.specs.indexOf(t):-1;return r>-1?n.panels[r]:null}var Uc=U.fromClass(class{constructor(e){this.input=e.state.facet(Kc),this.specs=this.input.filter(e=>e),this.panels=this.specs.map(t=>t(e));let t=e.state.facet(Vc);this.top=new Wc(e,!0,t.topContainer),this.bottom=new Wc(e,!1,t.bottomContainer),this.top.sync(this.panels.filter(e=>e.top)),this.bottom.sync(this.panels.filter(e=>!e.top));for(let e of this.panels)e.dom.classList.add(`cm-panel`),e.mount&&e.mount()}update(e){let t=e.state.facet(Vc);this.top.container!=t.topContainer&&(this.top.sync([]),this.top=new Wc(e.view,!0,t.topContainer)),this.bottom.container!=t.bottomContainer&&(this.bottom.sync([]),this.bottom=new Wc(e.view,!1,t.bottomContainer)),this.top.syncClasses(),this.bottom.syncClasses();let n=e.state.facet(Kc);if(n!=this.input){let t=n.filter(e=>e),r=[],i=[],a=[],o=[];for(let n of t){let t=this.specs.indexOf(n),s;t<0?(s=n(e.view),o.push(s)):(s=this.panels[t],s.update&&s.update(e)),r.push(s),(s.top?i:a).push(s)}this.specs=t,this.panels=r,this.top.sync(i),this.bottom.sync(a);for(let e of o)e.dom.classList.add(`cm-panel`),e.mount&&e.mount()}else for(let t of this.panels)t.update&&t.update(e)}destroy(){this.top.sync([]),this.bottom.sync([])}},{provide:e=>K.scrollMargins.of(t=>{let n=t.plugin(e);return n&&{top:n.top.scrollMargin(),bottom:n.bottom.scrollMargin()}})}),Wc=class{constructor(e,t,n){this.view=e,this.top=t,this.container=n,this.dom=void 0,this.classes=``,this.panels=[],this.syncClasses()}sync(e){for(let t of this.panels)t.destroy&&e.indexOf(t)<0&&t.destroy();this.panels=e,this.syncDOM()}syncDOM(){if(this.panels.length==0){this.dom&&=(this.dom.remove(),void 0);return}if(!this.dom){this.dom=document.createElement(`div`),this.dom.className=this.top?`cm-panels cm-panels-top`:`cm-panels cm-panels-bottom`;let e=this.container||this.view.dom;e.insertBefore(this.dom,this.top?e.firstChild:null)}let e=this.dom.firstChild;for(let t of this.panels)if(t.dom.parentNode==this.dom){for(;e!=t.dom;)e=Gc(e);e=e.nextSibling}else this.dom.insertBefore(t.dom,e);for(;e;)e=Gc(e)}scrollMargin(){return!this.dom||this.container?0:Math.max(0,this.top?this.dom.getBoundingClientRect().bottom-Math.max(0,this.view.scrollDOM.getBoundingClientRect().top):Math.min(innerHeight,this.view.scrollDOM.getBoundingClientRect().bottom)-this.dom.getBoundingClientRect().top)}syncClasses(){if(this.container&&this.classes!=this.view.themeClasses){for(let e of this.classes.split(` `))e&&this.container.classList.remove(e);for(let e of(this.classes=this.view.themeClasses).split(` `))e&&this.container.classList.add(e)}}};function Gc(e){let t=e.nextSibling;return e.remove(),t}var Kc=M.define({enables:Uc});function qc(e,t){let n,r=new Promise(e=>n=e),i=e=>Qc(e,t,n);e.state.field(Yc,!1)?e.dispatch({effects:Xc.of(i)}):e.dispatch({effects:N.appendConfig.of(Yc.init(()=>[i]))});let a=Zc.of(i);return{close:a,result:r.then(t=>((e.win.queueMicrotask||(t=>e.win.setTimeout(t,10)))(()=>{e.state.field(Yc).indexOf(i)>-1&&e.dispatch({effects:a})}),t))}}function Jc(e,t){let n=e.state.field(Yc,!1)||[];for(let r of n){let n=Hc(e,r);if(n&&n.dom.classList.contains(t))return n}return null}var Yc=Je.define({create(){return[]},update(e,t){for(let n of t.effects)n.is(Xc)?e=[n.value].concat(e):n.is(Zc)&&(e=e.filter(e=>e!=n.value));return e},provide:e=>Kc.computeN([e],t=>t.field(e))}),Xc=N.define(),Zc=N.define();function Qc(e,t,n){let r=t.content?t.content(e,()=>s(null)):null;if(!r){if(r=L(`form`),t.input){let e=L(`input`,t.input);/^(text|password|number|email|tel|url)$/.test(e.type)&&e.classList.add(`cm-textfield`),e.name||=`input`,r.appendChild(L(`label`,(t.label||``)+`: `,e))}else r.appendChild(document.createTextNode(t.label||``));r.appendChild(document.createTextNode(` `)),r.appendChild(L(`button`,{class:`cm-button`,type:`submit`},t.submitLabel||`OK`))}let i=r.nodeName==`FORM`?[r]:r.querySelectorAll(`form`);for(let e=0;e<i.length;e++){let t=i[e];t.addEventListener(`keydown`,e=>{e.keyCode==27?(e.preventDefault(),s(null)):e.keyCode==13&&(e.preventDefault(),s(t))}),t.addEventListener(`submit`,e=>{e.preventDefault(),s(t)})}let a=L(`div`,r,L(`button`,{onclick:()=>s(null),"aria-label":e.state.phrase(`close`),class:`cm-dialog-close`,type:`button`},[`×`]));t.class&&(a.className=t.class),a.classList.add(`cm-dialog`);let o=typeof t.focus==`string`?r.querySelector(t.focus):t.focus?r.querySelector(`input`)||r.querySelector(`button`):null;function s(t){a.contains(a.ownerDocument.activeElement)&&e.focus(),n(t)}return{dom:a,top:t.top,mount:()=>{o&&(`select`in o?o.select():`focus`in o&&o.focus())},destroy:()=>n(null)}}var $c=class extends kt{compare(e){return this==e||this.constructor==e.constructor&&this.eq(e)}eq(e){return!1}destroy(e){}};$c.prototype.elementClass=``,$c.prototype.toDOM=void 0,$c.prototype.mapMode=je.TrackBefore,$c.prototype.startSide=$c.prototype.endSide=-1,$c.prototype.point=!0;var el=M.define(),tl=M.define(),nl={class:``,renderEmptyElements:!1,elementStyle:``,markers:()=>I.empty,lineMarker:()=>null,widgetMarker:()=>null,lineMarkerChange:null,initialSpacer:null,updateSpacer:null,domEventHandlers:{},side:`before`},rl=M.define();function il(e){return[ol(),rl.of({...nl,...e})]}var al=M.define({combine:e=>e.some(e=>e)});function ol(e){let t=[sl];return e&&e.fixed===!1&&t.push(al.of(!0)),t}var sl=U.fromClass(class{constructor(e){this.view=e,this.domAfter=null,this.prevViewport=e.viewport,this.dom=document.createElement(`div`),this.dom.className=`cm-gutters cm-gutters-before`,this.dom.setAttribute(`aria-hidden`,`true`),this.dom.style.minHeight=this.view.contentHeight/this.view.scaleY+`px`,this.gutters=e.state.facet(rl).map(t=>new dl(e,t)),this.fixed=!e.state.facet(al);for(let e of this.gutters)e.config.side==`after`?this.getDOMAfter().appendChild(e.dom):this.dom.appendChild(e.dom);this.fixed&&(this.dom.style.position=`sticky`),this.syncGutters(!1),e.scrollDOM.insertBefore(this.dom,e.contentDOM)}getDOMAfter(){return this.domAfter||(this.domAfter=document.createElement(`div`),this.domAfter.className=`cm-gutters cm-gutters-after`,this.domAfter.setAttribute(`aria-hidden`,`true`),this.domAfter.style.minHeight=this.view.contentHeight/this.view.scaleY+`px`,this.domAfter.style.position=this.fixed?`sticky`:``,this.view.scrollDOM.appendChild(this.domAfter)),this.domAfter}update(e){if(this.updateGutters(e)){let t=this.prevViewport,n=e.view.viewport,r=Math.min(t.to,n.to)-Math.max(t.from,n.from);this.syncGutters(r<(n.to-n.from)*.8)}if(e.geometryChanged){let e=this.view.contentHeight/this.view.scaleY+`px`;this.dom.style.minHeight=e,this.domAfter&&(this.domAfter.style.minHeight=e)}this.view.state.facet(al)!=!this.fixed&&(this.fixed=!this.fixed,this.dom.style.position=this.fixed?`sticky`:``,this.domAfter&&(this.domAfter.style.position=this.fixed?`sticky`:``)),this.prevViewport=e.view.viewport}syncGutters(e){let t=this.dom.nextSibling;e&&(this.dom.remove(),this.domAfter&&this.domAfter.remove());let n=I.iter(this.view.state.facet(el),this.view.viewport.from),r=[],i=this.gutters.map(e=>new ul(e,this.view.viewport,-this.view.documentPadding.top));for(let e of this.view.viewportLineBlocks)if(r.length&&(r=[]),Array.isArray(e.type)){let t=!0;for(let a of e.type)if(a.type==kn.Text&&t){ll(n,r,a.from);for(let e of i)e.line(this.view,a,r);t=!1}else if(a.widget)for(let e of i)e.widget(this.view,a)}else if(e.type==kn.Text){ll(n,r,e.from);for(let t of i)t.line(this.view,e,r)}else if(e.widget)for(let t of i)t.widget(this.view,e);for(let e of i)e.finish();e&&(this.view.scrollDOM.insertBefore(this.dom,t),this.domAfter&&this.view.scrollDOM.appendChild(this.domAfter))}updateGutters(e){let t=e.startState.facet(rl),n=e.state.facet(rl),r=e.docChanged||e.heightChanged||e.viewportChanged||!I.eq(e.startState.facet(el),e.state.facet(el),e.view.viewport.from,e.view.viewport.to);if(t==n)for(let t of this.gutters)t.update(e)&&(r=!0);else{r=!0;let i=[];for(let r of n){let n=t.indexOf(r);n<0?i.push(new dl(this.view,r)):(this.gutters[n].update(e),i.push(this.gutters[n]))}for(let e of this.gutters)e.dom.remove(),i.indexOf(e)<0&&e.destroy();for(let e of i)e.config.side==`after`?this.getDOMAfter().appendChild(e.dom):this.dom.appendChild(e.dom);this.gutters=i}return r}destroy(){for(let e of this.gutters)e.destroy();this.dom.remove(),this.domAfter&&this.domAfter.remove()}},{provide:e=>K.scrollMargins.of(t=>{let n=t.plugin(e);if(!n||n.gutters.length==0||!n.fixed)return null;let r=n.dom.offsetWidth*t.scaleX,i=n.domAfter?n.domAfter.offsetWidth*t.scaleX:0;return t.textDirection==V.LTR?{left:r,right:i}:{right:r,left:i}})});function cl(e){return Array.isArray(e)?e:[e]}function ll(e,t,n){for(;e.value&&e.from<=n;)e.from==n&&t.push(e.value),e.next()}var ul=class{constructor(e,t,n){this.gutter=e,this.height=n,this.i=0,this.cursor=I.iter(e.markers,t.from)}addElement(e,t,n){let{gutter:r}=this,i=(t.top-this.height)/e.scaleY,a=t.height/e.scaleY;if(this.i==r.elements.length){let t=new fl(e,a,i,n);r.elements.push(t),r.dom.appendChild(t.dom)}else r.elements[this.i].update(e,a,i,n);this.height=t.bottom,this.i++}line(e,t,n){let r=[];ll(this.cursor,r,t.from),n.length&&(r=r.concat(n));let i=this.gutter.config.lineMarker(e,t,r);i&&r.unshift(i);let a=this.gutter;(r.length!=0||a.config.renderEmptyElements)&&this.addElement(e,t,r)}widget(e,t){let n=this.gutter.config.widgetMarker(e,t.widget,t),r=n?[n]:null;for(let n of e.state.facet(tl)){let i=n(e,t.widget,t);i&&(r||=[]).push(i)}r&&this.addElement(e,t,r)}finish(){let e=this.gutter;for(;e.elements.length>this.i;){let t=e.elements.pop();e.dom.removeChild(t.dom),t.destroy()}}},dl=class{constructor(e,t){this.view=e,this.config=t,this.elements=[],this.spacer=null,this.dom=document.createElement(`div`),this.dom.className=`cm-gutter`+(this.config.class?` `+this.config.class:``);for(let n in t.domEventHandlers)this.dom.addEventListener(n,r=>{let i=r.target,a;if(i!=this.dom&&this.dom.contains(i)){for(;i.parentNode!=this.dom;)i=i.parentNode;let e=i.getBoundingClientRect();a=(e.top+e.bottom)/2}else a=r.clientY;let o=e.lineBlockAtHeight(a-e.documentTop);t.domEventHandlers[n](e,o,r)&&r.preventDefault()});this.markers=cl(t.markers(e)),t.initialSpacer&&(this.spacer=new fl(e,0,0,[t.initialSpacer(e)]),this.dom.appendChild(this.spacer.dom),this.spacer.dom.style.cssText+=`visibility: hidden; pointer-events: none`)}update(e){let t=this.markers;if(this.markers=cl(this.config.markers(e.view)),this.spacer&&this.config.updateSpacer){let t=this.config.updateSpacer(this.spacer.markers[0],e);t!=this.spacer.markers[0]&&this.spacer.update(e.view,0,0,[t])}let n=e.view.viewport;return!I.eq(this.markers,t,n.from,n.to)||(this.config.lineMarkerChange?this.config.lineMarkerChange(e):!1)}destroy(){for(let e of this.elements)e.destroy()}},fl=class{constructor(e,t,n,r){this.height=-1,this.above=0,this.markers=[],this.dom=document.createElement(`div`),this.dom.className=`cm-gutterElement`,this.update(e,t,n,r)}update(e,t,n,r){this.height!=t&&(this.height=t,this.dom.style.height=t+`px`),this.above!=n&&(this.dom.style.marginTop=(this.above=n)?n+`px`:``),pl(this.markers,r)||this.setMarkers(e,r)}setMarkers(e,t){let n=`cm-gutterElement`,r=this.dom.firstChild;for(let i=0,a=0;;){let o=a,s=i<t.length?t[i++]:null,c=!1;if(s){let e=s.elementClass;e&&(n+=` `+e);for(let e=a;e<this.markers.length;e++)if(this.markers[e].compare(s)){o=e,c=!0;break}}else o=this.markers.length;for(;a<o;){let e=this.markers[a++];if(e.toDOM){e.destroy(r);let t=r.nextSibling;r.remove(),r=t}}if(!s)break;s.toDOM&&(c?r=r.nextSibling:this.dom.insertBefore(s.toDOM(e),r)),c&&a++}this.dom.className=n,this.markers=t}destroy(){this.setMarkers(null,[])}};function pl(e,t){if(e.length!=t.length)return!1;for(let n=0;n<e.length;n++)if(!e[n].compare(t[n]))return!1;return!0}var ml=M.define(),hl=M.define(),gl=M.define({combine(e){return Ot(e,{formatNumber:String,domEventHandlers:{}},{domEventHandlers(e,t){let n=Object.assign({},e);for(let e in t){let r=n[e],i=t[e];n[e]=r?(e,t,n)=>r(e,t,n)||i(e,t,n):i}return n}})}}),_l=class extends $c{constructor(e){super(),this.number=e}eq(e){return this.number==e.number}toDOM(){return document.createTextNode(this.number)}};function vl(e,t){return e.state.facet(gl).formatNumber(t,e.state)}var yl=rl.compute([gl],e=>({class:`cm-lineNumbers`,renderEmptyElements:!1,markers(e){return e.state.facet(ml)},lineMarker(e,t,n){return n.some(e=>e.toDOM)?null:new _l(vl(e,e.state.doc.lineAt(t.from).number))},widgetMarker:(e,t,n)=>{for(let r of e.state.facet(hl)){let i=r(e,t,n);if(i)return i}return null},lineMarkerChange:e=>e.startState.facet(gl)!=e.state.facet(gl),initialSpacer(e){return new _l(vl(e,xl(e.state.doc.lines)))},updateSpacer(e,t){let n=vl(t.view,xl(t.view.state.doc.lines));return n==e.number?e:new _l(n)},domEventHandlers:e.facet(gl).domEventHandlers,side:`before`}));function bl(e={}){return[gl.of(e),ol(),yl]}function xl(e){let t=9;for(;t<e;)t=t*10+9;return t}var Sl=new class extends $c{constructor(){super(...arguments),this.elementClass=`cm-activeLineGutter`}},Cl=el.compute([`selection`],e=>{let t=[],n=-1;for(let r of e.selection.ranges){let i=e.doc.lineAt(r.head).from;i>n&&(n=i,t.push(Sl.range(i)))}return I.of(t)});function wl(){return Cl}var Tl=1024,El=0,Dl=class{constructor(e,t){this.from=e,this.to=t}},q=class{constructor(e={}){this.id=El++,this.perNode=!!e.perNode,this.deserialize=e.deserialize||(()=>{throw Error(`This node type doesn't define a deserialize function`)}),this.combine=e.combine||null}add(e){if(this.perNode)throw RangeError(`Can't add per-node props to node types`);return typeof e!=`function`&&(e=Al.match(e)),t=>{let n=e(t);return n===void 0?null:[this,n]}}};q.closedBy=new q({deserialize:e=>e.split(` `)}),q.openedBy=new q({deserialize:e=>e.split(` `)}),q.group=new q({deserialize:e=>e.split(` `)}),q.isolate=new q({deserialize:e=>{if(e&&e!=`rtl`&&e!=`ltr`&&e!=`auto`)throw RangeError(`Invalid value for isolate: `+e);return e||`auto`}}),q.contextHash=new q({perNode:!0}),q.lookAhead=new q({perNode:!0}),q.mounted=new q({perNode:!0});var Ol=class{constructor(e,t,n,r=!1){this.tree=e,this.overlay=t,this.parser=n,this.bracketed=r}static get(e){return e&&e.props&&e.props[q.mounted.id]}},kl=Object.create(null),Al=class e{constructor(e,t,n,r=0){this.name=e,this.props=t,this.id=n,this.flags=r}static define(t){let n=t.props&&t.props.length?Object.create(null):kl,r=+!!t.top|(t.skipped?2:0)|(t.error?4:0)|(t.name==null?8:0),i=new e(t.name||``,n,t.id,r);if(t.props){for(let e of t.props)if(Array.isArray(e)||(e=e(i)),e){if(e[0].perNode)throw RangeError(`Can't store a per-node prop on a node type`);n[e[0].id]=e[1]}}return i}prop(e){return this.props[e.id]}get isTop(){return(this.flags&1)>0}get isSkipped(){return(this.flags&2)>0}get isError(){return(this.flags&4)>0}get isAnonymous(){return(this.flags&8)>0}is(e){if(typeof e==`string`){if(this.name==e)return!0;let t=this.prop(q.group);return t?t.indexOf(e)>-1:!1}return this.id==e}static match(e){let t=Object.create(null);for(let n in e)for(let r of n.split(` `))t[r]=e[n];return e=>{for(let n=e.prop(q.group),r=-1;r<(n?n.length:0);r++){let i=t[r<0?e.name:n[r]];if(i)return i}}}};Al.none=new Al(``,Object.create(null),0,8);var jl=class e{constructor(e){this.types=e;for(let t=0;t<e.length;t++)if(e[t].id!=t)throw RangeError(`Node type ids should correspond to array positions when creating a node set`)}extend(...t){let n=[];for(let e of this.types){let r=null;for(let n of t){let t=n(e);if(t){r||=Object.assign({},e.props);let n=t[1],i=t[0];i.combine&&i.id in r&&(n=i.combine(r[i.id],n)),r[i.id]=n}}n.push(r?new Al(e.name,r,e.id,e.flags):e)}return new e(n)}},Ml=new WeakMap,Nl=new WeakMap,J;(function(e){e[e.ExcludeBuffers=1]=`ExcludeBuffers`,e[e.IncludeAnonymous=2]=`IncludeAnonymous`,e[e.IgnoreMounts=4]=`IgnoreMounts`,e[e.IgnoreOverlays=8]=`IgnoreOverlays`,e[e.EnterBracketed=16]=`EnterBracketed`})(J||={});var Y=class e{constructor(e,t,n,r,i){if(this.type=e,this.children=t,this.positions=n,this.length=r,this.props=null,i&&i.length){this.props=Object.create(null);for(let[e,t]of i)this.props[typeof e==`number`?e:e.id]=t}}toString(){let e=Ol.get(this);if(e&&!e.overlay)return e.tree.toString();let t=``;for(let e of this.children){let n=e.toString();n&&(t&&(t+=`,`),t+=n)}return this.type.name?(/\W/.test(this.type.name)&&!this.type.isError?JSON.stringify(this.type.name):this.type.name)+(t.length?`(`+t+`)`:``):t}cursor(e=0){return new ql(this.topNode,e)}cursorAt(e,t=0,n=0){let r=new ql(Ml.get(this)||this.topNode);return r.moveTo(e,t),Ml.set(this,r._tree),r}get topNode(){return new zl(this,0,0,null)}resolve(e,t=0){let n=Ll(Ml.get(this)||this.topNode,e,t,!1);return Ml.set(this,n),n}resolveInner(e,t=0){let n=Ll(Nl.get(this)||this.topNode,e,t,!0);return Nl.set(this,n),n}resolveStack(e,t=0){return Kl(this,e,t)}iterate(e){let{enter:t,leave:n,from:r=0,to:i=this.length}=e,a=e.mode||0,o=(a&J.IncludeAnonymous)>0;for(let e=this.cursor(a|J.IncludeAnonymous);;){let a=!1;if(e.from<=i&&e.to>=r&&(!o&&e.type.isAnonymous||t(e)!==!1)){if(e.firstChild())continue;a=!0}for(;a&&n&&(o||!e.type.isAnonymous)&&n(e),!e.nextSibling();){if(!e.parent())return;a=!0}}}prop(e){return e.perNode?this.props?this.props[e.id]:void 0:this.type.prop(e)}get propValues(){let e=[];if(this.props)for(let t in this.props)e.push([+t,this.props[t]]);return e}balance(t={}){return this.children.length<=8?this:Ql(Al.none,this.children,this.positions,0,this.children.length,0,this.length,(t,n,r)=>new e(this.type,t,n,r,this.propValues),t.makeTree||((t,n,r)=>new e(Al.none,t,n,r)))}static build(e){return Yl(e)}};Y.empty=new Y(Al.none,[],[],0);var Pl=class e{constructor(e,t){this.buffer=e,this.index=t}get id(){return this.buffer[this.index-4]}get start(){return this.buffer[this.index-3]}get end(){return this.buffer[this.index-2]}get size(){return this.buffer[this.index-1]}get pos(){return this.index}next(){this.index-=4}fork(){return new e(this.buffer,this.index)}},Fl=class e{constructor(e,t,n){this.buffer=e,this.length=t,this.set=n}get type(){return Al.none}toString(){let e=[];for(let t=0;t<this.buffer.length;)e.push(this.childString(t)),t=this.buffer[t+3];return e.join(`,`)}childString(e){let t=this.buffer[e],n=this.buffer[e+3],r=this.set.types[t],i=r.name;if(/\W/.test(i)&&!r.isError&&(i=JSON.stringify(i)),e+=4,n==e)return i;let a=[];for(;e<n;)a.push(this.childString(e)),e=this.buffer[e+3];return i+`(`+a.join(`,`)+`)`}findChild(e,t,n,r,i){let{buffer:a}=this,o=-1;for(let s=e;s!=t&&!(Il(i,r,a[s+1],a[s+2])&&(o=s,n>0));s=a[s+3]);return o}slice(t,n,r){let i=this.buffer,a=new Uint16Array(n-t),o=0;for(let e=t,s=0;e<n;){a[s++]=i[e++],a[s++]=i[e++]-r;let n=a[s++]=i[e++]-r;a[s++]=i[e++]-t,o=Math.max(o,n)}return new e(a,o,this.set)}};function Il(e,t,n,r){switch(e){case-2:return n<t;case-1:return r>=t&&n<t;case 0:return n<t&&r>t;case 1:return n<=t&&r>t;case 2:return r>t;case 4:return!0}}function Ll(e,t,n,r){for(;e.from==e.to||(n<1?e.from>=t:e.from>t)||(n>-1?e.to<=t:e.to<t);){let t=!r&&e instanceof zl&&e.index<0?null:e.parent;if(!t)return e;e=t}let i=r?0:J.IgnoreOverlays;if(r)for(let r=e,a=r.parent;a;r=a,a=r.parent)r instanceof zl&&r.index<0&&a.enter(t,n,i)?.from!=r.from&&(e=a);for(;;){let r=e.enter(t,n,i);if(!r)return e;e=r}}var Rl=class{cursor(e=0){return new ql(this,e)}getChild(e,t=null,n=null){let r=Bl(this,e,t,n);return r.length?r[0]:null}getChildren(e,t=null,n=null){return Bl(this,e,t,n)}resolve(e,t=0){return Ll(this,e,t,!1)}resolveInner(e,t=0){return Ll(this,e,t,!0)}matchContext(e){return Vl(this.parent,e)}enterUnfinishedNodesBefore(e){let t=this.childBefore(e),n=this;for(;t;){let e=t.lastChild;if(!e||e.to!=t.to)break;e.type.isError&&e.from==e.to?(n=t,t=e.prevSibling):t=e}return n}get node(){return this}get next(){return this.parent}},zl=class e extends Rl{constructor(e,t,n,r){super(),this._tree=e,this.from=t,this.index=n,this._parent=r}get type(){return this._tree.type}get name(){return this._tree.type.name}get to(){return this.from+this._tree.length}nextChild(t,n,r,i,a=0){for(let o=this;;){for(let{children:s,positions:c}=o._tree,l=n>0?s.length:-1;t!=l;t+=n){let l=s[t],u=c[t]+o.from,d;if(a&J.EnterBracketed&&l instanceof Y&&(d=Ol.get(l))&&!d.overlay&&d.bracketed&&r>=u&&r<=u+l.length||Il(i,r,u,u+l.length)){if(l instanceof Fl){if(a&J.ExcludeBuffers)continue;let e=l.findChild(0,l.buffer.length,n,r-u,i);if(e>-1)return new Ul(new Hl(o,l,t,u),null,e)}else if(a&J.IncludeAnonymous||!l.type.isAnonymous||Jl(l)){let s;if(!(a&J.IgnoreMounts)&&(s=Ol.get(l))&&!s.overlay)return new e(s.tree,u,t,o);let c=new e(l,u,t,o);return a&J.IncludeAnonymous||!c.type.isAnonymous?c:c.nextChild(n<0?l.children.length-1:0,n,r,i,a)}}}if(a&J.IncludeAnonymous||!o.type.isAnonymous||(t=o.index>=0?o.index+n:n<0?-1:o._parent._tree.children.length,o=o._parent,!o))return null}}get firstChild(){return this.nextChild(0,1,0,4)}get lastChild(){return this.nextChild(this._tree.children.length-1,-1,0,4)}childAfter(e){return this.nextChild(0,1,e,2)}childBefore(e){return this.nextChild(this._tree.children.length-1,-1,e,-2)}prop(e){return this._tree.prop(e)}enter(t,n,r=0){let i;if(!(r&J.IgnoreOverlays)&&(i=Ol.get(this._tree))&&i.overlay){let a=t-this.from,o=r&J.EnterBracketed&&i.bracketed;for(let{from:t,to:r}of i.overlay)if((n>0||o?t<=a:t<a)&&(n<0||o?r>=a:r>a))return new e(i.tree,i.overlay[0].from+this.from,-1,this)}return this.nextChild(0,1,t,n,r)}nextSignificantParent(){let e=this;for(;e.type.isAnonymous&&e._parent;)e=e._parent;return e}get parent(){return this._parent?this._parent.nextSignificantParent():null}get nextSibling(){return this._parent&&this.index>=0?this._parent.nextChild(this.index+1,1,0,4):null}get prevSibling(){return this._parent&&this.index>=0?this._parent.nextChild(this.index-1,-1,0,4):null}get tree(){return this._tree}toTree(){return this._tree}toString(){return this._tree.toString()}};function Bl(e,t,n,r){let i=e.cursor(),a=[];if(!i.firstChild())return a;if(n!=null){for(let e=!1;!e;)if(e=i.type.is(n),!i.nextSibling())return a}for(;;){if(r!=null&&i.type.is(r))return a;if(i.type.is(t)&&a.push(i.node),!i.nextSibling())return r==null?a:[]}}function Vl(e,t,n=t.length-1){for(let r=e;n>=0;r=r.parent){if(!r)return!1;if(!r.type.isAnonymous){if(t[n]&&t[n]!=r.name)return!1;n--}}return!0}var Hl=class{constructor(e,t,n,r){this.parent=e,this.buffer=t,this.index=n,this.start=r}},Ul=class e extends Rl{get name(){return this.type.name}get from(){return this.context.start+this.context.buffer.buffer[this.index+1]}get to(){return this.context.start+this.context.buffer.buffer[this.index+2]}constructor(e,t,n){super(),this.context=e,this._parent=t,this.index=n,this.type=e.buffer.set.types[e.buffer.buffer[n]]}child(t,n,r){let{buffer:i}=this.context,a=i.findChild(this.index+4,i.buffer[this.index+3],t,n-this.context.start,r);return a<0?null:new e(this.context,this,a)}get firstChild(){return this.child(1,0,4)}get lastChild(){return this.child(-1,0,4)}childAfter(e){return this.child(1,e,2)}childBefore(e){return this.child(-1,e,-2)}prop(e){return this.type.prop(e)}enter(t,n,r=0){if(r&J.ExcludeBuffers)return null;let{buffer:i}=this.context,a=i.findChild(this.index+4,i.buffer[this.index+3],n>0?1:-1,t-this.context.start,n);return a<0?null:new e(this.context,this,a)}get parent(){return this._parent||this.context.parent.nextSignificantParent()}externalSibling(e){return this._parent?null:this.context.parent.nextChild(this.context.index+e,e,0,4)}get nextSibling(){let{buffer:t}=this.context,n=t.buffer[this.index+3];return n<(this._parent?t.buffer[this._parent.index+3]:t.buffer.length)?new e(this.context,this._parent,n):this.externalSibling(1)}get prevSibling(){let{buffer:t}=this.context,n=this._parent?this._parent.index+4:0;return this.index==n?this.externalSibling(-1):new e(this.context,this._parent,t.findChild(n,this.index,-1,0,4))}get tree(){return null}toTree(){let e=[],t=[],{buffer:n}=this.context,r=this.index+4,i=n.buffer[this.index+3];if(i>r){let a=n.buffer[this.index+1];e.push(n.slice(r,i,a)),t.push(0)}return new Y(this.type,e,t,this.to-this.from)}toString(){return this.context.buffer.childString(this.index)}};function Wl(e){if(!e.length)return null;let t=0,n=e[0];for(let r=1;r<e.length;r++){let i=e[r];(i.from>n.from||i.to<n.to)&&(n=i,t=r)}let r=n instanceof zl&&n.index<0?null:n.parent,i=e.slice();return r?i[t]=r:i.splice(t,1),new Gl(i,n)}var Gl=class{constructor(e,t){this.heads=e,this.node=t}get next(){return Wl(this.heads)}};function Kl(e,t,n){let r=e.resolveInner(t,n),i=null;for(let e=r instanceof zl?r:r.context.parent;e;e=e.parent)if(e.index<0){let a=e.parent;(i||=[r]).push(a.resolve(t,n)),e=a}else{let a=Ol.get(e.tree);if(a&&a.overlay&&a.overlay[0].from<=t&&a.overlay[a.overlay.length-1].to>=t){let o=new zl(a.tree,a.overlay[0].from+e.from,-1,e);(i||=[r]).push(Ll(o,t,n,!1))}}return i?Wl(i):r}var ql=class{get name(){return this.type.name}constructor(e,t=0){if(this.buffer=null,this.stack=[],this.index=0,this.bufferNode=null,this.mode=t&~J.EnterBracketed,e instanceof zl)this.yieldNode(e);else{this._tree=e.context.parent,this.buffer=e.context;for(let t=e._parent;t;t=t._parent)this.stack.unshift(t.index);this.bufferNode=e,this.yieldBuf(e.index)}}yieldNode(e){return e?(this._tree=e,this.type=e.type,this.from=e.from,this.to=e.to,!0):!1}yieldBuf(e,t){this.index=e;let{start:n,buffer:r}=this.buffer;return this.type=t||r.set.types[r.buffer[e]],this.from=n+r.buffer[e+1],this.to=n+r.buffer[e+2],!0}yield(e){return e?e instanceof zl?(this.buffer=null,this.yieldNode(e)):(this.buffer=e.context,this.yieldBuf(e.index,e.type)):!1}toString(){return this.buffer?this.buffer.buffer.childString(this.index):this._tree.toString()}enterChild(e,t,n){if(!this.buffer)return this.yield(this._tree.nextChild(e<0?this._tree._tree.children.length-1:0,e,t,n,this.mode));let{buffer:r}=this.buffer,i=r.findChild(this.index+4,r.buffer[this.index+3],e,t-this.buffer.start,n);return i<0?!1:(this.stack.push(this.index),this.yieldBuf(i))}firstChild(){return this.enterChild(1,0,4)}lastChild(){return this.enterChild(-1,0,4)}childAfter(e){return this.enterChild(1,e,2)}childBefore(e){return this.enterChild(-1,e,-2)}enter(e,t,n=this.mode){return this.buffer?n&J.ExcludeBuffers?!1:this.enterChild(1,e,t):this.yield(this._tree.enter(e,t,n))}parent(){if(!this.buffer)return this.yieldNode(this.mode&J.IncludeAnonymous?this._tree._parent:this._tree.parent);if(this.stack.length)return this.yieldBuf(this.stack.pop());let e=this.mode&J.IncludeAnonymous?this.buffer.parent:this.buffer.parent.nextSignificantParent();return this.buffer=null,this.yieldNode(e)}sibling(e){if(!this.buffer)return this._tree._parent?this.yield(this._tree.index<0?null:this._tree._parent.nextChild(this._tree.index+e,e,0,4,this.mode)):!1;let{buffer:t}=this.buffer,n=this.stack.length-1;if(e<0){let e=n<0?0:this.stack[n]+4;if(this.index!=e)return this.yieldBuf(t.findChild(e,this.index,-1,0,4))}else{let e=t.buffer[this.index+3];if(e<(n<0?t.buffer.length:t.buffer[this.stack[n]+3]))return this.yieldBuf(e)}return n<0&&this.yield(this.buffer.parent.nextChild(this.buffer.index+e,e,0,4,this.mode))}nextSibling(){return this.sibling(1)}prevSibling(){return this.sibling(-1)}atLastNode(e){let t,n,{buffer:r}=this;if(r){if(e>0){if(this.index<r.buffer.buffer.length)return!1}else for(let e=0;e<this.index;e++)if(r.buffer.buffer[e+3]<this.index)return!1;({index:t,parent:n}=r)}else({index:t,_parent:n}=this._tree);for(;n;{index:t,_parent:n}=n)if(t>-1)for(let r=t+e,i=e<0?-1:n._tree.children.length;r!=i;r+=e){let e=n._tree.children[r];if(this.mode&J.IncludeAnonymous||e instanceof Fl||!e.type.isAnonymous||Jl(e))return!1}return!0}move(e,t){if(t&&this.enterChild(e,0,4))return!0;for(;;){if(this.sibling(e))return!0;if(this.atLastNode(e)||!this.parent())return!1}}next(e=!0){return this.move(1,e)}prev(e=!0){return this.move(-1,e)}moveTo(e,t=0){for(;(this.from==this.to||(t<1?this.from>=e:this.from>e)||(t>-1?this.to<=e:this.to<e))&&this.parent(););for(;this.enterChild(1,e,t););return this}get node(){if(!this.buffer)return this._tree;let e=this.bufferNode,t=null,n=0;if(e&&e.context==this.buffer)scan:for(let r=this.index,i=this.stack.length;i>=0;){for(let a=e;a;a=a._parent)if(a.index==r){if(r==this.index)return a;t=a,n=i+1;break scan}r=this.stack[--i]}for(let e=n;e<this.stack.length;e++)t=new Ul(this.buffer,t,this.stack[e]);return this.bufferNode=new Ul(this.buffer,t,this.index)}get tree(){return this.buffer?null:this._tree._tree}iterate(e,t){for(let n=0;;){let r=!1;if(this.type.isAnonymous||e(this)!==!1){if(this.firstChild()){n++;continue}this.type.isAnonymous||(r=!0)}for(;;){if(r&&t&&t(this),r=this.type.isAnonymous,!n)return;if(this.nextSibling())break;this.parent(),n--,r=!0}}}matchContext(e){if(!this.buffer)return Vl(this.node.parent,e);let{buffer:t}=this.buffer,{types:n}=t.set;for(let r=e.length-1,i=this.stack.length-1;r>=0;i--){if(i<0)return Vl(this._tree,e,r);let a=n[t.buffer[this.stack[i]]];if(!a.isAnonymous){if(e[r]&&e[r]!=a.name)return!1;r--}}return!0}};function Jl(e){return e.children.some(e=>e instanceof Fl||!e.type.isAnonymous||Jl(e))}function Yl(e){let{buffer:t,nodeSet:n,maxBufferLength:r=Tl,reused:i=[],minRepeatType:a=n.types.length}=e,o=Array.isArray(t)?new Pl(t,t.length):t,s=n.types,c=0,l=0;function u(e,t,_,v,y,ee){let{id:b,start:x,end:S,size:te}=o,ne=l,re=c;if(te<0){if(o.next(),te==-1){let t=i[b];_.push(t),v.push(x-e);return}if(te==-3){c=b;return}if(te==-4){l=b;return}throw RangeError(`Unrecognized record size: ${te}`)}let C=s[b],ie,ae,w=x-e;if(S-x<=r&&(ae=h(o.pos-t,y))){let t=new Uint16Array(ae.size-ae.skip),r=o.pos-ae.size,i=t.length;for(;o.pos>r;)i=g(ae.start,t,i);ie=new Fl(t,S-ae.start,n),w=ae.start-e}else{let e=o.pos-te;o.next();let t=[],n=[],i=b>=a?b:-1,s=0,c=S;for(;o.pos>e;)i>=0&&o.id==i&&o.size>=0?(o.end<=c-r&&(p(t,n,x,s,o.end,c,i,ne,re),s=t.length,c=o.end),o.next()):ee>2500?d(x,e,t,n):u(x,e,t,n,i,ee+1);if(i>=0&&s>0&&s<t.length&&p(t,n,x,s,x,c,i,ne,re),t.reverse(),n.reverse(),i>-1&&s>0){let e=f(C,re);ie=Ql(C,t,n,0,t.length,0,S-x,e,e)}else ie=m(C,t,n,S-x,ne-S,re)}_.push(ie),v.push(w)}function d(e,t,i,a){let s=[],c=0,l=-1;for(;o.pos>t;){let{id:e,start:t,end:n,size:i}=o;if(i>4)o.next();else if(l>-1&&t<l)break;else l<0&&(l=n-r),s.push(e,t,n),c++,o.next()}if(c){let t=new Uint16Array(c*4),r=s[s.length-2];for(let e=s.length-3,n=0;e>=0;e-=3)t[n++]=s[e],t[n++]=s[e+1]-r,t[n++]=s[e+2]-r,t[n++]=n;i.push(new Fl(t,s[2]-r,n)),a.push(r-e)}}function f(e,t){return(n,r,i)=>{let a=0,o=n.length-1,s,c;if(o>=0&&(s=n[o])instanceof Y){if(!o&&s.type==e&&s.length==i)return s;(c=s.prop(q.lookAhead))&&(a=r[o]+s.length+c)}return m(e,n,r,i,a,t)}}function p(e,t,r,i,a,o,s,c,l){let u=[],d=[];for(;e.length>i;)u.push(e.pop()),d.push(t.pop()+r-a);e.push(m(n.types[s],u,d,o-a,c-o,l)),t.push(a-r)}function m(e,t,n,r,i,a,o){if(a){let e=[q.contextHash,a];o=o?[e].concat(o):[e]}if(i>25){let e=[q.lookAhead,i];o=o?[e].concat(o):[e]}return new Y(e,t,n,r,o)}function h(e,t){let n=o.fork(),i=0,s=0,c=0,l=n.end-r,u={size:0,start:0,skip:0};scan:for(let r=n.pos-e;n.pos>r;){let e=n.size;if(n.id==t&&e>=0){u.size=i,u.start=s,u.skip=c,c+=4,i+=4,n.next();continue}let o=n.pos-e;if(e<0||o<r||n.start<l)break;let d=n.id>=a?4:0,f=n.start;for(n.next();n.pos>o;){if(n.size<0){if(n.size==-3||n.size==-4)d+=4;else break scan}else n.id>=a&&(d+=4);n.next()}s=f,i+=e,c+=d}return(t<0||i==e)&&(u.size=i,u.start=s,u.skip=c),u.size>4?u:void 0}function g(e,t,n){let{id:r,start:i,end:s,size:u}=o;if(o.next(),u>=0&&r<a){let a=n;if(u>4){let r=o.pos-(u-4);for(;o.pos>r;)n=g(e,t,n)}t[--n]=a,t[--n]=s-e,t[--n]=i-e,t[--n]=r}else u==-3?c=r:u==-4&&(l=r);return n}let _=[],v=[];for(;o.pos>0;)u(e.start||0,e.bufferStart||0,_,v,-1,0);let y=e.length??(_.length?v[0]+_[0].length:0);return new Y(s[e.topID],_.reverse(),v.reverse(),y)}var Xl=new WeakMap;function Zl(e,t){if(!e.isAnonymous||t instanceof Fl||t.type!=e)return 1;let n=Xl.get(t);if(n==null){n=1;for(let r of t.children){if(r.type!=e||!(r instanceof Y)){n=1;break}n+=Zl(e,r)}Xl.set(t,n)}return n}function Ql(e,t,n,r,i,a,o,s,c){let l=0;for(let n=r;n<i;n++)l+=Zl(e,t[n]);let u=Math.ceil(l*1.5/8),d=[],f=[];function p(t,n,r,i,o){for(let s=r;s<i;){let r=s,l=n[s],m=Zl(e,t[s]);for(s++;s<i;s++){let n=Zl(e,t[s]);if(m+n>=u)break;m+=n}if(s==r+1){if(m>u){let e=t[r];p(e.children,e.positions,0,e.children.length,n[r]+o);continue}d.push(t[r])}else{let i=n[s-1]+t[s-1].length-l;d.push(Ql(e,t,n,r,s,l,i,null,c))}f.push(l+o-a)}}return p(t,n,r,i,0),(s||c)(d,f,o)}var $l=class e{constructor(e,t,n,r,i=!1,a=!1){this.from=e,this.to=t,this.tree=n,this.offset=r,this.open=!!i|(a?2:0)}get openStart(){return(this.open&1)>0}get openEnd(){return(this.open&2)>0}static addTree(t,n=[],r=!1){let i=[new e(0,t.length,t,0,!1,r)];for(let e of n)e.to>t.length&&i.push(e);return i}static applyChanges(t,n,r=128){if(!n.length)return t;let i=[],a=1,o=t.length?t[0]:null;for(let s=0,c=0,l=0;;s++){let u=s<n.length?n[s]:null,d=u?u.fromA:1e9;if(d-c>=r)for(;o&&o.from<d;){let n=o;if(c>=n.from||d<=n.to||l){let t=Math.max(n.from,c)-l,r=Math.min(n.to,d)-l;n=t>=r?null:new e(t,r,n.tree,n.offset+l,s>0,!!u)}if(n&&i.push(n),o.to>d)break;o=a<t.length?t[a++]:null}if(!u)break;c=u.toA,l=u.toA-u.toB}return i}},eu=class{startParse(e,t,n){return typeof e==`string`&&(e=new tu(e)),n=n?n.length?n.map(e=>new Dl(e.from,e.to)):[new Dl(0,0)]:[new Dl(0,e.length)],this.createParse(e,t||[],n)}parse(e,t,n){let r=this.startParse(e,t,n);for(;;){let e=r.advance();if(e)return e}}},tu=class{constructor(e){this.string=e}get length(){return this.string.length}chunk(e){return this.string.slice(e)}get lineChunks(){return!1}read(e,t){return this.string.slice(e,t)}};new q({perNode:!0});var nu=0,ru=class e{constructor(e,t,n,r){this.name=e,this.set=t,this.base=n,this.modified=r,this.id=nu++}toString(){let{name:e}=this;for(let t of this.modified)t.name&&(e=`${t.name}(${e})`);return e}static define(t,n){let r=typeof t==`string`?t:`?`;if(t instanceof e&&(n=t),n?.base)throw Error(`Can not derive from a modified tag`);let i=new e(r,[],null,[]);if(i.set.push(i),n)for(let e of n.set)i.set.push(e);return i}static defineModifier(e){let t=new au(e);return e=>e.modified.indexOf(t)>-1?e:au.get(e.base||e,e.modified.concat(t).sort((e,t)=>e.id-t.id))}},iu=0,au=class e{constructor(e){this.name=e,this.instances=[],this.id=iu++}static get(t,n){if(!n.length)return t;let r=n[0].instances.find(e=>e.base==t&&ou(n,e.modified));if(r)return r;let i=[],a=new ru(t.name,i,t,n);for(let e of n)e.instances.push(a);let o=su(n);for(let n of t.set)if(!n.modified.length)for(let t of o)i.push(e.get(n,t));return a}};function ou(e,t){return e.length==t.length&&e.every((e,n)=>e==t[n])}function su(e){let t=[[]];for(let n=0;n<e.length;n++)for(let r=0,i=t.length;r<i;r++)t.push(t[r].concat(e[n]));return t.sort((e,t)=>t.length-e.length)}function cu(e){let t=Object.create(null);for(let n in e){let r=e[n];Array.isArray(r)||(r=[r]);for(let e of n.split(` `))if(e){let n=[],i=2,a=e;for(let t=0;;){if(a==`...`&&t>0&&t+3==e.length){i=1;break}let r=/^"(?:[^"\\]|\\.)*?"|[^\/!]+/.exec(a);if(!r)throw RangeError(`Invalid path: `+e);if(n.push(r[0]==`*`?``:r[0][0]==`"`?JSON.parse(r[0]):r[0]),t+=r[0].length,t==e.length)break;let o=e[t++];if(t==e.length&&o==`!`){i=0;break}if(o!=`/`)throw RangeError(`Invalid path: `+e);a=e.slice(t)}let o=n.length-1,s=n[o];if(!s)throw RangeError(`Invalid path: `+e);t[s]=new uu(r,i,o>0?n.slice(0,o):null).sort(t[s])}}return lu.add(t)}var lu=new q({combine(e,t){let n,r,i;for(;e||t;){if(!e||t&&e.depth<=t.depth?(i=t,t=t.next):(i=e,e=e.next),n&&n.mode==i.mode&&!i.context&&!n.context)continue;let a=new uu(i.tags,i.mode,i.context);n?n.next=a:r=a,n=a}return r}}),uu=class{constructor(e,t,n,r){this.tags=e,this.mode=t,this.context=n,this.next=r}get opaque(){return this.mode==0}get inherit(){return this.mode==1}sort(e){return!e||e.depth<this.depth?(this.next=e,this):(e.next=this.sort(e.next),e)}get depth(){return this.context?this.context.length:0}};uu.empty=new uu([],2,null);function du(e,t){let n=Object.create(null);for(let t of e)if(!Array.isArray(t.tag))n[t.tag.id]=t.class;else for(let e of t.tag)n[e.id]=t.class;let{scope:r,all:i=null}=t||{};return{style:e=>{let t=i;for(let r of e)for(let e of r.set){let r=n[e.id];if(r){t=t?t+` `+r:r;break}}return t},scope:r}}function fu(e,t){let n=null;for(let r of e){let e=r.style(t);e&&(n=n?n+` `+e:e)}return n}function pu(e,t,n,r=0,i=e.length){let a=new mu(r,Array.isArray(t)?t:[t],n);a.highlightRange(e.cursor(),r,i,``,a.highlighters),a.flush(i)}var mu=class{constructor(e,t,n){this.at=e,this.highlighters=t,this.span=n,this.class=``}startSpan(e,t){t!=this.class&&(this.flush(e),e>this.at&&(this.at=e),this.class=t)}flush(e){e>this.at&&this.class&&this.span(this.at,e,this.class)}highlightRange(e,t,n,r,i){let{type:a,from:o,to:s}=e;if(o>=n||s<=t)return;a.isTop&&(i=this.highlighters.filter(e=>!e.scope||e.scope(a)));let c=r,l=hu(e)||uu.empty,u=fu(i,l.tags);if(u&&(c&&(c+=` `),c+=u,l.mode==1&&(r+=(r?` `:``)+u)),this.startSpan(Math.max(t,o),c),l.opaque)return;let d=e.tree&&e.tree.prop(q.mounted);if(d&&d.overlay){let a=e.node.enter(d.overlay[0].from+o,1),l=this.highlighters.filter(e=>!e.scope||e.scope(d.tree.type)),u=e.firstChild();for(let f=0,p=o;;f++){let m=f<d.overlay.length?d.overlay[f]:null,h=m?m.from+o:s,g=Math.max(t,p),_=Math.min(n,h);if(g<_&&u)for(;e.from<_&&(this.highlightRange(e,g,_,r,i),this.startSpan(Math.min(_,e.to),c),!(e.to>=h||!e.nextSibling())););if(!m||h>n)break;p=m.to+o,p>t&&(this.highlightRange(a.cursor(),Math.max(t,m.from+o),Math.min(n,p),``,l),this.startSpan(Math.min(n,p),c))}u&&e.parent()}else if(e.firstChild()){d&&(r=``);do if(!(e.to<=t)){if(e.from>=n)break;this.highlightRange(e,t,n,r,i),this.startSpan(Math.min(n,e.to),c)}while(e.nextSibling());e.parent()}}};function hu(e){let t=e.type.prop(lu);for(;t&&t.context&&!e.matchContext(t.context);)t=t.next;return t||null}var X=ru.define,gu=X(),_u=X(),vu=X(_u),yu=X(_u),bu=X(),xu=X(bu),Su=X(bu),Cu=X(),wu=X(Cu),Tu=X(),Eu=X(),Du=X(),Ou=X(Du),ku=X(),Z={comment:gu,lineComment:X(gu),blockComment:X(gu),docComment:X(gu),name:_u,variableName:X(_u),typeName:vu,tagName:X(vu),propertyName:yu,attributeName:X(yu),className:X(_u),labelName:X(_u),namespace:X(_u),macroName:X(_u),literal:bu,string:xu,docString:X(xu),character:X(xu),attributeValue:X(xu),number:Su,integer:X(Su),float:X(Su),bool:X(bu),regexp:X(bu),escape:X(bu),color:X(bu),url:X(bu),keyword:Tu,self:X(Tu),null:X(Tu),atom:X(Tu),unit:X(Tu),modifier:X(Tu),operatorKeyword:X(Tu),controlKeyword:X(Tu),definitionKeyword:X(Tu),moduleKeyword:X(Tu),operator:Eu,derefOperator:X(Eu),arithmeticOperator:X(Eu),logicOperator:X(Eu),bitwiseOperator:X(Eu),compareOperator:X(Eu),updateOperator:X(Eu),definitionOperator:X(Eu),typeOperator:X(Eu),controlOperator:X(Eu),punctuation:Du,separator:X(Du),bracket:Ou,angleBracket:X(Ou),squareBracket:X(Ou),paren:X(Ou),brace:X(Ou),content:Cu,heading:wu,heading1:X(wu),heading2:X(wu),heading3:X(wu),heading4:X(wu),heading5:X(wu),heading6:X(wu),contentSeparator:X(Cu),list:X(Cu),quote:X(Cu),emphasis:X(Cu),strong:X(Cu),link:X(Cu),monospace:X(Cu),strikethrough:X(Cu),inserted:X(),deleted:X(),changed:X(),invalid:X(),meta:ku,documentMeta:X(ku),annotation:X(ku),processingInstruction:X(ku),definition:ru.defineModifier(`definition`),constant:ru.defineModifier(`constant`),function:ru.defineModifier(`function`),standard:ru.defineModifier(`standard`),local:ru.defineModifier(`local`),special:ru.defineModifier(`special`)};for(let e in Z){let t=Z[e];t instanceof ru&&(t.name=e)}du([{tag:Z.link,class:`tok-link`},{tag:Z.heading,class:`tok-heading`},{tag:Z.emphasis,class:`tok-emphasis`},{tag:Z.strong,class:`tok-strong`},{tag:Z.keyword,class:`tok-keyword`},{tag:Z.atom,class:`tok-atom`},{tag:Z.bool,class:`tok-bool`},{tag:Z.url,class:`tok-url`},{tag:Z.labelName,class:`tok-labelName`},{tag:Z.inserted,class:`tok-inserted`},{tag:Z.deleted,class:`tok-deleted`},{tag:Z.literal,class:`tok-literal`},{tag:Z.string,class:`tok-string`},{tag:Z.number,class:`tok-number`},{tag:[Z.regexp,Z.escape,Z.special(Z.string)],class:`tok-string2`},{tag:Z.variableName,class:`tok-variableName`},{tag:Z.local(Z.variableName),class:`tok-variableName tok-local`},{tag:Z.definition(Z.variableName),class:`tok-variableName tok-definition`},{tag:Z.special(Z.variableName),class:`tok-variableName2`},{tag:Z.definition(Z.propertyName),class:`tok-propertyName tok-definition`},{tag:Z.typeName,class:`tok-typeName`},{tag:Z.namespace,class:`tok-namespace`},{tag:Z.className,class:`tok-className`},{tag:Z.macroName,class:`tok-macroName`},{tag:Z.propertyName,class:`tok-propertyName`},{tag:Z.operator,class:`tok-operator`},{tag:Z.comment,class:`tok-comment`},{tag:Z.meta,class:`tok-meta`},{tag:Z.invalid,class:`tok-invalid`},{tag:Z.punctuation,class:`tok-punctuation`}]);function Au(e,t,n,r=0,i=0){t??(t=e.search(/[^\s\u00a0]/),t==-1&&(t=e.length));let a=i;for(let i=r;i<t;i++)e.charCodeAt(i)==9?a+=n-a%n:a++;return a}var ju=class{constructor(e,t,n,r){this.string=e,this.tabSize=t,this.indentUnit=n,this.overrideIndent=r,this.pos=0,this.start=0,this.lastColumnPos=0,this.lastColumnValue=0}eol(){return this.pos>=this.string.length}sol(){return this.pos==0}peek(){return this.string.charAt(this.pos)||void 0}next(){if(this.pos<this.string.length)return this.string.charAt(this.pos++)}eat(e){let t=this.string.charAt(this.pos),n;if(n=typeof e==`string`?t==e:t&&(e instanceof RegExp?e.test(t):e(t)),n)return++this.pos,t}eatWhile(e){let t=this.pos;for(;this.eat(e););return this.pos>t}eatSpace(){let e=this.pos;for(;/[\s\u00a0]/.test(this.string.charAt(this.pos));)++this.pos;return this.pos>e}skipToEnd(){this.pos=this.string.length}skipTo(e){let t=this.string.indexOf(e,this.pos);if(t>-1)return this.pos=t,!0}backUp(e){this.pos-=e}column(){return this.lastColumnPos<this.start&&(this.lastColumnValue=Au(this.string,this.start,this.tabSize,this.lastColumnPos,this.lastColumnValue),this.lastColumnPos=this.start),this.lastColumnValue}indentation(){return this.overrideIndent??Au(this.string,null,this.tabSize)}match(e,t,n){if(typeof e==`string`){let r=e=>n?e.toLowerCase():e;return r(this.string.substr(this.pos,e.length))==r(e)?(t!==!1&&(this.pos+=e.length),!0):null}{let n=this.string.slice(this.pos).match(e);return n&&n.index>0?null:(n&&t!==!1&&(this.pos+=n[0].length),n)}}current(){return this.string.slice(this.start,this.pos)}},Mu=new q;function Nu(e){return M.define({combine:e?t=>t.concat(e):void 0})}var Pu=new q,Fu=class{constructor(e,t,n=[],r=``){this.data=e,this.name=r,F.prototype.hasOwnProperty(`tree`)||Object.defineProperty(F.prototype,"tree",{get(){return Lu(this)}}),this.parser=t,this.extension=[Ku.of(this),F.languageData.of((e,t,n)=>{let r=Iu(e,t,n),i=r.type.prop(Mu);if(!i)return[];let a=e.facet(i),o=r.type.prop(Pu);if(o){let i=r.resolve(t-r.from,n);for(let t of o)if(t.test(i,e)){let n=e.facet(t.facet);return t.type==`replace`?n:n.concat(a)}}return a})].concat(n)}isActiveAt(e,t,n=-1){return Iu(e,t,n).type.prop(Mu)==this.data}findRegions(e){let t=e.facet(Ku);if(t?.data==this.data)return[{from:0,to:e.doc.length}];if(!t||!t.allowsNesting)return[];let n=[],r=(e,t)=>{if(e.prop(Mu)==this.data){n.push({from:t,to:t+e.length});return}let i=e.prop(q.mounted);if(i){if(i.tree.prop(Mu)==this.data){if(i.overlay)for(let e of i.overlay)n.push({from:e.from+t,to:e.to+t});else n.push({from:t,to:t+e.length});return}if(i.overlay){let e=n.length;if(r(i.tree,i.overlay[0].from+t),n.length>e)return}}for(let n=0;n<e.children.length;n++){let i=e.children[n];i instanceof Y&&r(i,e.positions[n]+t)}};return r(Lu(e),0),n}get allowsNesting(){return!0}};Fu.setState=N.define();function Iu(e,t,n){let r=e.facet(Ku),i=Lu(e).topNode;if(!r||r.allowsNesting)for(let e=i;e;e=e.enter(t,n,J.ExcludeBuffers|J.EnterBracketed))e.type.isTop&&(i=e);return i}function Lu(e){let t=e.field(Fu.state,!1);return t?t.tree:Y.empty}var Ru=class{constructor(e){this.doc=e,this.cursorPos=0,this.string=``,this.cursor=e.iter()}get length(){return this.doc.length}syncTo(e){return this.string=this.cursor.next(e-this.cursorPos).value,this.cursorPos=e+this.string.length,this.cursorPos-this.string.length}chunk(e){return this.syncTo(e),this.string}get lineChunks(){return!0}read(e,t){let n=this.cursorPos-this.string.length;return e<n||t>=this.cursorPos?this.doc.sliceString(e,t):this.string.slice(e-n,t-n)}},zu=null,Bu=class e{constructor(e,t,n=[],r,i,a,o,s){this.parser=e,this.state=t,this.fragments=n,this.tree=r,this.treeLen=i,this.viewport=a,this.skipped=o,this.scheduleOn=s,this.parse=null,this.tempSkipped=[]}static create(t,n,r){return new e(t,n,[],Y.empty,0,r,[],null)}startParse(){return this.parser.startParse(new Ru(this.state.doc),this.fragments)}work(e,t){return t!=null&&t>=this.state.doc.length&&(t=void 0),this.tree!=Y.empty&&this.isDone(t??this.state.doc.length)?(this.takeTree(),!0):this.withContext(()=>{if(typeof e==`number`){let t=Date.now()+e;e=()=>Date.now()>t}for(this.parse||=this.startParse(),t!=null&&(this.parse.stoppedAt==null||this.parse.stoppedAt>t)&&t<this.state.doc.length&&this.parse.stopAt(t);;){let n=this.parse.advance();if(n){if(this.fragments=this.withoutTempSkipped($l.addTree(n,this.fragments,this.parse.stoppedAt!=null)),this.treeLen=this.parse.stoppedAt??this.state.doc.length,this.tree=n,this.parse=null,this.treeLen<(t??this.state.doc.length))this.parse=this.startParse();else return!0}if(e())return!1}})}takeTree(){let e,t;this.parse&&(e=this.parse.parsedPos)>=this.treeLen&&((this.parse.stoppedAt==null||this.parse.stoppedAt>e)&&this.parse.stopAt(e),this.withContext(()=>{for(;!(t=this.parse.advance()););}),this.treeLen=e,this.tree=t,this.fragments=this.withoutTempSkipped($l.addTree(this.tree,this.fragments,!0)),this.parse=null)}withContext(e){let t=zu;zu=this;try{return e()}finally{zu=t}}withoutTempSkipped(e){for(let t;t=this.tempSkipped.pop();)e=Vu(e,t.from,t.to);return e}changes(t,n){let{fragments:r,tree:i,treeLen:a,viewport:o,skipped:s}=this;if(this.takeTree(),!t.empty){let e=[];if(t.iterChangedRanges((t,n,r,i)=>e.push({fromA:t,toA:n,fromB:r,toB:i})),r=$l.applyChanges(r,e),i=Y.empty,a=0,o={from:t.mapPos(o.from,-1),to:t.mapPos(o.to,1)},this.skipped.length){s=[];for(let e of this.skipped){let n=t.mapPos(e.from,1),r=t.mapPos(e.to,-1);n<r&&s.push({from:n,to:r})}}}return new e(this.parser,n,r,i,a,o,s,this.scheduleOn)}updateViewport(e){if(this.viewport.from==e.from&&this.viewport.to==e.to)return!1;this.viewport=e;let t=this.skipped.length;for(let t=0;t<this.skipped.length;t++){let{from:n,to:r}=this.skipped[t];n<e.to&&r>e.from&&(this.fragments=Vu(this.fragments,n,r),this.skipped.splice(t--,1))}return this.skipped.length>=t?!1:(this.reset(),!0)}reset(){this.parse&&=(this.takeTree(),null)}skipUntilInView(e,t){this.skipped.push({from:e,to:t})}static getSkippingParser(e){return new class extends eu{createParse(t,n,r){let i=r[0].from,a=r[r.length-1].to;return{parsedPos:i,advance(){let t=zu;if(t){for(let e of r)t.tempSkipped.push(e);e&&(t.scheduleOn=t.scheduleOn?Promise.all([t.scheduleOn,e]):e)}return this.parsedPos=a,new Y(Al.none,[],[],a-i)},stoppedAt:null,stopAt(){}}}}}isDone(e){e=Math.min(e,this.state.doc.length);let t=this.fragments;return this.treeLen>=e&&t.length&&t[0].from==0&&t[0].to>=e}static get(){return zu}};function Vu(e,t,n){return $l.applyChanges(e,[{fromA:t,toA:n,fromB:t,toB:n}])}var Hu=class e{constructor(e){this.context=e,this.tree=e.tree}apply(t){if(!t.docChanged&&this.tree==this.context.tree)return this;let n=this.context.changes(t.changes,t.state),r=this.context.treeLen==t.startState.doc.length?void 0:Math.max(t.changes.mapPos(this.context.treeLen),n.viewport.to);return n.work(20,r)||n.takeTree(),new e(n)}static init(t){let n=Math.min(3e3,t.doc.length),r=Bu.create(t.facet(Ku).parser,t,{from:0,to:n});return r.work(20,n)||r.takeTree(),new e(r)}};Fu.state=Je.define({create:Hu.init,update(e,t){for(let e of t.effects)if(e.is(Fu.setState))return e.value;return t.startState.facet(Ku)==t.state.facet(Ku)?e.apply(t):Hu.init(t.state)}});var Uu=e=>{let t=setTimeout(()=>e(),500);return()=>clearTimeout(t)};typeof requestIdleCallback<`u`&&(Uu=e=>{let t=-1,n=setTimeout(()=>{t=requestIdleCallback(e,{timeout:400})},100);return()=>t<0?clearTimeout(n):cancelIdleCallback(t)});var Wu=typeof navigator<`u`&&navigator.scheduling?.isInputPending?()=>navigator.scheduling.isInputPending():null,Gu=U.fromClass(class{constructor(e){this.view=e,this.working=null,this.workScheduled=0,this.chunkEnd=-1,this.chunkBudget=-1,this.work=this.work.bind(this),this.scheduleWork()}update(e){let t=this.view.state.field(Fu.state).context;(t.updateViewport(e.view.viewport)||this.view.viewport.to>t.treeLen)&&this.scheduleWork(),(e.docChanged||e.selectionSet)&&(this.view.hasFocus&&(this.chunkBudget+=50),this.scheduleWork()),this.checkAsyncSchedule(t)}scheduleWork(){if(this.working)return;let{state:e}=this.view,t=e.field(Fu.state);t.tree==t.context.tree&&t.context.isDone(e.doc.length)||(this.working=Uu(this.work))}work(e){this.working=null;let t=Date.now();if(this.chunkEnd<t&&(this.chunkEnd<0||this.view.hasFocus)&&(this.chunkEnd=t+3e4,this.chunkBudget=3e3),this.chunkBudget<=0)return;let{state:n,viewport:{to:r}}=this.view,i=n.field(Fu.state);if(i.tree==i.context.tree&&i.context.isDone(r+1e5))return;let a=Date.now()+Math.min(this.chunkBudget,100,e&&!Wu?Math.max(25,e.timeRemaining()-5):1e9),o=i.context.treeLen<r&&n.doc.length>r+1e3,s=i.context.work(()=>Wu&&Wu()||Date.now()>a,r+(o?0:1e5));this.chunkBudget-=Date.now()-t,(s||this.chunkBudget<=0)&&(i.context.takeTree(),this.view.dispatch({effects:Fu.setState.of(new Hu(i.context))})),this.chunkBudget>0&&(!s||o)&&this.scheduleWork(),this.checkAsyncSchedule(i.context)}checkAsyncSchedule(e){e.scheduleOn&&=(this.workScheduled++,e.scheduleOn.then(()=>this.scheduleWork()).catch(e=>Jr(this.view.state,e)).then(()=>this.workScheduled--),null)}destroy(){this.working&&this.working()}isWorking(){return!!(this.working||this.workScheduled>0)}},{eventHandlers:{focus(){this.scheduleWork()}}}),Ku=M.define({combine(e){return e.length?e[0]:null},enables:e=>[Fu.state,Gu,K.contentAttributes.compute([e],t=>{let n=t.facet(e);return n&&n.name?{"data-language":n.name}:{}})]}),qu=M.define(),Ju=M.define({combine:e=>{if(!e.length)return`  `;let t=e[0];if(!t||/\S/.test(t)||Array.from(t).some(e=>e!=t[0]))throw Error(`Invalid indent unit: `+JSON.stringify(e[0]));return t}});function Yu(e){let t=e.facet(Ju);return t.charCodeAt(0)==9?e.tabSize*t.length:t.length}function Xu(e,t){let n=``,r=e.tabSize,i=e.facet(Ju)[0];if(i==`	`){for(;t>=r;)n+=`	`,t-=r;i=` `}for(let e=0;e<t;e++)n+=i;return n}function Zu(e,t){e instanceof F&&(e=new Qu(e));for(let n of e.state.facet(qu)){let r=n(e,t);if(r!==void 0)return r}let n=Lu(e.state);return n.length>=t?ed(e,n,t):null}var Qu=class{constructor(e,t={}){this.state=e,this.options=t,this.unit=Yu(e)}lineAt(e,t=1){let n=this.state.doc.lineAt(e),{simulateBreak:r,simulateDoubleBreak:i}=this.options;return r!=null&&r>=n.from&&r<=n.to?i&&r==e?{text:``,from:e}:(t<0?r<e:r<=e)?{text:n.text.slice(r-n.from),from:r}:{text:n.text.slice(0,r-n.from),from:n.from}:n}textAfterPos(e,t=1){if(this.options.simulateDoubleBreak&&e==this.options.simulateBreak)return``;let{text:n,from:r}=this.lineAt(e,t);return n.slice(e-r,Math.min(n.length,e+100-r))}column(e,t=1){let{text:n,from:r}=this.lineAt(e,t),i=this.countColumn(n,e-r),a=this.options.overrideIndentation?this.options.overrideIndentation(r):-1;return a>-1&&(i+=a-this.countColumn(n,n.search(/\S|$/))),i}countColumn(e,t=e.length){return qt(e,this.state.tabSize,t)}lineIndent(e,t=1){let{text:n,from:r}=this.lineAt(e,t),i=this.options.overrideIndentation;if(i){let e=i(r);if(e>-1)return e}return this.countColumn(n,n.search(/\S|$/))}get simulatedBreak(){return this.options.simulateBreak||null}},$u=new q;function ed(e,t,n){let r=t.resolveStack(n),i=t.resolveInner(n,-1).resolve(n,0).enterUnfinishedNodesBefore(n);if(i!=r.node){let e=[];for(let t=i;t&&!(t.from<r.node.from||t.to>r.node.to||t.from==r.node.from&&t.type==r.node.type);t=t.parent)e.push(t);for(let t=e.length-1;t>=0;t--)r={node:e[t],next:r}}return td(r,e,n)}function td(e,t,n){for(let r=e;r;r=r.next){let e=rd(r.node);if(e)return e(ad.create(t,n,r))}return 0}function nd(e){return e.pos==e.options.simulateBreak&&e.options.simulateDoubleBreak}function rd(e){let t=e.type.prop($u);if(t)return t;let n=e.firstChild,r;if(n&&(r=n.type.prop(q.closedBy))){let t=e.lastChild,n=t&&r.indexOf(t.name)>-1;return e=>cd(e,!0,1,void 0,n&&!nd(e)?t.from:void 0)}return e.parent==null?id:null}function id(){return 0}var ad=class e extends Qu{constructor(e,t,n){super(e.state,e.options),this.base=e,this.pos=t,this.context=n}get node(){return this.context.node}static create(t,n,r){return new e(t,n,r)}get textAfter(){return this.textAfterPos(this.pos)}get baseIndent(){return this.baseIndentFor(this.node)}baseIndentFor(e){let t=this.state.doc.lineAt(e.from);for(;;){let n=e.resolve(t.from);for(;n.parent&&n.parent.from==n.from;)n=n.parent;if(od(n,e))break;t=this.state.doc.lineAt(n.from)}return this.lineIndent(t.from)}continue(){return td(this.context.next,this.base,this.pos)}};function od(e,t){for(let n=t;n;n=n.parent)if(e==n)return!0;return!1}function sd(e){let t=e.node,n=t.childAfter(t.from),r=t.lastChild;if(!n)return null;let i=e.options.simulateBreak,a=e.state.doc.lineAt(n.from),o=i==null||i<=a.from?a.to:Math.min(a.to,i);for(let e=n.to;;){let i=t.childAfter(e);if(!i||i==r)return null;if(!i.type.isSkipped){if(i.from>=o)return null;let e=/^ */.exec(a.text.slice(n.to-a.from))[0].length;return{from:n.from,to:n.to+e}}e=i.to}}function cd(e,t,n,r,i){let a=e.textAfter,o=a.match(/^\s*/)[0].length,s=r&&a.slice(o,o+r.length)==r||i==e.pos+o,c=t?sd(e):null;return c?s?e.column(c.from):e.column(c.to):e.baseIndent+(s?0:e.unit*n)}var ld=200;function ud(){return F.transactionFilter.of(e=>{if(!e.docChanged||!e.isUserEvent(`input.type`)&&!e.isUserEvent(`input.complete`))return e;let t=e.startState.languageDataAt(`indentOnInput`,e.startState.selection.main.head);if(!t.length)return e;let n=e.newDoc,{head:r}=e.newSelection.main,i=n.lineAt(r);if(r>i.from+ld)return e;let a=n.sliceString(i.from,r);if(!t.some(e=>e.test(a)))return e;let{state:o}=e,s=-1,c=[];for(let{head:e}of o.selection.ranges){let t=o.doc.lineAt(e);if(t.from==s)continue;s=t.from;let n=Zu(o,t.from);if(n==null)continue;let r=/^\s*/.exec(t.text)[0],i=Xu(o,n);r!=i&&c.push({from:t.from,to:t.from+r.length,insert:i})}return c.length?[e,{changes:c,sequential:!0}]:e})}var dd=M.define(),fd=new q;function pd(e,t,n){let r=Lu(e);if(r.length<n)return null;let i=r.resolveStack(n,1),a=null;for(let o=i;o;o=o.next){let i=o.node;if(i.to<=n||i.from>n)continue;if(a&&i.from<t)break;let s=i.type.prop(fd);if(s&&(i.to<r.length-50||r.length==e.doc.length||!md(i))){let r=s(i,e);r&&r.from<=n&&r.from>=t&&r.to>n&&(a=r)}}return a}function md(e){let t=e.lastChild;return t&&t.to==e.to&&t.type.isError}function hd(e,t,n){for(let r of e.facet(dd)){let i=r(e,t,n);if(i)return i}return pd(e,t,n)}function gd(e,t){let n=t.mapPos(e.from,1),r=t.mapPos(e.to,-1);return n>=r?void 0:{from:n,to:r}}var _d=N.define({map:gd}),vd=N.define({map:gd});function yd(e){let t=[];for(let{head:n}of e.state.selection.ranges)t.some(e=>e.from<=n&&e.to>=n)||t.push(e.lineBlockAt(n));return t}var bd=Je.define({create(){return z.none},update(e,t){t.isUserEvent(`delete`)&&t.changes.iterChangedRanges((t,n)=>e=xd(e,t,n)),e=e.map(t.changes);let n=[];for(let r of t.effects)r.is(_d)&&!Cd(e,r.value.from,r.value.to)?n.push(r.value):r.is(vd)&&(e=e.update({filter:(e,t)=>r.value.from!=e||r.value.to!=t,filterFrom:r.value.from,filterTo:r.value.to}));if(n.length){let{preparePlaceholder:r}=t.state.facet(Ad),i=n.map(e=>(r?z.replace({widget:new Pd(r(t.state,e))}):Nd).range(e.from,e.to));e=e.update({add:i})}return t.selection&&(e=xd(e,t.selection.main.head)),e},provide:e=>K.decorations.from(e),toJSON(e,t){let n=[];return e.between(0,t.doc.length,(e,t)=>{n.push(e,t)}),n},fromJSON(e){if(!Array.isArray(e)||e.length%2)throw RangeError(`Invalid JSON for fold state`);let t=[];for(let n=0;n<e.length;){let r=e[n++],i=e[n++];if(typeof r!=`number`||typeof i!=`number`)throw RangeError(`Invalid JSON for fold state`);t.push(Nd.range(r,i))}return z.set(t,!0)}});function xd(e,t,n=t){let r=!1;return e.between(t,n,(e,i)=>{e<n&&i>t&&(r=!0)}),r?e.update({filterFrom:t,filterTo:n,filter:(e,r)=>e>=n||r<=t}):e}function Sd(e,t,n){var r;let i=null;return(r=e.field(bd,!1))==null||r.between(t,n,(e,t)=>{(!i||i.from>e)&&(i={from:e,to:t})}),i}function Cd(e,t,n){let r=!1;return e.between(t,t,(e,i)=>{e==t&&i==n&&(r=!0)}),r}function wd(e,t){return e.field(bd,!1)?t:t.concat(N.appendConfig.of(jd()))}var Td=e=>{for(let t of yd(e)){let n=hd(e.state,t.from,t.to);if(n)return e.dispatch({effects:wd(e.state,[_d.of(n),Dd(e,n)])}),!0}return!1},Ed=e=>{if(!e.state.field(bd,!1))return!1;let t=[];for(let n of yd(e)){let r=Sd(e.state,n.from,n.to);r&&t.push(vd.of(r),Dd(e,r,!1))}return t.length&&e.dispatch({effects:t}),t.length>0};function Dd(e,t,n=!0){let r=e.state.doc.lineAt(t.from).number,i=e.state.doc.lineAt(t.to).number;return K.announce.of(`${e.state.phrase(n?`Folded lines`:`Unfolded lines`)} ${r} ${e.state.phrase(`to`)} ${i}.`)}var Od=[{key:`Ctrl-Shift-[`,mac:`Cmd-Alt-[`,run:Td},{key:`Ctrl-Shift-]`,mac:`Cmd-Alt-]`,run:Ed},{key:`Ctrl-Alt-[`,run:e=>{let{state:t}=e,n=[];for(let r=0;r<t.doc.length;){let i=e.lineBlockAt(r),a=hd(t,i.from,i.to);a&&n.push(_d.of(a)),r=(a?e.lineBlockAt(a.to):i).to+1}return n.length&&e.dispatch({effects:wd(e.state,n)}),!!n.length}},{key:`Ctrl-Alt-]`,run:e=>{let t=e.state.field(bd,!1);if(!t||!t.size)return!1;let n=[];return t.between(0,e.state.doc.length,(e,t)=>{n.push(vd.of({from:e,to:t}))}),e.dispatch({effects:n}),!0}}],kd={placeholderDOM:null,preparePlaceholder:null,placeholderText:`…`},Ad=M.define({combine(e){return Ot(e,kd)}});function jd(e){let t=[bd,Rd];return e&&t.push(Ad.of(e)),t}function Md(e,t){let{state:n}=e,r=n.facet(Ad),i=t=>{let n=e.lineBlockAt(e.posAtDOM(t.target)),r=Sd(e.state,n.from,n.to);r&&e.dispatch({effects:vd.of(r)}),t.preventDefault()};if(r.placeholderDOM)return r.placeholderDOM(e,i,t);let a=document.createElement(`span`);return a.textContent=r.placeholderText,a.setAttribute(`aria-label`,n.phrase(`folded code`)),a.title=n.phrase(`unfold`),a.className=`cm-foldPlaceholder`,a.onclick=i,a}var Nd=z.replace({widget:new class extends On{toDOM(e){return Md(e,null)}}}),Pd=class extends On{constructor(e){super(),this.value=e}eq(e){return this.value==e.value}toDOM(e){return Md(e,this.value)}},Fd={openText:`⌄`,closedText:`›`,markerDOM:null,domEventHandlers:{},foldingChanged:()=>!1},Id=class extends $c{constructor(e,t){super(),this.config=e,this.open=t}eq(e){return this.config==e.config&&this.open==e.open}toDOM(e){if(this.config.markerDOM)return this.config.markerDOM(this.open);let t=document.createElement(`span`);return t.textContent=this.open?this.config.openText:this.config.closedText,t.title=e.state.phrase(this.open?`Fold line`:`Unfold line`),t}};function Ld(e={}){let t={...Fd,...e},n=new Id(t,!0),r=new Id(t,!1),i=U.fromClass(class{constructor(e){this.from=e.viewport.from,this.markers=this.buildMarkers(e)}update(e){(e.docChanged||e.viewportChanged||e.startState.facet(Ku)!=e.state.facet(Ku)||e.startState.field(bd,!1)!=e.state.field(bd,!1)||Lu(e.startState)!=Lu(e.state)||t.foldingChanged(e))&&(this.markers=this.buildMarkers(e.view))}buildMarkers(e){let t=new It;for(let i of e.viewportLineBlocks){let a=Sd(e.state,i.from,i.to)?r:hd(e.state,i.from,i.to)?n:null;a&&t.add(i.from,i.from,a)}return t.finish()}}),{domEventHandlers:a}=t;return[i,il({class:`cm-foldGutter`,markers(e){return e.plugin(i)?.markers||I.empty},initialSpacer(){return new Id(t,!1)},domEventHandlers:{...a,click:(e,t,n)=>{if(a.click&&a.click(e,t,n))return!0;let r=Sd(e.state,t.from,t.to);if(r)return e.dispatch({effects:vd.of(r)}),!0;let i=hd(e.state,t.from,t.to);return i?(e.dispatch({effects:_d.of(i)}),!0):!1}}}),jd()]}var Rd=K.baseTheme({".cm-foldPlaceholder":{backgroundColor:`#eee`,border:`1px solid #ddd`,color:`#888`,borderRadius:`.2em`,margin:`0 1px`,padding:`0 1px`,cursor:`pointer`},".cm-foldGutter span":{padding:`0 1px`,cursor:`pointer`}}),zd=class e{constructor(e,t){this.specs=e;let n;function r(e){let t=$t.newName();return(n||=Object.create(null))[`.`+t]=e,t}let i=typeof t.all==`string`?t.all:t.all?r(t.all):void 0,a=t.scope;this.scope=a instanceof Fu?e=>e.prop(Mu)==a.data:a?e=>e==a:void 0,this.style=du(e.map(e=>({tag:e.tag,class:e.class||r(Object.assign({},e,{tag:null}))})),{all:i}).style,this.module=n?new $t(n):null,this.themeType=t.themeType}static define(t,n){return new e(t,n||{})}},Bd=M.define(),Vd=M.define({combine(e){return e.length?[e[0]]:null}});function Hd(e){let t=e.facet(Bd);return t.length?t:e.facet(Vd)}function Ud(e,t){let n=[Gd],r;return e instanceof zd&&(e.module&&n.push(K.styleModule.of(e.module)),r=e.themeType),t?.fallback?n.push(Vd.of(e)):r?n.push(Bd.computeN([K.darkTheme],t=>t.facet(K.darkTheme)==(r==`dark`)?[e]:[])):n.push(Bd.of(e)),n}var Wd=class{constructor(e){this.markCache=Object.create(null),this.tree=Lu(e.state),this.decorations=this.buildDeco(e,Hd(e.state)),this.decoratedTo=e.viewport.to}update(e){let t=Lu(e.state),n=Hd(e.state),r=n!=Hd(e.startState),{viewport:i}=e.view,a=e.changes.mapPos(this.decoratedTo,1);t.length<i.to&&!r&&t.type==this.tree.type&&a>=i.to?(this.decorations=this.decorations.map(e.changes),this.decoratedTo=a):(t!=this.tree||e.viewportChanged||r)&&(this.tree=t,this.decorations=this.buildDeco(e.view,n),this.decoratedTo=i.to)}buildDeco(e,t){if(!t||!this.tree.length)return z.none;let n=new It;for(let{from:r,to:i}of e.visibleRanges)pu(this.tree,t,(e,t,r)=>{n.add(e,t,this.markCache[r]||(this.markCache[r]=z.mark({class:r})))},r,i);return n.finish()}},Gd=Ze.high(U.fromClass(Wd,{decorations:e=>e.decorations})),Kd=zd.define([{tag:Z.meta,color:`#404740`},{tag:Z.link,textDecoration:`underline`},{tag:Z.heading,textDecoration:`underline`,fontWeight:`bold`},{tag:Z.emphasis,fontStyle:`italic`},{tag:Z.strong,fontWeight:`bold`},{tag:Z.strikethrough,textDecoration:`line-through`},{tag:Z.keyword,color:`#708`},{tag:[Z.atom,Z.bool,Z.url,Z.contentSeparator,Z.labelName],color:`#219`},{tag:[Z.literal,Z.inserted],color:`#164`},{tag:[Z.string,Z.deleted],color:`#a11`},{tag:[Z.regexp,Z.escape,Z.special(Z.string)],color:`#e40`},{tag:Z.definition(Z.variableName),color:`#00f`},{tag:Z.local(Z.variableName),color:`#30a`},{tag:[Z.typeName,Z.namespace],color:`#085`},{tag:Z.className,color:`#167`},{tag:[Z.special(Z.variableName),Z.macroName],color:`#256`},{tag:Z.definition(Z.propertyName),color:`#00c`},{tag:Z.comment,color:`#940`},{tag:Z.invalid,color:`#f00`}]),qd=K.baseTheme({"&.cm-focused .cm-matchingBracket":{backgroundColor:`#328c8252`},"&.cm-focused .cm-nonmatchingBracket":{backgroundColor:`#bb555544`}}),Jd=1e4,Yd=`()[]{}`,Xd=M.define({combine(e){return Ot(e,{afterCursor:!0,brackets:Yd,maxScanDistance:Jd,renderMatch:$d})}}),Zd=z.mark({class:`cm-matchingBracket`}),Qd=z.mark({class:`cm-nonmatchingBracket`});function $d(e){let t=[],n=e.matched?Zd:Qd;return t.push(n.range(e.start.from,e.start.to)),e.end&&t.push(n.range(e.end.from,e.end.to)),t}function ef(e){let t=[],n=e.facet(Xd);for(let r of e.selection.ranges){if(!r.empty)continue;let i=sf(e,r.head,-1,n)||r.head>0&&sf(e,r.head-1,1,n)||n.afterCursor&&(sf(e,r.head,1,n)||r.head<e.doc.length&&sf(e,r.head+1,-1,n));i&&(t=t.concat(n.renderMatch(i,e)))}return z.set(t,!0)}var tf=[U.fromClass(class{constructor(e){this.paused=!1,this.decorations=ef(e.state)}update(e){(e.docChanged||e.selectionSet||this.paused)&&(e.view.composing?(this.decorations=this.decorations.map(e.changes),this.paused=!0):(this.decorations=ef(e.state),this.paused=!1))}},{decorations:e=>e.decorations}),qd];function nf(e={}){return[Xd.of(e),tf]}var rf=new q;function af(e,t,n){let r=e.prop(t<0?q.openedBy:q.closedBy);if(r)return r;if(e.name.length==1){let r=n.indexOf(e.name);if(r>-1&&r%2==+(t<0))return[n[r+t]]}return null}function of(e){let t=e.type.prop(rf);return t?t(e.node):e}function sf(e,t,n,r={}){let i=r.maxScanDistance||Jd,a=r.brackets||Yd,o=Lu(e),s=o.resolveInner(t,n);for(let r=s;r;r=r.parent){let i=af(r.type,n,a);if(i&&r.from<r.to){let o=of(r);if(o&&(n>0?t>=o.from&&t<o.to:t>o.from&&t<=o.to))return cf(e,t,n,r,o,i,a)}}return lf(e,t,n,o,s.type,i,a)}function cf(e,t,n,r,i,a,o){let s=r.parent,c={from:i.from,to:i.to},l=0,u=s?.cursor();if(u&&(n<0?u.childBefore(r.from):u.childAfter(r.to)))do if(n<0?u.to<=r.from:u.from>=r.to){if(l==0&&a.indexOf(u.type.name)>-1&&u.from<u.to){let e=of(u);return{start:c,end:e?{from:e.from,to:e.to}:void 0,matched:!0}}if(af(u.type,n,o))l++;else if(af(u.type,-n,o)){if(l==0){let e=of(u);return{start:c,end:e&&e.from<e.to?{from:e.from,to:e.to}:void 0,matched:!1}}l--}}while(n<0?u.prevSibling():u.nextSibling());return{start:c,matched:!1}}function lf(e,t,n,r,i,a,o){if(n<0?!t:t==e.doc.length)return null;let s=n<0?e.sliceDoc(t-1,t):e.sliceDoc(t,t+1),c=o.indexOf(s);if(c<0||c%2==0!=n>0)return null;let l={from:n<0?t-1:t,to:n>0?t+1:t},u=e.doc.iterRange(t,n>0?e.doc.length:0),d=0;for(let e=0;!u.next().done&&e<=a;){let a=u.value;n<0&&(e+=a.length);let s=t+e*n;for(let e=n>0?0:a.length-1,t=n>0?a.length:-1;e!=t;e+=n){let t=o.indexOf(a[e]);if(!(t<0||r.resolveInner(s+e,1).type!=i)){if(t%2==0==n>0)d++;else if(d==1)return{start:l,end:{from:s+e,to:s+e+1},matched:t>>1==c>>1};else d--}}n>0&&(e+=a.length)}return u.done?{start:l,matched:!1}:null}function uf(e){return{name:e.name||``,token:e.token,blankLine:e.blankLine||(()=>{}),startState:e.startState||(()=>!0),copyState:e.copyState||df,indent:e.indent||(()=>null),languageData:e.languageData||{},tokenTable:e.tokenTable||yf,mergeTokens:e.mergeTokens!==!1}}function df(e){if(typeof e!=`object`)return e;let t={};for(let n in e){let r=e[n];t[n]=r instanceof Array?r.slice():r}return t}var ff=new WeakMap,pf=class e extends Fu{constructor(e){let t=Nu(e.languageData),n=uf(e),r,i=new class extends eu{createParse(e,t,n){return new _f(r,e,t,n)}};super(t,i,[],e.name),this.topNode=kf(t,this),r=this,this.streamParser=n,this.stateAfter=new q({perNode:!0}),this.tokenTable=e.tokenTable?new Tf(n.tokenTable):Ef}static define(t){return new e(t)}getIndent(e){let t,{overrideIndentation:n}=e.options;n&&(t=ff.get(e.state),t!=null&&t<e.pos-1e4&&(t=void 0));let r=mf(this,e.node.tree,e.node.from,e.node.from,t??e.pos),i,a;if(r?(a=r.state,i=r.pos+1):(a=this.streamParser.startState(e.unit),i=e.node.from),e.pos-i>1e4)return null;for(;i<e.pos;){let t=e.state.doc.lineAt(i),r=Math.min(e.pos,t.to);if(t.length){let i=n?n(t.from):-1,o=new ju(t.text,e.state.tabSize,e.unit,i<0?void 0:i);for(;o.pos<r-t.from;)vf(this.streamParser.token,o,a)}else this.streamParser.blankLine(a,e.unit);if(r==e.pos)break;i=t.to+1}let o=e.lineAt(e.pos);return n&&t==null&&ff.set(e.state,o.from),this.streamParser.indent(a,/^\s*(.*)/.exec(o.text)[1],e)}get allowsNesting(){return!1}};function mf(e,t,n,r,i){let a=n>=r&&n+t.length<=i&&t.prop(e.stateAfter);if(a)return{state:e.streamParser.copyState(a),pos:n+t.length};for(let a=t.children.length-1;a>=0;a--){let o=t.children[a],s=n+t.positions[a],c=o instanceof Y&&s<i&&mf(e,o,s,r,i);if(c)return c}return null}function hf(e,t,n,r,i){if(i&&n<=0&&r>=t.length)return t;!i&&n==0&&t.type==e.topNode&&(i=!0);for(let a=t.children.length-1;a>=0;a--){let o=t.positions[a],s=t.children[a],c;if(o<r&&s instanceof Y){if(!(c=hf(e,s,n-o,r-o,i)))break;return i?new Y(t.type,t.children.slice(0,a).concat(c),t.positions.slice(0,a+1),o+c.length):c}}return null}function gf(e,t,n,r,i){for(let i of t){let t=i.from+(i.openStart?25:0),a=i.to-(i.openEnd?25:0),o=t<=n&&a>n&&mf(e,i.tree,0-i.offset,n,a),s;if(o&&o.pos<=r&&(s=hf(e,i.tree,n+i.offset,o.pos+i.offset,!1)))return{state:o.state,tree:s}}return{state:e.streamParser.startState(i?Yu(i):4),tree:Y.empty}}var _f=class{constructor(e,t,n,r){this.lang=e,this.input=t,this.fragments=n,this.ranges=r,this.stoppedAt=null,this.chunks=[],this.chunkPos=[],this.chunk=[],this.chunkReused=void 0,this.rangeIndex=0,this.to=r[r.length-1].to;let i=Bu.get(),a=r[0].from,{state:o,tree:s}=gf(e,n,a,this.to,i?.state);this.state=o,this.parsedPos=this.chunkStart=a+s.length;for(let e=0;e<s.children.length;e++)this.chunks.push(s.children[e]),this.chunkPos.push(s.positions[e]);i&&this.parsedPos<i.viewport.from-1e5&&r.some(e=>e.from<=i.viewport.from&&e.to>=i.viewport.from)&&(this.state=this.lang.streamParser.startState(Yu(i.state)),i.skipUntilInView(this.parsedPos,i.viewport.from),this.parsedPos=i.viewport.from),this.moveRangeIndex()}advance(){let e=Bu.get(),t=this.stoppedAt==null?this.to:Math.min(this.to,this.stoppedAt),n=Math.min(t,this.chunkStart+512);for(e&&(n=Math.min(n,e.viewport.to));this.parsedPos<n;)this.parseLine(e);return this.chunkStart<this.parsedPos&&this.finishChunk(),this.parsedPos>=t?this.finish():e&&this.parsedPos>=e.viewport.to?(e.skipUntilInView(this.parsedPos,t),this.finish()):null}stopAt(e){this.stoppedAt=e}lineAfter(e){let t=this.input.chunk(e);if(this.input.lineChunks)t==`
`&&(t=``);else{let e=t.indexOf(`
`);e>-1&&(t=t.slice(0,e))}return e+t.length<=this.to?t:t.slice(0,this.to-e)}nextLine(){let e=this.parsedPos,t=this.lineAfter(e),n=e+t.length;for(let e=this.rangeIndex;;){let r=this.ranges[e].to;if(r>=n||(t=t.slice(0,r-(n-t.length)),e++,e==this.ranges.length))break;let i=this.ranges[e].from,a=this.lineAfter(i);t+=a,n=i+a.length}return{line:t,end:n}}skipGapsTo(e,t,n){for(;;){let r=this.ranges[this.rangeIndex].to,i=e+t;if(n>0?r>i:r>=i)break;let a=this.ranges[++this.rangeIndex].from;t+=a-r}return t}moveRangeIndex(){for(;this.ranges[this.rangeIndex].to<this.parsedPos;)this.rangeIndex++}emitToken(e,t,n,r){let i=4;if(this.ranges.length>1){r=this.skipGapsTo(t,r,1),t+=r;let e=this.chunk.length;r=this.skipGapsTo(n,r,-1),n+=r,i+=this.chunk.length-e}let a=this.chunk.length-4;return this.lang.streamParser.mergeTokens&&i==4&&a>=0&&this.chunk[a]==e&&this.chunk[a+2]==t?this.chunk[a+2]=n:this.chunk.push(e,t,n,i),r}parseLine(e){let{line:t,end:n}=this.nextLine(),r=0,{streamParser:i}=this.lang,a=new ju(t,e?e.state.tabSize:4,e?Yu(e.state):2);if(a.eol())i.blankLine(this.state,a.indentUnit);else for(;!a.eol();){let e=vf(i.token,a,this.state);if(e&&(r=this.emitToken(this.lang.tokenTable.resolve(e),this.parsedPos+a.start,this.parsedPos+a.pos,r)),a.start>1e4)break}this.parsedPos=n,this.moveRangeIndex(),this.parsedPos<this.to&&this.parsedPos++}finishChunk(){let e=Y.build({buffer:this.chunk,start:this.chunkStart,length:this.parsedPos-this.chunkStart,nodeSet:xf,topID:0,maxBufferLength:512,reused:this.chunkReused});e=new Y(e.type,e.children,e.positions,e.length,[[this.lang.stateAfter,this.lang.streamParser.copyState(this.state)]]),this.chunks.push(e),this.chunkPos.push(this.chunkStart-this.ranges[0].from),this.chunk=[],this.chunkReused=void 0,this.chunkStart=this.parsedPos}finish(){return new Y(this.lang.topNode,this.chunks,this.chunkPos,this.parsedPos-this.ranges[0].from).balance()}};function vf(e,t,n){t.start=t.pos;for(let r=0;r<10;r++){let r=e(t,n);if(t.pos>t.start)return r}throw Error(`Stream parser failed to advance stream.`)}var yf=Object.create(null),bf=[Al.none],xf=new jl(bf),Sf=[],Cf=Object.create(null),wf=Object.create(null);for(let[e,t]of[[`variable`,`variableName`],[`variable-2`,`variableName.special`],[`string-2`,`string.special`],[`def`,`variableName.definition`],[`tag`,`tagName`],[`attribute`,`attributeName`],[`type`,`typeName`],[`builtin`,`variableName.standard`],[`qualifier`,`modifier`],[`error`,`invalid`],[`header`,`heading`],[`property`,`propertyName`]])wf[e]=Of(yf,t);var Tf=class{constructor(e){this.extra=e,this.table=Object.assign(Object.create(null),wf)}resolve(e){return e?this.table[e]||(this.table[e]=Of(this.extra,e)):0}},Ef=new Tf(yf);function Df(e,t){Sf.indexOf(e)>-1||(Sf.push(e),console.warn(t))}function Of(e,t){let n=[];for(let r of t.split(` `)){let t=[];for(let n of r.split(`.`)){let r=e[n]||Z[n];r?typeof r==`function`?t.length?t=t.map(r):Df(n,`Modifier ${n} used at start of tag`):t.length?Df(n,`Tag ${n} used as modifier`):t=Array.isArray(r)?r:[r]:Df(n,`Unknown highlighting tag ${n}`)}for(let e of t)n.push(e)}if(!n.length)return 0;let r=t.replace(/ /g,`_`),i=r+` `+n.map(e=>e.id),a=Cf[i];if(a)return a.id;let o=Cf[i]=Al.define({id:bf.length,name:r,props:[cu({[r]:n})]});return bf.push(o),o.id}function kf(e,t){let n=Al.define({id:bf.length,name:`Document`,props:[Mu.add(()=>e),$u.add(()=>e=>t.getIndent(e))],top:!0});return bf.push(n),n}V.RTL,V.LTR;var Af=e=>{let{state:t}=e,n=t.doc.lineAt(t.selection.main.from),r=Ff(e.state,n.from);return r.line?Mf(e):r.block?Pf(e):!1};function jf(e,t){return({state:n,dispatch:r})=>{if(n.readOnly)return!1;let i=e(t,n);return i?(r(n.update(i)),!0):!1}}var Mf=jf(Bf,0),Nf=jf(zf,0),Pf=jf((e,t)=>zf(e,t,Rf(t)),0);function Ff(e,t){let n=e.languageDataAt(`commentTokens`,t,1);return n.length?n[0]:{}}var If=50;function Lf(e,{open:t,close:n},r,i){let a=e.sliceDoc(r-If,r),o=e.sliceDoc(i,i+If),s=/\s*$/.exec(a)[0].length,c=/^\s*/.exec(o)[0].length,l=a.length-s;if(a.slice(l-t.length,l)==t&&o.slice(c,c+n.length)==n)return{open:{pos:r-s,margin:s&&1},close:{pos:i+c,margin:c&&1}};let u,d;i-r<=100?u=d=e.sliceDoc(r,i):(u=e.sliceDoc(r,r+If),d=e.sliceDoc(i-If,i));let f=/^\s*/.exec(u)[0].length,p=/\s*$/.exec(d)[0].length,m=d.length-p-n.length;return u.slice(f,f+t.length)==t&&d.slice(m,m+n.length)==n?{open:{pos:r+f+t.length,margin:+!!/\s/.test(u.charAt(f+t.length))},close:{pos:i-p-n.length,margin:+!!/\s/.test(d.charAt(m-1))}}:null}function Rf(e){let t=[];for(let n of e.selection.ranges){let r=e.doc.lineAt(n.from),i=n.to<=r.to?r:e.doc.lineAt(n.to);i.from>r.from&&i.from==n.to&&(i=n.to==r.to+1?r:e.doc.lineAt(n.to-1));let a=t.length-1;a>=0&&t[a].to>r.from?t[a].to=i.to:t.push({from:r.from+/^\s*/.exec(r.text)[0].length,to:i.to})}return t}function zf(e,t,n=t.selection.ranges){let r=n.map(e=>Ff(t,e.from).block);if(!r.every(e=>e))return null;let i=n.map((e,n)=>Lf(t,r[n],e.from,e.to));if(e!=2&&!i.every(e=>e))return{changes:t.changes(n.map((e,t)=>i[t]?[]:[{from:e.from,insert:r[t].open+` `},{from:e.to,insert:` `+r[t].close}]))};if(e!=1&&i.some(e=>e)){let e=[];for(let t=0,n;t<i.length;t++)if(n=i[t]){let i=r[t],{open:a,close:o}=n;e.push({from:a.pos-i.open.length,to:a.pos+a.margin},{from:o.pos-o.margin,to:o.pos+i.close.length})}return{changes:e}}return null}function Bf(e,t,n=t.selection.ranges){let r=[],i=-1;ranges:for(let{from:e,to:a}of n){let n=r.length,o=1e9,s;for(let n=e;n<=a;){let c=t.doc.lineAt(n);if(s==null&&(s=Ff(t,c.from).line,!s))continue ranges;if(c.from>i&&(e==a||a>c.from)){i=c.from;let e=/^\s*/.exec(c.text)[0].length,t=e==c.length,n=c.text.slice(e,e+s.length)==s?e:-1;e<c.text.length&&e<o&&(o=e),r.push({line:c,comment:n,token:s,indent:e,empty:t,single:!1})}n=c.to+1}if(o<1e9)for(let e=n;e<r.length;e++)r[e].indent<r[e].line.text.length&&(r[e].indent=o);r.length==n+1&&(r[n].single=!0)}if(e!=2&&r.some(e=>e.comment<0&&(!e.empty||e.single))){let e=[];for(let{line:t,token:n,indent:i,empty:a,single:o}of r)(o||!a)&&e.push({from:t.from+i,insert:n+` `});let n=t.changes(e);return{changes:n,selection:t.selection.map(n,1)}}if(e!=1&&r.some(e=>e.comment>=0)){let e=[];for(let{line:t,comment:n,token:i}of r)if(n>=0){let r=t.from+n,a=r+i.length;t.text[a-t.from]==` `&&a++,e.push({from:r,to:a})}return{changes:e}}return null}var Vf=ft.define(),Hf=ft.define(),Uf=M.define(),Wf=M.define({combine(e){return Ot(e,{minDepth:100,newGroupDelay:500,joinToEvent:(e,t)=>t},{minDepth:Math.max,newGroupDelay:Math.min,joinToEvent:(e,t)=>(n,r)=>e(n,r)||t(n,r)})}}),Gf=Je.define({create(){return up.empty},update(e,t){let n=t.state.facet(Wf),r=t.annotation(Vf);if(r){let i=Qf.fromTransaction(t,r.selection),a=r.side,o=a==0?e.undone:e.done;return o=i?$f(o,o.length,n.minDepth,i):ap(o,t.startState.selection),new up(a==0?r.rest:o,a==0?o:r.rest)}let i=t.annotation(Hf);if((i==`full`||i==`before`)&&(e=e.isolate()),t.annotation(ht.addToHistory)===!1)return t.changes.empty?e:e.addMapping(t.changes.desc);let a=Qf.fromTransaction(t),o=t.annotation(ht.time),s=t.annotation(ht.userEvent);return a?e=e.addChanges(a,o,s,n,t):t.selection&&(e=e.addSelection(t.startState.selection,o,s,n.newGroupDelay)),(i==`full`||i==`after`)&&(e=e.isolate()),e},toJSON(e){return{done:e.done.map(e=>e.toJSON()),undone:e.undone.map(e=>e.toJSON())}},fromJSON(e){return new up(e.done.map(Qf.fromJSON),e.undone.map(Qf.fromJSON))}});function Kf(e={}){return[Gf,Wf.of(e),K.domEventHandlers({beforeinput(e,t){let n=e.inputType==`historyUndo`?Jf:e.inputType==`historyRedo`?Yf:null;return n?(e.preventDefault(),n(t)):!1}})]}function qf(e,t){return function({state:n,dispatch:r}){if(!t&&n.readOnly)return!1;let i=n.field(Gf,!1);if(!i)return!1;let a=i.pop(e,n,t);return a?(r(a),!0):!1}}var Jf=qf(0,!1),Yf=qf(1,!1),Xf=qf(0,!0),Zf=qf(1,!0),Qf=class e{constructor(e,t,n,r,i){this.changes=e,this.effects=t,this.mapped=n,this.startSelection=r,this.selectionsAfter=i}setSelAfter(t){return new e(this.changes,this.effects,this.mapped,this.startSelection,t)}toJSON(){return{changes:this.changes?.toJSON(),mapped:this.mapped?.toJSON(),startSelection:this.startSelection?.toJSON(),selectionsAfter:this.selectionsAfter.map(e=>e.toJSON())}}static fromJSON(t){return new e(t.changes&&Ne.fromJSON(t.changes),[],t.mapped&&Me.fromJSON(t.mapped),t.startSelection&&j.fromJSON(t.startSelection),t.selectionsAfter.map(j.fromJSON))}static fromTransaction(t,n){let r=rp;for(let e of t.startState.facet(Uf)){let n=e(t);n.length&&(r=r.concat(n))}return!r.length&&t.changes.empty?null:new e(t.changes.invert(t.startState.doc),r,void 0,n||t.startState.selection,rp)}static selection(t){return new e(void 0,rp,void 0,void 0,t)}};function $f(e,t,n,r){let i=t+1>n+20?t-n-1:0,a=e.slice(i,t);return a.push(r),a}function ep(e,t){let n=[],r=!1;return e.iterChangedRanges((e,t)=>n.push(e,t)),t.iterChangedRanges((e,t,i,a)=>{for(let e=0;e<n.length;){let t=n[e++],o=n[e++];a>=t&&i<=o&&(r=!0)}}),r}function tp(e,t){return e.ranges.length==t.ranges.length&&e.ranges.filter((e,n)=>e.empty!=t.ranges[n].empty).length===0}function np(e,t){return e.length?t.length?e.concat(t):e:t}var rp=[],ip=200;function ap(e,t){if(e.length){let n=e[e.length-1],r=n.selectionsAfter.slice(Math.max(0,n.selectionsAfter.length-ip));return r.length&&r[r.length-1].eq(t)?e:(r.push(t),$f(e,e.length-1,1e9,n.setSelAfter(r)))}return[Qf.selection([t])]}function op(e){let t=e[e.length-1],n=e.slice();return n[e.length-1]=t.setSelAfter(t.selectionsAfter.slice(0,t.selectionsAfter.length-1)),n}function sp(e,t){if(!e.length)return e;let n=e.length,r=rp;for(;n;){let i=cp(e[n-1],t,r);if(i.changes&&!i.changes.empty||i.effects.length){let t=e.slice(0,n);return t[n-1]=i,t}t=i.mapped,n--,r=i.selectionsAfter}return r.length?[Qf.selection(r)]:rp}function cp(e,t,n){let r=np(e.selectionsAfter.length?e.selectionsAfter.map(e=>e.map(t)):rp,n);if(!e.changes)return Qf.selection(r);let i=e.changes.map(t),a=t.mapDesc(e.changes,!0),o=e.mapped?e.mapped.composeDesc(a):a;return new Qf(i,N.mapEffects(e.effects,t),o,e.startSelection.map(a),r)}var lp=/^(input\.type|delete)($|\.)/,up=class e{constructor(e,t,n=0,r=void 0){this.done=e,this.undone=t,this.prevTime=n,this.prevUserEvent=r}isolate(){return this.prevTime?new e(this.done,this.undone):this}addChanges(t,n,r,i,a){let o=this.done,s=o[o.length-1];return o=s&&s.changes&&!s.changes.empty&&t.changes&&(!r||lp.test(r))&&(!s.selectionsAfter.length&&n-this.prevTime<i.newGroupDelay&&i.joinToEvent(a,ep(s.changes,t.changes))||r==`input.type.compose`)?$f(o,o.length-1,i.minDepth,new Qf(t.changes.compose(s.changes),np(N.mapEffects(t.effects,s.changes),s.effects),s.mapped,s.startSelection,rp)):$f(o,o.length,i.minDepth,t),new e(o,rp,n,r)}addSelection(t,n,r,i){let a=this.done.length?this.done[this.done.length-1].selectionsAfter:rp;return a.length>0&&n-this.prevTime<i&&r==this.prevUserEvent&&r&&/^select($|\.)/.test(r)&&tp(a[a.length-1],t)?this:new e(ap(this.done,t),this.undone,n,r)}addMapping(t){return new e(sp(this.done,t),sp(this.undone,t),this.prevTime,this.prevUserEvent)}pop(e,t,n){let r=e==0?this.done:this.undone;if(r.length==0)return null;let i=r[r.length-1],a=i.selectionsAfter[0]||(i.startSelection?i.startSelection.map(i.changes.invertedDesc,1):t.selection);if(n&&i.selectionsAfter.length)return t.update({selection:i.selectionsAfter[i.selectionsAfter.length-1],annotations:Vf.of({side:e,rest:op(r),selection:a}),userEvent:e==0?`select.undo`:`select.redo`,scrollIntoView:!0});if(i.changes){let n=r.length==1?rp:r.slice(0,r.length-1);return i.mapped&&(n=sp(n,i.mapped)),t.update({changes:i.changes,selection:i.startSelection,effects:i.effects,annotations:Vf.of({side:e,rest:n,selection:a}),filter:!1,userEvent:e==0?`undo`:`redo`,scrollIntoView:!0})}return null}};up.empty=new up(rp,rp);var dp=[{key:`Mod-z`,run:Jf,preventDefault:!0},{key:`Mod-y`,mac:`Mod-Shift-z`,run:Yf,preventDefault:!0},{linux:`Ctrl-Shift-z`,run:Yf,preventDefault:!0},{key:`Mod-u`,run:Xf,preventDefault:!0},{key:`Alt-u`,mac:`Mod-Shift-u`,run:Zf,preventDefault:!0}];function fp(e,t){return j.create(e.ranges.map(t),e.mainIndex)}function pp(e,t){return e.update({selection:t,scrollIntoView:!0,userEvent:`select`})}function mp({state:e,dispatch:t},n){let r=fp(e.selection,n);return!r.eq(e.selection,!0)&&(t(pp(e,r)),!0)}function hp(e,t){return j.cursor(t?e.to:e.from)}function gp(e,t){return mp(e,n=>n.empty?e.moveByChar(n,t):hp(n,t))}function _p(e){return e.textDirectionAt(e.state.selection.main.head)==V.LTR}var vp=e=>gp(e,!_p(e)),yp=e=>gp(e,_p(e));function bp(e,t){return mp(e,n=>n.empty?e.moveByGroup(n,t):hp(n,t))}var xp=e=>bp(e,!_p(e)),Sp=e=>bp(e,_p(e));typeof Intl<`u`&&Intl.Segmenter;function Cp(e,t,n){if(t.type.prop(n))return!0;let r=t.to-t.from;return r&&(r>2||/[^\s,.;:]/.test(e.sliceDoc(t.from,t.to)))||t.firstChild}function wp(e,t,n){let r=Lu(e).resolveInner(t.head),i=n?q.closedBy:q.openedBy;for(let a=t.head;;){let t=n?r.childAfter(a):r.childBefore(a);if(!t)break;Cp(e,t,i)?r=t:a=n?t.to:t.from}let a=r.type.prop(i),o,s;return s=a&&(o=n?sf(e,r.from,1):sf(e,r.to,-1))&&o.matched?n?o.end.to:o.end.from:n?r.to:r.from,j.cursor(s,n?-1:1)}var Tp=e=>mp(e,t=>wp(e.state,t,!_p(e))),Ep=e=>mp(e,t=>wp(e.state,t,_p(e)));function Dp(e,t){return mp(e,n=>{if(!n.empty)return hp(n,t);let r=e.moveVertically(n,t);return r.head==n.head?e.moveToLineBoundary(n,t):r})}var Op=e=>Dp(e,!1),kp=e=>Dp(e,!0);function Ap(e){let t=e.scrollDOM.clientHeight<e.scrollDOM.scrollHeight-2,n=0,r=0,i;if(t){for(let t of e.state.facet(K.scrollMargins)){let i=t(e);i?.top&&(n=Math.max(i?.top,n)),i?.bottom&&(r=Math.max(i?.bottom,r))}i=e.scrollDOM.clientHeight-n-r}else i=(e.dom.ownerDocument.defaultView||window).innerHeight;return{marginTop:n,marginBottom:r,selfScroll:t,height:Math.max(e.defaultLineHeight,i-5)}}function jp(e,t){let n=Ap(e),{state:r}=e,i=fp(r.selection,r=>r.empty?e.moveVertically(r,t,n.height):hp(r,t));if(i.eq(r.selection))return!1;let a;if(n.selfScroll){let t=e.coordsAtPos(r.selection.main.head),o=e.scrollDOM.getBoundingClientRect(),s=o.top+n.marginTop,c=o.bottom-n.marginBottom;t&&t.top>s&&t.bottom<c&&(a=K.scrollIntoView(i.main.head,{y:`start`,yMargin:t.top-s}))}return e.dispatch(pp(r,i),{effects:a}),!0}var Mp=e=>jp(e,!1),Np=e=>jp(e,!0);function Pp(e,t,n){let r=e.lineBlockAt(t.head),i=e.moveToLineBoundary(t,n);if(i.head==t.head&&i.head!=(n?r.to:r.from)&&(i=e.moveToLineBoundary(t,n,!1)),!n&&i.head==r.from&&r.length){let n=/^\s*/.exec(e.state.sliceDoc(r.from,Math.min(r.from+100,r.to)))[0].length;n&&t.head!=r.from+n&&(i=j.cursor(r.from+n))}return i}var Fp=e=>mp(e,t=>Pp(e,t,!0)),Ip=e=>mp(e,t=>Pp(e,t,!1)),Lp=e=>mp(e,t=>Pp(e,t,!_p(e))),Rp=e=>mp(e,t=>Pp(e,t,_p(e))),zp=e=>mp(e,t=>e.moveToLineBoundary(t,!1,!1)),Bp=e=>mp(e,t=>e.moveToLineBoundary(t,!0,!1));function Vp(e,t,n){let r=!1,i=fp(e.selection,t=>{let i=sf(e,t.head,-1)||sf(e,t.head,1)||t.head>0&&sf(e,t.head-1,1)||t.head<e.doc.length&&sf(e,t.head+1,-1);if(!i||!i.end)return t;r=!0;let a=i.start.from==t.head?i.end.to:i.end.from;return n?j.range(t.anchor,a):j.cursor(a)});return r?(t(pp(e,i)),!0):!1}var Hp=({state:e,dispatch:t})=>Vp(e,t,!1);function Up(e,t,n){let r=fp(e.state.selection,e=>{e.undirectional&&e.head>=e.anchor!=t&&(e=j.range(e.head,e.anchor));let r=n(e);return j.range(e.anchor,r.head,r.goalColumn,r.bidiLevel||void 0,r.assoc)});return!r.eq(e.state.selection)&&(e.dispatch(pp(e.state,r)),!0)}function Wp(e,t){return Up(e,t,n=>e.moveByChar(n,t))}var Gp=e=>Wp(e,!_p(e)),Kp=e=>Wp(e,_p(e));function qp(e,t){return Up(e,t,n=>e.moveByGroup(n,t))}var Jp=e=>qp(e,!_p(e)),Yp=e=>qp(e,_p(e)),Xp=e=>{let t=!_p(e);return Up(e,t,n=>wp(e.state,n,t))},Zp=e=>{let t=_p(e);return Up(e,t,n=>wp(e.state,n,t))};function Qp(e,t){return Up(e,t,n=>e.moveVertically(n,t))}var $p=e=>Qp(e,!1),em=e=>Qp(e,!0);function tm(e,t){return Up(e,t,n=>e.moveVertically(n,t,Ap(e).height))}var nm=e=>tm(e,!1),rm=e=>tm(e,!0),im=e=>Up(e,!0,t=>Pp(e,t,!0)),am=e=>Up(e,!1,t=>Pp(e,t,!1)),om=e=>{let t=!_p(e);return Up(e,t,n=>Pp(e,n,t))},sm=e=>{let t=_p(e);return Up(e,t,n=>Pp(e,n,t))},cm=e=>Up(e,!1,t=>j.cursor(e.lineBlockAt(t.head).from)),lm=e=>Up(e,!0,t=>j.cursor(e.lineBlockAt(t.head).to)),um=({state:e,dispatch:t})=>(t(pp(e,{anchor:0})),!0),dm=({state:e,dispatch:t})=>(t(pp(e,{anchor:e.doc.length})),!0),fm=({state:e,dispatch:t})=>(t(pp(e,{anchor:e.selection.main.anchor,head:0})),!0),pm=({state:e,dispatch:t})=>(t(pp(e,{anchor:e.selection.main.anchor,head:e.doc.length})),!0),mm=({state:e,dispatch:t})=>(t(e.update({selection:{anchor:0,head:e.doc.length},userEvent:`select`})),!0),hm=({state:e,dispatch:t})=>{let n=Pm(e).map(({from:t,to:n})=>j.undirectionalRange(t,Math.min(n+1,e.doc.length)));return t(e.update({selection:j.create(n),userEvent:`select`})),!0},gm=({state:e,dispatch:t})=>{let n=fp(e.selection,t=>{let n=Lu(e),r=n.resolveStack(t.from,1);if(t.empty){let e=n.resolveStack(t.from,-1);e.node.from>=r.node.from&&e.node.to<=r.node.to&&(r=e)}for(let e=r;e;e=e.next){let{node:n}=e;if((n.from<t.from&&n.to>=t.to||n.to>t.to&&n.from<=t.from)&&e.next)return j.undirectionalRange(n.from,n.to)}return t});return!n.eq(e.selection)&&(t(pp(e,n)),!0)};function _m(e,t){let{state:n}=e,r=n.selection,i=n.selection.ranges.slice();for(let r of n.selection.ranges){let a=n.doc.lineAt(r.head);if(t?a.to<e.state.doc.length:a.from>0)for(let n=r;;){let r=e.moveVertically(n,t);if(r.head<a.from||r.head>a.to){i.some(e=>e.head==r.head)||i.push(r);break}if(r.head==n.head)break;n=r}}return i.length!=r.ranges.length&&(e.dispatch(pp(n,j.create(i,i.length-1))),!0)}var vm=e=>_m(e,!1),ym=e=>_m(e,!0),bm=({state:e,dispatch:t})=>{let n=e.selection,r=null;return n.ranges.length>1?r=j.create([n.main]):n.main.empty||(r=j.create([j.cursor(n.main.head)])),r?(t(pp(e,r)),!0):!1};function xm(e,t){if(e.state.readOnly)return!1;let n=`delete.selection`,{state:r}=e,i=r.changeByRange(r=>{let{from:i,to:a}=r;if(i==a){let o=t(r);o<i?(n=`delete.backward`,o=Sm(e,o,!1)):o>i&&(n=`delete.forward`,o=Sm(e,o,!0)),i=Math.min(i,o),a=Math.max(a,o)}else i=Sm(e,i,!1),a=Sm(e,a,!0);return i==a?{range:r}:{changes:{from:i,to:a},range:j.cursor(i,i<r.head?-1:1)}});return!i.changes.empty&&(e.dispatch(r.update(i,{scrollIntoView:!0,userEvent:n,effects:n==`delete.selection`?K.announce.of(r.phrase(`Selection deleted`)):void 0})),!0)}function Sm(e,t,n){if(e instanceof K)for(let r of e.state.facet(K.atomicRanges).map(t=>t(e)))r.between(t,t,(e,r)=>{e<t&&r>t&&(t=n?r:e)});return t}var Cm=(e,t,n)=>xm(e,r=>{let i=r.from,{state:a}=e,o=a.doc.lineAt(i),s,c;if(n&&!t&&i>o.from&&i<o.from+200&&!/[^ \t]/.test(s=o.text.slice(0,i-o.from))){if(s[s.length-1]==`	`)return i-1;let e=qt(s,a.tabSize)%Yu(a)||Yu(a);for(let t=0;t<e&&s[s.length-1-t]==` `;t++)i--;c=i}else c=k(o.text,i-o.from,t,t)+o.from,c==i&&o.number!=(t?a.doc.lines:1)?c+=t?1:-1:!t&&/[\ufe00-\ufe0f]/.test(o.text.slice(c-o.from,i-o.from))&&(c=k(o.text,c-o.from,!1,!1)+o.from);return c}),wm=e=>Cm(e,!1,!0),Tm=e=>Cm(e,!0,!1),Em=(e,t)=>xm(e,n=>{let r=n.head,{state:i}=e,a=i.doc.lineAt(r),o=i.charCategorizer(r);for(let e=null;;){if(r==(t?a.to:a.from)){r==n.head&&a.number!=(t?i.doc.lines:1)&&(r+=t?1:-1);break}let s=k(a.text,r-a.from,t)+a.from,c=a.text.slice(Math.min(r,s)-a.from,Math.max(r,s)-a.from),l=o(c);if(e!=null&&l!=e)break;(c!=` `||r!=n.head)&&(e=l),r=s}return r}),Dm=e=>Em(e,!1),Om=e=>Em(e,!0),km=e=>xm(e,t=>{let n=e.lineBlockAt(t.head).to;return t.head<n?n:Math.min(e.state.doc.length,t.head+1)}),Am=e=>xm(e,t=>{let n=e.moveToLineBoundary(t,!1).head;return t.head>n?n:Math.max(0,t.head-1)}),jm=e=>xm(e,t=>{let n=e.moveToLineBoundary(t,!0).head;return t.head<n?n:Math.min(e.state.doc.length,t.head+1)}),Mm=({state:e,dispatch:t})=>{if(e.readOnly)return!1;let n=e.changeByRange(e=>({changes:{from:e.from,to:e.to,insert:D.of([``,``])},range:j.cursor(e.from)}));return t(e.update(n,{scrollIntoView:!0,userEvent:`input`})),!0},Nm=({state:e,dispatch:t})=>{if(e.readOnly)return!1;let n=e.changeByRange(t=>{if(!t.empty||t.from==0||t.from==e.doc.length)return{range:t};let n=t.from,r=e.doc.lineAt(n),i=n==r.from?n-1:k(r.text,n-r.from,!1)+r.from,a=n==r.to?n+1:k(r.text,n-r.from,!0)+r.from;return{changes:{from:i,to:a,insert:e.doc.slice(n,a).append(e.doc.slice(i,n))},range:j.cursor(a)}});return!n.changes.empty&&(t(e.update(n,{scrollIntoView:!0,userEvent:`move.character`})),!0)};function Pm(e){let t=[],n=-1;for(let r of e.selection.ranges){let i=e.doc.lineAt(r.from),a=e.doc.lineAt(r.to);if(!r.empty&&r.to==a.from&&(a=e.doc.lineAt(r.to-1)),n>=i.number){let e=t[t.length-1];e.to=a.to,e.ranges.push(r)}else t.push({from:i.from,to:a.to,ranges:[r]});n=a.number+1}return t}function Fm(e,t,n){if(e.readOnly)return!1;let r=[],i=[];for(let t of Pm(e)){if(n?t.to==e.doc.length:t.from==0)continue;let a=e.doc.lineAt(n?t.to+1:t.from-1),o=a.length+1;if(n){r.push({from:t.to,to:a.to},{from:t.from,insert:a.text+e.lineBreak});for(let n of t.ranges)i.push(j.range(Math.min(e.doc.length,n.anchor+o),Math.min(e.doc.length,n.head+o)))}else{r.push({from:a.from,to:t.from},{from:t.to,insert:e.lineBreak+a.text});for(let e of t.ranges)i.push(j.range(e.anchor-o,e.head-o))}}return r.length?(t(e.update({changes:r,scrollIntoView:!0,selection:j.create(i,e.selection.mainIndex),userEvent:`move.line`})),!0):!1}var Im=({state:e,dispatch:t})=>Fm(e,t,!1),Lm=({state:e,dispatch:t})=>Fm(e,t,!0);function Rm(e,t,n){if(e.readOnly)return!1;let r=[];for(let t of Pm(e))n?r.push({from:t.from,insert:e.doc.slice(t.from,t.to)+e.lineBreak}):r.push({from:t.to,insert:e.lineBreak+e.doc.slice(t.from,t.to)});let i=e.changes(r);return t(e.update({changes:i,selection:e.selection.map(i,n?1:-1),scrollIntoView:!0,userEvent:`input.copyline`})),!0}var zm=({state:e,dispatch:t})=>Rm(e,t,!1),Bm=({state:e,dispatch:t})=>Rm(e,t,!0),Vm=e=>{if(e.state.readOnly)return!1;let{state:t}=e,n=t.changes(Pm(t).map(({from:e,to:n})=>(e>0?e--:n<t.doc.length&&n++,{from:e,to:n}))),r=fp(t.selection,t=>{let n;if(e.lineWrapping){let r=e.lineBlockAt(t.head),i=e.coordsAtPos(t.head,t.assoc||1);i&&(n=r.bottom+e.documentTop-i.bottom+e.defaultLineHeight/2)}return e.moveVertically(t,!0,n)}).map(n);return e.dispatch({changes:n,selection:r,scrollIntoView:!0,userEvent:`delete.line`}),!0};function Hm(e,t){if(/\(\)|\[\]|\{\}/.test(e.sliceDoc(t-1,t+1)))return{from:t,to:t};let n=Lu(e).resolveInner(t),r=n.childBefore(t),i=n.childAfter(t),a;return r&&i&&r.to<=t&&i.from>=t&&(a=r.type.prop(q.closedBy))&&a.indexOf(i.name)>-1&&e.doc.lineAt(r.to).from==e.doc.lineAt(i.from).from&&!/\S/.test(e.sliceDoc(r.to,i.from))?{from:r.to,to:i.from}:null}var Um=Gm(!1),Wm=Gm(!0);function Gm(e){return({state:t,dispatch:n})=>{if(t.readOnly)return!1;let r=t.changeByRange(n=>{let{from:r,to:i}=n,a=t.doc.lineAt(r),o=!e&&r==i&&Hm(t,r);e&&(r=i=(i<=a.to?a:t.doc.lineAt(i)).to);let s=new Qu(t,{simulateBreak:r,simulateDoubleBreak:!!o}),c=Zu(s,r);for(c??=qt(/^\s*/.exec(t.doc.lineAt(r).text)[0],t.tabSize);i<a.to&&/\s/.test(a.text[i-a.from]);)i++;o?{from:r,to:i}=o:r>a.from&&r<a.from+100&&!/\S/.test(a.text.slice(0,r))&&(r=a.from);let l=[``,Xu(t,c)];return o&&l.push(Xu(t,s.lineIndent(a.from,-1))),{changes:{from:r,to:i,insert:D.of(l)},range:j.cursor(r+1+l[1].length)}});return n(t.update(r,{scrollIntoView:!0,userEvent:`input`})),!0}}function Km(e,t){let n=-1;return e.changeByRange(r=>{let i=[];for(let a=r.from;a<=r.to;){let o=e.doc.lineAt(a);o.number>n&&(r.empty||r.to>o.from)&&(t(o,i,r),n=o.number),a=o.to+1}let a=e.changes(i);return{changes:i,range:j.range(a.mapPos(r.anchor,1),a.mapPos(r.head,1))}})}var qm=({state:e,dispatch:t})=>{if(e.readOnly)return!1;let n=Object.create(null),r=new Qu(e,{overrideIndentation:e=>n[e]??-1}),i=Km(e,(t,i,a)=>{let o=Zu(r,t.from);if(o==null)return;/\S/.test(t.text)||(o=0);let s=/^\s*/.exec(t.text)[0],c=Xu(e,o);(s!=c||a.from<t.from+s.length)&&(n[t.from]=o,i.push({from:t.from,to:t.from+s.length,insert:c}))});return i.changes.empty||t(e.update(i,{userEvent:`indent`})),!0},Jm=({state:e,dispatch:t})=>!e.readOnly&&(t(e.update(Km(e,(t,n)=>{n.push({from:t.from,insert:e.facet(Ju)})}),{userEvent:`input.indent`})),!0),Ym=({state:e,dispatch:t})=>!e.readOnly&&(t(e.update(Km(e,(t,n)=>{let r=/^\s*/.exec(t.text)[0];if(!r)return;let i=qt(r,e.tabSize),a=0,o=Xu(e,Math.max(0,i-Yu(e)));for(;a<r.length&&a<o.length&&r.charCodeAt(a)==o.charCodeAt(a);)a++;n.push({from:t.from+a,to:t.from+r.length,insert:o.slice(a)})}),{userEvent:`delete.dedent`})),!0),Xm=e=>(e.setTabFocusMode(),!0),Zm=[{key:`Ctrl-b`,run:vp,shift:Gp,preventDefault:!0},{key:`Ctrl-f`,run:yp,shift:Kp},{key:`Ctrl-p`,run:Op,shift:$p},{key:`Ctrl-n`,run:kp,shift:em},{key:`Ctrl-a`,run:zp,shift:cm},{key:`Ctrl-e`,run:Bp,shift:lm},{key:`Ctrl-d`,run:Tm},{key:`Ctrl-h`,run:wm},{key:`Ctrl-k`,run:km},{key:`Ctrl-Alt-h`,run:Dm},{key:`Ctrl-o`,run:Mm},{key:`Ctrl-t`,run:Nm},{key:`Ctrl-v`,run:Np}],Qm=[{key:`ArrowLeft`,run:vp,shift:Gp,preventDefault:!0},{key:`Mod-ArrowLeft`,mac:`Alt-ArrowLeft`,run:xp,shift:Jp,preventDefault:!0},{mac:`Cmd-ArrowLeft`,run:Lp,shift:om,preventDefault:!0},{key:`ArrowRight`,run:yp,shift:Kp,preventDefault:!0},{key:`Mod-ArrowRight`,mac:`Alt-ArrowRight`,run:Sp,shift:Yp,preventDefault:!0},{mac:`Cmd-ArrowRight`,run:Rp,shift:sm,preventDefault:!0},{key:`ArrowUp`,run:Op,shift:$p,preventDefault:!0},{mac:`Cmd-ArrowUp`,run:um,shift:fm},{mac:`Ctrl-ArrowUp`,run:Mp,shift:nm},{key:`ArrowDown`,run:kp,shift:em,preventDefault:!0},{mac:`Cmd-ArrowDown`,run:dm,shift:pm},{mac:`Ctrl-ArrowDown`,run:Np,shift:rm},{key:`PageUp`,run:Mp,shift:nm},{key:`PageDown`,run:Np,shift:rm},{key:`Home`,run:Ip,shift:am,preventDefault:!0},{key:`Mod-Home`,run:um,shift:fm},{key:`End`,run:Fp,shift:im,preventDefault:!0},{key:`Mod-End`,run:dm,shift:pm},{key:`Enter`,run:Um,shift:Um},{key:`Mod-a`,run:mm},{key:`Backspace`,run:wm,shift:wm,preventDefault:!0},{key:`Delete`,run:Tm,preventDefault:!0},{key:`Mod-Backspace`,mac:`Alt-Backspace`,run:Dm,preventDefault:!0},{key:`Mod-Delete`,mac:`Alt-Delete`,run:Om,preventDefault:!0},{mac:`Mod-Backspace`,run:Am,preventDefault:!0},{mac:`Mod-Delete`,run:jm,preventDefault:!0}].concat(Zm.map(e=>({mac:e.key,run:e.run,shift:e.shift}))),$m=[{key:`Alt-ArrowLeft`,mac:`Ctrl-ArrowLeft`,run:Tp,shift:Xp},{key:`Alt-ArrowRight`,mac:`Ctrl-ArrowRight`,run:Ep,shift:Zp},{key:`Alt-ArrowUp`,run:Im},{key:`Shift-Alt-ArrowUp`,run:zm},{key:`Alt-ArrowDown`,run:Lm},{key:`Shift-Alt-ArrowDown`,run:Bm},{key:`Mod-Alt-ArrowUp`,run:vm},{key:`Mod-Alt-ArrowDown`,run:ym},{key:`Escape`,run:bm},{key:`Mod-Enter`,run:Wm},{key:`Alt-l`,mac:`Ctrl-l`,run:hm},{key:`Mod-i`,run:gm,preventDefault:!0},{key:`Mod-[`,run:Ym},{key:`Mod-]`,run:Jm},{key:`Mod-Alt-\\`,run:qm},{key:`Shift-Mod-k`,run:Vm},{key:`Shift-Mod-\\`,run:Hp},{key:`Mod-/`,run:Af},{key:`Alt-A`,mac:`Ctrl-A`,run:Nf},{key:`Ctrl-m`,mac:`Shift-Alt-m`,run:Xm}].concat(Qm),eh={key:`Tab`,run:Jm,shift:Ym},th=typeof String.prototype.normalize==`function`?e=>e.normalize(`NFKD`):e=>e,nh=class{constructor(e,t,n=0,r=e.length,i,a){this.test=a,this.value={from:0,to:0,precise:!1},this.done=!1,this.matches=[],this.buffer=``,this.bufferPos=0,this.iter=e.iterRange(n,r),this.bufferStart=n,this.normalize=i?e=>i(th(e)):th,this.query=this.normalize(t)}peek(){if(this.bufferPos==this.buffer.length){if(this.bufferStart+=this.buffer.length,this.iter.next(),this.iter.done)return-1;this.bufferPos=0,this.buffer=this.iter.value}return De(this.buffer,this.bufferPos)}next(){for(;this.matches.length;)this.matches.pop();return this.nextOverlapping()}nextOverlapping(){for(;;){let e=this.peek();if(e<0)return this.done=!0,this;let t=Oe(e),n=this.bufferStart+this.bufferPos;this.bufferPos+=ke(e);let r=this.normalize(t);if(r.length)for(let e=0,i=n,a=!0;;e++){let n=r.charCodeAt(e),o=this.match(n,i,a,this.bufferPos+this.bufferStart,e==r.length-1);if(o)return this.value=o,this;if(e==r.length-1)break;a&&e<t.length&&t.charCodeAt(e)==n?i++:a=!1}}}match(e,t,n,r,i){let a=null;for(let t=0;t<this.matches.length;){let n=this.matches[t],o=!1;this.query.charCodeAt(n.index)==e&&(n.index==this.query.length-1?a={from:n.from,to:r,precise:i&&n.precise}:(n.index++,o=!0)),o?t++:this.matches.splice(t,1)}return this.query.charCodeAt(0)==e&&(this.query.length==1?a={from:t,to:r,precise:n&&i}:this.matches.push({from:t,index:1,precise:n})),a&&this.test&&!this.test(a.from,a.to,this.buffer,this.bufferStart)&&(a=null),a}};typeof Symbol<`u`&&(nh.prototype[Symbol.iterator]=function(){return this});var rh={from:-1,to:-1,match:/.*/.exec(``),precise:!0},ih=`gm`+(/x/.unicode==null?``:`u`),ah=class{constructor(e,t,n,r=0,i=e.length){if(this.text=e,this.to=i,this.curLine=``,this.done=!1,this.value=rh,/\\[sWDnr]|\n|\r|\[\^/.test(t))return new ch(e,t,n,r,i);this.re=new RegExp(t,ih+(n?.ignoreCase?`i`:``)),this.test=n?.test,this.iter=e.iter();let a=e.lineAt(r);this.curLineStart=a.from,this.matchPos=uh(e,r),this.getLine(this.curLineStart)}getLine(e){this.iter.next(e),this.iter.lineBreak?this.curLine=``:(this.curLine=this.iter.value,this.curLineStart+this.curLine.length>this.to&&(this.curLine=this.curLine.slice(0,this.to-this.curLineStart)),this.iter.next())}nextLine(){this.curLineStart=this.curLineStart+this.curLine.length+1,this.curLineStart>this.to?this.curLine=``:this.getLine(0)}next(){for(let e=this.matchPos-this.curLineStart;;){this.re.lastIndex=e;let t=this.matchPos<=this.to&&this.re.exec(this.curLine);if(t){let n=this.curLineStart+t.index,r=n+t[0].length;if(this.matchPos=uh(this.text,r+ +(n==r)),n==this.curLineStart+this.curLine.length&&this.nextLine(),(n<r||n>this.value.to)&&(!this.test||this.test(n,r,t)))return this.value={from:n,to:r,precise:!0,match:t},this;e=this.matchPos-this.curLineStart}else if(this.curLineStart+this.curLine.length<this.to)this.nextLine(),e=0;else return this.done=!0,this}}},oh=new WeakMap,sh=class e{constructor(e,t){this.from=e,this.text=t}get to(){return this.from+this.text.length}static get(t,n,r){let i=oh.get(t);if(!i||i.from>=r||i.to<=n){let i=new e(n,t.sliceString(n,r));return oh.set(t,i),i}if(i.from==n&&i.to==r)return i;let{text:a,from:o}=i;return o>n&&(a=t.sliceString(n,o)+a,o=n),i.to<r&&(a+=t.sliceString(i.to,r)),oh.set(t,new e(o,a)),new e(n,a.slice(n-o,r-o))}},ch=class{constructor(e,t,n,r,i){this.text=e,this.to=i,this.done=!1,this.value=rh,this.matchPos=uh(e,r),this.re=new RegExp(t,ih+(n?.ignoreCase?`i`:``)),this.test=n?.test,this.flat=sh.get(e,r,this.chunkEnd(r+5e3))}chunkEnd(e){return e>=this.to?this.to:this.text.lineAt(e).to}next(){for(;;){let e=this.re.lastIndex=this.matchPos-this.flat.from,t=this.re.exec(this.flat.text);if(t&&!t[0]&&t.index==e&&(this.re.lastIndex=e+1,t=this.re.exec(this.flat.text)),t){let e=this.flat.from+t.index,n=e+t[0].length;if((this.flat.to>=this.to||t.index+t[0].length<=this.flat.text.length-10)&&(!this.test||this.test(e,n,t)))return this.value={from:e,to:n,precise:!0,match:t},this.matchPos=uh(this.text,n+ +(e==n)),this}if(this.flat.to==this.to)return this.done=!0,this;this.flat=sh.get(this.text,this.flat.from,this.chunkEnd(this.flat.from+this.flat.text.length*2))}}};typeof Symbol<`u`&&(ah.prototype[Symbol.iterator]=ch.prototype[Symbol.iterator]=function(){return this});function lh(e){try{return new RegExp(e,ih),!0}catch{return!1}}function uh(e,t){if(t>=e.length)return t;let n=e.lineAt(t),r;for(;t<n.to&&(r=n.text.charCodeAt(t-n.from))>=56320&&r<57344;)t++;return t}var dh=e=>{let t=Jc(e,`cm-goto-line`);if(t){let e=t.dom.querySelector(`input[type=text]`);return e&&e.select(),!0}let{state:n}=e,r=String(n.doc.lineAt(e.state.selection.main.head).number),{close:i,result:a}=qc(e,{class:`cm-goto-line`,label:n.phrase(`Go to line`),input:{type:`text`,name:`line`,value:r},focus:!0,submitLabel:n.phrase(`go`)});return a.then(t=>{let r=t&&/^([+-])?(\d+)?(:\d+)?(%)?$/.exec(t.elements.line.value);if(!r){e.dispatch({effects:i});return}let a=n.doc.lineAt(n.selection.main.head),[,o,s,c,l]=r,u=c?+c.slice(1):0,d=s?+s:a.number;if(s&&l){let e=d/100;o&&(e=e*(o==`-`?-1:1)+a.number/n.doc.lines),d=Math.round(n.doc.lines*e)}else s&&o&&(d=d*(o==`-`?-1:1)+a.number);let f=n.doc.line(Math.max(1,Math.min(n.doc.lines,d))),p=j.cursor(f.from+Math.max(0,Math.min(u,f.length)));e.dispatch({effects:[i,K.scrollIntoView(p.from,{y:`center`})],selection:p})}),!0},fh={highlightWordAroundCursor:!1,minSelectionLength:1,maxMatches:100,wholeWords:!1},ph=M.define({combine(e){return Ot(e,fh,{highlightWordAroundCursor:(e,t)=>e||t,minSelectionLength:Math.min,maxMatches:Math.min})}});function mh(e){let t=[bh,yh];return e&&t.push(ph.of(e)),t}var hh=z.mark({class:`cm-selectionMatch`}),gh=z.mark({class:`cm-selectionMatch cm-selectionMatch-main`});function _h(e,t,n,r){return(n==0||e(t.sliceDoc(n-1,n))!=P.Word)&&(r==t.doc.length||e(t.sliceDoc(r,r+1))!=P.Word)}function vh(e,t,n,r){return e(t.sliceDoc(n,n+1))==P.Word&&e(t.sliceDoc(r-1,r))==P.Word}var yh=U.fromClass(class{constructor(e){this.decorations=this.getDeco(e)}update(e){(e.selectionSet||e.docChanged||e.viewportChanged)&&(this.decorations=this.getDeco(e.view))}getDeco(e){let t=e.state.facet(ph),{state:n}=e,r=n.selection;if(r.ranges.length>1)return z.none;let i=r.main,a,o=null;if(i.empty){if(!t.highlightWordAroundCursor)return z.none;let e=n.wordAt(i.head);if(!e)return z.none;o=n.charCategorizer(i.head),a=n.sliceDoc(e.from,e.to)}else{let e=i.to-i.from;if(e<t.minSelectionLength||e>200)return z.none;if(t.wholeWords){if(a=n.sliceDoc(i.from,i.to),o=n.charCategorizer(i.head),!(_h(o,n,i.from,i.to)&&vh(o,n,i.from,i.to)))return z.none}else if(a=n.sliceDoc(i.from,i.to),!a)return z.none}let s=[];for(let r of e.visibleRanges){let e=new nh(n.doc,a,r.from,r.to);for(;!e.next().done;){let{from:r,to:a}=e.value;if((!o||_h(o,n,r,a))&&(i.empty&&r<=i.from&&a>=i.to?s.push(gh.range(r,a)):(r>=i.to||a<=i.from)&&s.push(hh.range(r,a)),s.length>t.maxMatches))return z.none}}return z.set(s)}},{decorations:e=>e.decorations}),bh=K.baseTheme({".cm-selectionMatch":{backgroundColor:`#99ff7780`},".cm-searchMatch .cm-selectionMatch":{backgroundColor:`transparent`}}),xh=({state:e,dispatch:t})=>{let{selection:n}=e,r=j.create(n.ranges.map(t=>e.wordAt(t.head)||j.cursor(t.head)),n.mainIndex);return!r.eq(n)&&(t(e.update({selection:r})),!0)};function Sh(e,t){let{main:n,ranges:r}=e.selection,i=e.wordAt(n.head),a=i&&i.from==n.from&&i.to==n.to;for(let n=!1,i=new nh(e.doc,t,r[r.length-1].to);;)if(i.next(),i.done){if(n)return null;i=new nh(e.doc,t,0,Math.max(0,r[r.length-1].from-1)),n=!0}else{if(n&&r.some(e=>e.from==i.value.from))continue;if(a){let t=e.wordAt(i.value.from);if(!t||t.from!=i.value.from||t.to!=i.value.to)continue}return i.value}}var Ch=({state:e,dispatch:t})=>{let{ranges:n}=e.selection;if(n.some(e=>e.from===e.to))return xh({state:e,dispatch:t});let r=e.sliceDoc(n[0].from,n[0].to);if(e.selection.ranges.some(t=>e.sliceDoc(t.from,t.to)!=r))return!1;let i=Sh(e,r);return i?(t(e.update({selection:e.selection.addRange(j.range(i.from,i.to),!1),effects:K.scrollIntoView(i.to)})),!0):!1},wh=M.define({combine(e){return Ot(e,{top:!1,caseSensitive:!1,literal:!1,regexp:!1,wholeWord:!1,createPanel:e=>new ig(e),scrollToMatch:e=>K.scrollIntoView(e)})}}),Th=class{constructor(e){this.search=e.search,this.caseSensitive=!!e.caseSensitive,this.literal=!!e.literal,this.regexp=!!e.regexp,this.replace=e.replace||``,this.valid=!!this.search&&(!this.regexp||lh(this.search)),this.unquoted=this.unquote(this.search),this.wholeWord=!!e.wholeWord,this.test=e.test}unquote(e){return this.literal?e:e.replace(/\\([nrt\\])/g,(e,t)=>t==`n`?`
`:t==`r`?`\r`:t==`t`?`	`:`\\`)}eq(e){return this.search==e.search&&this.replace==e.replace&&this.caseSensitive==e.caseSensitive&&this.regexp==e.regexp&&this.wholeWord==e.wholeWord&&this.test==e.test}create(){return this.regexp?new Ih(this):new Ah(this)}getCursor(e,t=0,n){let r=e.doc?e:F.create({doc:e});return n??=r.doc.length,this.regexp?Mh(this,r,t,n):Oh(this,r,t,n)}},Eh=class{constructor(e){this.spec=e}};function Dh(e,t,n){return(r,i,a,o)=>n&&!n(r,i,a,o)?!1:e(r>=o&&i<=o+a.length?a.slice(r-o,i-o):t.doc.sliceString(r,i),t,r,i)}function Oh(e,t,n,r){let i;return e.wholeWord&&(i=kh(t.doc,t.charCategorizer(t.selection.main.head))),e.test&&(i=Dh(e.test,t,i)),new nh(t.doc,e.unquoted,n,r,e.caseSensitive?void 0:e=>e.toLowerCase(),i)}function kh(e,t){return(n,r,i,a)=>((a>n||a+i.length<r)&&(a=Math.max(0,n-2),i=e.sliceString(a,Math.min(e.length,r+2))),(t(Nh(i,n-a))!=P.Word||t(Ph(i,n-a))!=P.Word)&&(t(Ph(i,r-a))!=P.Word||t(Nh(i,r-a))!=P.Word))}var Ah=class extends Eh{constructor(e){super(e)}nextMatch(e,t,n){let r=Oh(this.spec,e,n,e.doc.length).nextOverlapping();if(r.done){let n=Math.min(e.doc.length,t+this.spec.unquoted.length);r=Oh(this.spec,e,0,n).nextOverlapping()}return r.done||r.value.from==t&&r.value.to==n?null:r.value}prevMatchInRange(e,t,n){for(let r=n;;){let n=Math.max(t,r-1e4-this.spec.unquoted.length),i=Oh(this.spec,e,n,r),a=null;for(;!i.nextOverlapping().done;)a=i.value;if(a)return a;if(n==t)return null;r-=1e4}}prevMatch(e,t,n){let r=this.prevMatchInRange(e,0,t);return r||=this.prevMatchInRange(e,Math.max(0,n-this.spec.unquoted.length),e.doc.length),r&&(r.from!=t||r.to!=n)?r:null}getReplacement(e){return this.spec.unquote(this.spec.replace)}matchAll(e,t){let n=Oh(this.spec,e,0,e.doc.length),r=[];for(;!n.next().done;){if(r.length>=t)return null;r.push(n.value)}return r}highlight(e,t,n,r){let i=Oh(this.spec,e,Math.max(0,t-this.spec.unquoted.length),Math.min(n+this.spec.unquoted.length,e.doc.length));for(;!i.next().done;)r(i.value.from,i.value.to)}};function jh(e,t,n){return(r,i,a)=>(!n||n(r,i,a))&&e(a[0],t,r,i)}function Mh(e,t,n,r){let i;return e.wholeWord&&(i=Fh(t.charCategorizer(t.selection.main.head))),e.test&&(i=jh(e.test,t,i)),new ah(t.doc,e.search,{ignoreCase:!e.caseSensitive,test:i},n,r)}function Nh(e,t){return e.slice(k(e,t,!1),t)}function Ph(e,t){return e.slice(t,k(e,t))}function Fh(e){return(t,n,r)=>!r[0].length||(e(Nh(r.input,r.index))!=P.Word||e(Ph(r.input,r.index))!=P.Word)&&(e(Ph(r.input,r.index+r[0].length))!=P.Word||e(Nh(r.input,r.index+r[0].length))!=P.Word)}var Ih=class extends Eh{nextMatch(e,t,n){let r=Mh(this.spec,e,n,e.doc.length).next();return r.done&&(r=Mh(this.spec,e,0,t).next()),r.done?null:r.value}prevMatchInRange(e,t,n){for(let r=1;;r++){let i=Math.max(t,n-r*1e4),a=Mh(this.spec,e,i,n),o=null;for(;!a.next().done;)o=a.value;if(o&&(i==t||o.from>i+10))return o;if(i==t)return null}}prevMatch(e,t,n){return this.prevMatchInRange(e,0,t)||this.prevMatchInRange(e,n,e.doc.length)}getReplacement(e){return this.spec.unquote(this.spec.replace).replace(/\$([$&]|\d+)/g,(t,n)=>{if(n==`&`)return e.match[0];if(n==`$`)return`$`;for(let t=n.length;t>0;t--){let r=+n.slice(0,t);if(r>0&&r<e.match.length)return e.match[r]+n.slice(t)}return t})}matchAll(e,t){let n=Mh(this.spec,e,0,e.doc.length),r=[];for(;!n.next().done;){if(r.length>=t)return null;r.push(n.value)}return r}highlight(e,t,n,r){let i=Mh(this.spec,e,Math.max(0,t-250),Math.min(n+250,e.doc.length));for(;!i.next().done;)r(i.value.from,i.value.to)}},Lh=N.define(),Rh=N.define(),zh=Je.define({create(e){return new Bh(Qh(e).create(),null)},update(e,t){for(let n of t.effects)n.is(Lh)?e=new Bh(n.value.create(),e.panel):n.is(Rh)&&(e=new Bh(e.query,n.value?Zh:null));return e},provide:e=>Kc.from(e,e=>e.panel)}),Bh=class{constructor(e,t){this.query=e,this.panel=t}},Vh=z.mark({class:`cm-searchMatch`}),Hh=z.mark({class:`cm-searchMatch cm-searchMatch-selected`}),Uh=U.fromClass(class{constructor(e){this.view=e,this.decorations=this.highlight(e.state.field(zh))}update(e){let t=e.state.field(zh);(t!=e.startState.field(zh)||e.docChanged||e.selectionSet||e.viewportChanged)&&(this.decorations=this.highlight(t))}highlight({query:e,panel:t}){if(!t||!e.spec.valid)return z.none;let{view:n}=this,r=new It;for(let t=0,i=n.visibleRanges,a=i.length;t<a;t++){let{from:o,to:s}=i[t];for(;t<a-1&&s>i[t+1].from-500;)s=i[++t].to;e.highlight(n.state,o,s,(e,t)=>{let i=n.state.selection.ranges.some(n=>n.from==e&&n.to==t);r.add(e,t,i?Hh:Vh)})}return r.finish()}},{decorations:e=>e.decorations});function Wh(e){return t=>{let n=t.state.field(zh,!1);return n&&n.query.spec.valid?e(t,n):tg(t)}}var Gh=Wh((e,{query:t})=>{let{to:n}=e.state.selection.main,r=t.nextMatch(e.state,n,n);if(!r)return!1;let i=j.single(r.from,r.to),a=e.state.facet(wh);return e.dispatch({selection:i,effects:[cg(e,r),a.scrollToMatch(i.main,e)],userEvent:`select.search`}),eg(e),!0}),Kh=Wh((e,{query:t})=>{let{state:n}=e,{from:r}=n.selection.main,i=t.prevMatch(n,r,r);if(!i)return!1;let a=j.single(i.from,i.to),o=e.state.facet(wh);return e.dispatch({selection:a,effects:[cg(e,i),o.scrollToMatch(a.main,e)],userEvent:`select.search`}),eg(e),!0}),qh=Wh((e,{query:t})=>{let n=t.matchAll(e.state,1e3);return!n||!n.length?!1:(e.dispatch({selection:j.create(n.map(e=>j.range(e.from,e.to))),userEvent:`select.search.matches`}),!0)}),Jh=({state:e,dispatch:t})=>{let n=e.selection;if(n.ranges.length>1||n.main.empty)return!1;let{from:r,to:i}=n.main,a=[],o=0;for(let t=new nh(e.doc,e.sliceDoc(r,i));!t.next().done;){if(a.length>1e3)return!1;t.value.from==r&&(o=a.length),a.push(j.range(t.value.from,t.value.to))}return t(e.update({selection:j.create(a,o),userEvent:`select.search.matches`})),!0},Yh=Wh((e,{query:t})=>{let{state:n}=e,{from:r,to:i}=n.selection.main;if(n.readOnly)return!1;let a=t.nextMatch(n,r,r);if(!a)return!1;let o=a,s=[],c,l,u=[];o.precise?o.from==r&&o.to==i&&(l=n.toText(t.getReplacement(o)),s.push({from:o.from,to:o.to,insert:l}),o=t.nextMatch(n,o.from,o.to),u.push(K.announce.of(n.phrase(`replaced match on line $`,n.doc.lineAt(r).number)+`.`))):o=t.nextMatch(n,o.from,o.to);let d=e.state.changes(s);return o&&(c=j.single(o.from,o.to).map(d),u.push(cg(e,o)),u.push(n.facet(wh).scrollToMatch(c.main,e))),e.dispatch({changes:d,selection:c,effects:u,userEvent:`input.replace`}),!0}),Xh=Wh((e,{query:t})=>{if(e.state.readOnly)return!1;let n=[];for(let r of t.matchAll(e.state,1e9)){let{from:e,to:i,precise:a}=r;a&&n.push({from:e,to:i,insert:t.getReplacement(r)})}if(!n.length)return!1;let r=e.state.phrase(`replaced $ matches`,n.length)+`.`;return e.dispatch({changes:n,effects:K.announce.of(r),userEvent:`input.replace.all`}),!0});function Zh(e){return e.state.facet(wh).createPanel(e)}function Qh(e,t){let n=e.selection.main,r=n.empty||n.to>n.from+100?``:e.sliceDoc(n.from,n.to);if(t&&!r)return t;let i=e.facet(wh);return new Th({search:t?.literal??i.literal?r:r.replace(/\n/g,`\\n`),caseSensitive:t?.caseSensitive??i.caseSensitive,literal:t?.literal??i.literal,regexp:t?.regexp??i.regexp,wholeWord:t?.wholeWord??i.wholeWord})}function $h(e){let t=Hc(e,Zh);return t&&t.dom.querySelector(`[main-field]`)}function eg(e){let t=$h(e);t&&t==e.root.activeElement&&t.select()}var tg=e=>{let t=e.state.field(zh,!1);if(t&&t.panel){let n=$h(e);if(n&&n!=e.root.activeElement){let r=Qh(e.state,t.query.spec);r.valid&&e.dispatch({effects:Lh.of(r)}),n.focus(),n.select()}}else e.dispatch({effects:[Rh.of(!0),t?Lh.of(Qh(e.state,t.query.spec)):N.appendConfig.of(ug)]});return!0},ng=e=>{let t=e.state.field(zh,!1);if(!t||!t.panel)return!1;let n=Hc(e,Zh);return n&&n.dom.contains(e.root.activeElement)&&e.focus(),e.dispatch({effects:Rh.of(!1)}),!0},rg=[{key:`Mod-f`,run:tg,scope:`editor search-panel`},{key:`F3`,run:Gh,shift:Kh,scope:`editor search-panel`,preventDefault:!0},{key:`Mod-g`,run:Gh,shift:Kh,scope:`editor search-panel`,preventDefault:!0},{key:`Escape`,run:ng,scope:`editor search-panel`},{key:`Mod-Shift-l`,run:Jh},{key:`Mod-Alt-g`,run:dh},{key:`Mod-d`,run:Ch,preventDefault:!0}],ig=class{constructor(e){this.view=e;let t=this.query=e.state.field(zh).query.spec;this.commit=this.commit.bind(this),this.searchField=L(`input`,{value:t.search,placeholder:ag(e,`Find`),"aria-label":ag(e,`Find`),class:`cm-textfield`,name:`search`,form:``,"main-field":`true`,onchange:this.commit,onkeyup:this.commit}),this.replaceField=L(`input`,{value:t.replace,placeholder:ag(e,`Replace`),"aria-label":ag(e,`Replace`),class:`cm-textfield`,name:`replace`,form:``,onchange:this.commit,onkeyup:this.commit}),this.caseField=L(`input`,{type:`checkbox`,name:`case`,form:``,checked:t.caseSensitive,onchange:this.commit}),this.reField=L(`input`,{type:`checkbox`,name:`re`,form:``,checked:t.regexp,onchange:this.commit}),this.wordField=L(`input`,{type:`checkbox`,name:`word`,form:``,checked:t.wholeWord,onchange:this.commit});function n(e,t,n){return L(`button`,{class:`cm-button`,name:e,onclick:t,type:`button`},n)}this.dom=L(`div`,{onkeydown:e=>this.keydown(e),class:`cm-search`},[this.searchField,n(`next`,()=>Gh(e),[ag(e,`next`)]),n(`prev`,()=>Kh(e),[ag(e,`previous`)]),n(`select`,()=>qh(e),[ag(e,`all`)]),L(`label`,null,[this.caseField,ag(e,`match case`)]),L(`label`,null,[this.reField,ag(e,`regexp`)]),L(`label`,null,[this.wordField,ag(e,`by word`)]),...e.state.readOnly?[]:[L(`br`),this.replaceField,n(`replace`,()=>Yh(e),[ag(e,`replace`)]),n(`replaceAll`,()=>Xh(e),[ag(e,`replace all`)])],L(`button`,{name:`close`,onclick:()=>ng(e),"aria-label":ag(e,`close`),type:`button`},[`×`])])}commit(){let e=new Th({search:this.searchField.value,caseSensitive:this.caseField.checked,regexp:this.reField.checked,wholeWord:this.wordField.checked,replace:this.replaceField.value});e.eq(this.query)||(this.query=e,this.view.dispatch({effects:Lh.of(e)}))}keydown(e){_s(this.view,e,`search-panel`)?e.preventDefault():e.keyCode==13&&e.target==this.searchField?(e.preventDefault(),(e.shiftKey?Kh:Gh)(this.view)):e.keyCode==13&&e.target==this.replaceField&&(e.preventDefault(),Yh(this.view))}update(e){for(let t of e.transactions)for(let e of t.effects)e.is(Lh)&&!e.value.eq(this.query)&&this.setQuery(e.value)}setQuery(e){this.query=e,this.searchField.value=e.search,this.replaceField.value=e.replace,this.caseField.checked=e.caseSensitive,this.reField.checked=e.regexp,this.wordField.checked=e.wholeWord}mount(){this.searchField.select()}get pos(){return 80}get top(){return this.view.state.facet(wh).top}};function ag(e,t){return e.state.phrase(t)}var og=30,sg=/[\s\.,:;?!]/;function cg(e,{from:t,to:n}){let r=e.state.doc.lineAt(t),i=e.state.doc.lineAt(n).to,a=Math.max(r.from,t-og),o=Math.min(i,n+og),s=e.state.sliceDoc(a,o);if(a!=r.from){for(let e=0;e<og;e++)if(!sg.test(s[e+1])&&sg.test(s[e])){s=s.slice(e);break}}if(o!=i){for(let e=s.length-1;e>s.length-og;e--)if(!sg.test(s[e-1])&&sg.test(s[e])){s=s.slice(0,e);break}}return K.announce.of(`${e.state.phrase(`current match`)}. ${s} ${e.state.phrase(`on line`)} ${r.number}.`)}var lg=K.baseTheme({".cm-panel.cm-search":{padding:`2px 6px 4px`,position:`relative`,"& [name=close]":{position:`absolute`,top:`0`,right:`4px`,backgroundColor:`inherit`,border:`none`,font:`inherit`,padding:0,margin:0},"& input, & button, & label":{margin:`.2em .6em .2em 0`},"& input[type=checkbox]":{marginRight:`.2em`},"& label":{fontSize:`80%`,whiteSpace:`pre`}},"&light .cm-searchMatch":{backgroundColor:`#ffff0054`},"&dark .cm-searchMatch":{backgroundColor:`#00ffff8a`},"&light .cm-searchMatch-selected":{backgroundColor:`#ff6a0054`},"&dark .cm-searchMatch-selected":{backgroundColor:`#ff00ff8a`}}),ug=[zh,Ze.low(Uh),lg],dg=class{constructor(e,t,n,r){this.state=e,this.pos=t,this.explicit=n,this.view=r,this.abortListeners=[],this.abortOnDocChange=!1}tokenBefore(e){let t=Lu(this.state).resolveInner(this.pos,-1);for(;t&&e.indexOf(t.name)<0;)t=t.parent;return t?{from:t.from,to:this.pos,text:this.state.sliceDoc(t.from,this.pos),type:t.type}:null}matchBefore(e){let t=this.state.doc.lineAt(this.pos),n=Math.max(t.from,this.pos-250),r=t.text.slice(n-t.from,this.pos-t.from),i=r.search(_g(e,!1));return i<0?null:{from:n+i,to:this.pos,text:r.slice(i)}}get aborted(){return this.abortListeners==null}addEventListener(e,t,n){e==`abort`&&this.abortListeners&&(this.abortListeners.push(t),n&&n.onDocChange&&(this.abortOnDocChange=!0))}};function fg(e){let t=Object.keys(e).join(``),n=/\w/.test(t);return n&&(t=t.replace(/\w/g,``)),`[${n?`\\w`:``}${t.replace(/[^\w\s]/g,`\\$&`)}]`}function pg(e){let t=Object.create(null),n=Object.create(null);for(let{label:r}of e){t[r[0]]=!0;for(let e=1;e<r.length;e++)n[r[e]]=!0}let r=fg(t)+fg(n)+`*$`;return[RegExp(`^`+r),new RegExp(r)]}function mg(e){let t=e.map(e=>typeof e==`string`?{label:e}:e),[n,r]=t.every(e=>/^\w+$/.test(e.label))?[/\w*$/,/\w+$/]:pg(t);return e=>{let i=e.matchBefore(r);return i||e.explicit?{from:i?i.from:e.pos,options:t,validFor:n}:null}}var hg=class{constructor(e,t,n,r){this.completion=e,this.source=t,this.match=n,this.score=r}};function gg(e){return e.selection.main.from}function _g(e,t){let{source:n}=e,r=t&&n[0]!=`^`,i=n[n.length-1]!=`$`;return!r&&!i?e:RegExp(`${r?`^`:``}(?:${n})${i?`$`:``}`,e.flags??(e.ignoreCase?`i`:``))}var vg=ft.define();function yg(e,t,n,r){let{main:i}=e.selection,a=n-i.from,o=r-i.from;return{...e.changeByRange(s=>{if(s!=i&&n!=r&&e.sliceDoc(s.from+a,s.from+o)!=e.sliceDoc(n,r))return{range:s};let c=e.toText(t);return{changes:{from:s.from+a,to:r==i.from?s.to:s.from+o,insert:c},range:j.cursor(s.from+a+c.length)}}),scrollIntoView:!0,userEvent:`input.complete`}}var bg=new WeakMap;function xg(e){if(!Array.isArray(e))return e;let t=bg.get(e);return t||bg.set(e,t=mg(e)),t}var Sg=N.define(),Cg=N.define(),wg=class{constructor(e){this.pattern=e,this.chars=[],this.folded=[],this.any=[],this.precise=[],this.byWord=[],this.score=0,this.matched=[];for(let t=0;t<e.length;){let n=De(e,t),r=ke(n);this.chars.push(n);let i=e.slice(t,t+r),a=i.toUpperCase();this.folded.push(De(a==i?i.toLowerCase():a,0)),t+=r}this.astral=e.length!=this.chars.length}ret(e,t){return this.score=e,this.matched=t,this}match(e){if(this.pattern.length==0)return this.ret(-100,[]);if(e.length<this.pattern.length)return null;let{chars:t,folded:n,any:r,precise:i,byWord:a}=this;if(t.length==1){let r=De(e,0),i=ke(r),a=i==e.length?0:-100;if(r!=t[0]){if(r==n[0])a+=-200;else return null}return this.ret(a,[0,i])}let o=e.indexOf(this.pattern);if(o==0)return this.ret(e.length==this.pattern.length?0:-100,[0,this.pattern.length]);let s=t.length,c=0;if(o<0){for(let i=0,a=Math.min(e.length,200);i<a&&c<s;){let a=De(e,i);(a==t[c]||a==n[c])&&(r[c++]=i),i+=ke(a)}if(c<s)return null}let l=0,u=0,d=!1,f=0,p=-1,m=-1,h=/[a-z]/.test(e),g=!0;for(let r=0,c=Math.min(e.length,200),_=0;r<c&&u<s;){let c=De(e,r);o<0&&(l<s&&c==t[l]&&(i[l++]=r),f<s&&(c==t[f]||c==n[f]?(f==0&&(p=r),m=r+1,f++):f=0));let v,y=c<255?c>=48&&c<=57||c>=97&&c<=122?2:+(c>=65&&c<=90):(v=Oe(c))==v.toLowerCase()?v==v.toUpperCase()?0:2:1;(!r||y==1&&h||_==0&&y!=0)&&(t[u]==c||n[u]==c&&(d=!0)?a[u++]=r:a.length&&(g=!1)),_=y,r+=ke(c)}return u==s&&a[0]==0&&g?this.result(-100+(d?-200:0),a,e):f==s&&p==0?this.ret(-200-e.length+(m==e.length?0:-100),[0,m]):o>-1?this.ret(-700-e.length,[o,o+this.pattern.length]):f==s?this.ret(-900-e.length,[p,m]):u==s?this.result(-100+(d?-200:0)+-700+(g?0:-1100),a,e):t.length==2?null:this.result((r[0]?-700:0)+-200+-1100,r,e)}result(e,t,n){let r=[],i=0;for(let e of t){let t=e+(this.astral?ke(De(n,e)):1);i&&r[i-1]==e?r[i-1]=t:(r[i++]=e,r[i++]=t)}return this.ret(e-n.length,r)}},Tg=class{constructor(e){this.pattern=e,this.matched=[],this.score=0,this.folded=e.toLowerCase()}match(e){if(e.length<this.pattern.length)return null;let t=e.slice(0,this.pattern.length),n=t==this.pattern?0:t.toLowerCase()==this.folded?-200:null;return n==null?null:(this.matched=[0,t.length],this.score=n+(e.length==this.pattern.length?0:-100),this)}},Eg=M.define({combine(e){return Ot(e,{activateOnTyping:!0,activateOnCompletion:()=>!1,activateOnTypingDelay:100,selectOnOpen:!0,override:null,closeOnBlur:!0,maxRenderedOptions:100,defaultKeymap:!0,tooltipClass:()=>``,optionClass:()=>``,aboveCursor:!1,icons:!0,addToOptions:[],positionInfo:Og,filterStrict:!1,compareCompletions:(e,t)=>(e.sortText||e.label).localeCompare(t.sortText||t.label),interactionDelay:75,updateSyncTime:100},{defaultKeymap:(e,t)=>e&&t,closeOnBlur:(e,t)=>e&&t,icons:(e,t)=>e&&t,tooltipClass:(e,t)=>n=>Dg(e(n),t(n)),optionClass:(e,t)=>n=>Dg(e(n),t(n)),addToOptions:(e,t)=>e.concat(t),filterStrict:(e,t)=>e||t})}});function Dg(e,t){return e?t?e+` `+t:e:t}function Og(e,t,n,r,i,a){let o=e.textDirection==V.RTL,s=o,c=!1,l=`top`,u,d,f=t.left-i.left,p=i.right-t.right,m=r.right-r.left,h=r.bottom-r.top;if(s&&f<Math.min(m,p)?s=!1:!s&&p<Math.min(m,f)&&(s=!0),m<=(s?f:p))u=Math.max(i.top,Math.min(n.top,i.bottom-h))-t.top,d=Math.min(400,s?f:p);else{c=!0,d=Math.min(400,(o?t.right:i.right-t.left)-30);let e=i.bottom-t.bottom;e>=h||e>t.top?u=n.bottom-t.top:(l=`bottom`,u=t.bottom-n.top)}let g=(t.bottom-t.top)/a.offsetHeight,_=(t.right-t.left)/a.offsetWidth;return{style:`${l}: ${u/g}px; max-width: ${d/_}px`,class:`cm-completionInfo-`+(c?o?`left-narrow`:`right-narrow`:s?`left`:`right`)}}var kg=N.define();function Ag(e){let t=e.addToOptions.slice();return e.icons&&t.push({render(e){let t=document.createElement(`div`);return t.classList.add(`cm-completionIcon`),e.type&&t.classList.add(...e.type.split(/\s+/g).map(e=>`cm-completionIcon-`+e)),t.setAttribute(`aria-hidden`,`true`),t},position:20}),t.push({render(e,t,n,r){let i=document.createElement(`span`);i.className=`cm-completionLabel`;let a=e.displayLabel||e.label,o=0;for(let e=0;e<r.length;){let t=r[e++],n=r[e++];t>o&&i.appendChild(document.createTextNode(a.slice(o,t)));let s=i.appendChild(document.createElement(`span`));s.appendChild(document.createTextNode(a.slice(t,n))),s.className=`cm-completionMatchedText`,o=n}return o<a.length&&i.appendChild(document.createTextNode(a.slice(o))),i},position:50},{render(e){if(!e.detail)return null;let t=document.createElement(`span`);return t.className=`cm-completionDetail`,t.textContent=e.detail,t},position:80}),t.sort((e,t)=>e.position-t.position).map(e=>e.render)}function jg(e,t,n){if(e<=n)return{from:0,to:e};if(t<0&&(t=0),t<=e>>1){let e=Math.floor(t/n);return{from:e*n,to:(e+1)*n}}let r=Math.ceil((e-t)/n);return{from:e-r*n,to:e-(r-1)*n}}var Mg=class{constructor(e,t,n){this.view=e,this.stateField=t,this.applyCompletion=n,this.info=null,this.infoDestroy=null,this.placeInfoReq={read:()=>this.measureInfo(),write:e=>this.placeInfo(e),key:this},this.space=null,this.currentClass=``;let r=e.state.field(t),{options:i,selected:a}=r.open,o=e.state.facet(Eg);this.optionContent=Ag(o),this.optionClass=o.optionClass,this.tooltipClass=o.tooltipClass,this.range=jg(i.length,a,o.maxRenderedOptions),this.dom=document.createElement(`div`),this.dom.className=`cm-tooltip-autocomplete`,this.updateTooltipClass(e.state),this.dom.addEventListener(`mousedown`,n=>{let{options:r}=e.state.field(t).open;for(let t=n.target,i;t&&t!=this.dom;t=t.parentNode)if(t.nodeName==`LI`&&(i=/-(\d+)$/.exec(t.id))&&+i[1]<r.length){this.applyCompletion(e,r[+i[1]]),n.preventDefault();return}if(n.target==this.list){let t=this.list.classList.contains(`cm-completionListIncompleteTop`)&&n.clientY<this.list.firstChild.getBoundingClientRect().top?this.range.from-1:this.list.classList.contains(`cm-completionListIncompleteBottom`)&&n.clientY>this.list.lastChild.getBoundingClientRect().bottom?this.range.to:null;t!=null&&(e.dispatch({effects:kg.of(t)}),n.preventDefault())}}),this.dom.addEventListener(`focusout`,t=>{let n=e.state.field(this.stateField,!1);n&&n.tooltip&&e.state.facet(Eg).closeOnBlur&&t.relatedTarget!=e.contentDOM&&e.dispatch({effects:Cg.of(null)})}),this.showOptions(i,r.id)}mount(){this.updateSel()}showOptions(e,t){this.list&&this.list.remove(),this.list=this.dom.appendChild(this.createListBox(e,t,this.range)),this.list.addEventListener(`scroll`,()=>{this.info&&this.view.requestMeasure(this.placeInfoReq)})}update(e){let t=e.state.field(this.stateField),n=e.startState.field(this.stateField);if(this.updateTooltipClass(e.state),t!=n){let{options:r,selected:i,disabled:a}=t.open;n.open&&n.open.options==r||(this.range=jg(r.length,i,e.state.facet(Eg).maxRenderedOptions),this.showOptions(r,t.id)),this.updateSel(),a!=n.open?.disabled&&this.dom.classList.toggle(`cm-tooltip-autocomplete-disabled`,!!a)}}updateTooltipClass(e){let t=this.tooltipClass(e);if(t!=this.currentClass){for(let e of this.currentClass.split(` `))e&&this.dom.classList.remove(e);for(let e of t.split(` `))e&&this.dom.classList.add(e);this.currentClass=t}}positioned(e){this.space=e,this.info&&this.view.requestMeasure(this.placeInfoReq)}updateSel(){let e=this.view.state.field(this.stateField),t=e.open;(t.selected>-1&&t.selected<this.range.from||t.selected>=this.range.to)&&(this.range=jg(t.options.length,t.selected,this.view.state.facet(Eg).maxRenderedOptions),this.showOptions(t.options,e.id));let n=this.updateSelectedOption(t.selected);if(n){this.destroyInfo();let{completion:r}=t.options[t.selected],{info:i}=r;if(!i)return;let a=typeof i==`string`?document.createTextNode(i):i(r);if(!a)return;`then`in a?a.then(t=>{t&&this.view.state.field(this.stateField,!1)==e&&this.addInfoPane(t,r)}).catch(e=>Jr(this.view.state,e,`completion info`)):(this.addInfoPane(a,r),n.setAttribute(`aria-describedby`,this.info.id))}}addInfoPane(e,t){this.destroyInfo();let n=this.info=document.createElement(`div`);if(n.className=`cm-tooltip cm-completionInfo`,n.id=`cm-completionInfo-`+Math.floor(Math.random()*65535).toString(16),e.nodeType!=null)n.appendChild(e),this.infoDestroy=null;else{let{dom:t,destroy:r}=e;n.appendChild(t),this.infoDestroy=r||null}this.dom.appendChild(n),this.view.requestMeasure(this.placeInfoReq)}updateSelectedOption(e){let t=null;for(let n=this.list.firstChild,r=this.range.from;n;n=n.nextSibling,r++)n.nodeName!=`LI`||!n.id?r--:r==e?n.hasAttribute(`aria-selected`)||(n.setAttribute(`aria-selected`,`true`),t=n):n.hasAttribute(`aria-selected`)&&(n.removeAttribute(`aria-selected`),n.removeAttribute(`aria-describedby`));return t&&Pg(this.list,t),t}measureInfo(){let e=this.dom.querySelector(`[aria-selected]`);if(!e||!this.info)return null;let t=this.dom.getBoundingClientRect(),n=this.info.getBoundingClientRect(),r=e.getBoundingClientRect(),i=this.space;if(!i){let e=this.dom.ownerDocument.documentElement;i={left:0,top:0,right:e.clientWidth,bottom:e.clientHeight}}return r.top>Math.min(i.bottom,t.bottom)-10||r.bottom<Math.max(i.top,t.top)+10?null:this.view.state.facet(Eg).positionInfo(this.view,t,r,n,i,this.dom)}placeInfo(e){this.info&&(e?(e.style&&(this.info.style.cssText=e.style),this.info.className=`cm-tooltip cm-completionInfo `+(e.class||``)):this.info.style.cssText=`top: -1e6px`)}createListBox(e,t,n){let r=document.createElement(`ul`);r.id=t,r.setAttribute(`role`,`listbox`),r.setAttribute(`aria-expanded`,`true`),r.setAttribute(`aria-label`,this.view.state.phrase(`Completions`)),r.addEventListener(`mousedown`,e=>{e.target==r&&e.preventDefault()});let i=null;for(let a=n.from;a<n.to;a++){let{completion:o,match:s}=e[a],{section:c}=o;if(c){let e=typeof c==`string`?c:c.name;if(e!=i&&(a>n.from||n.from==0)){if(i=e,typeof c!=`string`&&c.header)r.appendChild(c.header(c));else{let t=r.appendChild(document.createElement(`completion-section`));t.textContent=e}}}let l=r.appendChild(document.createElement(`li`));l.id=t+`-`+a,l.setAttribute(`role`,`option`);let u=this.optionClass(o);u&&(l.className=u);for(let e of this.optionContent){let t=e(o,this.view.state,this.view,s);t&&l.appendChild(t)}}return n.from&&r.classList.add(`cm-completionListIncompleteTop`),n.to<e.length&&r.classList.add(`cm-completionListIncompleteBottom`),r}destroyInfo(){this.info&&=(this.infoDestroy&&this.infoDestroy(),this.info.remove(),null)}destroy(){this.destroyInfo()}};function Ng(e,t){return n=>new Mg(n,e,t)}function Pg(e,t){let n=e.getBoundingClientRect(),r=t.getBoundingClientRect(),i=n.height/e.offsetHeight;r.top<n.top?e.scrollTop-=(n.top-r.top)/i:r.bottom>n.bottom&&(e.scrollTop+=(r.bottom-n.bottom)/i)}function Fg(e){return(e.boost||0)*100+(e.apply?10:0)+(e.info?5:0)+ +!!e.type}function Ig(e,t){let n=[],r=null,i=null,a=e=>{n.push(e);let{section:t}=e.completion;if(t){r||=[];let e=typeof t==`string`?t:t.name;r.some(t=>t.name==e)||r.push(typeof t==`string`?{name:e}:t)}},o=t.facet(Eg);for(let r of e)if(r.hasResult()){let e=r.result.getMatch;if(r.result.filter===!1)for(let t of r.result.options)a(new hg(t,r.source,e?e(t):[],1e9-n.length));else{let n=t.sliceDoc(r.from,r.to),s,c=o.filterStrict?new Tg(n):new wg(n);for(let t of r.result.options)if(s=c.match(t.label)){let n=t.displayLabel?e?e(t,s.matched):[]:s.matched,o=s.score+(t.boost||0);if(a(new hg(t,r.source,n,o)),typeof t.section==`object`&&t.section.rank===`dynamic`){let{name:e}=t.section;i||=Object.create(null),i[e]=Math.max(o,i[e]||-1e9)}}}}if(r){let e=Object.create(null),t=0,a=(e,t)=>(e.rank===`dynamic`&&t.rank===`dynamic`?i[t.name]-i[e.name]:0)||(typeof e.rank==`number`?e.rank:1e9)-(typeof t.rank==`number`?t.rank:1e9)||(e.name<t.name?-1:1);for(let n of r.sort(a))t-=1e5,e[n.name]=t;for(let t of n){let{section:n}=t.completion;n&&(t.score+=e[typeof n==`string`?n:n.name])}}let s=[],c=null,l=o.compareCompletions;for(let e of n.sort((e,t)=>t.score-e.score||l(e.completion,t.completion))){let t=e.completion;!c||c.label!=t.label||c.detail!=t.detail||c.type!=null&&t.type!=null&&c.type!=t.type||c.apply!=t.apply||c.boost!=t.boost?s.push(e):Fg(e.completion)>Fg(c)&&(s[s.length-1]=e),c=e.completion}return s}var Lg=class e{constructor(e,t,n,r,i,a){this.options=e,this.attrs=t,this.tooltip=n,this.timestamp=r,this.selected=i,this.disabled=a}setSelected(t,n){return t==this.selected||t>=this.options.length?this:new e(this.options,Hg(n,t),this.tooltip,this.timestamp,t,this.disabled)}static build(t,n,r,i,a,o){if(i&&!o&&t.some(e=>e.isPending))return i.setDisabled();let s=Ig(t,n);if(!s.length)return i&&t.some(e=>e.isPending)?i.setDisabled():null;let c=n.facet(Eg).selectOnOpen?0:-1;if(i&&i.selected!=c&&i.selected!=-1){let e=i.options[i.selected].completion;for(let t=0;t<s.length;t++)if(s[t].completion==e){c=t;break}}return new e(s,Hg(r,c),{pos:t.reduce((e,t)=>t.hasResult()?Math.min(e,t.from):e,1e8),create:Zg,above:a.aboveCursor},i?i.timestamp:Date.now(),c,!1)}map(t){return new e(this.options,this.attrs,{...this.tooltip,pos:t.mapPos(this.tooltip.pos)},this.timestamp,this.selected,this.disabled)}setDisabled(){return new e(this.options,this.attrs,this.tooltip,this.timestamp,this.selected,!0)}},Rg=class e{constructor(e,t,n){this.active=e,this.id=t,this.open=n}static start(){return new e(Ug,`cm-ac-`+Math.floor(Math.random()*2e6).toString(36),null)}update(t){let{state:n}=t,r=n.facet(Eg),i=(r.override||n.languageDataAt(`autocomplete`,gg(n)).map(xg)).map(e=>(this.active.find(t=>t.source==e)||new Gg(e,+!!this.active.some(e=>e.state!=0))).update(t,r));i.length==this.active.length&&i.every((e,t)=>e==this.active[t])&&(i=this.active);let a=this.open,o=t.effects.some(e=>e.is(Jg));a&&t.docChanged&&(a=a.map(t.changes)),t.selection||i.some(e=>e.hasResult()&&t.changes.touchesRange(e.from,e.to))||!zg(i,this.active)||o?a=Lg.build(i,n,this.id,a,r,o):a&&a.disabled&&!i.some(e=>e.isPending)&&(a=null),!a&&i.every(e=>!e.isPending)&&i.some(e=>e.hasResult())&&(i=i.map(e=>e.hasResult()?new Gg(e.source,0):e));for(let e of t.effects)e.is(kg)&&(a&&=a.setSelected(e.value,this.id));return i==this.active&&a==this.open?this:new e(i,this.id,a)}get tooltip(){return this.open?this.open.tooltip:null}get attrs(){return this.open?this.open.attrs:this.active.length?Bg:Vg}};function zg(e,t){if(e==t)return!0;for(let n=0,r=0;;){for(;n<e.length&&!e[n].hasResult();)n++;for(;r<t.length&&!t[r].hasResult();)r++;let i=n==e.length,a=r==t.length;if(i||a)return i==a;if(e[n++].result!=t[r++].result)return!1}}var Bg={"aria-autocomplete":`list`},Vg={};function Hg(e,t){let n={"aria-autocomplete":`list`,"aria-haspopup":`listbox`,"aria-controls":e};return t>-1&&(n[`aria-activedescendant`]=e+`-`+t),n}var Ug=[];function Wg(e,t){if(e.isUserEvent(`input.complete`)){let n=e.annotation(vg);if(n&&t.activateOnCompletion(n))return 12}let n=e.isUserEvent(`input.type`);return n&&t.activateOnTyping?5:n?1:e.isUserEvent(`delete.backward`)?2:e.selection?8:e.docChanged?16:0}var Gg=class e{constructor(e,t,n=!1){this.source=e,this.state=t,this.explicit=n}hasResult(){return!1}get isPending(){return this.state==1}update(t,n){let r=Wg(t,n),i=this;(r&8||r&16&&this.touches(t))&&(i=new e(i.source,0)),r&4&&i.state==0&&(i=new e(this.source,1)),i=i.updateFor(t,r);for(let n of t.effects)if(n.is(Sg))i=new e(i.source,1,n.value);else if(n.is(Cg))i=new e(i.source,0);else if(n.is(Jg))for(let e of n.value)e.source==i.source&&(i=e);return i}updateFor(e,t){return this.map(e.changes)}map(e){return this}touches(e){return e.changes.touchesRange(gg(e.state))}},Kg=class e extends Gg{constructor(e,t,n,r,i,a){super(e,3,t),this.limit=n,this.result=r,this.from=i,this.to=a}hasResult(){return!0}updateFor(t,n){if(!(n&3))return this.map(t.changes);let r=this.result;r.map&&!t.changes.empty&&(r=r.map(r,t.changes));let i=t.changes.mapPos(this.from),a=t.changes.mapPos(this.to,1),o=gg(t.state);if(o>a||!r||n&2&&(gg(t.startState)==this.from||o<this.limit))return new Gg(this.source,n&4?1:0);let s=t.changes.mapPos(this.limit);return qg(r.validFor,t.state,i,a)?new e(this.source,this.explicit,s,r,i,a):r.update&&(r=r.update(r,i,a,new dg(t.state,o,!1)))?new e(this.source,this.explicit,s,r,r.from,r.to??gg(t.state)):new Gg(this.source,1,this.explicit)}map(t){if(t.empty)return this;let n=this.result.map?this.result.map(this.result,t):this.result;return n?new e(this.source,this.explicit,t.mapPos(this.limit),n,t.mapPos(this.from),t.mapPos(this.to,1)):new Gg(this.source,0)}touches(e){return e.changes.touchesRange(this.from,this.to)}};function qg(e,t,n,r){if(!e)return!1;let i=t.sliceDoc(n,r);return typeof e==`function`?e(i,n,r,t):_g(e,!0).test(i)}var Jg=N.define({map(e,t){return e.map(e=>e.map(t))}}),Yg=Je.define({create(){return Rg.start()},update(e,t){return e.update(t)},provide:e=>[Oc.from(e,e=>e.tooltip),K.contentAttributes.from(e,e=>e.attrs)]});function Xg(e,t){let n=t.completion.apply||t.completion.label,r=e.state.field(Yg).active.find(e=>e.source==t.source);return r instanceof Kg&&(typeof n==`string`?e.dispatch({...yg(e.state,n,r.from,r.to),annotations:vg.of(t.completion)}):n(e,t.completion,r.from,r.to),!0)}var Zg=Ng(Yg,Xg);function Qg(e,t=`option`){return n=>{let r=n.state.field(Yg,!1);if(!r||!r.open||r.open.disabled||Date.now()-r.open.timestamp<n.state.facet(Eg).interactionDelay)return!1;let i=1,a;t==`page`&&(a=zc(n,r.open.tooltip))&&(i=Math.max(2,Math.floor(a.dom.offsetHeight/a.dom.querySelector(`li`).offsetHeight)-1));let{length:o}=r.open.options,s=r.open.selected>-1?r.open.selected+i*(e?1:-1):e?0:o-1;return s<0?s=t==`page`?0:o-1:s>=o&&(s=t==`page`?o-1:0),n.dispatch({effects:kg.of(s)}),!0}}var $g=e=>{let t=e.state.field(Yg,!1);return e.state.readOnly||!t||!t.open||t.open.selected<0||t.open.disabled||Date.now()-t.open.timestamp<e.state.facet(Eg).interactionDelay?!1:Xg(e,t.open.options[t.open.selected])},e_=e=>e.state.field(Yg,!1)?(e.dispatch({effects:Sg.of(!0)}),!0):!1,t_=e=>{let t=e.state.field(Yg,!1);return!t||!t.active.some(e=>e.state!=0)?!1:(e.dispatch({effects:Cg.of(null)}),!0)},n_=class{constructor(e,t){this.active=e,this.context=t,this.time=Date.now(),this.updates=[],this.done=void 0}},r_=50,i_=1e3,a_=U.fromClass(class{constructor(e){this.view=e,this.debounceUpdate=-1,this.running=[],this.debounceAccept=-1,this.pendingStart=!1,this.composing=0;for(let t of e.state.field(Yg).active)t.isPending&&this.startQuery(t)}update(e){let t=e.state.field(Yg),n=e.state.facet(Eg);if(!e.selectionSet&&!e.docChanged&&e.startState.field(Yg)==t)return;let r=e.transactions.some(e=>{let t=Wg(e,n);return t&8||(e.selection||e.docChanged)&&!(t&3)});for(let t=0;t<this.running.length;t++){let n=this.running[t];if(r||n.context.abortOnDocChange&&e.docChanged||n.updates.length+e.transactions.length>r_&&Date.now()-n.time>i_){for(let e of n.context.abortListeners)try{e()}catch(e){Jr(this.view.state,e)}n.context.abortListeners=null,this.running.splice(t--,1)}else n.updates.push(...e.transactions)}this.debounceUpdate>-1&&clearTimeout(this.debounceUpdate),e.transactions.some(e=>e.effects.some(e=>e.is(Sg)))&&(this.pendingStart=!0);let i=this.pendingStart?50:n.activateOnTypingDelay;if(this.debounceUpdate=t.active.some(e=>e.isPending&&!this.running.some(t=>t.active.source==e.source))?setTimeout(()=>this.startUpdate(),i):-1,this.composing!=0)for(let t of e.transactions)t.isUserEvent(`input.type`)?this.composing=2:this.composing==2&&t.selection&&(this.composing=3)}startUpdate(){this.debounceUpdate=-1,this.pendingStart=!1;let{state:e}=this.view,t=e.field(Yg);for(let e of t.active)e.isPending&&!this.running.some(t=>t.active.source==e.source)&&this.startQuery(e);this.running.length&&t.open&&t.open.disabled&&(this.debounceAccept=setTimeout(()=>this.accept(),this.view.state.facet(Eg).updateSyncTime))}startQuery(e){let{state:t}=this.view,n=new dg(t,gg(t),e.explicit,this.view),r=new n_(e,n);this.running.push(r),Promise.resolve(e.source(n)).then(e=>{r.context.aborted||(r.done=e||null,this.scheduleAccept())},e=>{this.view.dispatch({effects:Cg.of(null)}),Jr(this.view.state,e)})}scheduleAccept(){this.running.every(e=>e.done!==void 0)?this.accept():this.debounceAccept<0&&(this.debounceAccept=setTimeout(()=>this.accept(),this.view.state.facet(Eg).updateSyncTime))}accept(){this.debounceAccept>-1&&clearTimeout(this.debounceAccept),this.debounceAccept=-1;let e=[],t=this.view.state.facet(Eg),n=this.view.state.field(Yg);for(let r=0;r<this.running.length;r++){let i=this.running[r];if(i.done===void 0)continue;if(this.running.splice(r--,1),i.done){let n=gg(i.updates.length?i.updates[0].startState:this.view.state),r=Math.min(n,i.done.from+ +!i.active.explicit),a=new Kg(i.active.source,i.active.explicit,r,i.done,i.done.from,i.done.to??n);for(let e of i.updates)a=a.update(e,t);if(a.hasResult()){e.push(a);continue}}let a=n.active.find(e=>e.source==i.active.source);if(a&&a.isPending){if(i.done==null){let n=new Gg(i.active.source,0);for(let e of i.updates)n=n.update(e,t);n.isPending||e.push(n)}else this.startQuery(a)}}(e.length||n.open&&n.open.disabled)&&this.view.dispatch({effects:Jg.of(e)})}},{eventHandlers:{blur(e){let t=this.view.state.field(Yg,!1);if(t&&t.tooltip&&this.view.state.facet(Eg).closeOnBlur){let n=t.open&&zc(this.view,t.open.tooltip);n&&n.dom.contains(e.relatedTarget)||setTimeout(()=>this.view.dispatch({effects:Cg.of(null)}),10)}},compositionstart(){this.composing=1},compositionend(){this.composing==3&&setTimeout(()=>this.view.dispatch({effects:Sg.of(!1)}),20),this.composing=0}}}),o_=typeof navigator==`object`&&/Win/.test(navigator.platform),s_=Ze.highest(K.domEventHandlers({keydown(e,t){let n=t.state.field(Yg,!1);if(!n||!n.open||n.open.disabled||n.open.selected<0||e.key.length>1||e.ctrlKey&&!(o_&&e.altKey)||e.metaKey)return!1;let r=n.open.options[n.open.selected],i=n.active.find(e=>e.source==r.source),a=r.completion.commitCharacters||i.result.commitCharacters;return a&&a.indexOf(e.key)>-1&&Xg(t,r),!1}})),c_=K.baseTheme({".cm-tooltip.cm-tooltip-autocomplete":{"& > ul":{fontFamily:`monospace`,whiteSpace:`nowrap`,overflow:`hidden auto`,maxWidth_fallback:`700px`,maxWidth:`min(700px, 95vw)`,minWidth:`250px`,maxHeight:`10em`,height:`100%`,listStyle:`none`,margin:0,padding:0,"& > li, & > completion-section":{padding:`1px 3px`,lineHeight:1.2},"& > li":{overflowX:`hidden`,textOverflow:`ellipsis`,cursor:`pointer`},"& > completion-section":{display:`list-item`,borderBottom:`1px solid silver`,paddingLeft:`0.5em`,opacity:.7}}},"&light .cm-tooltip-autocomplete ul li[aria-selected]":{background:`#17c`,color:`white`},"&light .cm-tooltip-autocomplete-disabled ul li[aria-selected]":{background:`#777`},"&dark .cm-tooltip-autocomplete ul li[aria-selected]":{background:`#347`,color:`white`},"&dark .cm-tooltip-autocomplete-disabled ul li[aria-selected]":{background:`#444`},".cm-completionListIncompleteTop:before, .cm-completionListIncompleteBottom:after":{content:`"···"`,opacity:.5,display:`block`,textAlign:`center`,cursor:`pointer`},".cm-tooltip.cm-completionInfo":{position:`absolute`,padding:`3px 9px`,width:`max-content`,maxWidth:`400px`,boxSizing:`border-box`,whiteSpace:`pre-line`},".cm-completionInfo.cm-completionInfo-left":{right:`100%`},".cm-completionInfo.cm-completionInfo-right":{left:`100%`},".cm-completionInfo.cm-completionInfo-left-narrow":{right:`30px`},".cm-completionInfo.cm-completionInfo-right-narrow":{left:`30px`},"&light .cm-snippetField":{backgroundColor:`#00000022`},"&dark .cm-snippetField":{backgroundColor:`#ffffff22`},".cm-snippetFieldPosition":{verticalAlign:`text-top`,width:0,height:`1.15em`,display:`inline-block`,margin:`0 -0.7px -.7em`,borderLeft:`1.4px dotted #888`},".cm-completionMatchedText":{textDecoration:`underline`},".cm-completionDetail":{marginLeft:`0.5em`,fontStyle:`italic`},".cm-completionIcon":{fontSize:`90%`,width:`.8em`,display:`inline-block`,textAlign:`center`,paddingRight:`.6em`,opacity:`0.6`,boxSizing:`content-box`},".cm-completionIcon-function, .cm-completionIcon-method":{"&:after":{content:`'ƒ'`}},".cm-completionIcon-class":{"&:after":{content:`'○'`}},".cm-completionIcon-interface":{"&:after":{content:`'◌'`}},".cm-completionIcon-variable":{"&:after":{content:`'𝑥'`}},".cm-completionIcon-constant":{"&:after":{content:`'𝐶'`}},".cm-completionIcon-type":{"&:after":{content:`'𝑡'`}},".cm-completionIcon-enum":{"&:after":{content:`'∪'`}},".cm-completionIcon-property":{"&:after":{content:`'□'`}},".cm-completionIcon-keyword":{"&:after":{content:`'🔑︎'`}},".cm-completionIcon-namespace":{"&:after":{content:`'▢'`}},".cm-completionIcon-text":{"&:after":{content:`'abc'`,fontSize:`50%`,verticalAlign:`middle`}}}),l_={brackets:[`(`,`[`,`{`,`'`,`"`],before:`)]}:;>`,stringPrefixes:[]},u_=N.define({map(e,t){return t.mapPos(e,-1,je.TrackAfter)??void 0}}),d_=new class extends kt{};d_.startSide=1,d_.endSide=-1;var f_=Je.define({create(){return I.empty},update(e,t){if(e=e.map(t.changes),t.selection){let n=t.state.doc.lineAt(t.selection.main.head);e=e.update({filter:e=>e>=n.from&&e<=n.to})}for(let n of t.effects)n.is(u_)&&(e=e.update({add:[d_.range(n.value,n.value+1)]}));return e}});function p_(){return[v_,f_]}var m_=`()[]{}<>«»»«［］｛｝`;function h_(e){for(let t=0;t<16;t+=2)if(m_.charCodeAt(t)==e)return m_.charAt(t+1);return Oe(e<128?e:e+1)}function g_(e,t){return e.languageDataAt(`closeBrackets`,t)[0]||l_}var __=typeof navigator==`object`&&/Android\b/.test(navigator.userAgent),v_=K.inputHandler.of((e,t,n,r)=>{if((__?e.composing:e.compositionStarted)||e.state.readOnly)return!1;let i=e.state.selection.main;if(r.length>2||r.length==2&&ke(De(r,0))==1||t!=i.from||n!=i.to)return!1;let a=b_(e.state,r);return a?(e.dispatch(a),!0):!1}),y_=[{key:`Backspace`,run:({state:e,dispatch:t})=>{if(e.readOnly)return!1;let n=g_(e,e.selection.main.head).brackets||l_.brackets,r=null,i=e.changeByRange(t=>{if(t.empty){let r=C_(e.doc,t.head);for(let i of n)if(i==r&&S_(e.doc,t.head)==h_(De(i,0)))return{changes:{from:t.head-i.length,to:t.head+i.length},range:j.cursor(t.head-i.length)}}return{range:r=t}});return r||t(e.update(i,{scrollIntoView:!0,userEvent:`delete.backward`})),!r}}];function b_(e,t){let n=g_(e,e.selection.main.head),r=n.brackets||l_.brackets;for(let i of r){let a=h_(De(i,0));if(t==i)return a==i?E_(e,i,r.indexOf(i+i+i)>-1,n):w_(e,i,a,n.before||l_.before);if(t==a&&x_(e,e.selection.main.from))return T_(e,i,a)}return null}function x_(e,t){let n=!1;return e.field(f_).between(0,e.doc.length,e=>{e==t&&(n=!0)}),n}function S_(e,t){let n=e.sliceString(t,t+2);return n.slice(0,ke(De(n,0)))}function C_(e,t){let n=e.sliceString(t-2,t);return ke(De(n,0))==n.length?n:n.slice(1)}function w_(e,t,n,r){let i=null,a=e.changeByRange(a=>{if(!a.empty)return{changes:[{insert:t,from:a.from},{insert:n,from:a.to}],effects:u_.of(a.to+t.length),range:j.range(a.anchor+t.length,a.head+t.length)};let o=S_(e.doc,a.head);return!o||/\s/.test(o)||r.indexOf(o)>-1?{changes:{insert:t+n,from:a.head},effects:u_.of(a.head+t.length),range:j.cursor(a.head+t.length)}:{range:i=a}});return i?null:e.update(a,{scrollIntoView:!0,userEvent:`input.type`})}function T_(e,t,n){let r=null,i=e.changeByRange(t=>t.empty&&S_(e.doc,t.head)==n?{changes:{from:t.head,to:t.head+n.length,insert:n},range:j.cursor(t.head+n.length)}:r={range:t});return r?null:e.update(i,{scrollIntoView:!0,userEvent:`input.type`})}function E_(e,t,n,r){let i=r.stringPrefixes||l_.stringPrefixes,a=null,o=e.changeByRange(r=>{if(!r.empty)return{changes:[{insert:t,from:r.from},{insert:t,from:r.to}],effects:u_.of(r.to+t.length),range:j.range(r.anchor+t.length,r.head+t.length)};let o=r.head,s=S_(e.doc,o),c;if(s==t){if(D_(e,o))return{changes:{insert:t+t,from:o},effects:u_.of(o+t.length),range:j.cursor(o+t.length)};if(x_(e,o)){let r=n&&e.sliceDoc(o,o+t.length*3)==t+t+t?t+t+t:t;return{changes:{from:o,to:o+r.length,insert:r},range:j.cursor(o+r.length)}}}else if(n&&e.sliceDoc(o-2*t.length,o)==t+t&&(c=k_(e,o-2*t.length,i))>-1&&D_(e,c))return{changes:{insert:t+t+t+t,from:o},effects:u_.of(o+t.length),range:j.cursor(o+t.length)};else if(e.charCategorizer(o)(s)!=P.Word&&k_(e,o,i)>-1&&!O_(e,o,t,i))return{changes:{insert:t+t,from:o},effects:u_.of(o+t.length),range:j.cursor(o+t.length)};return{range:a=r}});return a?null:e.update(o,{scrollIntoView:!0,userEvent:`input.type`})}function D_(e,t){let n=Lu(e).resolveInner(t+1);return n.parent&&n.from==t}function O_(e,t,n,r){let i=Lu(e).resolveInner(t,-1),a=r.reduce((e,t)=>Math.max(e,t.length),0);for(let o=0;o<5;o++){let o=e.sliceDoc(i.from,Math.min(i.to,i.from+n.length+a)),s=o.indexOf(n);if(!s||s>-1&&r.indexOf(o.slice(0,s))>-1){let t=i.firstChild;for(;t&&t.from==i.from&&t.to-t.from>n.length+s;){if(e.sliceDoc(t.to-n.length,t.to)==n)return!1;t=t.firstChild}return!0}let c=i.to==t&&i.parent;if(!c)break;i=c}return!1}function k_(e,t,n){let r=e.charCategorizer(t);if(r(e.sliceDoc(t-1,t))!=P.Word)return t;for(let i of n){let n=t-i.length;if(e.sliceDoc(n,t)==i&&r(e.sliceDoc(n-1,n))!=P.Word)return n}return-1}function A_(e={}){return[s_,Yg,Eg.of(e),a_,M_,c_]}var j_=[{key:`Ctrl-Space`,run:e_},{mac:"Alt-`",run:e_},{mac:`Alt-i`,run:e_},{key:`Escape`,run:t_},{key:`ArrowDown`,run:Qg(!0)},{key:`ArrowUp`,run:Qg(!1)},{key:`PageDown`,run:Qg(!0,`page`)},{key:`PageUp`,run:Qg(!1,`page`)},{key:`Enter`,run:$g}],M_=Ze.highest(ms.computeN([Eg],e=>e.facet(Eg).defaultKeymap?[j_]:[])),N_=class{constructor(e,t,n){this.from=e,this.to=t,this.diagnostic=n}},P_=class e{constructor(e,t,n){this.diagnostics=e,this.panel=t,this.selected=n}static init(t,n,r){let i=r.facet(Z_).markerFilter;i&&(t=i(t,r));let a=t.slice().sort((e,t)=>e.from-t.from||e.to-t.to),o=new It,s=[],c=0,l=r.doc.iter(),u=0,d=r.doc.length;for(let e=0;;){let t=e==a.length?null:a[e];if(!t&&!s.length)break;let n,r;if(s.length)n=c,r=s.reduce((e,t)=>Math.min(e,t.to),t&&t.from>n?t.from:1e8);else{if(n=t.from,n>d)break;r=t.to,s.push(t),e++}for(;e<a.length;){let t=a[e];if(t.from==n&&(t.to>t.from||t.to==n))s.push(t),e++,r=Math.min(t.to,r);else{r=Math.min(t.from,r);break}}r=Math.min(r,d);let i=!1;if(s.some(e=>e.from==n&&(e.to==r||r==d))&&(i=n==r,!i&&r-n<10)){let e=n-(u+l.value.length);e>0&&(l.next(e),u=n);for(let e=n;;){if(e>=r){i=!0;break}if(!l.lineBreak&&u+l.value.length>e)break;e=u+l.value.length,u+=l.value.length,l.next()}}let f=lv(s);if(i)o.add(n,n,z.widget({widget:new nv(f),diagnostics:s.slice()}));else{let e=s.reduce((e,t)=>t.markClass?e+` `+t.markClass:e,``);o.add(n,r,z.mark({class:`cm-lintRange cm-lintRange-`+f+e,diagnostics:s.slice(),inclusiveEnd:s.some(e=>e.to>r)}))}if(c=r,c==d)break;for(let e=0;e<s.length;e++)s[e].to<=c&&s.splice(e--,1)}let f=o.finish();return new e(f,n,F_(f))}};function F_(e,t=null,n=0){let r=null;return e.between(n,1e9,(e,n,{spec:i})=>{if(!(t&&i.diagnostics.indexOf(t)<0)){if(!r)r=new N_(e,n,t||i.diagnostics[0]);else if(i.diagnostics.indexOf(r.diagnostic)<0)return!1;else r=new N_(r.from,n,r.diagnostic)}}),r}function I_(e,t){let n=t.pos,r=t.end||n,i=e.state.facet(Z_).hideOn(e,n,r);if(i!=null)return i;let a=e.startState.doc.lineAt(t.pos);return!!(e.effects.some(e=>e.is(z_))||e.changes.touchesRange(a.from,Math.max(a.to,r)))}function L_(e,t){return e.field(H_,!1)?t:t.concat(N.appendConfig.of(bv))}function R_(e,t){return{effects:L_(e,[z_.of(t)])}}var z_=N.define(),B_=N.define(),V_=N.define(),H_=Je.define({create(){return new P_(z.none,null,null)},update(e,t){if(t.docChanged&&e.diagnostics.size){let n=e.diagnostics.map(t.changes),r=null,i=e.panel;if(e.selected){let i=t.changes.mapPos(e.selected.from,1);r=F_(n,e.selected.diagnostic,i)||F_(n,null,i)}!n.size&&i&&t.state.facet(Z_).autoPanel&&(i=null),e=new P_(n,i,r)}for(let n of t.effects)if(n.is(z_)){let r=t.state.facet(Z_).autoPanel?n.value.length?iv.open:null:e.panel;e=P_.init(n.value,r,t.state)}else n.is(B_)?e=new P_(e.diagnostics,n.value?iv.open:null,e.selected):n.is(V_)&&(e=new P_(e.diagnostics,e.panel,n.value));return e},provide:e=>[Kc.from(e,e=>e.panel),K.decorations.from(e,e=>e.diagnostics)]}),U_=z.mark({class:`cm-lintRange cm-lintRange-active`});function W_(e,t,n){let{diagnostics:r}=e.state.field(H_),i,a=-1,o=-1;r.between(t-+(n<0),t+ +(n>0),(e,r,{spec:s})=>{if(t>=e&&t<=r&&(e==r||(t>e||n>0)&&(t<r||n<0)))return i=s.diagnostics,a=e,o=r,!1});let s=e.state.facet(Z_).tooltipFilter;return i&&s&&(i=s(i,e.state)),i?{pos:a,end:o,above:!0,create(){return{dom:G_(e,i)}}}:null}function G_(e,t){return L(`ul`,{class:`cm-tooltip-lint`},t.map(t=>tv(e,t,!1)))}var K_=e=>{let t=e.state.field(H_,!1);t&&t.panel||e.dispatch({effects:L_(e.state,[B_.of(!0)])});let n=Hc(e,iv.open);return n&&n.dom.querySelector(`.cm-panel-lint ul`).focus(),!0},q_=e=>{let t=e.state.field(H_,!1);return!t||!t.panel?!1:(e.dispatch({effects:B_.of(!1)}),!0)},J_=[{key:`Mod-Shift-m`,run:K_,preventDefault:!0},{key:`F8`,run:e=>{let t=e.state.field(H_,!1);if(!t)return!1;let n=e.state.selection.main,r=F_(t.diagnostics,null,n.to+1);return!r&&(r=F_(t.diagnostics,null,0),!r||r.from==n.from&&r.to==n.to)?!1:(e.dispatch({selection:{anchor:r.from,head:r.to},scrollIntoView:!0}),Rc(e,r.from,1,{tooltip:yv,until:e=>e.docChanged||e.newSelection.main.head<r.from||e.newSelection.main.head>r.to}),!0)}}],Y_=U.fromClass(class{constructor(e){this.view=e,this.timeout=-1,this.set=!0;let{delay:t}=e.state.facet(Z_);this.lintTime=Date.now()+t,this.run=this.run.bind(this),this.timeout=setTimeout(this.run,t)}run(){clearTimeout(this.timeout);let e=Date.now();if(e<this.lintTime-10)this.timeout=setTimeout(this.run,this.lintTime-e);else{this.set=!1;let{state:e}=this.view,{sources:t}=e.facet(Z_);t.length&&X_(t.map(e=>Promise.resolve(e(this.view))),t=>{this.view.state.doc==e.doc&&this.view.dispatch(R_(this.view.state,t.reduce((e,t)=>e.concat(t))))},e=>{Jr(this.view.state,e)})}}update(e){let t=e.state.facet(Z_);(e.docChanged||t!=e.startState.facet(Z_)||t.needsRefresh&&t.needsRefresh(e))&&(this.lintTime=Date.now()+t.delay,this.set||(this.set=!0,this.timeout=setTimeout(this.run,t.delay)))}force(){this.set&&(this.lintTime=Date.now(),this.run())}destroy(){clearTimeout(this.timeout)}});function X_(e,t,n){let r=[],i=-1;for(let a of e)a.then(n=>{r.push(n),clearTimeout(i),r.length==e.length?t(r):i=setTimeout(()=>t(r),200)},n)}var Z_=M.define({combine(e){return{sources:e.map(e=>e.source).filter(e=>e!=null),...Ot(e.map(e=>e.config),{delay:750,markerFilter:null,tooltipFilter:null,needsRefresh:null,hideOn:()=>null},{delay:Math.max,markerFilter:Q_,tooltipFilter:Q_,needsRefresh:(e,t)=>e?t?n=>e(n)||t(n):e:t,hideOn:(e,t)=>e?t?(n,r,i)=>e(n,r,i)||t(n,r,i):e:t,autoPanel:(e,t)=>e||t})}}});function Q_(e,t){return e?t?(n,r)=>t(e(n,r),r):e:t}function $_(e,t={}){return[Z_.of({source:e,config:t}),Y_,bv]}function ev(e){let t=[];if(e)actions:for(let{name:n}of e){for(let e=0;e<n.length;e++){let r=n[e];if(/[a-zA-Z]/.test(r)&&!t.some(e=>e.toLowerCase()==r.toLowerCase())){t.push(r);continue actions}}t.push(``)}return t}function tv(e,t,n){let r=n?ev(t.actions):[];return L(`li`,{class:`cm-diagnostic cm-diagnostic-`+t.severity},L(`span`,{class:`cm-diagnosticText`},t.renderMessage?t.renderMessage(e):t.message),t.actions?.map((n,i)=>{let a=!1,o=r=>{if(r.preventDefault(),a)return;a=!0;let i=F_(e.state.field(H_).diagnostics,t);i&&n.apply(e,i.from,i.to)},{name:s}=n,c=r[i]?s.indexOf(r[i]):-1,l=c<0?s:[s.slice(0,c),L(`u`,s.slice(c,c+1)),s.slice(c+1)];return L(`button`,{type:`button`,class:`cm-diagnosticAction`+(n.markClass?` `+n.markClass:``),onclick:o,onmousedown:o,"aria-label":` Action: ${s}${c<0?``:` (access key "${r[i]})"`}.`},l)}),t.source&&L(`div`,{class:`cm-diagnosticSource`},t.source))}var nv=class extends On{constructor(e){super(),this.sev=e}eq(e){return e.sev==this.sev}toDOM(){return L(`span`,{class:`cm-lintPoint cm-lintPoint-`+this.sev})}},rv=class{constructor(e,t){this.diagnostic=t,this.id=`item_`+Math.floor(Math.random()*4294967295).toString(16),this.dom=tv(e,t,!0),this.dom.id=this.id,this.dom.setAttribute(`role`,`option`)}},iv=class e{constructor(e){this.view=e,this.items=[];let t=t=>{if(!(t.ctrlKey||t.altKey||t.metaKey)){if(t.keyCode==27)q_(this.view),this.view.focus();else if(t.keyCode==38||t.keyCode==33)this.moveSelection((this.selectedIndex-1+this.items.length)%this.items.length);else if(t.keyCode==40||t.keyCode==34)this.moveSelection((this.selectedIndex+1)%this.items.length);else if(t.keyCode==36)this.moveSelection(0);else if(t.keyCode==35)this.moveSelection(this.items.length-1);else if(t.keyCode==13)this.view.focus();else if(t.keyCode>=65&&t.keyCode<=90&&this.selectedIndex>=0){let{diagnostic:n}=this.items[this.selectedIndex],r=ev(n.actions);for(let i=0;i<r.length;i++)if(r[i].toUpperCase().charCodeAt(0)==t.keyCode){let t=F_(this.view.state.field(H_).diagnostics,n);t&&n.actions[i].apply(e,t.from,t.to)}}else return;t.preventDefault()}},n=e=>{for(let t=0;t<this.items.length;t++)this.items[t].dom.contains(e.target)&&this.moveSelection(t)};this.list=L(`ul`,{tabIndex:0,role:`listbox`,"aria-label":this.view.state.phrase(`Diagnostics`),onkeydown:t,onclick:n}),this.dom=L(`div`,{class:`cm-panel-lint`},this.list,L(`button`,{type:`button`,name:`close`,"aria-label":this.view.state.phrase(`close`),onclick:()=>q_(this.view)},`×`)),this.update()}get selectedIndex(){let e=this.view.state.field(H_).selected;if(!e)return-1;for(let t=0;t<this.items.length;t++)if(this.items[t].diagnostic==e.diagnostic)return t;return-1}update(){let{diagnostics:e,selected:t}=this.view.state.field(H_),n=0,r=!1,i=null,a=new Set;for(e.between(0,this.view.state.doc.length,(e,o,{spec:s})=>{for(let e of s.diagnostics){if(a.has(e))continue;a.add(e);let o=-1,s;for(let t=n;t<this.items.length;t++)if(this.items[t].diagnostic==e){o=t;break}o<0?(s=new rv(this.view,e),this.items.splice(n,0,s),r=!0):(s=this.items[o],o>n&&(this.items.splice(n,o-n),r=!0)),t&&s.diagnostic==t.diagnostic?s.dom.hasAttribute(`aria-selected`)||(s.dom.setAttribute(`aria-selected`,`true`),i=s):s.dom.hasAttribute(`aria-selected`)&&s.dom.removeAttribute(`aria-selected`),n++}});n<this.items.length&&!(this.items.length==1&&this.items[0].diagnostic.from<0);)r=!0,this.items.pop();this.items.length==0&&(this.items.push(new rv(this.view,{from:-1,to:-1,severity:`info`,message:this.view.state.phrase(`No diagnostics`)})),r=!0),i?(this.list.setAttribute(`aria-activedescendant`,i.id),this.view.requestMeasure({key:this,read:()=>({sel:i.dom.getBoundingClientRect(),panel:this.list.getBoundingClientRect()}),write:({sel:e,panel:t})=>{let n=t.height/this.list.offsetHeight;e.top<t.top?this.list.scrollTop-=(t.top-e.top)/n:e.bottom>t.bottom&&(this.list.scrollTop+=(e.bottom-t.bottom)/n)}})):this.selectedIndex<0&&this.list.removeAttribute(`aria-activedescendant`),r&&this.sync()}sync(){let e=this.list.firstChild;function t(){let t=e;e=t.nextSibling,t.remove()}for(let n of this.items)if(n.dom.parentNode==this.list){for(;e!=n.dom;)t();e=n.dom.nextSibling}else this.list.insertBefore(n.dom,e);for(;e;)t()}moveSelection(e){if(this.selectedIndex<0)return;let t=F_(this.view.state.field(H_).diagnostics,this.items[e].diagnostic);t&&this.view.dispatch({selection:{anchor:t.from,head:t.to},scrollIntoView:!0,effects:V_.of(t)})}static open(t){return new e(t)}};function av(e,t=`viewBox="0 0 40 40"`){return`url('data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" ${t}>${encodeURIComponent(e)}</svg>')`}function ov(e){return av(`<path d="m0 2.5 l2 -1.5 l1 0 l2 1.5 l1 0" stroke="${e}" fill="none" stroke-width=".7"/>`,`width="6" height="3"`)}var sv=K.baseTheme({".cm-diagnostic":{padding:`3px 6px 3px 8px`,marginLeft:`-1px`,display:`block`,whiteSpace:`pre-wrap`},".cm-diagnostic-error":{borderLeft:`5px solid #d11`},".cm-diagnostic-warning":{borderLeft:`5px solid orange`},".cm-diagnostic-info":{borderLeft:`5px solid #999`},".cm-diagnostic-hint":{borderLeft:`5px solid #66d`},".cm-diagnosticAction":{font:`inherit`,border:`none`,padding:`2px 4px`,backgroundColor:`#444`,color:`white`,borderRadius:`3px`,marginLeft:`8px`,cursor:`pointer`},".cm-diagnosticSource":{fontSize:`70%`,opacity:.7},".cm-lintRange":{backgroundPosition:`left bottom`,backgroundRepeat:`repeat-x`,paddingBottom:`0.7px`},".cm-lintRange-error":{backgroundImage:ov(`#f11`)},".cm-lintRange-warning":{backgroundImage:ov(`orange`)},".cm-lintRange-info":{backgroundImage:ov(`#999`)},".cm-lintRange-hint":{backgroundImage:ov(`#66d`)},".cm-lintRange-active":{backgroundColor:`#ffdd9980`},".cm-tooltip-lint":{padding:0,margin:0},".cm-lintPoint":{position:`relative`,"&:after":{content:`""`,position:`absolute`,bottom:0,left:`-2px`,borderLeft:`3px solid transparent`,borderRight:`3px solid transparent`,borderBottom:`4px solid #d11`}},".cm-lintPoint-warning":{"&:after":{borderBottomColor:`orange`}},".cm-lintPoint-info":{"&:after":{borderBottomColor:`#999`}},".cm-lintPoint-hint":{"&:after":{borderBottomColor:`#66d`}},".cm-panel.cm-panel-lint":{position:`relative`,"& ul":{maxHeight:`100px`,overflowY:`auto`,"& [aria-selected]":{backgroundColor:`#ddd`,"& u":{textDecoration:`underline`}},"&:focus [aria-selected]":{background_fallback:`#bdf`,backgroundColor:`Highlight`,color_fallback:`white`,color:`HighlightText`},"& u":{textDecoration:`none`},padding:0,margin:0},"& [name=close]":{position:`absolute`,top:`0`,right:`2px`,background:`inherit`,border:`none`,font:`inherit`,padding:0,margin:0}},"&dark .cm-lintRange-active":{backgroundColor:`#86714a80`},"&dark .cm-panel.cm-panel-lint ul":{"& [aria-selected]":{backgroundColor:`#2e343e`}}});function cv(e){return e==`error`?4:e==`warning`?3:e==`info`?2:1}function lv(e){let t=`hint`,n=1;for(let r of e){let e=cv(r.severity);e>n&&(n=e,t=r.severity)}return t}var uv=class extends $c{constructor(e){super(),this.diagnostics=e,this.severity=lv(e)}toDOM(e){let t=document.createElement(`div`);t.className=`cm-lint-marker cm-lint-marker-`+this.severity;let n=this.diagnostics,r=e.state.facet(xv).tooltipFilter;return r&&(n=r(n,e.state)),n.length&&(t.onmouseover=()=>fv(e,t,n)),t}};function dv(e,t){let n=r=>{let i=t.getBoundingClientRect();if(!(r.clientX>i.left-10&&r.clientX<i.right+10&&r.clientY>i.top-10&&r.clientY<i.bottom+10)){for(let e=r.target;e;e=e.parentNode)if(e.nodeType==1&&e.classList.contains(`cm-tooltip-lint`))return;window.removeEventListener(`mousemove`,n),e.state.field(_v)&&e.dispatch({effects:gv.of(null)})}};window.addEventListener(`mousemove`,n)}function fv(e,t,n){function r(){let r=e.elementAtHeight(t.getBoundingClientRect().top+5-e.documentTop);e.coordsAtPos(r.from)&&e.dispatch({effects:gv.of({pos:r.from,above:!1,clip:!1,create(){return{dom:G_(e,n),getCoords:()=>t.getBoundingClientRect()}}})}),t.onmouseout=t.onmousemove=null,dv(e,t)}let{hoverTime:i}=e.state.facet(xv),a=setTimeout(r,i);t.onmouseout=()=>{clearTimeout(a),t.onmouseout=t.onmousemove=null},t.onmousemove=()=>{clearTimeout(a),a=setTimeout(r,i)}}function pv(e,t){let n=Object.create(null);for(let r of t){let t=e.lineAt(r.from);(n[t.from]||(n[t.from]=[])).push(r)}let r=[];for(let e in n)r.push(new uv(n[e]).range(+e));return I.of(r,!0)}var mv=il({class:`cm-gutter-lint`,markers:e=>e.state.field(hv),widgetMarker:(e,t,n)=>{let r=[];return e.state.field(hv).between(n.from,n.to,(e,t,i)=>{e>n.from&&e<n.to&&r.push(...i.diagnostics)}),r.length?new uv(r):null}}),hv=Je.define({create(){return I.empty},update(e,t){e=e.map(t.changes);let n=t.state.facet(xv).markerFilter;for(let r of t.effects)if(r.is(z_)){let i=r.value;n&&(i=n(i||[],t.state)),e=pv(t.state.doc,i.slice(0))}return e}}),gv=N.define(),_v=Je.define({create(){return null},update(e,t){return e&&t.docChanged&&(e=I_(t,e)?null:{...e,pos:t.changes.mapPos(e.pos)}),t.effects.reduce((e,t)=>t.is(gv)?t.value:e,e)},provide:e=>Oc.from(e)}),vv=K.baseTheme({".cm-gutter-lint":{width:`1.4em`,"& .cm-gutterElement":{padding:`.2em`}},".cm-lint-marker":{width:`1em`,height:`1em`},".cm-lint-marker-info":{content:av(`<path fill="#aaf" stroke="#77e" stroke-width="6" stroke-linejoin="round" d="M5 5L35 5L35 35L5 35Z"/>`)},".cm-lint-marker-warning":{content:av(`<path fill="#fe8" stroke="#fd7" stroke-width="6" stroke-linejoin="round" d="M20 6L37 35L3 35Z"/>`)},".cm-lint-marker-error":{content:av(`<circle cx="20" cy="20" r="15" fill="#f87" stroke="#f43" stroke-width="6"/>`)}}),yv=Lc(W_,{hideOn:I_}),bv=[H_,K.decorations.compute([H_],e=>{let{selected:t,panel:n}=e.field(H_);return!t||!n||t.from==t.to?z.none:z.set([U_.range(t.from,t.to)])}),yv,sv],xv=M.define({combine(e){return Ot(e,{hoverTime:300,markerFilter:null,tooltipFilter:null})}});function Sv(e={}){return[xv.of(e),hv,mv,vv,_v]}var Cv=[bl(),wl(),$s(),Kf(),Ld(),Ns(),Us(),F.allowMultipleSelections.of(!0),ud(),Ud(Kd,{fallback:!0}),nf(),p_(),A_(),mc(),_c(),oc(),mh(),ms.of([...y_,...$m,...rg,...dp,...Od,...j_,...J_])],wv=pf.define(s),Q=c(),Tv=zd.define([{tag:Z.keyword,color:`var(--syn-keyword)`,fontWeight:`600`},{tag:Z.typeName,color:`var(--syn-type)`},{tag:Z.className,color:`var(--syn-event)`},{tag:Z.special(Z.variableName),color:`var(--syn-name)`},{tag:Z.string,color:`var(--syn-string)`},{tag:Z.number,color:`var(--syn-number)`},{tag:Z.atom,color:`var(--syn-number)`},{tag:Z.comment,color:`var(--syn-comment)`,fontStyle:`italic`},{tag:Z.operator,color:`var(--syn-op)`}]),Ev=K.theme({"&":{height:`100%`,fontSize:`var(--editor-font, 13px)`,backgroundColor:`var(--editor-bg)`,color:`var(--fg)`},".cm-scroller":{fontFamily:`var(--mono)`,lineHeight:`1.5`},".cm-gutters":{backgroundColor:`var(--editor-bg)`,color:`var(--muted)`,border:`none`},".cm-activeLine, .cm-activeLineGutter":{backgroundColor:`var(--active-line)`},"&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection":{backgroundColor:`var(--selection) !important`},".cm-cursor":{borderLeftColor:`var(--fg)`},".cm-tooltip":{backgroundColor:`var(--panel)`,color:`var(--fg)`,border:`1px solid var(--border)`},".cm-flashLine":{backgroundColor:`var(--flash)`},"&.cm-lineWrapping .cm-line":{paddingLeft:`calc(2ch + 6px)`,textIndent:`-2ch`}});function Dv({path:e,value:t,diagnostics:n,jump:r,onChange:i,onRun:a}){let o=(0,l.useRef)(null),s=(0,l.useRef)(void 0),c=(0,l.useRef)(new Map),u=(0,l.useRef)(e),d=(0,l.useRef)({onChange:i,onRun:a});d.current={onChange:i,onRun:a};let f=(0,l.useRef)([Cv,ms.of([{key:`Mod-Enter`,run:()=>(d.current.onRun(),!0)},eh]),wv,K.lineWrapping,Ud(Tv),Ev,Sv(),$_(null),K.updateListener.of(e=>{e.docChanged&&d.current.onChange(u.current,e.state.doc.toString())})]),p=(e,t)=>{let n=c.current.get(e);return n&&n.doc.toString()===t?n:F.create({doc:t,extensions:f.current})};return(0,l.useEffect)(()=>(s.current=new K({state:p(e,t),parent:o.current}),()=>s.current?.destroy()),[]),(0,l.useEffect)(()=>{let n=s.current;u.current===e?n.state.doc.toString()!==t&&n.setState(p(e,t)):(c.current.set(u.current,n.state),u.current=e,n.setState(p(e,t)))},[e,t]),(0,l.useEffect)(()=>{let e=s.current,t=e.state.doc,r=n.filter(e=>e.line>=1&&e.line<=t.lines).map(e=>{let n=t.line(e.line),r=Math.min(n.from+Math.max(e.col-1,0),n.to),i=/^\w+/.exec(t.sliceString(r,n.to))?.[0].length??0;return{from:r,to:Math.max(r+i,r===n.to?r:r+1),severity:`error`,message:e.message}});e.dispatch(R_(e.state,r))},[n,e,t]),(0,l.useEffect)(()=>{if(!r)return;let e=s.current,t=e.state.doc,n=t.line(Math.min(Math.max(r.line,1),t.lines));e.dispatch({selection:{anchor:n.from},effects:K.scrollIntoView(n.from,{y:`center`})}),r.focus!==!1&&e.focus()},[r]),(0,Q.jsx)(`div`,{className:`editor`,ref:o})}var Ov=class{worker;nextId=1;pending=new Map;constructor(){this.start()}start(){this.worker=new Worker(new URL(new URL(`p.worker-DezJ6YKG.js`,import.meta.url).href,``+import.meta.url),{type:`module`}),this.worker.onmessage=e=>{let t=this.pending.get(e.data.id);if(t){if(e.data.progress)return t.onProgress?.(e.data.progress);this.pending.delete(e.data.id),e.data.error?t.reject(Error(e.data.error)):t.resolve(e.data.result)}}}send(e,t){let n=this.nextId++;return new Promise((r,i)=>{this.pending.set(n,{resolve:r,reject:i,onProgress:t}),this.worker.postMessage({id:n,req:e})})}compile(e){return this.send({kind:`compile`,files:e})}check(e,t){return this.send({kind:`check`,...e},t)}cancel(){this.worker.terminate();for(let e of this.pending.values())e.reject(Error(`cancelled`));this.pending.clear(),this.start()}};function kv(e,t){let n=t.lane===void 0?`The model`:e.lanes[t.lane].label,r=t.to===void 0?`another actor`:e.lanes[t.to].label,i=t.payload===void 0?``:` with ${JSON.stringify(t.payload)}`;if(t.inspection?.write){let e=t.inspection.write;return`${n} writes ${e.name}: ${e.before} → ${e.after}.${e.unchanged?` The value stays the same.`:``}`}if(t.inspection)return`${n}: ${t.inspection.phase===`after`?`assignment completed`:`about to execute this statement`}.`;switch(t.kind){case`create`:return`${t.lane===t.to?`${r} is created`:`${n} creates ${r}`}. Each actor has its own state and data.`;case`send`:return`${n} sends ${t.label}${i} to ${r}. Sending queues the message; it does not execute the receiver's handler.`;case`dequeue`:return`${n} takes ${t.label}${i} from its inbox and begins or resumes handling it.`;case`state`:return`${n} enters ${t.label}.${t.hot?` This is a hot state: the model requires progress out of it.`:``}`;case`monitor`:return`${n} observes ${t.label}${i} to check a rule. An observer is separate from the actors implementing the system.`;case`error`:return`This execution violates a rule or encounters a runtime failure. Inspect the preceding messages and assignments to understand how it reached this point.`;case`raise`:return`${n} raises an event for itself, interrupting its current action.`;case`halt`:return`${n} stops.`;case`print`:return`The model prints a message: ${t.log}`;default:return t.log}}function Av(e,t){let n=new Map;for(let r=0;r<=t;r++){let t=e.steps[r],i=t.kind===`create`?t.to:t.lane;if(i!==void 0&&!n.has(i)&&n.set(i,{inbox:[]}),t.to!==void 0&&t.kind===`send`){let e=n.get(t.to)??{inbox:[]};e.inbox.push(r),n.set(t.to,e)}if(t.lane===void 0)continue;let a=n.get(t.lane);t.kind===`state`&&(a.state=t.label),t.inspection&&(a.snapshot=t.inspection,a.at=r,a.state=t.inspection.state),t.kind===`dequeue`&&t.peer!==void 0&&(a.inbox=a.inbox.filter(e=>e!==t.peer))}return n}function jv(e,t){let n=e.steps[t]?.lane;for(let r=t-1;r>=0;r--)if(e.steps[r].lane===n&&e.steps[r].inspection)return e.steps[r].inspection}function Mv({trace:e,cursor:t,onSelect:n,onOpen:r,sourceAvailable:i=!0}){let a=e.steps[t],o=Av(e,t),s=jv(e,t),c=a?.inspection,l=c?.write;if(!a)return(0,Q.jsx)(`p`,{children:`No execution steps were recorded.`});let u=[{label:`Start`,i:0},{label:`First message`,i:e.steps.findIndex(e=>e.kind===`send`)},{label:`First assignment`,i:e.steps.findIndex(e=>e.inspection?.phase===`after`)},{label:`Failure`,i:e.steps.findIndex(e=>e.kind===`error`)}].filter(e=>e.i>=0).sort((e,t)=>e.i-t.i);return(0,Q.jsxs)(`div`,{className:`replay-story`,children:[(0,Q.jsxs)(`details`,{className:`replay-landmarks`,children:[(0,Q.jsx)(`summary`,{children:`Jump to a step`}),(0,Q.jsx)(`div`,{className:`replay-milestones`,"aria-label":`Execution landmarks`,children:u.map(e=>(0,Q.jsxs)(`button`,{onClick:()=>n(e.i),children:[e.label,(0,Q.jsxs)(`span`,{children:[`step `,e.i+1]})]},e.label))})]}),(0,Q.jsxs)(`section`,{className:`replay-caption ${a.kind===`error`?`failure`:``}`,"aria-live":`polite`,children:[(0,Q.jsxs)(`span`,{className:`eyebrow`,children:[`STEP `,t+1,` · `,c?c.phase===`after`?`AFTER ASSIGNMENT`:`BEFORE STATEMENT`:a.kind.toUpperCase()]}),(0,Q.jsx)(`h3`,{children:kv(e,a)}),c?(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsxs)(`button`,{className:`link source-link`,disabled:!i,onClick:()=>r(c.source),children:[c.source.file,`:`,c.source.line,` ↗`]}),(0,Q.jsx)(`pre`,{className:`replay-code`,children:c.source.text}),c.phase===`after`&&(0,Q.jsxs)(`div`,{className:`assignment-deltas`,children:[l?(0,Q.jsxs)(`div`,{children:[(0,Q.jsx)(`code`,{children:l.name}),(0,Q.jsxs)(`span`,{children:[l.before,` → `,(0,Q.jsx)(`strong`,{children:l.after}),l.unchanged&&(0,Q.jsx)(`em`,{children:` · same value written again`})]})]}):[`fields`,`locals`].flatMap(e=>Object.entries(c[e]).filter(([t,n])=>s?.[e][t]!==n).map(([t,n])=>(0,Q.jsxs)(`div`,{children:[(0,Q.jsx)(`code`,{children:t}),(0,Q.jsxs)(`span`,{children:[s?.[e][t]??`not captured`,` → `,(0,Q.jsx)(`strong`,{children:n})]})]},`${e}-${t}`))),(0,Q.jsx)(`p`,{className:`model-hint`,children:`The assignment ran even if the stored value stayed the same. Values below were captured by the checker.`})]})]}):(0,Q.jsx)(`pre`,{className:`replay-code`,children:a.kind===`error`?a.label:a.log}),a.peer!==void 0&&(0,Q.jsxs)(`button`,{className:`link`,onClick:()=>n(a.peer),children:[a.kind===`send`?`Jump to when this message is handled`:`Jump to when this message was sent`,` · step `,a.peer+1]})]}),(0,Q.jsx)(Nv,{trace:e,onSelect:n}),(0,Q.jsxs)(`details`,{className:`replay-actor-details`,children:[(0,Q.jsx)(`summary`,{children:`Actor variables and pending messages`}),(0,Q.jsxs)(`h3`,{className:`actor-heading`,children:[`Actors at this point `,(0,Q.jsx)(`span`,{children:`Snapshots and recorded messages`})]}),(0,Q.jsx)(`div`,{className:`replay-actors`,children:[...o].map(([r,i])=>(0,Q.jsxs)(`section`,{className:`replay-actor ${a.lane===r?`active`:``}`,children:[(0,Q.jsxs)(`div`,{className:`actor-title`,children:[(0,Q.jsx)(`strong`,{children:e.lanes[r].label}),(0,Q.jsx)(`span`,{className:`model-tag`,children:e.lanes[r].monitor?`observer`:`actor`})]}),(0,Q.jsx)(`p`,{className:`actor-state`,children:i.state??`Created · waiting to enter its start state`}),i.snapshot?(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`dl`,{className:`actor-values`,children:Object.entries(i.snapshot.fields).map(([e,t])=>(0,Q.jsxs)(`div`,{children:[(0,Q.jsx)(`dt`,{children:e}),(0,Q.jsx)(`dd`,{children:t})]},e))}),Object.keys(i.snapshot.locals).length>0&&(0,Q.jsxs)(`details`,{open:a.lane===r,children:[(0,Q.jsx)(`summary`,{children:`Local variables · last captured frame`}),(0,Q.jsx)(`dl`,{className:`actor-values`,children:Object.entries(i.snapshot.locals).map(([e,t])=>(0,Q.jsxs)(`div`,{children:[(0,Q.jsx)(`dt`,{children:e}),(0,Q.jsx)(`dd`,{children:t})]},e))})]}),(0,Q.jsxs)(`p`,{className:`capture-note`,children:[`Captured `,i.snapshot.phase,` step `,i.at+1,i.at===t?``:` · last known values`]})]}):(0,Q.jsx)(`p`,{className:`capture-note`,children:`Variables have not been captured yet.`}),!e.lanes[r].monitor&&(0,Q.jsxs)(`div`,{className:`actor-inbox`,children:[(0,Q.jsxs)(`span`,{className:`eyebrow`,children:[`PENDING MESSAGES · `,i.inbox.length]}),i.inbox.slice(0,5).map(t=>(0,Q.jsxs)(`button`,{onClick:()=>n(t),children:[e.steps[t].label,` `,(0,Q.jsx)(`code`,{children:e.steps[t].payload===void 0?``:JSON.stringify(e.steps[t].payload)})]},t)),i.inbox.length>5&&(0,Q.jsxs)(`span`,{children:[`+`,i.inbox.length-5,` more`]})]})]},r))})]}),(0,Q.jsx)(`p`,{className:`model-hint`,children:`Replay follows one execution found by the checker. It does not explore new choices as you step. Actor values show their latest capture; inboxes are reconstructed from recorded sends and receives.`})]})}function Nv({trace:e,onSelect:t}){let[n,r]=(0,l.useState)(``),i=new Map;e.steps.forEach((e,t)=>{let n=e.inspection?.write;if(!n||n.scope!==`fields`||e.lane===void 0)return;let r=`${e.lane}:${n.name}`,a=i.get(r)??{lane:e.lane,name:n.name,rows:[],repeated:0};a.rows.push(t),n.unchanged&&a.repeated++,i.set(r,a)});let a=[...i].sort((e,t)=>t[1].repeated-e[1].repeated||t[1].rows.length-e[1].rows.length),o=i.get(n)??a[0]?.[1];if(!o)return null;let s=[...i].find(([,e])=>e===o)[0],c=e.steps.map((e,t)=>({s:e,i:t})).filter(({s:e})=>(e.kind===`send`||e.kind===`dequeue`)&&(e.lane===o.lane||e.to===o.lane));return(0,Q.jsxs)(`details`,{className:`variable-history`,open:!0,children:[(0,Q.jsx)(`summary`,{children:`Investigate this execution · recorded writes`}),(0,Q.jsx)(`p`,{className:`model-hint`,children:`Inspect a variable across the entire execution, then jump to a write or a related message. Repeated values are evidence to inspect, not automatically a bug.`}),(0,Q.jsxs)(`label`,{children:[`Variable `,(0,Q.jsx)(`select`,{"aria-label":`Variable history`,value:s,onChange:e=>r(e.target.value),children:a.map(([t,n])=>(0,Q.jsxs)(`option`,{value:t,children:[e.lanes[n.lane].label,`.`,n.name,` · `,n.rows.length,` writes`]},t))})]}),(0,Q.jsx)(`ol`,{className:`write-history`,children:o.rows.map(n=>{let r=e.steps[n],i=r.inspection.write,a;for(let t=n-1;t>=0;t--)if(e.steps[t].lane===r.lane&&e.steps[t].kind===`dequeue`){a=t;break}let s=a===void 0?void 0:e.steps[a].peer,c=s===void 0?void 0:e.steps[s].lane,l;if(c!==void 0&&a!==void 0)for(let t=a-1;t>=0;t--){let n=e.steps[t];if(n.kind===`send`&&n.lane===o.lane&&n.to===c){l=t;break}}return(0,Q.jsxs)(`li`,{children:[(0,Q.jsxs)(`button`,{onClick:()=>t(n),children:[(0,Q.jsxs)(`span`,{children:[`Step `,n+1,` · `,i.name]}),(0,Q.jsxs)(`strong`,{children:[i.before,` → `,i.after]}),i.unchanged&&(0,Q.jsx)(`span`,{className:`same-write`,children:`same value written again`}),a!==void 0&&(0,Q.jsxs)(`small`,{children:[`Most recent handled message: `,e.steps[a].label,c===void 0?``:` from ${e.lanes[c].label}`]})]}),l!==void 0&&(0,Q.jsxs)(`button`,{className:`prior-message`,onClick:()=>t(l),children:[(0,Q.jsxs)(`span`,{children:[`Earlier message to that sender: `,(0,Q.jsx)(`strong`,{children:e.steps[l].label}),e.steps[l].payload===void 0?``:` · ${JSON.stringify(e.steps[l].payload)}`]}),(0,Q.jsxs)(`span`,{children:[`step `,l+1,` ↗`]})]})]},n)})}),(0,Q.jsxs)(`details`,{className:`history-messages`,children:[(0,Q.jsxs)(`summary`,{children:[`Related messages · `,c.length]}),(0,Q.jsx)(`ol`,{children:c.map(({s:n,i:r})=>(0,Q.jsx)(`li`,{children:(0,Q.jsxs)(`button`,{className:`link`,onClick:()=>t(r),children:[`Step `,r+1,` · `,kv(e,n)]})},r))})]})]})}function Pv(e){let t=new Map;e?.lanes.forEach((e,n)=>t.set(n,e.id.replace(/\(.*$/,``).replace(/^.*\./,``)));for(let n of e?.steps??[])n.lane!==void 0&&n.inspection&&t.set(n.lane,n.inspection.machine);return t}function Fv(e){let t=e.split(`
`).flatMap(e=>e.match(/.{1,36}(?:\s|$)|.{1,36}/g)?.map(e=>e.trim())??[``]);return{text:t.join(`
`),width:Math.max(20,...t.map(e=>e.length*7))+12,height:t.length*16+8}}function Iv(e,t){let n=Pv(t),r=t?new Set(n.values()):void 0,i=e.machines.filter(e=>e.kind===`machine`&&(!r||r.has(e.name))),a=new Map(i.map((e,t)=>[e.name,`m${t}`])),o={},s={},c={},l=[];for(let e of i){let t=a.get(e.name),r=[...n.values()].filter(t=>t===e.name).length,i=e.name+(r>1?` × ${r}`:``);o[t]={kind:`machine`,machine:e.name,label:i},c[t]=e.source;let u=e.states.map((n,r)=>{let i=`${t}-s${r}`;return o[i]={kind:`state`,machine:e.name,state:n.name,label:n.name,start:n.start},c[i]=n.source,{id:i,width:Math.max(140,Math.min(350,n.name.length*8+28)),height:68}}),d=[];for(let[n,r]of e.states.entries())for(let i of r.actions)for(let r of i.targets){let a=e.states.findIndex(e=>e.name===r.state);if(a<0)continue;let o=`${t}-t${d.length}`;s[o]={kind:`transition`,conditional:r.conditional},c[o]=r.source,d.push({id:o,sources:[`${t}-s${n}`],targets:[`${t}-s${a}`],labels:[Fv(i.label+(r.guard?`\n[${r.guard}]`:``))]})}l.push({id:t,children:u,edges:d,labels:[Fv(i)],ports:[],layoutOptions:{"elk.algorithm":`layered`,"elk.direction":`DOWN`,"elk.padding":`[top=52,left=24,bottom=24,right=24]`,"elk.spacing.nodeNode":`40`,"elk.layered.spacing.nodeNodeBetweenLayers":`60`,"elk.nodeLabels.placement":`INSIDE H_CENTER V_TOP`,"elk.portConstraints":`FIXED_SIDE`}})}let u=new Map,d=(e,t,n)=>{if(!e||!t||!a.has(e)||!a.has(t)||e===t)return;let r=`${e}\u0000${t}`,i=u.get(r)??new Set;i.add(n),u.set(r,i)};if(t)for(let e of t.steps)e.kind===`send`&&e.lane!==void 0&&e.to!==void 0&&d(n.get(e.lane),n.get(e.to),e.label);else for(let e of i)for(let t of i)for(let n of e.sent??[])t.received?.includes(n)&&d(e.name,t.name,n);let f=[];for(let[e,n]of u){let[r,i]=e.split(`\0`),o=a.get(r),c=a.get(i),u=`route${f.length}`,d=`${o}-${u}-out`,p=`${c}-${u}-in`;l.find(e=>e.id===o).ports.push({id:d,width:6,height:6,layoutOptions:{"elk.port.side":`EAST`}}),l.find(e=>e.id===c).ports.push({id:p,width:6,height:6,layoutOptions:{"elk.port.side":`WEST`}});let m=[...n];f.push({id:u,sources:[d],targets:[p],labels:[Fv(m.slice(0,2).join(`
`)+(m.length>2?`\n+${m.length-2} events`:``))]}),s[u]={kind:`route`,observed:!!t}}return{graph:{id:`system`,children:l,edges:f,layoutOptions:{"elk.algorithm":`layered`,"elk.direction":`RIGHT`,"elk.edgeRouting":`ORTHOGONAL`,"elk.hierarchyHandling":`SEPARATE_CHILDREN`,"elk.padding":`[top=24,left=24,bottom=24,right=24]`,"elk.spacing.nodeNode":`60`,"elk.layered.spacing.nodeNodeBetweenLayers":`90`,"elk.spacing.edgeNode":`24`,"elk.layered.spacing.edgeNodeBetweenLayers":`30`}},nodes:o,edges:s,sources:c}}function Lv(e,t=-1){let n=Pv(e),r=new Map;for(let n of e?.steps.slice(0,t+1)??[])n.lane!==void 0&&(n.kind===`state`&&r.set(n.lane,n.label),n.inspection&&r.set(n.lane,n.inspection.state));let i=new Set;for(let[e,t]of r)i.add(JSON.stringify([n.get(e),t]));let a=e?.steps[t];return{active:i,failed:a?.kind===`error`&&a.lane!==void 0?JSON.stringify([n.get(a.lane),r.get(a.lane)]):void 0}}function Rv({model:e,trace:t,cursor:n,onOpen:r,readOnly:i=!1}){let a=(0,l.useMemo)(()=>Iv(e,t),[e,t]),[o,s]=(0,l.useState)(),[c,u]=(0,l.useState)(1),[d,f]=(0,l.useState)(!1),p=(0,l.useRef)(null),m=(0,l.useId)().replace(/:/g,``),h=(0,l.useMemo)(()=>Lv(t,n),[t,n]);(0,l.useEffect)(()=>{let e=new Worker(new URL(new URL(`layout.worker-DIbe2U4g.js`,import.meta.url).href,``+import.meta.url),{type:`module`});return e.onmessage=e=>s({input:a,...e.data}),e.onerror=e=>s({input:a,error:e.message||`Layout worker failed`}),e.postMessage({graph:a.graph}),()=>e.terminate()},[a]);let g=o?.input===a,_=o?.graph?.width??640,v=o?.graph?.height??300,y=e=>{!i&&a.sources[e]&&(f(!1),r(a.sources[e]))},ee=e=>({role:`button`,tabIndex:i?-1:0,"aria-disabled":i,onClick:t=>{t.stopPropagation(),y(e)},onKeyDown:t=>{(t.key===`Enter`||t.key===` `)&&(t.preventDefault(),t.stopPropagation(),y(e))}}),b=(e=[],t=!1)=>e.map((e,n)=>(0,Q.jsxs)(`g`,{transform:`translate(${e.x??0} ${e.y??0})`,children:[t&&(0,Q.jsx)(`rect`,{className:`system-label-bg`,width:e.width,height:e.height,rx:`3`}),(0,Q.jsx)(`text`,{className:t?`system-edge-label`:`system-machine-label`,x:(e.width??0)/2,y:`17`,textAnchor:`middle`,children:e.text?.split(`
`).map((t,n)=>(0,Q.jsx)(`tspan`,{x:(e.width??0)/2,dy:n?16:0,children:t},n))})]},n)),x=e=>{let t=a.edges[e.id],n=t?.kind===`route`,r=!!a.sources[e.id];return(0,Q.jsxs)(`g`,{className:`system-edge ${n?`route`:`transition`}`,...r?ee(e.id):{},"aria-label":e.labels?.map(e=>e.text).join(` `),children:[e.sections?.map((e,r)=>{let i=[e.startPoint,...e.bendPoints??[],e.endPoint];return(0,Q.jsx)(`path`,{d:i.map((e,t)=>`${t?`L`:`M`}${e.x},${e.y}`).join(` `),fill:`none`,stroke:n?`var(--accent)`:`var(--muted)`,strokeWidth:`1.6`,strokeDasharray:n?t.observed?void 0:`6 4`:t?.conditional?`4 3`:void 0,markerEnd:`url(#${m}-${n?`route`:`state`})`},r)}),b(e.labels,!0)]},e.id)},S=e=>{let t=a.nodes[e.id];if(!t)return null;let n=JSON.stringify([t.machine,t.state]),r=t.kind===`state`&&h.active.has(n),i=r&&h.failed===n;return(0,Q.jsxs)(`g`,{transform:`translate(${e.x??0} ${e.y??0})`,className:`system-node ${t.kind} ${r?`active`:``} ${i?`failed`:``}`,...t.kind===`state`?ee(e.id):{},"aria-label":t.kind===`state`?`Open state ${t.label}`:void 0,children:[(0,Q.jsx)(`rect`,{width:e.width,height:e.height,rx:t.kind===`machine`?14:10}),t.kind===`machine`?(0,Q.jsx)(`g`,{className:`system-title`,...ee(e.id),"aria-label":`Open machine ${t.label}`,children:b(e.labels)}):(0,Q.jsxs)(`g`,{className:`system-title`,children:[(0,Q.jsx)(`text`,{className:`system-state-label`,x:(e.width??0)/2,y:t.start?28:38,textAnchor:`middle`,children:t.label}),t.start&&(0,Q.jsx)(`text`,{className:`system-start-label`,x:(e.width??0)/2,y:`47`,textAnchor:`middle`,children:`start`})]}),e.edges?.map(x),e.children?.map(S)]},e.id)};return(0,Q.jsxs)(`div`,{className:`system-diagram ${d?`expanded`:``}`,onKeyDown:e=>{e.key===`Escape`&&f(!1)},children:[(0,Q.jsxs)(`div`,{className:`model-toolbar`,children:[(0,Q.jsx)(`strong`,{children:`All machines`}),(0,Q.jsxs)(`div`,{className:`system-zoom`,children:[(0,Q.jsx)(`button`,{"aria-label":`Zoom out system map`,onClick:()=>u(e=>Math.max(.25,e-.25)),children:`−`}),(0,Q.jsx)(`button`,{onClick:()=>u((p.current?.clientWidth??_)/_),children:`Fit`}),(0,Q.jsx)(`button`,{"aria-label":`Zoom in system map`,onClick:()=>u(e=>Math.min(4,e+.25)),children:`+`}),(0,Q.jsx)(`button`,{onClick:()=>f(e=>!e),children:d?`Close expanded diagram`:`Expand`})]})]}),(0,Q.jsxs)(`div`,{className:`system-legend`,"aria-label":`Diagram key`,children:[(0,Q.jsxs)(`span`,{children:[(0,Q.jsx)(`i`,{className:`legend-state`}),`State changes`]}),(0,Q.jsxs)(`span`,{children:[(0,Q.jsx)(`i`,{className:t?`legend-route observed`:`legend-route`}),t?`Observed event routes`:`Compatible event routes`]})]}),(0,Q.jsx)(`p`,{className:`model-hint`,children:t?`Routes cover the recorded execution. Highlighted states reflect this step; instances are grouped by type.`:`Run Check to see actual routes. Select a machine or state to open its code.`}),!g&&(0,Q.jsx)(`p`,{role:`status`,children:`Arranging state machines…`}),g&&o.error&&(0,Q.jsxs)(`p`,{role:`alert`,children:[`Could not arrange this diagram. `,o.error]}),g&&o.graph&&(0,Q.jsx)(`div`,{className:`system-viewport`,ref:p,children:(0,Q.jsxs)(`svg`,{className:`system-svg`,"aria-label":`Linked state-machine diagram`,viewBox:`0 0 ${_} ${v}`,style:{width:c*_},children:[(0,Q.jsx)(`defs`,{children:[`state`,`route`].map(e=>(0,Q.jsx)(`marker`,{id:`${m}-${e}`,markerWidth:`8`,markerHeight:`8`,refX:`7`,refY:`4`,orient:`auto`,children:(0,Q.jsx)(`path`,{d:`M0 0 L8 4 L0 8 Z`,fill:e===`route`?`var(--accent)`:`var(--muted)`})},e))}),o.graph.children?.map(S),o.graph.edges?.map(x)]})}),(0,Q.jsxs)(`details`,{className:`model-reference`,children:[(0,Q.jsx)(`summary`,{children:`Diagram data · ELK JSON`}),(0,Q.jsx)(`pre`,{className:`replay-code`,children:JSON.stringify(a.graph,null,2)})]})]})}function zv({model:e,activeMachine:t,activeState:n,onOpen:r,readOnly:i=!1,initialMachine:a,failed:o=!1}){let[s,c]=(0,l.useState)(``),u=e.machines.find(e=>e.name===(s||t||a))??e.machines[0];return u?(0,Q.jsxs)(`div`,{className:`state-explorer`,children:[(0,Q.jsxs)(`div`,{className:`model-toolbar`,children:[e.machines.length<=5?(0,Q.jsx)(`div`,{className:`machine-picker`,role:`group`,"aria-label":`State machine`,children:e.machines.map(e=>(0,Q.jsx)(`button`,{"aria-pressed":e.name===u.name,onClick:()=>c(e.name),children:e.name},e.name))}):(0,Q.jsxs)(`label`,{children:[`State machine `,(0,Q.jsx)(`select`,{"aria-label":`State machine`,value:u.name,onChange:e=>c(e.target.value),children:e.machines.map(e=>(0,Q.jsxs)(`option`,{value:e.name,children:[e.name,e.kind===`machine`?``:` · ${e.kind}`]},e.name))})]}),t&&(0,Q.jsx)(`button`,{className:`link`,onClick:()=>c(``),children:`Follow active actor`})]}),(0,Q.jsx)(`p`,{className:`model-hint`,children:`States are the boxes. Arrows show how this machine can change state. Select a box or arrow to read its code.`}),(0,Q.jsx)(Bv,{failed:o,readOnly:i,machine:u,current:u.name===t?n:void 0,onOpen:r}),(0,Q.jsxs)(`details`,{className:`state-actions`,children:[(0,Q.jsx)(`summary`,{children:`Actions and source`}),u.states.map(e=>(0,Q.jsxs)(`section`,{className:u.name===t&&e.name===n?`state-action active`:`state-action`,children:[(0,Q.jsx)(`button`,{className:`link`,disabled:i,onClick:()=>r(e.source),children:(0,Q.jsx)(`strong`,{children:e.name})}),e.start&&(0,Q.jsx)(`span`,{className:`model-tag`,children:`start`}),e.hot&&(0,Q.jsx)(`span`,{className:`model-tag`,children:`hot · progress required`}),e.actions.length===0&&(0,Q.jsx)(`p`,{className:`muted`,children:`No actions declared.`}),e.actions.map((e,t)=>(0,Q.jsxs)(`button`,{className:`state-action-row`,disabled:i,onClick:()=>r(e.source),children:[(0,Q.jsx)(`span`,{children:e.label}),(0,Q.jsx)(`span`,{children:e.targets.length?e.targets.map(e=>`${e.conditional?`may go to`:`→`} ${e.state}`).join(` · `):`action / stays unless it transitions`})]},t))]},e.name))]})]}):(0,Q.jsx)(`p`,{className:`model-empty`,children:`No state machines in this project yet. Add a machine in the editor.`})}function Bv({machine:e,current:t,onOpen:n,readOnly:r,failed:i}){let a=(0,l.useId)().replace(/:/g,``),o=e.states,s=o.flatMap((e,t)=>e.actions.flatMap(e=>e.targets.map(n=>({from:t,to:o.findIndex(e=>e.name===n.state),label:e.label,...n})))).filter(e=>e.to>=0),c=e=>65+e*145,u=Math.max(180,o.length*145+20);return(0,Q.jsxs)(`div`,{className:`state-graph`,children:[(0,Q.jsxs)(`div`,{className:`state-graph-title`,children:[(0,Q.jsx)(`strong`,{children:e.name}),(0,Q.jsx)(`span`,{children:t?`${i?`Failure detected in`:`Current state:`} ${t}`:`Declared states`})]}),(0,Q.jsxs)(`svg`,{viewBox:`0 0 620 ${u}`,role:`img`,"aria-label":`States and transitions of ${e.name}`,children:[(0,Q.jsx)(`defs`,{children:(0,Q.jsx)(`marker`,{id:a,markerWidth:`8`,markerHeight:`8`,refX:`7`,refY:`4`,orient:`auto`,children:(0,Q.jsx)(`path`,{d:`M0,0 L8,4 L0,8 z`,fill:`currentColor`})})}),s.map((e,t)=>{let i=c(e.from),s=c(e.to),l=355+t%5*12,u=e.from===e.to?`M310 ${i-18} C${l+70} ${i-85},${l+70} ${i+85},310 ${i+18}`:`M310 ${i} C${l} ${i},${l} ${s},310 ${s}`,d=e.from===e.to?i:(i+s)/2;return(0,Q.jsxs)(`g`,{className:`state-edge`,role:`button`,"aria-label":`${o[e.from].name}: ${e.label} to ${e.state}`,"aria-disabled":r,tabIndex:r?-1:0,onClick:()=>{r||n(e.source)},onKeyDown:t=>{r||t.key!==`Enter`&&t.key!==` `||(t.preventDefault(),n(e.source))},children:[(0,Q.jsx)(`path`,{d:u,fill:`none`,stroke:`currentColor`,strokeWidth:`1.5`,strokeDasharray:e.conditional?`5 4`:void 0,markerEnd:`url(#${a})`}),(0,Q.jsx)(`text`,{x:395,y:d-3,className:`edge-label`,children:e.label.length>28?e.label.slice(0,26)+`…`:e.label}),e.conditional&&(0,Q.jsx)(`text`,{x:395,y:d+14,className:`edge-condition`,children:e.guard?e.guard.length>35?e.guard.slice(0,33)+`…`:e.guard:`when its condition is true`}),(0,Q.jsxs)(`title`,{children:[e.label,` → `,e.state,e.conditional?` (conditional)`:``]})]},t)}),o.map((e,o)=>{let s=e.actions.filter(e=>!e.targets.length).map(e=>e.label===`entry`?`on entering`:e.label===`exit`?`on leaving`:e.label);return(0,Q.jsxs)(`g`,{className:`state-node ${e.name===t?`active ${i?`failed`:``}`:``}`,role:`button`,"aria-disabled":r,tabIndex:r?-1:0,"aria-label":`Open state ${e.name}`,onClick:()=>{r||n(e.source)},onKeyDown:t=>{r||t.key!==`Enter`&&t.key!==` `||(t.preventDefault(),n(e.source))},children:[e.start&&(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`path`,{d:`M12 ${c(o)} H78`,stroke:`currentColor`,markerEnd:`url(#${a})`}),(0,Q.jsx)(`text`,{x:`12`,y:c(o)-10,children:`start`})]}),(0,Q.jsx)(`rect`,{x:`85`,y:c(o)-42,width:`225`,height:`84`,rx:`16`}),(0,Q.jsx)(`text`,{x:`197`,y:c(o)-14,textAnchor:`middle`,children:e.name.length>25?e.name.slice(0,23)+`…`:e.name}),(0,Q.jsx)(`text`,{className:`state-node-actions`,x:`197`,y:c(o)+8,textAnchor:`middle`,children:s.slice(0,2).join(` · `).slice(0,35)}),s.length>2&&(0,Q.jsxs)(`text`,{className:`state-node-actions`,x:`197`,y:c(o)+25,textAnchor:`middle`,children:[`+ `,s.length-2,` more actions`]}),e.name===t&&(0,Q.jsx)(`text`,{className:`state-current`,x:`197`,y:c(o)+60,textAnchor:`middle`,children:i?`● failure detected here`:`● current state`}),(0,Q.jsxs)(`title`,{children:[e.name,e.name===t?` · active in replay`:``]})]},e.name)})]}),s.length===0&&(0,Q.jsx)(`p`,{className:`model-hint`,children:`One state can handle many events. This machine has no explicit state changes.`})]})}function Vv(e,t){let n=e[t],r=n.peer===void 0?void 0:e[n.peer]?.lane;return[n.lane,n.to,r].filter(e=>e!==void 0)}function Hv(e,t){let n=e.steps.map((e,t)=>t);if(!t.size)return{rows:n,cols:e.lanes.map((e,t)=>t)};let r=n.filter(n=>{let r=e.steps[n];return r.kind===`error`||r.kind===`print`||Vv(e.steps,n).some(e=>t.has(e))}),i=new Set(t);for(let t of r)for(let n of Vv(e.steps,t))i.add(n);return{rows:r,cols:e.lanes.map((e,t)=>t).filter(e=>i.has(e))}}var $=e=>typeof e==`string`?e:void 0;function Uv(e){let t=JSON.parse(e),n=[],r=new Map,i=[],a=new Map,o,s,c=(e,t=!1)=>{if(!e)return;let i=r.get(e);return i===void 0&&(i=n.length,n.push({id:e,label:e,monitor:t}),r.set(e,i)),i},l=e=>JSON.stringify(e??null);for(let{type:e,details:n}of t){let t=n.log??e,u=n.payload===`null`?void 0:n.payload;switch(e){case`Inspection`:{let e=n.inspection;i.push({kind:`note`,lane:c($(n.id)),label:e.source.text.split(`
`)[0],log:t,inspection:e});break}case`CreateMonitor`:c($(n.monitor),!0);break;case`CreateStateMachine`:{let e=c($(n.id)),a=r.get($(n.creatorName)??``);i.push({kind:`create`,lane:a??e,to:e,label:`new`,log:t,payload:u});break}case`StateTransition`:case`MonitorStateTransition`:n.isEntry&&i.push({kind:`state`,lane:c($(n.id)??$(n.monitor),e===`MonitorStateTransition`),label:$(n.state)??`?`,log:t,hot:n.isInHotState===!0||/enters hot state/.test(t)});break;case`SendEvent`:{let e=c($(n.sender)),r=c($(n.target)),o=i.length;if(i.push({kind:`send`,lane:e,to:r,label:$(n.event)??`?`,log:t,payload:u}),r!==void 0){let e=a.get(r)??[];e.push({event:$(n.event)??``,key:l(u),step:o}),a.set(r,e)}break}case`DequeueEvent`:case`ReceiveEvent`:{let e=c($(n.id)),r=$(n.event)??`?`,o=i.length,s={kind:`dequeue`,lane:e,label:r,log:t,payload:u};if(e!==void 0){let t=a.get(e)??[],n=t.findIndex(e=>e.event===r&&e.key===l(u));n<0&&(n=t.findIndex(e=>e.event===r)),n>=0&&(s.peer=t[n].step,i[t[n].step].peer=o,t.splice(n,1))}i.push(s);break}case`MonitorProcessEvent`:i.push({kind:`monitor`,lane:c($(n.monitor),!0),to:r.get($(n.sender)??``),label:$(n.event)??`?`,log:t,payload:u});break;case`Announce`:i.push({kind:`announce`,lane:c($(n.id)),label:`announce ${$(n.event)}`,log:t,payload:u});break;case`RaiseEvent`:case`MonitorRaiseEvent`:i.push({kind:`raise`,lane:c($(n.id)??$(n.monitor)),label:`raise ${$(n.event)}`,log:t});break;case`Print`:i.push({kind:`print`,label:t,log:t});break;case`Halt`:i.push({kind:`halt`,lane:c($(n.id)),label:`halt`,log:t});break;case`AssertionFailure`:{o=$(n.error)??t;let e=[...r.keys()].filter(e=>o.includes(e)).sort((e,t)=>t.length-e.length)[0],a=[...i].reverse().find(e=>e.lane!==void 0)?.lane;i.push({kind:`error`,lane:e===void 0?a:r.get(e),label:o,log:t});break}case`StrategyDescription`:s=$(n.strategyDescription);break;case`GotoState`:case`WaitMultipleEvents`:break;default:i.push({kind:`note`,lane:c($(n.id)??$(n.monitor)),label:e,log:t,payload:u})}}let u=n.map((e,t)=>t).sort((e,t)=>Number(n[e].monitor)-Number(n[t].monitor)||e-t),d=new Map(u.map((e,t)=>[e,t])),f=e=>e===void 0?void 0:d.get(e);return{lanes:u.map(e=>n[e]),steps:i.map(e=>({...e,lane:f(e.lane),to:f(e.to)})),error:o,strategy:s}}function Wv(e){if(e==null)return``;if(typeof e!=`object`)return String(e);let t=JSON.stringify(e);return t.length>60?t.slice(0,57)+`…`:t}var Gv=150,Kv=96,qv=30,Jv=10,Yv=44,Xv=.35,Zv=56,Qv=2.5,$v=6,ey=e=>t=>t*e+e/2,ty=e=>Jv+e*qv+qv/2,ny=e=>Math.min(Qv,Math.max(Xv,e)),ry=7.2;function iy(e,t){let n=Math.max(4,Math.floor(t/ry));return e.length>n?e.slice(0,n-1)+`…`:e}function ay(e,t,n,r,i=7){let a=Math.atan2(r-t,n-e),o=e=>`${n-i*Math.cos(a+e)},${r-i*Math.sin(a+e)}`;return`M${n},${r} L${o(.42)} L${o(-.42)} z`}function oy(e){let t=0;return e.map(e=>e.monitor?`var(--spec)`:`var(--lane-${t++%$v})`)}function sy({trace:e,rows:t,cols:n,color:r,focus:i,cursor:a,hover:o,onSelect:s,onHover:c,onToggleLane:u}){let{lanes:d,steps:f}=e,[p]=(0,l.useState)(()=>matchMedia(`(max-width: 600px)`).matches?Kv:Gv),m=ey(p),h=Math.max(n.length,1)*p,g=20+t.length*qv+Zv+16,_=(0,l.useMemo)(()=>{let e=new Map(n.map((e,t)=>[e,t])),r=new Map(t.map((e,t)=>[e,t]));return{col:t=>t===void 0?void 0:e.get(t),row:e=>e===void 0?void 0:r.get(e)}},[t,n]),v=(0,l.useRef)(null),[y,ee]=(0,l.useState)(1),[b,x]=(0,l.useState)(!1),S=(0,l.useRef)(null),te=(0,l.useMemo)(()=>{let e=new Map;for(let t of d)e.set(dy(t.id),(e.get(dy(t.id))??0)+1);return new Set([...e].filter(([,e])=>e>1).map(([e])=>e))},[d]),ne=(0,l.useMemo)(()=>{let e=d.map(e=>e.monitor?0:-1);return t.forEach((t,n)=>{let r=f[t];r.kind===`create`&&r.to!==void 0&&e[r.to]<0&&(e[r.to]=n),r.lane!==void 0&&e[r.lane]<0&&(e[r.lane]=n)}),e.map(e=>Math.max(e,0))},[d,f,t]),re=(0,l.useCallback)(()=>{let e=v.current;return e?ny((e.clientWidth-4)/h):1},[h]),C=(0,l.useRef)(y),ie=(0,l.useCallback)((e,t,n)=>{let r=v.current;if(!r)return;let i=C.current,a=ny(e),o=t??r.clientWidth/2,s=n??r.clientHeight/2,c=S.current,l=c?c.cx*i-c.sx:r.scrollLeft,u=c?c.cy*i+Yv*i-c.sy:r.scrollTop;S.current={cx:(l+o)/i,cy:(u+s-Yv*i)/i,sx:o,sy:s},C.current=a,ee(a)},[]);(0,l.useLayoutEffect)(()=>{let e=v.current,t=S.current;e&&t&&(S.current=null,e.scrollLeft=t.cx*y-t.sx,e.scrollTop=t.cy*y+Yv*y-t.sy)},[y]),(0,l.useLayoutEffect)(()=>{C.current=1,ee(1)},[e,re]),(0,l.useEffect)(()=>{let e=v.current;if(!e)return;let t=t=>{if(!t.ctrlKey&&!t.metaKey)return;t.preventDefault();let n=e.getBoundingClientRect(),r=Math.exp(-Math.max(-50,Math.min(50,t.deltaY))*.006);ie(C.current*r,t.clientX-n.left,t.clientY-n.top)};return e.addEventListener(`wheel`,t,{passive:!1}),()=>e.removeEventListener(`wheel`,t)},[ie]);let ae=(0,l.useRef)(new Map),w=(0,l.useRef)(null),T=()=>{let[e,t]=[...ae.current.values()];return Math.hypot(e.x-t.x,e.y-t.y)},oe=e=>{e.pointerType===`touch`&&(ae.current.set(e.pointerId,{x:e.clientX,y:e.clientY}),ae.current.size===2&&(w.current={d:T(),z:C.current}))},se=e=>{if(!ae.current.has(e.pointerId))return;ae.current.set(e.pointerId,{x:e.clientX,y:e.clientY});let t=w.current,n=v.current;if(!t||ae.current.size!==2||!n)return;let[r,i]=[...ae.current.values()],a=n.getBoundingClientRect();ie(t.z*(T()/t.d),(r.x+i.x)/2-a.left,(r.y+i.y)/2-a.top)},ce=e=>{ae.current.delete(e.pointerId),ae.current.size<2&&(w.current=null)};return(0,l.useEffect)(()=>{let e=v.current;if(!e)return;let t=_.row(a);if(t===void 0)return;let n=Yv*C.current,r=n+(ty(t)-qv)*C.current,i=n+(ty(t)+qv)*C.current;r<e.scrollTop+n?e.scrollTop=r-n:i>e.scrollTop+e.clientHeight-Zv&&(e.scrollTop=i-e.clientHeight+Zv);let o=f[a];if(!o)return;let s=[o.lane,o.to,o.peer===void 0?void 0:f[o.peer]?.lane].map(_.col).filter(e=>e!==void 0);if(!s.length)return;let c=C.current,l=Math.min(...s)*p*c,u=(Math.max(...s)+1)*p*c;if(o.kind===`error`){let e=ly(o.label,ey(p)(s[0]),h);l=(e.left-8)*c,u=(e.left+e.w+8)*c}u-l>e.clientWidth?e.scrollLeft=(l+u)/2-e.clientWidth/2:l<e.scrollLeft?e.scrollLeft=l:u>e.scrollLeft+e.clientWidth&&(e.scrollLeft=u-e.clientWidth)},[a,f,p,h,_]),(0,Q.jsxs)(`div`,{className:`seq-wrap`,children:[(0,Q.jsxs)(`div`,{className:`seq`,ref:v,onPointerDown:oe,onPointerMove:se,onPointerUp:ce,onPointerCancel:ce,children:[(0,Q.jsx)(`svg`,{className:`seq-head`,width:h*y,height:Yv*y,viewBox:`0 0 ${h} ${Yv}`,role:`presentation`,children:n.map((e,t)=>{let n=d[e];return(0,Q.jsxs)(`g`,{className:i.has(e)?`lane focused`:`lane`,transform:`translate(${m(t)},${Yv/2})`,style:{"--c":r[e]},onClick:()=>u(e),children:[(0,Q.jsx)(`rect`,{x:-p/2+4,y:-15,width:p-8,height:30,rx:15,className:n.monitor?`lane-head monitor`:`lane-head`}),(0,Q.jsx)(fy,{id:n.id,monitor:n.monitor,w:p,showId:te.has(dy(n.id)),focused:i.has(e)})]},n.id)})}),(0,Q.jsxs)(`svg`,{width:h*y,height:g*y,viewBox:`0 0 ${h} ${g}`,className:`seq-body`,children:[n.map((e,t)=>(0,Q.jsx)(`line`,{x1:m(t),x2:m(t),y1:ty(ne[e])-qv/2,y2:g,className:d[e].monitor?`lifeline monitor`:`lifeline`,style:{"--c":r[e]}},d[e].id)),t.map(e=>(0,Q.jsx)(cy,{i:e,s:f[e],W:p,width:h,cursor:a,hover:o===e,color:r,layout:_,onSelect:s,onHover:c},e))]})]}),b&&(0,Q.jsx)(my,{onClose:()=>x(!1)}),(0,Q.jsxs)(`div`,{className:`zoom-controls`,role:`group`,"aria-label":`Zoom`,children:[(0,Q.jsx)(`button`,{className:`key-toggle`,onClick:()=>x(!b),"aria-expanded":b,children:`Key`}),(0,Q.jsx)(`button`,{onClick:()=>ie(y/1.25),"aria-label":`Zoom out`,disabled:y<=Xv,children:`−`}),(0,Q.jsxs)(`button`,{className:`zoom-level`,onClick:()=>ie(y===1?re():1),title:`Toggle 100% / fit width`,children:[Math.round(y*100),`%`]}),(0,Q.jsx)(`button`,{onClick:()=>ie(y*1.25),"aria-label":`Zoom in`,disabled:y>=Qv,children:`+`})]})]})}function cy({i:e,s:t,W:n,width:r,cursor:i,hover:a,color:o,layout:s,onSelect:c,onHover:l}){let u=ey(n),d=e=>u(s.col(e)??0),f=ty(s.row(e)??0),p=e>i,m=e===i,h=t.peer!==void 0&&t.peer===i,g=[`row`,p&&`future`,(m||h)&&`selected`,a&&`hover`,`k-${t.kind}`].filter(Boolean).join(` `),_=t.lane===void 0?0:d(t.lane),v=t.lane===void 0?`var(--fg)`:o[t.lane],y=null;switch(t.kind){case`create`:if(t.lane!==void 0&&t.to!==void 0&&t.lane!==t.to){let e=d(t.to),n=e+(e>_?-6:6);y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:_,y1:f,x2:n,y2:f,className:`msg create`}),(0,Q.jsx)(`path`,{d:ay(_,f,n,f),className:`arrowhead create`}),(0,Q.jsx)(`text`,{x:(_+e)/2,y:f-5,textAnchor:`middle`,className:`msg-label muted`,children:`new`})]})}else t.to!==void 0&&(y=(0,Q.jsx)(`circle`,{cx:d(t.to),cy:f,r:4,className:`dot`,style:{fill:o[t.to]}}));break;case`send`:{if(t.lane===void 0||t.to===void 0)break;let e=d(t.to),n=s.row(t.peer),r=n===void 0?f:ty(n),i=e>=_?1:-1;y=t.to===t.lane?(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`path`,{d:`M${_},${f} h18 a8,8 0 0 1 8,8 V${r-8} a8,8 0 0 1 -8,8 h-12`,className:`msg`,fill:`none`}),(0,Q.jsx)(`path`,{d:ay(_+18,r,_+6,r),className:`arrowhead`}),(0,Q.jsx)(py,{x:_+32,y:f-4,anchor:`start`,event:t.label,payload:t.payload,max:130})]}):(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:_,y1:f,x2:e-i*5,y2:r,className:t.peer===void 0?`msg pending`:`msg`}),(0,Q.jsx)(`path`,{d:ay(_,f,e-i*5,r),className:`arrowhead`}),(0,Q.jsx)(py,{x:_+i*10,y:f-6,anchor:i>0?`start`:`end`,event:t.label,payload:t.payload,max:Math.max(Math.abs(e-_)-20,90)})]});break}case`dequeue`:y=(0,Q.jsx)(`circle`,{cx:_,cy:f,r:4,className:`dot`});break;case`state`:{let e=Math.min(n-8,18+t.label.length*ry),i=Math.min(Math.max(_,e/2+2),r-e/2-2);y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:i-e/2,y:f-10,width:e,height:20,rx:10,className:t.hot?`state hot`:`state`}),(0,Q.jsx)(`text`,{x:i,y:f,dy:`0.35em`,textAnchor:`middle`,className:`state-label`,children:iy(t.label,n-16)})]});break}case`monitor`:{let e=t.to===void 0?void 0:d(t.to);y=(0,Q.jsxs)(Q.Fragment,{children:[e!==void 0&&(0,Q.jsx)(`line`,{x1:e,y1:f,x2:_,y2:f,className:`observe`}),(0,Q.jsx)(`rect`,{x:_-5,y:f-5,width:10,height:10,rx:2,transform:`rotate(45 ${_} ${f})`,className:`diamond`}),(0,Q.jsx)(`text`,{x:_+11,y:f-6,className:`msg-label`,children:iy(t.label,n-20)})]});break}case`announce`:case`raise`:case`halt`:case`note`:y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:_-4,y:f-4,width:8,height:8,rx:2,className:`dot`}),(0,Q.jsx)(`text`,{x:_+9,y:f,dy:`0.35em`,className:`note-label`,children:iy(t.label,n-20)})]});break;case`print`:y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:6,y:f-9,width:Math.min(r-12,8+iy(`print: ${t.label}`,r-20).length*ry),height:18,rx:4,className:`label-bg`}),(0,Q.jsx)(`text`,{x:10,y:f,dy:`0.35em`,className:`print-label`,children:iy(`print: ${t.label}`,r-20)})]});break;case`error`:{let{left:e,w:n}=ly(t.label,t.lane===void 0?void 0:_,r);y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:e,y:f-12,width:n,height:24,rx:12,className:`error-bar`}),(0,Q.jsx)(`text`,{x:e+12,y:f,dy:`0.35em`,className:`error-label`,children:uy(t.label)})]});break}}return(0,Q.jsxs)(`g`,{className:g,"data-row":e,onClick:()=>c(e),onMouseEnter:()=>l(e),onMouseLeave:()=>l(void 0),style:{"--c":v},children:[(0,Q.jsx)(`rect`,{x:0,y:f-qv/2,width:r,height:qv,className:`row-hit`}),m&&(0,Q.jsx)(`rect`,{x:0,y:f-qv/2+3,width:3,height:24,rx:1.5,className:`row-cursor`}),(0,Q.jsx)(`title`,{children:t.log}),y]})}function ly(e,t,n){let r=Math.min(n-8,24+uy(e).length*7.8);return{left:Math.max(4,Math.min(t===void 0?4:t-16,n-r-4)),w:r}}function uy(e){return`✕ ${(/^([^:.]+)/.exec(e)?.[1]??e).trim()}`}var dy=e=>e.replace(/\(\d+\)$/,``);function fy({id:e,monitor:t,w:n,showId:r,focused:i}){let a=/^(.*?)(\(\d+\))?$/.exec(e),o=r?a[2]??``:``,s=n-14,c=(a[1].length+o.length)*ry,l=Math.max(.82,Math.min(1,s/c));return(0,Q.jsxs)(`text`,{textAnchor:`middle`,dy:`0.35em`,className:`lane-label`,style:l<1?{fontSize:`${(12*l).toFixed(1)}px`}:void 0,children:[(0,Q.jsx)(`title`,{children:`${t?`spec ${e}`:e} (${i?`click to stop focusing`:`click to show only its steps`})`}),iy(a[1],s/l-o.length*ry),o&&(0,Q.jsx)(`tspan`,{className:`lane-id`,children:o})]})}function py({x:e,y:t,anchor:n,event:r,payload:i,max:a}){let o=iy(r,a),s=a-o.length*ry-6,c=i!==void 0&&s>30?iy(Wv(i),s):``,l=(o.length+(c?c.length+1:0))*ry+8,u=n===`middle`?e-l/2:n===`end`?e-l+4:e-4;return(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:u,y:t-11,width:l,height:15,rx:4,className:`label-bg`}),(0,Q.jsxs)(`text`,{x:e,y:t,textAnchor:n,className:`msg-label`,children:[(0,Q.jsx)(`tspan`,{className:`ev`,children:o}),c&&(0,Q.jsxs)(`tspan`,{className:`payload`,children:[` `,c]})]})]})}function my({onClose:e}){let t=(e,t)=>(0,Q.jsxs)(`li`,{children:[(0,Q.jsx)(`svg`,{width:`40`,height:`18`,viewBox:`0 0 40 18`,"aria-hidden":`true`,children:e}),(0,Q.jsx)(`span`,{children:t})]});return(0,Q.jsxs)(`div`,{className:`legend`,role:`dialog`,"aria-label":`Diagram key`,children:[(0,Q.jsxs)(`div`,{className:`legend-head`,children:[(0,Q.jsx)(`strong`,{children:`Key`}),(0,Q.jsx)(`button`,{className:`ghost`,onClick:e,"aria-label":`Close key`,children:`✕`})]}),(0,Q.jsxs)(`ul`,{children:[t((0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:`4`,y1:`9`,x2:`30`,y2:`9`,className:`msg`,style:{"--c":`var(--lane-0)`}}),(0,Q.jsx)(`path`,{d:ay(4,9,34,9),className:`arrowhead`,style:{"--c":`var(--lane-0)`}})]}),`Message, in the sender's color; it ends where the receiver handles it`),t((0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:`4`,y1:`9`,x2:`30`,y2:`9`,className:`msg pending`,style:{"--c":`var(--lane-0)`}}),(0,Q.jsx)(`path`,{d:ay(4,9,34,9),className:`arrowhead`,style:{"--c":`var(--lane-0)`}})]}),`Sent but never handled`),t((0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:`4`,y1:`9`,x2:`30`,y2:`9`,className:`msg create`}),(0,Q.jsx)(`path`,{d:ay(4,9,34,9),className:`arrowhead create`})]}),`Created a machine`),t((0,Q.jsx)(`rect`,{x:`4`,y:`2`,width:`32`,height:`14`,rx:`7`,className:`state`,style:{"--c":`var(--lane-1)`}}),`Entered a state`),t((0,Q.jsx)(`rect`,{x:`4`,y:`2`,width:`32`,height:`14`,rx:`7`,className:`state hot`}),`Spec in a hot state (must leave it eventually)`),t((0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:`4`,y1:`9`,x2:`26`,y2:`9`,className:`observe`}),(0,Q.jsx)(`rect`,{x:`26`,y:`4`,width:`10`,height:`10`,rx:`2`,transform:`rotate(45 31 9)`,className:`diamond`})]}),`Spec observed an event`),t((0,Q.jsx)(`rect`,{x:`4`,y:`3`,width:`32`,height:`12`,rx:`6`,className:`error-bar`}),`Where the bug was detected`),t((0,Q.jsx)(`rect`,{x:`4`,y:`2`,width:`32`,height:`14`,rx:`7`,className:`lane-head`,style:{"--c":`var(--lane-0)`}}),`Tap a machine's name to show only its steps`)]})]})}var hy=/([\w./-]+\.p):(\d+):(\d+)/,gy=820,_y=560;function vy({json:e,view:t,model:n,stale:r,onOpenLocation:i,onShowDiagram:a}){let o=(0,l.useMemo)(()=>Uv(e),[e]),s=(0,l.useMemo)(()=>oy(o.lanes),[o]),[c,u]=(0,l.useState)(o.steps.length-1),[d,f]=(0,l.useState)(n?`states`:`story`),[p,m]=(0,l.useState)(!1),[h,g]=(0,l.useState)(!1),[_,v]=(0,l.useState)(!1),[y,ee]=(0,l.useState)(!1),[b,x]=(0,l.useState)(new Set),[S,te]=(0,l.useState)(),ne=(0,l.useRef)(null),[re,C]=(0,l.useState)(`none`),{rows:ie,cols:ae}=(0,l.useMemo)(()=>Hv(o,b),[o,b]),w=(0,l.useMemo)(()=>d===`sequence`?ie.filter(e=>!o.steps[e].inspection):ie,[ie,d,o]),T=w.indexOf(c),oe=w.length-1;(0,l.useEffect)(()=>{u(o.steps.length-1),v(!1),x(new Set)},[o]),(0,l.useEffect)(()=>{if(T>=0||!w.length)return;let e=w.filter(e=>e<c);u(e.length?e[e.length-1]:w[0])},[T,w,c]),(0,l.useEffect)(()=>{if(!_)return;if(T>=oe){v(!1);return}let e=setTimeout(()=>u(w[Math.min(T+1,oe)]),1100);return()=>clearTimeout(e)},[_,T,oe,w]),(0,l.useLayoutEffect)(()=>{let e=ne.current;if(!e)return;let t=new ResizeObserver(()=>C(e.clientWidth>=gy?`side`:e.clientHeight>=_y&&!matchMedia(`(max-width: 900px)`).matches?`bottom`:`none`));return t.observe(e),()=>t.disconnect()},[]);let se=e=>{e!==void 0&&(v(!1),u(e))},ce=e=>se(w[Math.max(0,Math.min(e,oe))]),le=e=>x(t=>{let n=new Set(t);return n.has(e)?n.delete(e):n.add(e),n}),E=o.steps[c],ue=E?.inspection?.source;(0,l.useEffect)(()=>{h&&ue&&!r&&i(ue.file,ue.line,!1)},[h,ue,r,i]);let de=(0,l.useMemo)(()=>Av(o,c),[o,c]),fe=E?.lane===void 0?void 0:de.get(E.lane),pe=fe?.snapshot?.machine??(E?.lane===void 0?void 0:o.lanes[E.lane].id.replace(/\(.*$/,``).replace(/^.*\./,``)),me=o.error?hy.exec(o.error):null,he=(0,Q.jsx)(yy,{trace:o,rows:w,color:s,cursor:c,hover:S,onSelect:e=>{se(e),t===`log`&&a()},onHover:te});return(0,Q.jsxs)(`div`,{className:`trace`,ref:ne,children:[r&&(0,Q.jsx)(`div`,{className:`stale-trace`,role:`status`,children:`Source or test changed. This is the previous execution. Check again to replay your edits; source links are paused.`}),t===`diagram`&&(0,Q.jsxs)(`div`,{className:`replay-toolbar`,children:[(0,Q.jsxs)(`div`,{className:`view-switch`,"aria-label":`Replay view`,children:[(0,Q.jsx)(`button`,{"aria-pressed":d===`states`,onClick:()=>f(`states`),disabled:!n,children:`State diagram`}),(0,Q.jsx)(`button`,{"aria-pressed":d===`story`,onClick:()=>f(`story`),children:`Details`}),(0,Q.jsx)(`button`,{"aria-pressed":d===`sequence`,onClick:()=>f(`sequence`),children:`Messages`})]}),(0,Q.jsxs)(`label`,{className:`follow-source`,children:[(0,Q.jsx)(`input`,{type:`checkbox`,checked:h,disabled:r,onChange:e=>g(e.target.checked)}),`Follow source`]})]}),o.error&&d!==`states`&&(0,Q.jsxs)(`div`,{className:y?`trace-error open`:`trace-error`,onClick:()=>ee(!y),children:[(0,Q.jsxs)(`span`,{className:`trace-error-text`,children:[(0,Q.jsx)(`strong`,{children:`Bug:`}),` `,o.error]}),me&&(0,Q.jsxs)(`button`,{className:`link`,disabled:r,onClick:e=>{e.stopPropagation(),r||i(me[1],Number(me[2]))},children:[(0,Q.jsxs)(`span`,{className:`goto-long`,children:[`Go to `,me[1],`:`,me[2]]}),(0,Q.jsxs)(`span`,{className:`goto-short`,children:[`line `,me[2],` →`]})]})]}),t===`diagram`&&(0,Q.jsxs)(`div`,{className:`trace-controls`,children:[(0,Q.jsx)(`button`,{onClick:()=>ce(0),title:`First step`,"aria-label":`First step`,className:`jump`,children:`⏮`}),(0,Q.jsx)(`button`,{onClick:()=>ce(T-1),title:`Previous step`,"aria-label":`Previous step`,children:`◀`}),(0,Q.jsx)(`button`,{className:`primary`,onClick:()=>{!_&&T>=oe&&u(w[0]),v(!_)},title:_?`Pause`:`Play`,children:_?`Pause`:`Play`}),(0,Q.jsx)(`button`,{onClick:()=>ce(T+1),title:`Next step`,"aria-label":`Next step`,children:`▶`}),(0,Q.jsx)(`button`,{onClick:()=>ce(oe),title:`Last step`,"aria-label":`Last step`,className:`jump`,children:`⏭`}),(0,Q.jsx)(`input`,{type:`range`,min:0,max:Math.max(oe,0),value:Math.max(T,0),onChange:e=>ce(Number(e.target.value)),"aria-label":`Step`}),(0,Q.jsxs)(`span`,{className:`muted`,children:[T+1,`/`,w.length]})]}),b.size>0&&(0,Q.jsxs)(`div`,{className:`trace-focus`,children:[(0,Q.jsx)(`span`,{className:`muted`,children:`Only steps of`}),[...b].map(e=>(0,Q.jsxs)(`button`,{className:`chip`,style:{"--c":s[e]},onClick:()=>le(e),"aria-label":`Stop focusing ${o.lanes[e].id}`,children:[(0,Q.jsx)(`span`,{className:`chip-name`,children:o.lanes[e].id}),` `,(0,Q.jsx)(`span`,{"aria-hidden":`true`,children:`✕`})]},e)),(0,Q.jsx)(`button`,{className:`link`,onClick:()=>x(new Set),children:`Show all`})]}),t===`log`?he:d===`story`?(0,Q.jsx)(Mv,{sourceAvailable:!r,trace:o,cursor:c,onSelect:se,onOpen:e=>{r||i(e.file,e.line)}}):d===`states`&&n?(0,Q.jsxs)(`div`,{className:`replay-state-panel`,children:[E&&(0,Q.jsxs)(`div`,{className:`state-replay-summary ${E.kind===`error`?`failure`:``}`,"aria-live":`polite`,children:[E.kind===`error`&&(0,Q.jsx)(`strong`,{children:`Check failed`}),(0,Q.jsx)(`p`,{children:E.kind===`error`?(o.error??E.label).replace(/^Assertion Failed:\s*[^\s]+\.p:\d+:\d+\s*/,``):kv(o,E)}),E.kind===`error`&&me&&(0,Q.jsx)(`button`,{className:`link`,disabled:r,onClick:()=>i(me[1],Number(me[2])),children:`Open failed assertion`})]}),(0,Q.jsxs)(`div`,{className:`view-switch`,"aria-label":`Diagram scope`,children:[(0,Q.jsx)(`button`,{"aria-pressed":p,onClick:()=>m(!0),children:`Together`}),(0,Q.jsx)(`button`,{"aria-pressed":!p,onClick:()=>m(!1),children:`One machine`})]}),p?(0,Q.jsx)(Rv,{model:n,trace:o,cursor:c,readOnly:r,onOpen:e=>{r||i(e.file,e.line)}}):(0,Q.jsx)(zv,{failed:E?.kind===`error`,readOnly:r,model:n,activeMachine:pe,activeState:fe?.state,onOpen:e=>{r||i(e.file,e.line)}},e),fe?.snapshot&&(0,Q.jsxs)(`details`,{className:`model-reference`,children:[(0,Q.jsxs)(`summary`,{children:[`Variables in `,o.lanes[E.lane].label]}),(0,Q.jsx)(`pre`,{className:`replay-code`,children:Object.entries({...fe.snapshot.fields,...fe.snapshot.locals}).map(([e,t])=>`${e} = ${t}`).join(`
`)}),(0,Q.jsxs)(`p`,{className:`capture-note`,children:[`Last captured at step `,fe.at+1,`.`]})]})]}):(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsxs)(`div`,{className:`trace-main list-${re}`,children:[(0,Q.jsx)(sy,{trace:o,rows:w,cols:ae,color:s,focus:b,cursor:c,hover:S,onSelect:se,onHover:te,onToggleLane:le}),re!==`none`&&he]}),E&&re===`none`&&(0,Q.jsxs)(`div`,{className:`step-detail`,children:[(0,Q.jsx)(`div`,{children:E.log}),E.payload!==void 0&&(0,Q.jsx)(`pre`,{children:JSON.stringify(E.payload,null,2)})]})]})]})}function yy({trace:e,rows:t,color:n,cursor:r,hover:i,onSelect:a,onHover:o}){let s=(0,l.useRef)(null);return(0,l.useEffect)(()=>{let e=s.current,t=e?.querySelector(`[data-step="${r}"]`);if(!e||!t)return;let n=t.offsetTop-e.offsetTop;n<e.scrollTop?e.scrollTop=n-8:n+t.offsetHeight>e.scrollTop+e.clientHeight&&(e.scrollTop=n+t.offsetHeight-e.clientHeight+8)},[r,t]),(0,Q.jsx)(`ol`,{className:`step-list`,ref:s,"aria-label":`Steps`,children:t.map(t=>{let s=e.steps[t],c=[t===r&&`current`,t===i&&`hover`,s.kind===`error`&&`error`].filter(Boolean).join(` `);return(0,Q.jsx)(`li`,{"data-step":t,className:c,children:(0,Q.jsxs)(`button`,{onClick:()=>a(t),onMouseEnter:()=>o(t),onMouseLeave:()=>o(void 0),"aria-current":t===r?`step`:void 0,children:[(0,Q.jsx)(`span`,{className:`n`,children:t+1}),(0,Q.jsx)(`span`,{className:`swatch`,style:{background:s.lane===void 0?`var(--muted)`:n[s.lane]}}),(0,Q.jsx)(`span`,{className:`text`,children:s.log})]})},t)})})}function by({model:e,testCase:t,pending:n,onOpen:r}){let[i,a]=(0,l.useState)(!1);if(!e)return(0,Q.jsxs)(`div`,{className:`model-overview`,children:[(0,Q.jsx)(`h2`,{children:`State machines`}),(0,Q.jsx)(`p`,{children:n?`Reading your model…`:`Fix the source errors to update the diagrams.`})]});let o=e.tests.find(e=>e.name===t);return(0,Q.jsxs)(`div`,{className:`model-overview`,children:[(0,Q.jsxs)(`div`,{className:`model-heading`,children:[(0,Q.jsx)(`h2`,{children:`State machines`}),(0,Q.jsx)(`span`,{className:`model-hint`,children:`From your current code`})]}),(0,Q.jsxs)(`div`,{className:`view-switch`,"aria-label":`Diagram scope`,children:[(0,Q.jsx)(`button`,{"aria-pressed":i,onClick:()=>a(!0),children:`Together`}),(0,Q.jsx)(`button`,{"aria-pressed":!i,onClick:()=>a(!1),children:`One machine`})]}),i?(0,Q.jsx)(Rv,{model:e,onOpen:r}):(0,Q.jsx)(zv,{model:e,initialMachine:o?.main,onOpen:r}),(0,Q.jsxs)(`details`,{className:`model-reference`,children:[(0,Q.jsx)(`summary`,{children:`Test and assertions`}),o&&(0,Q.jsxs)(`section`,{className:`test-brief`,children:[(0,Q.jsx)(`button`,{className:`link`,onClick:()=>r(o.source),children:(0,Q.jsx)(`strong`,{children:o.name})}),(0,Q.jsxs)(`p`,{children:[`Starts with `,o.main,`.`]}),(0,Q.jsx)(`pre`,{children:o.composition})]}),(0,Q.jsx)(`div`,{className:`model-rules`,children:e.machines.flatMap(e=>e.rules.map((t,n)=>(0,Q.jsxs)(`button`,{onClick:()=>r(t.source),children:[(0,Q.jsx)(`span`,{children:e.name}),(0,Q.jsx)(`code`,{children:t.condition})]},`${e.name}-${n}`)))}),(0,Q.jsx)(`p`,{className:`model-hint`,children:`The test chooses which declarations participate. Diagrams show control flow in the source; conditions still apply.`})]})]})}function xy({size:e=34}){return(0,Q.jsx)(`svg`,{className:`pea-mark`,width:e,height:e*96/132,viewBox:`0 -12 132 96`,"aria-hidden":`true`,children:(0,Q.jsxs)(`g`,{transform:`rotate(-18 64 36)`,children:[(0,Q.jsx)(`path`,{d:`M119 34 Q129 27 126 16`,fill:`none`,stroke:`var(--pod)`,strokeWidth:`5`,strokeLinecap:`round`}),(0,Q.jsx)(`path`,{d:`M8 36 A72 72 0 0 1 120 36 A72 72 0 0 1 8 36 Z`,fill:`var(--pod)`}),(0,Q.jsx)(`circle`,{cx:`41`,cy:`36`,r:`9`,fill:`var(--pea)`}),(0,Q.jsx)(`circle`,{cx:`64`,cy:`36`,r:`11`,fill:`var(--pea)`}),(0,Q.jsx)(`circle`,{cx:`87`,cy:`36`,r:`9.5`,fill:`var(--pea)`}),(0,Q.jsxs)(`g`,{fill:`#fff`,fillOpacity:`0.85`,children:[(0,Q.jsx)(`circle`,{cx:`38.8`,cy:`31.8`,r:`2.7`}),(0,Q.jsx)(`circle`,{cx:`61.4`,cy:`30.8`,r:`3.3`}),(0,Q.jsx)(`circle`,{cx:`84.7`,cy:`31.5`,r:`2.85`})]})]})})}function Sy(){return(0,Q.jsxs)(`span`,{className:`logo`,children:[(0,Q.jsx)(xy,{}),(0,Q.jsxs)(`span`,{className:`wordmark`,children:[`snap`,(0,Q.jsx)(`span`,{children:`pea`})]})]})}var Cy=()=>Math.floor(Math.random()*1e6),wy=e=>e.split(/\s+/).filter(Boolean),Ty=`https://github.com/crowecawcaw/snappea`,Ey=`(max-width: 900px)`,Dy=500,Oy={files:`Files`,model:`Model`,console:`Console`,trace:`Trace`,log:`Log`};function ky(){let e=(0,l.useMemo)(()=>new Ov,[]),[t,r]=(0,l.useState)(p[0]),[o,s]=(0,l.useState)({}),[c,u]=(0,l.useState)(``),[d,f]=(0,l.useState)([]),[h,v]=(0,l.useState)(p[0].testCase),[y,ee]=(0,l.useState)(10),[b,x]=(0,l.useState)(Cy),[S,te]=(0,l.useState)(!1),[ne,re]=(0,l.useState)(``),[C,ie]=(0,l.useState)(!1),[T,oe]=(0,l.useState)([]),[se,ce]=(0,l.useState)(``),[le,E]=(0,l.useState)(),[ue,de]=(0,l.useState)(``),[fe,pe]=(0,l.useState)(``),[me,he]=(0,l.useState)(),D=JSON.stringify(o),ge=(0,l.useMemo)(()=>t===m?void 0:_(t),[t]),_e=ge&&(Object.keys(ge).length!==Object.keys(o).length||Object.entries(ge).some(([e,t])=>o[e]!==t)),[ve,ye]=(0,l.useState)(),[be,xe]=(0,l.useState)(!1),[O,Se]=(0,l.useState)(),[Ce,we]=(0,l.useState)(),[k,Te]=(0,l.useState)(),[Ee,De]=(0,l.useState)(()=>matchMedia(Ey).matches),[Oe,ke]=(0,l.useState)(()=>matchMedia(Ey).matches?`files`:`model`),[Ae,je]=(0,l.useState)(),[Me,Ne]=(0,l.useState)(!1),A=(0,l.useRef)(!1),[Pe,Fe]=(0,l.useState)(!1),[Ie,Le]=(0,l.useState)(!1),Re=(0,l.useRef)(null),[ze,j]=(0,l.useState)(!1),[Be,Ve]=(0,l.useState)(!1),[M,He]=(0,l.useState)(!1),Ue=(0,l.useRef)(0),We=(0,l.useRef)(null),Ge=(0,l.useCallback)((e,t,n)=>{r(e),s(t.files),u(t.files[t.open]===void 0?g(Object.keys(t.files))[0]??``:t.open),v(t.testCase??e.testCase),ee(t.timeout??10),re(t.extraArgs??e.extraArgs??``),ie(t.strict??!1),Se(void 0),we(void 0),oe([]),E(void 0),de(``),ke(matchMedia(Ey).matches?`files`:`model`),A.current=n},[]),Ke=(0,l.useCallback)((e,t=e.open)=>Ge(e,{demo:e.id,files:_(e),open:t},!1),[Ge]),qe=(0,l.useCallback)(()=>{let e=n();if(e)try{let t=a(e);Ge(p.find(e=>e.id===t.demo)??m,t,!0);return}catch{alert(`This link's project couldn't be read; it may have been cut off. Opening a demo instead.`)}let t=new URLSearchParams(location.search),r=p.find(e=>e.id===t.get(`demo`))??p[0];Ke(r,t.get(`file`)??r.open)},[Ke,Ge]);(0,l.useEffect)(()=>{let e=Re.current;if(!e)return;let t=()=>Le(e.scrollHeight>e.clientHeight+1);t();let n=new ResizeObserver(t);return n.observe(e),()=>n.disconnect()},[t]),(0,l.useEffect)(()=>{let e=matchMedia(Ey),t=()=>{De(e.matches),e.matches||ke(e=>e===`files`?`model`:e)};return e.addEventListener(`change`,t),()=>e.removeEventListener(`change`,t)},[]),(0,l.useEffect)(()=>{qe(),Ne(!0);let e=()=>{n()&&qe()};return addEventListener(`hashchange`,e),()=>removeEventListener(`hashchange`,e)},[qe]),(0,l.useEffect)(()=>{if(!Me)return;let e=setTimeout(()=>{let e={demo:t===m?void 0:t.id,files:o,open:c,testCase:h,timeout:y,extraArgs:ne||void 0,strict:C||void 0},{url:n,tooBig:r}=i(e,A.current);Ve(r),n!==location.search+location.hash&&history.replaceState(null,``,n)},Dy);return()=>clearTimeout(e)},[Me,t,o,c,h,y,ne,C]),(0,l.useEffect)(()=>{if(!Me||Object.keys(o).length===0)return;let t=!1,n=setTimeout(async()=>{try{let n=await e.compile(o);if(t)return;oe(n.diagnostics),ce(n.console),ye(n.ok),E(n.model),de(JSON.stringify(o)),n.ok&&(f(n.tests),v(e=>n.tests.includes(e)?e:n.tests[0]??``))}catch{}},400);return()=>{t=!0,clearTimeout(n)}},[o,Me,e]);let Je=(0,l.useCallback)(async()=>{if(be||!h)return;let t=S?b:Cy();x(t),xe(!0),we(void 0),Te(void 0);try{let n=await e.compile(o),r=await e.check({files:o,testCase:h,timeout:y,seed:t,strict:C,extraArgs:wy(ne)},Te);Se(r),pe(JSON.stringify(o)),he(n.model),r.diagnostics.length&&oe(r.diagnostics),ke(r.bug&&r.traceJson?`trace`:`console`)}catch(e){let t=String(e.message??e);we(t===`cancelled`?`Stopped.`:`The checker crashed: ${t}`),Se(void 0),ke(`console`)}finally{xe(!1)}},[be,h,S,b,e,o,y,C,ne]),Ye=(0,l.useCallback)((e,t,n=!0)=>{let r=e.replace(/^\/work\//,``),i=o[r]===void 0?Object.keys(o).find(e=>e.endsWith(`/`+r)||e===r):r;i&&(u(i),je({line:t,nonce:Date.now(),focus:n}),n&&matchMedia(Ey).matches&&ke(`files`))},[o]),Xe=e=>{(!A.current||confirm(`Discard your edits and load another demo?`))&&Ke(p.find(t=>t.id===e))},Ze=async e=>{let t;try{t=await e}catch(e){alert(`Couldn't open those files: ${e.message??e}`);return}if(A.current&&!confirm(`Discard your edits and open these files?`))return;let n=g(Object.keys(t));Ge(m,{files:t,open:n.find(e=>e.endsWith(`.p`))??n[0]},!0)},Qe={onDragEnter:e=>{e.dataTransfer.types.includes(`Files`)&&(e.preventDefault(),Ue.current++,He(!0))},onDragOver:e=>{e.dataTransfer.types.includes(`Files`)&&(e.preventDefault(),e.dataTransfer.dropEffect=`copy`)},onDragLeave:()=>{--Ue.current<=0&&(Ue.current=0,He(!1))},onDrop:e=>{e.dataTransfer.types.includes(`Files`)&&(e.preventDefault(),Ue.current=0,He(!1),Ze(ae(e.dataTransfer)))}},$e=()=>{let e=prompt(`New file name`,`PSrc/NewMachine.p`)?.trim();if(!e)return;let t=e.endsWith(`.p`)||e.includes(`.`)?e:`${e}.p`;o[t]===void 0&&s({...o,[t]:``}),u(t),A.current=!0},et=(0,l.useMemo)(()=>g(Object.keys(o)),[o]),tt=(0,l.useMemo)(()=>{let e=new Map;for(let t of T)e.set(t.file,(e.get(t.file)??0)+1);return e},[T]),nt=!!O&&(fe!==D||O.testCase!==h),rt=be?k?Py(k):`Checking…`:Ce||(ve===!1||O&&!O.ok?`Compile error`:nt?`Previous run · source or test changed`:O?O.bug?`Bug found`:O.exitCode===0?`No bugs found (see console for completion)`:`Error (see console)`:ve?`Ready`:`Loading…`),it=be||nt?``:ve===!1||O?.bug||O&&(!O.ok||O.exitCode!==0)?`bad`:O?.ok?`good`:``,at=(0,Q.jsxs)(`div`,{className:Pe?`blurb open`:`blurb`,children:[(0,Q.jsx)(`p`,{ref:Re,children:_e?`You are editing “${t.title}”. Explore the current source in Model, or Check again to find and replay its executions.`:t.blurb}),(Ie||Pe)&&(0,Q.jsx)(`button`,{className:`more`,onClick:()=>Fe(!Pe),children:Pe?`less`:`more`})]}),ot=!be&&(ve===!1||O!==void 0&&!O.ok)&&T.length>0,st=Ee?be||!!Ce||ot:be||!!O||!!Ce||ve===!1,ct=O?.schedule&&!be&&!nt&&(0,Q.jsxs)(`span`,{className:`downloads`,children:[(0,Q.jsx)(Ny,{name:`${O.testCase}.schedule`,text:O.schedule}),O.traceText&&(0,Q.jsx)(Ny,{name:`${O.testCase}.txt`,text:O.traceText}),O.traceJson&&(0,Q.jsx)(Ny,{name:`${O.testCase}.trace.json`,text:O.traceJson})]});return(0,Q.jsxs)(`div`,{className:`app`,...Qe,children:[M&&(0,Q.jsx)(`div`,{className:`drop-overlay`,"aria-hidden":`true`,children:(0,Q.jsx)(`div`,{children:`Drop .p files, a folder or a .zip to open them`})}),(0,Q.jsxs)(`header`,{className:`top`,children:[(0,Q.jsxs)(`a`,{className:`brand`,href:`./`,"aria-label":`Snap Pea home`,children:[(0,Q.jsx)(Sy,{}),(0,Q.jsx)(`span`,{className:`brand-name`,children:`Playground`})]}),(0,Q.jsxs)(`select`,{className:`demo-select`,value:t.id,onChange:e=>Xe(e.target.value),"aria-label":`Demo`,children:[t===m&&(0,Q.jsx)(`option`,{value:m.id,children:m.title}),p.map(e=>(0,Q.jsx)(`option`,{value:e.id,children:e.title},e.id))]}),(0,Q.jsx)(`button`,{className:`ghost open-files`,onClick:()=>We.current?.click(),title:`Open .p files or a .zip (or drop them anywhere)`,children:`Open files`}),(0,Q.jsx)(`input`,{ref:We,type:`file`,multiple:!0,accept:`.p,.zip,.toml,.json,.txt,.md`,hidden:!0,onChange:e=>{let t=e.target.files;t?.length&&Ze(w(t)),e.target.value=``}}),Be&&(0,Q.jsx)(`span`,{className:`share-note`,title:`The project is over 32k characters compressed, so the page URL no longer holds your edits.`,children:`Too big to link`}),(0,Q.jsx)(`div`,{className:`spacer`}),(0,Q.jsx)(`a`,{className:`button ghost docs-link`,href:`./docs/overview.html`,children:`Docs`}),(0,Q.jsxs)(`a`,{className:`button ghost gh`,href:Ty,target:`_blank`,rel:`noreferrer`,"aria-label":`GitHub`,children:[(0,Q.jsx)(`svg`,{width:`20`,height:`20`,viewBox:`0 0 16 16`,"aria-hidden":`true`,fill:`currentColor`,children:(0,Q.jsx)(`path`,{d:`M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z`})}),(0,Q.jsx)(`span`,{className:`gh-text`,children:`GitHub`})]})]}),!Ee&&at,(0,Q.jsxs)(`main`,{className:`panes tab-${Oe}`,children:[(0,Q.jsxs)(`section`,{className:`pane editor-pane`,children:[Ee&&at,(0,Q.jsxs)(`div`,{className:`file-tabs`,role:`tablist`,children:[et.map(e=>{let t=e.lastIndexOf(`/`),n=tt.get(e);return(0,Q.jsxs)(`button`,{role:`tab`,"aria-selected":e===c,className:e===c?`file-tab active`:`file-tab`,onClick:()=>u(e),title:e,children:[t>=0&&(0,Q.jsx)(`span`,{className:`dir`,children:e.slice(0,t+1)}),e.slice(t+1),n?(0,Q.jsx)(`span`,{className:`badge`,children:n}):null]},e)}),(0,Q.jsx)(`button`,{className:`file-tab add`,onClick:$e,title:`New file`,"aria-label":`New file`,children:`+`})]}),c&&o[c]!==void 0&&(0,Q.jsx)(Dv,{path:c,value:o[c],diagnostics:T.filter(e=>e.file===c),jump:Ae,onChange:(e,t)=>{A.current=!0,s(n=>({...n,[e]:t}))},onRun:Je})]}),(0,Q.jsxs)(`section`,{className:`pane run-pane`,children:[(0,Q.jsxs)(`div`,{className:ze?`controls options-open`:`controls`,children:[(0,Q.jsxs)(`div`,{className:`run-bar`,children:[(0,Q.jsxs)(`label`,{className:`tc`,children:[(0,Q.jsx)(`span`,{className:`lbl`,children:`Test case`}),(0,Q.jsxs)(`select`,{value:h,onChange:e=>v(e.target.value),disabled:!d.length,"aria-label":`Test case`,children:[d.length===0&&(0,Q.jsx)(`option`,{value:h,children:h||`—`}),d.map(e=>(0,Q.jsx)(`option`,{value:e,children:e},e))]})]}),be?(0,Q.jsx)(`button`,{className:`danger run`,onClick:()=>e.cancel(),children:`Stop`}):(0,Q.jsx)(`button`,{className:`primary run`,onClick:Je,disabled:!h||ve===!1,title:`Ctrl/⌘+Enter`,children:ve===!1?T.length===1?`Fix 1 error`:T.length?`Fix ${T.length} errors`:`Fix errors`:`Check`})]}),(0,Q.jsxs)(`details`,{className:`options`,open:ze,onToggle:e=>j(e.target.open),children:[(0,Q.jsxs)(`summary`,{children:[(0,Q.jsx)(`svg`,{className:`chevron`,width:`10`,height:`10`,viewBox:`0 0 10 10`,"aria-hidden":`true`,children:(0,Q.jsx)(`path`,{d:`M3 1.5 L6.5 5 L3 8.5`,fill:`none`,stroke:`currentColor`,strokeWidth:`1.6`,strokeLinecap:`round`,strokeLinejoin:`round`})}),`Options`,(0,Q.jsxs)(`span`,{className:`muted`,children:[` `,`· up to `,y,`s`,S?` · seed ${b}`:``,C?` · strict`:``,ne.trim()?` · ${ne.trim()}`:``]})]}),(0,Q.jsxs)(`div`,{className:`options-grid`,children:[(0,Q.jsxs)(`label`,{children:[`Timeout (seconds)`,(0,Q.jsx)(`input`,{type:`number`,inputMode:`numeric`,min:1,max:3600,value:y,onChange:e=>ee(Math.min(3600,Math.max(1,Math.trunc(Number(e.target.value))||1)))})]}),(0,Q.jsxs)(`label`,{children:[`Seed`,(0,Q.jsx)(`input`,{type:`number`,inputMode:`numeric`,value:b,onChange:e=>{x(Number(e.target.value)||0),te(!0)}})]}),(0,Q.jsxs)(`label`,{className:`check`,children:[(0,Q.jsx)(`input`,{type:`checkbox`,checked:S,onChange:e=>te(e.target.checked)}),`Keep seed`]}),(0,Q.jsxs)(`label`,{className:`check`,title:`Fail on lint findings and missing coverage, as the snappea CLI does. Off: they are shown as warnings and the check goes on.`,children:[(0,Q.jsx)(`input`,{type:`checkbox`,checked:C,onChange:e=>ie(e.target.checked)}),`Strict checks`]}),(0,Q.jsxs)(`label`,{className:`span-all`,children:[`Extra flags`,(0,Q.jsx)(`input`,{value:ne,placeholder:`e.g. --timed`,onChange:e=>re(e.target.value),spellCheck:!1,autoCapitalize:`off`,autoCorrect:`off`})]})]})]})]}),(0,Q.jsxs)(`div`,{className:`out-head`,children:[(0,Q.jsxs)(`div`,{className:`out-tabs`,role:`tablist`,children:[(Ee?[`files`,`model`,`console`,`trace`,`log`]:[`model`,`console`,`trace`,`log`]).map(e=>(0,Q.jsxs)(`button`,{role:`tab`,"aria-selected":Oe===e,className:Oe===e?`out-tab active`:`out-tab`,onClick:()=>ke(e),disabled:(e===`trace`||e===`log`)&&!O?.traceJson,children:[Oy[e],e===`files`&&T.length>0&&(0,Q.jsx)(`span`,{className:`badge`,children:T.length}),e===`trace`&&O?.bug&&(0,Q.jsx)(`span`,{className:`bug-dot`,"aria-label":`bug found`})]},e)),Ee&&(0,Q.jsx)(`button`,{className:ze?`out-tab gear active`:`out-tab gear`,onClick:()=>j(!ze),"aria-expanded":ze,"aria-label":`Checker options`,title:`Checker options`,children:(0,Q.jsxs)(`svg`,{width:`20`,height:`20`,viewBox:`0 0 24 24`,"aria-hidden":`true`,fill:`none`,stroke:`currentColor`,strokeWidth:`1.8`,strokeLinecap:`round`,strokeLinejoin:`round`,children:[(0,Q.jsx)(`path`,{d:`M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0`}),(0,Q.jsx)(`circle`,{cx:`16`,cy:`6`,r:`2`}),(0,Q.jsx)(`circle`,{cx:`10`,cy:`12`,r:`2`}),(0,Q.jsx)(`circle`,{cx:`18`,cy:`18`,r:`2`})]})})]}),st&&(0,Q.jsxs)(`div`,{className:`status-line`,children:[(0,Q.jsx)(`span`,{className:`status ${it}`,children:rt}),be&&!!k?.time_limit&&(0,Q.jsx)(`span`,{className:`progress`,role:`progressbar`,"aria-label":`Time used`,"aria-valuemin":0,"aria-valuemax":100,"aria-valuenow":Math.round(Math.min(1,k.elapsed/k.time_limit)*100),children:(0,Q.jsx)(`span`,{style:{width:`${Math.min(1,k.elapsed/k.time_limit)*100}%`}})}),!be&&(ve===!1||O&&!O.ok)&&T[0]&&(0,Q.jsxs)(`button`,{className:`diag`,onClick:()=>Ye(T[0].file,T[0].line),children:[T[0].file,`:`,T[0].line,` `,T[0].message]}),O&&!be&&!ot&&(0,Q.jsxs)(`span`,{className:`muted`,children:[(O.millis/1e3).toFixed(2),` s`]}),!Ee&&ct]})]}),(0,Q.jsxs)(`div`,{className:`output`,children:[Oe===`model`&&(0,Q.jsx)(by,{model:ue===D?le:void 0,testCase:h,pending:ue!==D,onOpen:e=>Ye(e.file,e.line)}),Oe===`console`&&(0,Q.jsx)(My,{summary:!Ce&&O?.ok&&(O.bug||O.exitCode===0)?O.bug?`✕ ${jy(O)}`:`✓ No bugs found in ${O.testCase}`:void 0,bad:O?.bug,text:Ce??(O?O.console:ve===!1?se:``),onOpenLocation:Ye,empty:ve===!1?``:`Press Check (Ctrl/⌘+Enter) to explore schedules of the selected test case.`}),(Oe===`trace`||Oe===`log`)&&O?.traceJson&&(0,Q.jsx)(vy,{json:O.traceJson,model:me,stale:nt,view:Oe===`log`?`log`:`diagram`,onOpenLocation:Ye,onShowDiagram:()=>ke(`trace`)})]})]})]})]})}var Ay=/([\w./-]+\.p):(\d+)(?::(\d+))?/g;function jy(e){let t=`${e.traceText??``}\n${e.console}`.split(`
`).find(e=>/<ErrorLog>|Assertion Failed|Deadlock|liveness|Exception/i.test(e));return t?t.replace(/^.*?<ErrorLog>\s*/,``).trim():`Bug found`}function My({text:e,empty:t,summary:n,bad:r,onOpenLocation:i}){return e?(0,Q.jsxs)(`pre`,{className:`console`,children:[n&&(0,Q.jsx)(`div`,{className:r?`summary bad`:`summary good`,children:n}),e.split(`
`).map((e,t)=>{let n=/^FAIL\s|\bfailed in\b|Error|Assertion Failed|\bbug found\b|found a bug|Found [1-9]/i.test(e)?`line bad`:/^ok\s|\bpassed in\b|Found 0 bug/.test(e)?`line good`:e.startsWith(`$ `)?`line cmd`:`line`,r=[],a=0;for(let t of e.matchAll(Ay)){r.push(e.slice(a,t.index));let[n,o,s]=t;r.push((0,Q.jsx)(`button`,{className:`loc`,onClick:()=>i(o,Number(s)),children:n},t.index)),a=(t.index??0)+n.length}return r.push(e.slice(a)),(0,Q.jsx)(`div`,{className:n,children:r},t)})]}):(0,Q.jsx)(`div`,{className:`console empty`,children:t})}function Ny({name:e,text:t}){let n=(0,l.useMemo)(()=>URL.createObjectURL(new Blob([t],{type:`text/plain`})),[t]);return(0,l.useEffect)(()=>()=>URL.revokeObjectURL(n),[n]),(0,Q.jsxs)(`a`,{className:`download`,href:n,download:e,title:`Download ${e}`,children:[`⤓ `,e.slice(e.indexOf(`.`)+1)]})}function Py(e){let t=e.time_limit?`${e.elapsed.toFixed(1)} of ${e.time_limit} s`:`${e.elapsed.toFixed(1)} s`;if(e.phase===`mutation`)return`Mutation testing: ${e.detected} of ${e.mutants} mutants detected · ${t}`;let n=e.schedules.toLocaleString(`en-US`),r=e.bugs?` · ${e.bugs} bug${e.bugs===1?``:`s`}`:``;return`Checking ${e.test_case}: ${n} schedule${e.schedules===1?``:`s`}${r} · ${t}`}(0,u.createRoot)(document.getElementById(`root`)).render((0,Q.jsx)(l.StrictMode,{children:(0,Q.jsx)(ky,{})}));