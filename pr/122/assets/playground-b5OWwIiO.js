import"./styles-BXqT4JuN.js";import{t as e}from"./react-B967SsYR.js";import{t}from"./client-qLOb9Lxu.js";import{a as n,c as r,o as i,r as a,s as o,t as s}from"./pTokens-iJb68ucT.js";import{t as c}from"./jsx-runtime-BCv6A4_Z.js";var l=e(),u=t(),d=Object.assign({"../../design/time/RetryBudget/PSpec/Budget.p":`// The budget is an ordinary spec. In a spec handler, clocks are those of the
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
`}),f=`../../tests/upstream/Tutorial/`,p=[{id:`lost-update`,title:`Hello P: a lost update`,blurb:`Two clients increment a shared counter with a read and then a write. Check it to find the interleaving that loses an update, then step through the trace.`,roots:[{from:`../demos/hello/`}],open:`LostUpdate.p`,testCase:`tcTwoClients`},{id:`client-server`,title:`Bank client/server`,blurb:`Clients withdraw money from a bank server backed by a database. A safety spec checks balances never drop below 10; a liveness spec checks every request gets a reply. tcAbstractServer uses a deliberately buggy abstract bank.`,roots:[{from:`${f}1_ClientServer/`}],open:`PSrc/Server.p`,testCase:`tcAbstractServer`},{id:`two-phase-commit`,title:`Two-phase commit`,blurb:`A coordinator runs two-phase commit over participants, with timers and an optional failure injector. The Atomicity spec checks every participant agrees on each transaction.`,roots:[{from:`${f}2_TwoPhaseCommit/`},{from:`${f}Common/Timer/`,to:`Common/Timer/`},{from:`${f}Common/FailureInjector/`,to:`Common/FailureInjector/`}],open:`PSrc/Coordinator.p`,testCase:`tcMultipleClientsWithFailure`},{id:`espresso`,title:`Espresso machine`,blurb:`A coffee maker and its control panel, driven by sane and 'crazy' users pressing buttons in any order. The spec checks the machine never brews in an invalid state.`,roots:[{from:`${f}3_EspressoMachine/`}],open:`PSrc/CoffeeMakerControlPanel.p`,testCase:`tcCrazyUserUsingCoffeeMachine`},{id:`failure-detector`,title:`Failure detector`,blurb:`A heartbeat-based failure detector with nodes that can crash. The spec checks that only crashed nodes are ever reported as failed.`,roots:[{from:`${f}4_FailureDetector/`},{from:`${f}Common/Timer/`,to:`Common/Timer/`},{from:`${f}Common/FailureInjector/`,to:`Common/FailureInjector/`}],open:`PSrc/FailureDetector.p`,testCase:`tcTest_FailureDetector`},{id:`paxos`,title:`Single-decree Paxos`,blurb:`Proposers, acceptors and learners. The spec checks at most one value is ever chosen.`,roots:[{from:`${f}5_Paxos/`}],open:`PSrc/proposer.p`,testCase:`testBasicPaxos3on3`},{id:`raft`,title:`Raft`,blurb:`Leader election and log replication with unreliable networks, checked against safety (one leader per term, log matching, …) and liveness specs. Larger: allow more time to explore.`,roots:[{from:`${f}6_Raft/`}],open:`PSrc/Server.p`,testCase:`oneClientThreeServersUnreliable`},{id:`retry-budget`,title:`Time: retry budget`,blurb:`Uses the snappea time extension: a client retries with timeouts and backoff, and a spec asserts every request resolves within a 1 s budget. Set timeoutAfter = ms(250) in PSrc/RetryClient.p to fix it.`,roots:[{from:`../../design/time/RetryBudget/`}],open:`PSrc/RetryClient.p`,testCase:`tcRetry`,extraArgs:`--latency PTst/latency.toml --timed`}],m={id:`custom`,title:`Your files`,blurb:`Files you opened or that came from a shared link. Edits are kept in the page URL, so copy it to share this project. Drop .p files, a folder or a .zip anywhere to open others.`,roots:[],open:``,testCase:``},h=[`PSrc/`,`PSpec/`,`PTst/`,`Common/`];function g(e){let t=e=>{let t=h.findIndex(t=>e.startsWith(t));return t<0?e.includes(`/`)?h.length:-1:t};return[...e].sort((e,n)=>t(e)-t(n)||e.localeCompare(n))}function _(e){let t={};for(let n of e.roots)for(let[e,r]of Object.entries(d))e.startsWith(n.from)&&(t[(n.to??``)+e.slice(n.from.length)]=r);return t}var v=/\.(p|toml|json|txt|md)$/i,y=/(^|\/)(\.[^/]*|__MACOSX|PCheckerOutput|PGenerated|node_modules|target)(\/|$)/,ee=500,b=2e6;function x(e){return v.test(e)&&!y.test(e)}async function te(e,t,n){let i=r(e,{filter:e=>x(e.name)&&e.originalSize<=b});for(let[e,r]of Object.entries(i))n.push({path:t+e,read:async()=>r})}async function ne(e,t,n){/\.zip$/i.test(e.name)?await te(new Uint8Array(await e.arrayBuffer()),t.replace(/[^/]*$/,``),n):x(t)&&e.size<=b&&n.push({path:t,read:async()=>new Uint8Array(await e.arrayBuffer())})}function re(e){let t=e.createReader(),n=[];return new Promise((e,r)=>{let i=()=>t.readEntries(t=>{if(!t.length)return e(n);n.push(...t),i()},r);i()})}async function ie(e,t){let n=e.fullPath.replace(/^\//,``);if(e.isDirectory){if(y.test(n))return;for(let n of await re(e))await ie(n,t)}else await ne(await new Promise((t,n)=>e.file(t,n)),n,t)}function S(e){let t=0;for(;;){let n=e[0].slice(t).split(`/`);if(n.length<2)break;let r=n[0]+`/`;if(!e.every(e=>e.slice(t).startsWith(r)))break;t+=r.length}return e=>e.slice(t)}async function C(e){if(e.length>ee)throw Error(`Too many files (${e.length}); the limit is ${ee}.`);if(!e.some(e=>/\.p$/i.test(e.path)))throw Error(`No .p files found.`);let t=S(e.map(e=>e.path)),n={};for(let r of e)n[t(r.path)]=o(await r.read());return n}async function w(e){let t=[],n=[...e.items].map(e=>e.webkitGetAsEntry?.()).filter(e=>!!e),r=[...e.files];if(n.length)for(let e of n)await ie(e,t);else for(let e of r)await ne(e,e.name,t);return C(t)}async function ae(e){let t=[];for(let n of[...e])await ne(n,n.webkitRelativePath||n.name,t);return C(t)}var T=[],oe=[];(()=>{let e=`lc,34,7n,7,7b,19,,,,2,,2,,,20,b,1c,l,g,,2t,7,2,6,2,2,,4,z,,u,r,2j,b,1m,9,9,,o,4,,9,,3,,5,17,3,1n,9,16,o,,x,1i,3,,i,,7,a,2,t,3,1k,,,7,2,2,2,3,9,,a,2,q,,2,3,1k,,,5,4,2,2,3,3,,u,2,3,,b,3,1k,,,8,,3,,3,k,2,m,6,,3,1k,,,7,2,2,2,3,7,3,a,2,u,,1n,5,3,3,,4,9,,14,5,1j,,,7,,3,,4,7,2,b,2,t,3,1k,,,7,,3,,4,7,2,b,2,f,,c,4,1j,2,,7,,3,,4,9,,a,2,t,3,1y,,4,6,,,,8,i,2,1p,,,8,c,8,2q,,,a,b,7,21,2,r,,,,,,4,2,1d,k,,2,5,b,,10,9,,2u,b,,6,n,4,4,3,g,4,d,,,3,6,,f,,jj,3,qa,4,s,3,t,2,u,2,1s,w,9,,19,3,,,39,2,y,,3a,c,4,c,63,5,1l,a,,,,,2,o,2,,1c,1a,2,c,k,5,1b,h,12,9,c,3,u,d,1k,e,1c,k,48,3,,l,4,,6,,2,3,5i,1s,ek,,5f,x,2da,3,3x,,2o,w,fe,6,2x,2,n9w,4,,a,w,2,28,2,7k,,3,,4,,n,5,4,,2b,2,1e,i,q,i,d,,12,8,p,d,18,4,1b,e,10,,1v,e,c,,8,2,1a,,1f,,,3,2,2,5,2,,,15,5,5,2,6k,8,,2,fn4,,kh,g,g,g,a6,2,gt,,6a,,45,5,1ae,3,,2,5,4,14,3,4,,4l,2,fx,4,1t,5,8t,2,25,6,1y,b,1d,4,3e,3,1h,f,15,,2,2,a,4,19,b,7,,1p,3,10,e,g,2,18,,c,3,1c,e,8,4,,2,2k,c,6,,2,,4d,c,l,4,1j,2,,7,2,2,2,3,9,,a,2,2,7,3,5,1v,9,,,2,,,4,,5,,,e,2,2a,i,n,,29,k,6j,7,2,9,r,2,2a,h,2y,d,2t,3,2,a,74,f,6t,6,,2,2,4,,,,2,3x,7,2,7,3,,s,a,14,7,,4,8,,9,b,1a,g,5i,8,5j,8,,8,2a,m,,e,3e,6,3,,,2,,7,,,1u,5,,2,,5,9n,4,9,2,,,1c,7,3,5,n,,44l,,6,f,8ug,i,1xc,5,1n,7,t4,,,1j,7,4,29,,b,2,f57,2,3mp,1a,2,n,f2,5,3,6,8,8,2,7,u,4,44,3,1iz,1j,4,1e,8,,e,,m,5,,f,11s,7,,h,2,7,,2,,5,2s,,4g,7,af,,1p,4,e4,4,72,2,6r,,2,,7,2,5,,d6,7,31,7,240,5`.split(`,`).map(e=>e?parseInt(e,36):1);for(let t=0,n=0;t<e.length;t++)(t%2?oe:T).push(n+=e[t])})();function se(e){if(e<768)return!1;for(let t=0,n=T.length;;){let r=t+n>>1;if(e<T[r])n=r;else if(e>=oe[r])t=r+1;else return!0;if(t==n)return!1}}function ce(e){return e>=127462&&e<=127487}var le=8205;function ue(e,t,n=!0,r=!0){return(n?de:fe)(e,t,r)}function de(e,t,n){if(t==e.length)return t;t&&me(e.charCodeAt(t))&&he(e.charCodeAt(t-1))&&t--;let r=pe(e,t);for(t+=ge(r);t<e.length;){let i=pe(e,t);if(r==le||i==le||n&&se(i))t+=ge(i),r=i;else if(ce(i)){let n=0,r=t-2;for(;r>=0&&ce(pe(e,r));)n++,r-=2;if(n%2==0)break;t+=2}else break}return t}function fe(e,t,n){for(;t>1;){let r=de(e,t-2,n);if(r<t)return r;t--}return 0}function pe(e,t){let n=e.charCodeAt(t);if(!he(n)||t+1==e.length)return n;let r=e.charCodeAt(t+1);return me(r)?(n-55296<<10)+(r-56320)+65536:n}function me(e){return e>=56320&&e<57344}function he(e){return e>=55296&&e<56320}function ge(e){return e<65536?1:2}var E=class e{lineAt(e){if(e<0||e>this.length)throw RangeError(`Invalid position ${e} in document of length ${this.length}`);return this.lineInner(e,!1,1,0)}line(e){if(e<1||e>this.lines)throw RangeError(`Invalid line number ${e} in ${this.lines}-line document`);return this.lineInner(e,!0,1,0)}replace(e,t,n){[e,t]=Te(this,e,t);let r=[];return this.decompose(0,e,r,2),n.length&&n.decompose(0,n.length,r,3),this.decompose(t,this.length,r,1),ve.from(r,this.length-(t-e)+n.length)}append(e){return this.replace(this.length,this.length,e)}slice(e,t=this.length){[e,t]=Te(this,e,t);let n=[];return this.decompose(e,t,n,0),ve.from(n,t-e)}eq(e){if(e==this)return!0;if(e.length!=this.length||e.lines!=this.lines)return!1;let t=this.scanIdentical(e,1),n=this.length-this.scanIdentical(e,-1),r=new Se(this),i=new Se(e);for(let e=t,a=t;;){if(r.next(e),i.next(e),e=0,r.lineBreak!=i.lineBreak||r.done!=i.done||r.value!=i.value)return!1;if(a+=r.value.length,r.done||a>=n)return!0}}iter(e=1){return new Se(this,e)}iterRange(e,t=this.length){return new D(this,e,t)}iterLines(e,t){let n;if(e==null)n=this.iter();else{t??=this.lines+1;let r=this.line(e).from;n=this.iterRange(r,Math.max(r,t==this.lines+1?this.length:t<=1?0:this.line(t-1).to))}return new Ce(n)}toString(){return this.sliceString(0)}toJSON(){let e=[];return this.flatten(e),e}constructor(){}static of(t){if(t.length==0)throw RangeError(`A document must have at least one line`);return t.length==1&&!t[0]?e.empty:t.length<=32?new _e(t):ve.from(_e.split(t,[]))}},_e=class e extends E{constructor(e,t=ye(e)){super(),this.text=e,this.length=t}get lines(){return this.text.length}get children(){return null}lineInner(e,t,n,r){for(let i=0;;i++){let a=this.text[i],o=r+a.length;if((t?n:o)>=e)return new we(r,o,n,a);r=o+1,n++}}decompose(t,n,r,i){let a=t<=0&&n>=this.length?this:new e(xe(this.text,t,n),Math.min(n,this.length)-Math.max(0,t));if(i&1){let t=r.pop(),n=be(a.text,t.text.slice(),0,a.length);if(n.length<=32)r.push(new e(n,t.length+a.length));else{let t=n.length>>1;r.push(new e(n.slice(0,t)),new e(n.slice(t)))}}else r.push(a)}replace(t,n,r){if(!(r instanceof e))return super.replace(t,n,r);[t,n]=Te(this,t,n);let i=be(this.text,be(r.text,xe(this.text,0,t)),n),a=this.length+r.length-(n-t);return i.length<=32?new e(i,a):ve.from(e.split(i,[]),a)}sliceString(e,t=this.length,n=`
`){[e,t]=Te(this,e,t);let r=``;for(let i=0,a=0;i<=t&&a<this.text.length;a++){let o=this.text[a],s=i+o.length;i>e&&a&&(r+=n),e<s&&t>i&&(r+=o.slice(Math.max(0,e-i),t-i)),i=s+1}return r}flatten(e){for(let t of this.text)e.push(t)}scanIdentical(){return 0}static split(t,n){let r=[],i=-1;for(let a of t)r.push(a),i+=a.length+1,r.length==32&&(n.push(new e(r,i)),r=[],i=-1);return i>-1&&n.push(new e(r,i)),n}},ve=class e extends E{constructor(e,t){super(),this.children=e,this.length=t,this.lines=0;for(let t of e)this.lines+=t.lines}lineInner(e,t,n,r){for(let i=0;;i++){let a=this.children[i],o=r+a.length,s=n+a.lines-1;if((t?s:o)>=e)return a.lineInner(e,t,n,r);r=o+1,n=s+1}}decompose(e,t,n,r){for(let i=0,a=0;a<=t&&i<this.children.length;i++){let o=this.children[i],s=a+o.length;if(e<=s&&t>=a){let i=r&(a<=e|(s>=t?2:0));a>=e&&s<=t&&!i?n.push(o):o.decompose(e-a,t-a,n,i)}a=s+1}}replace(t,n,r){if([t,n]=Te(this,t,n),r.lines<this.lines)for(let i=0,a=0;i<this.children.length;i++){let o=this.children[i],s=a+o.length;if(t>=a&&n<=s){let c=o.replace(t-a,n-a,r),l=this.lines-o.lines+c.lines;if(c.lines<l>>4&&c.lines>l>>6){let a=this.children.slice();return a[i]=c,new e(a,this.length-(n-t)+r.length)}return super.replace(a,s,c)}a=s+1}return super.replace(t,n,r)}sliceString(e,t=this.length,n=`
`){[e,t]=Te(this,e,t);let r=``;for(let i=0,a=0;i<this.children.length&&a<=t;i++){let o=this.children[i],s=a+o.length;a>e&&i&&(r+=n),e<s&&t>a&&(r+=o.sliceString(e-a,t-a,n)),a=s+1}return r}flatten(e){for(let t of this.children)t.flatten(e)}scanIdentical(t,n){if(!(t instanceof e))return 0;let r=0,[i,a,o,s]=n>0?[0,0,this.children.length,t.children.length]:[this.children.length-1,t.children.length-1,-1,-1];for(;;i+=n,a+=n){if(i==o||a==s)return r;let e=this.children[i],c=t.children[a];if(e!=c)return r+e.scanIdentical(c,n);r+=e.length+1}}static from(t,n=t.reduce((e,t)=>e+t.length+1,-1)){let r=0;for(let e of t)r+=e.lines;if(r<32){let e=[];for(let n of t)n.flatten(e);return new _e(e,n)}let i=Math.max(32,r>>5),a=i<<1,o=i>>1,s=[],c=0,l=-1,u=[];function d(t){let n;if(t.lines>a&&t instanceof e)for(let e of t.children)d(e);else t.lines>o&&(c>o||!c)?(f(),s.push(t)):t instanceof _e&&c&&(n=u[u.length-1])instanceof _e&&t.lines+n.lines<=32?(c+=t.lines,l+=t.length+1,u[u.length-1]=new _e(n.text.concat(t.text),n.length+1+t.length)):(c+t.lines>i&&f(),c+=t.lines,l+=t.length+1,u.push(t))}function f(){c!=0&&(s.push(u.length==1?u[0]:e.from(u,l)),l=-1,c=u.length=0)}for(let e of t)d(e);return f(),s.length==1?s[0]:new e(s,n)}};E.empty=new _e([``],0);function ye(e){let t=-1;for(let n of e)t+=n.length+1;return t}function be(e,t,n=0,r=1e9){for(let i=0,a=0,o=!0;a<e.length&&i<=r;a++){let s=e[a],c=i+s.length;c>=n&&(c>r&&(s=s.slice(0,r-i)),i<n&&(s=s.slice(n-i)),o?(t[t.length-1]+=s,o=!1):t.push(s)),i=c+1}return t}function xe(e,t,n){return be(e,[``],t,n)}var Se=class{constructor(e,t=1){this.dir=t,this.done=!1,this.lineBreak=!1,this.value=``,this.nodes=[e],this.offsets=[t>0?1:(e instanceof _e?e.text.length:e.children.length)<<1]}nextInner(e,t){for(this.done=this.lineBreak=!1;;){let n=this.nodes.length-1,r=this.nodes[n],i=this.offsets[n],a=i>>1,o=r instanceof _e?r.text.length:r.children.length;if(a==(t>0?o:0)){if(n==0)return this.done=!0,this.value=``,this;t>0&&this.offsets[n-1]++,this.nodes.pop(),this.offsets.pop()}else if((i&1)==(t>0?0:1)){if(this.offsets[n]+=t,e==0)return this.lineBreak=!0,this.value=`
`,this;e--}else if(r instanceof _e){let i=r.text[a+(t<0?-1:0)];if(this.offsets[n]+=t,i.length>Math.max(0,e))return this.value=e==0?i:t>0?i.slice(e):i.slice(0,i.length-e),this;e-=i.length}else{let i=r.children[a+(t<0?-1:0)];e>i.length?(e-=i.length,this.offsets[n]+=t):(t<0&&this.offsets[n]--,this.nodes.push(i),this.offsets.push(t>0?1:(i instanceof _e?i.text.length:i.children.length)<<1))}}}next(e=0){return e<0&&(this.nextInner(-e,-this.dir),e=this.value.length),this.nextInner(e,this.dir)}},D=class{constructor(e,t,n){this.value=``,this.done=!1,this.cursor=new Se(e,t>n?-1:1),this.pos=t>n?e.length:0,this.from=Math.min(t,n),this.to=Math.max(t,n)}nextInner(e,t){if(t<0?this.pos<=this.from:this.pos>=this.to)return this.value=``,this.done=!0,this;e+=Math.max(0,t<0?this.pos-this.to:this.from-this.pos);let n=t<0?this.pos-this.from:this.to-this.pos;e>n&&(e=n),n-=e;let{value:r}=this.cursor.next(e);return this.pos+=(r.length+e)*t,this.value=r.length<=n?r:t<0?r.slice(r.length-n):r.slice(0,n),this.done=!this.value,this}next(e=0){return e<0?e=Math.max(e,this.from-this.pos):e>0&&(e=Math.min(e,this.to-this.pos)),this.nextInner(e,this.cursor.dir)}get lineBreak(){return this.cursor.lineBreak&&this.value!=``}},Ce=class{constructor(e){this.inner=e,this.afterBreak=!0,this.value=``,this.done=!1}next(e=0){let{done:t,lineBreak:n,value:r}=this.inner.next(e);return t&&this.afterBreak?(this.value=``,this.afterBreak=!1):t?(this.done=!0,this.value=``):n?this.afterBreak?this.value=``:(this.afterBreak=!0,this.next()):(this.value=r,this.afterBreak=!1),this}get lineBreak(){return!1}};typeof Symbol<`u`&&(E.prototype[Symbol.iterator]=function(){return this.iter()},Se.prototype[Symbol.iterator]=D.prototype[Symbol.iterator]=Ce.prototype[Symbol.iterator]=function(){return this});var we=class{constructor(e,t,n,r){this.from=e,this.to=t,this.number=n,this.text=r}get length(){return this.to-this.from}};function Te(e,t,n){return t=Math.max(0,Math.min(e.length,t)),[t,Math.max(t,Math.min(e.length,n))]}function O(e,t,n=!0,r=!0){return ue(e,t,n,r)}function Ee(e){return e>=56320&&e<57344}function De(e){return e>=55296&&e<56320}function Oe(e,t){let n=e.charCodeAt(t);if(!De(n)||t+1==e.length)return n;let r=e.charCodeAt(t+1);return Ee(r)?(n-55296<<10)+(r-56320)+65536:n}function ke(e){return e<=65535?String.fromCharCode(e):(e-=65536,String.fromCharCode((e>>10)+55296,(e&1023)+56320))}function Ae(e){return e<65536?1:2}var je=/\r\n?|\n/,Me=(function(e){return e[e.Simple=0]=`Simple`,e[e.TrackDel=1]=`TrackDel`,e[e.TrackBefore=2]=`TrackBefore`,e[e.TrackAfter=3]=`TrackAfter`,e})(Me||={}),Ne=class e{constructor(e){this.sections=e}get length(){let e=0;for(let t=0;t<this.sections.length;t+=2)e+=this.sections[t];return e}get newLength(){let e=0;for(let t=0;t<this.sections.length;t+=2){let n=this.sections[t+1];e+=n<0?this.sections[t]:n}return e}get empty(){return this.sections.length==0||this.sections.length==2&&this.sections[1]<0}iterGaps(e){for(let t=0,n=0,r=0;t<this.sections.length;){let i=this.sections[t++],a=this.sections[t++];a<0?(e(n,r,i),r+=i):r+=a,n+=i}}iterChangedRanges(e,t=!1){Ie(this,e,t)}get invertedDesc(){let t=[];for(let e=0;e<this.sections.length;){let n=this.sections[e++],r=this.sections[e++];r<0?t.push(n,r):t.push(r,n)}return new e(t)}composeDesc(e){return this.empty?e:e.empty?this:Re(this,e)}mapDesc(e,t=!1){return e.empty?this:Le(this,e,t)}mapPos(e,t=-1,n=Me.Simple){let r=0,i=0;for(let a=0;a<this.sections.length;){let o=this.sections[a++],s=this.sections[a++],c=r+o;if(s<0){if(c>e)return i+(e-r);i+=o}else{if(n!=Me.Simple&&c>=e&&(n==Me.TrackDel&&r<e&&c>e||n==Me.TrackBefore&&r<e||n==Me.TrackAfter&&c>e))return null;if(c>e||c==e&&t<0&&!o)return e==r||t<0?i:i+s;i+=s}r=c}if(e>r)throw RangeError(`Position ${e} is out of range for changeset of length ${r}`);return i}touchesRange(e,t=e){for(let n=0,r=0;n<this.sections.length&&r<=t;){let i=this.sections[n++],a=this.sections[n++],o=r+i;if(a>=0&&r<=t&&o>=e)return r<e&&o>t?`cover`:!0;r=o}return!1}toString(){let e=``;for(let t=0;t<this.sections.length;){let n=this.sections[t++],r=this.sections[t++];e+=(e?` `:``)+n+(r>=0?`:`+r:``)}return e}toJSON(){return this.sections}static fromJSON(t){if(!Array.isArray(t)||t.length%2||t.some(e=>typeof e!=`number`))throw RangeError(`Invalid JSON representation of ChangeDesc`);return new e(t)}static create(t){return new e(t)}},Pe=class e extends Ne{constructor(e,t){super(e),this.inserted=t}apply(e){if(this.length!=e.length)throw RangeError(`Applying change set to a document with the wrong length`);return Ie(this,(t,n,r,i,a)=>e=e.replace(r,r+(n-t),a),!1),e}mapDesc(e,t=!1){return Le(this,e,t,!0)}invert(t){let n=this.sections.slice(),r=[];for(let e=0,i=0;e<n.length;e+=2){let a=n[e],o=n[e+1];if(o>=0){n[e]=o,n[e+1]=a;let s=e>>1;for(;r.length<s;)r.push(E.empty);r.push(a?t.slice(i,i+a):E.empty)}i+=a}return new e(n,r)}compose(e){return this.empty?e:e.empty?this:Re(this,e,!0)}map(e,t=!1){return e.empty?this:Le(this,e,t,!0)}iterChanges(e,t=!1){Ie(this,e,t)}get desc(){return Ne.create(this.sections)}filter(t){let n=[],r=[],i=[],a=new ze(this);done:for(let e=0,o=0;;){let s=e==t.length?1e9:t[e++];for(;o<s||o==s&&a.len==0;){if(a.done)break done;let e=Math.min(a.len,s-o);k(i,e,-1);let t=a.ins==-1?-1:a.off==0?a.ins:0;k(n,e,t),t>0&&Fe(r,n,a.text),a.forward(e),o+=e}let c=t[e++];for(;o<c;){if(a.done)break done;let e=Math.min(a.len,c-o);k(n,e,-1),k(i,e,a.ins==-1?-1:a.off==0?a.ins:0),a.forward(e),o+=e}}return{changes:new e(n,r),filtered:Ne.create(i)}}toJSON(){let e=[];for(let t=0;t<this.sections.length;t+=2){let n=this.sections[t],r=this.sections[t+1];r<0?e.push(n):r==0?e.push([n]):e.push([n].concat(this.inserted[t>>1].toJSON()))}return e}static of(t,n,r){let i=[],a=[],o=0,s=null;function c(t=!1){if(!t&&!i.length)return;o<n&&k(i,n-o,-1);let r=new e(i,a);s=s?s.compose(r.map(s)):r,i=[],a=[],o=0}function l(t){if(Array.isArray(t))for(let e of t)l(e);else if(t instanceof e){if(t.length!=n)throw RangeError(`Mismatched change set length (got ${t.length}, expected ${n})`);c(),s=s?s.compose(t.map(s)):t}else{let{from:e,to:s=e,insert:l}=t;if(e>s||e<0||s>n)throw RangeError(`Invalid change range ${e} to ${s} (in doc of length ${n})`);let u=l?typeof l==`string`?E.of(l.split(r||je)):l:E.empty,d=u.length;if(e==s&&d==0)return;e<o&&c(),e>o&&k(i,e-o,-1),k(i,s-e,d),Fe(a,i,u),o=s}}return l(t),c(!s),s}static empty(t){return new e(t?[t,-1]:[],[])}static fromJSON(t){if(!Array.isArray(t))throw RangeError(`Invalid JSON representation of ChangeSet`);let n=[],r=[];for(let e=0;e<t.length;e++){let i=t[e];if(typeof i==`number`)n.push(i,-1);else if(!Array.isArray(i)||typeof i[0]!=`number`||i.some((e,t)=>t&&typeof e!=`string`))throw RangeError(`Invalid JSON representation of ChangeSet`);else if(i.length==1)n.push(i[0],0);else{for(;r.length<e;)r.push(E.empty);r[e]=E.of(i.slice(1)),n.push(i[0],r[e].length)}}return new e(n,r)}static createSet(t,n){return new e(t,n)}};function k(e,t,n,r=!1){if(t==0&&n<=0)return;let i=e.length-2;i>=0&&n<=0&&n==e[i+1]?e[i]+=t:i>=0&&t==0&&e[i]==0?e[i+1]+=n:r?(e[i]+=t,e[i+1]+=n):e.push(t,n)}function Fe(e,t,n){if(n.length==0)return;let r=t.length-2>>1;if(r<e.length)e[e.length-1]=e[e.length-1].append(n);else{for(;e.length<r;)e.push(E.empty);e.push(n)}}function Ie(e,t,n){let r=e.inserted;for(let i=0,a=0,o=0;o<e.sections.length;){let s=e.sections[o++],c=e.sections[o++];if(c<0)i+=s,a+=s;else{let l=i,u=a,d=E.empty;for(;l+=s,u+=c,c&&r&&(d=d.append(r[o-2>>1])),!(n||o==e.sections.length||e.sections[o+1]<0);)s=e.sections[o++],c=e.sections[o++];t(i,l,a,u,d),i=l,a=u}}}function Le(e,t,n,r=!1){let i=[],a=r?[]:null,o=new ze(e),s=new ze(t);for(let e=-1;;)if(o.done&&s.len||s.done&&o.len)throw Error(`Mismatched change set lengths`);else if(o.ins==-1&&s.ins==-1){let e=Math.min(o.len,s.len);k(i,e,-1),o.forward(e),s.forward(e)}else if(s.ins>=0&&(o.ins<0||e==o.i||o.off==0&&(s.len<o.len||s.len==o.len&&!n))){let t=s.len;for(k(i,s.ins,-1);t;){let n=Math.min(o.len,t);o.ins>=0&&e<o.i&&o.len<=n&&(k(i,0,o.ins),a&&Fe(a,i,o.text),e=o.i),o.forward(n),t-=n}s.next()}else if(o.ins>=0){let t=0,n=o.len;for(;n;)if(s.ins==-1){let e=Math.min(n,s.len);t+=e,n-=e,s.forward(e)}else if(s.ins==0&&s.len<n)n-=s.len,s.next();else break;k(i,t,e<o.i?o.ins:0),a&&e<o.i&&Fe(a,i,o.text),e=o.i,o.forward(o.len-n)}else if(o.done&&s.done)return a?Pe.createSet(i,a):Ne.create(i);else throw Error(`Mismatched change set lengths`)}function Re(e,t,n=!1){let r=[],i=n?[]:null,a=new ze(e),o=new ze(t);for(let e=!1;;)if(a.done&&o.done)return i?Pe.createSet(r,i):Ne.create(r);else if(a.ins==0)k(r,a.len,0,e),a.next();else if(o.len==0&&!o.done)k(r,0,o.ins,e),i&&Fe(i,r,o.text),o.next();else if(a.done||o.done)throw Error(`Mismatched change set lengths`);else{let t=Math.min(a.len2,o.len),n=r.length;if(a.ins==-1){let n=o.ins==-1?-1:o.off?0:o.ins;k(r,t,n,e),i&&n&&Fe(i,r,o.text)}else o.ins==-1?(k(r,a.off?0:a.len,t,e),i&&Fe(i,r,a.textBit(t))):(k(r,a.off?0:a.len,o.off?0:o.ins,e),i&&!o.off&&Fe(i,r,o.text));e=(a.ins>t||o.ins>=0&&o.len>t)&&(e||r.length>n),a.forward2(t),o.forward(t)}}var ze=class{constructor(e){this.set=e,this.i=0,this.next()}next(){let{sections:e}=this.set;this.i<e.length?(this.len=e[this.i++],this.ins=e[this.i++]):(this.len=0,this.ins=-2),this.off=0}get done(){return this.ins==-2}get len2(){return this.ins<0?this.len:this.ins}get text(){let{inserted:e}=this.set,t=this.i-2>>1;return t>=e.length?E.empty:e[t]}textBit(e){let{inserted:t}=this.set,n=this.i-2>>1;return n>=t.length&&!e?E.empty:t[n].slice(this.off,e==null?void 0:this.off+e)}forward(e){e==this.len?this.next():(this.len-=e,this.off+=e)}forward2(e){this.ins==-1?this.forward(e):e==this.ins?this.next():(this.ins-=e,this.off+=e)}},Be=class e{constructor(e,t,n,r){this.from=e,this.to=t,this.flags=n,this.goalColumn=r}get anchor(){return this.flags&32?this.to:this.from}get head(){return this.flags&32?this.from:this.to}get empty(){return this.from==this.to}get assoc(){return this.flags&8?-1:this.flags&16?1:0}get undirectional(){return(this.flags&64)>0}get bidiLevel(){let e=this.flags&7;return e==7?null:e}map(t,n=-1){let r,i;return this.empty?r=i=t.mapPos(this.from,n):(r=t.mapPos(this.from,1),i=t.mapPos(this.to,-1)),r==this.from&&i==this.to?this:new e(r,i,this.flags,this.goalColumn)}extend(e,t=e,n=0){if(e<=this.anchor&&t>=this.anchor)return A.range(e,t,void 0,void 0,n);let r=Math.abs(e-this.anchor)>Math.abs(t-this.anchor)?e:t;return A.range(this.anchor,r,void 0,void 0,n)}eq(e,t=!1){return this.anchor==e.anchor&&this.head==e.head&&this.goalColumn==e.goalColumn&&(!t||!this.empty||this.assoc==e.assoc)}toJSON(){return{anchor:this.anchor,head:this.head}}static fromJSON(e){if(!e||typeof e.anchor!=`number`||typeof e.head!=`number`)throw RangeError(`Invalid JSON representation for SelectionRange`);return A.range(e.anchor,e.head)}static create(t,n,r,i){return new e(t,n,r,i)}},A=class e{constructor(e,t){this.ranges=e,this.mainIndex=t}map(t,n=-1){return t.empty?this:e.create(this.ranges.map(e=>e.map(t,n)),this.mainIndex)}eq(e,t=!1){if(this.ranges.length!=e.ranges.length||this.mainIndex!=e.mainIndex)return!1;for(let n=0;n<this.ranges.length;n++)if(!this.ranges[n].eq(e.ranges[n],t))return!1;return!0}get main(){return this.ranges[this.mainIndex]}asSingle(){return this.ranges.length==1?this:new e([this.main],0)}addRange(t,n=!0){return e.create([t].concat(this.ranges),n?0:this.mainIndex+1)}replaceRange(t,n=this.mainIndex){let r=this.ranges.slice();return r[n]=t,e.create(r,this.mainIndex)}toJSON(){return{ranges:this.ranges.map(e=>e.toJSON()),main:this.mainIndex}}static fromJSON(t){if(!t||!Array.isArray(t.ranges)||typeof t.main!=`number`||t.main>=t.ranges.length)throw RangeError(`Invalid JSON representation for EditorSelection`);return new e(t.ranges.map(e=>Be.fromJSON(e)),t.main)}static single(t,n=t){return new e([e.range(t,n)],0)}static create(t,n=0){if(t.length==0)throw RangeError(`A selection needs at least one range`);for(let r=0,i=0;i<t.length;i++){let a=t[i];if(a.empty?a.from<=r:a.from<r)return e.normalized(t.slice(),n);r=a.to}return new e(t,n)}static cursor(e,t=0,n,r){return Be.create(e,e,(t==0?0:t<0?8:16)|(n==null?7:Math.min(6,n)),r)}static range(e,t,n,r,i){let a=r==null?7:Math.min(6,r);return!i&&e!=t&&(i=t<e?1:-1),i&&(a|=i<0?8:16),t<e?Be.create(t,e,a|32,n):Be.create(e,t,a,n)}static undirectionalRange(e,t){return Be.create(e,t,64,void 0)}static normalized(t,n=0){let r=t[n];t.sort((e,t)=>e.from-t.from),n=t.indexOf(r);for(let r=1;r<t.length;r++){let i=t[r],a=t[r-1];if(i.empty?i.from<=a.to:i.from<a.to){let o=a.from,s=Math.max(i.to,a.to);r<=n&&n--,t.splice(--r,2,i.anchor>i.head?e.range(s,o):e.range(o,s))}}return new e(t,n)}};function Ve(e,t){for(let n of e.ranges)if(n.to>t)throw RangeError(`Selection points outside of document`)}var He=0,j=class e{constructor(e,t,n,r,i){this.combine=e,this.compareInput=t,this.compare=n,this.isStatic=r,this.id=He++,this.default=e([]),this.extensions=typeof i==`function`?i(this):i}get reader(){return this}static define(t={}){return new e(t.combine||(e=>e),t.compareInput||((e,t)=>e===t),t.compare||(t.combine?(e,t)=>e===t:Ue),!!t.static,t.enables)}of(e){return new We([],this,0,e)}compute(e,t){if(this.isStatic)throw Error(`Can't compute a static facet`);return new We(e,this,1,t)}computeN(e,t){if(this.isStatic)throw Error(`Can't compute a static facet`);return new We(e,this,2,t)}from(e,t){return t||=e=>e,this.compute([e],n=>t(n.field(e)))}};function Ue(e,t){return e==t||e.length==t.length&&e.every((e,n)=>e===t[n])}var We=class{constructor(e,t,n,r){this.dependencies=e,this.facet=t,this.type=n,this.value=r,this.id=He++}dynamicSlot(e){let t=this.value,n=this.facet.compareInput,r=this.id,i=e[r]>>1,a=this.type==2,o=!1,s=!1,c=[];for(let t of this.dependencies)t==`doc`?o=!0:t==`selection`?s=!0:(e[t.id]??1)&1||c.push(e[t.id]);return{create(e){return e.values[i]=t(e),1},update(e,r){if(o&&r.docChanged||s&&(r.docChanged||r.selection)||Ke(e,c)){let r=t(e);if(a?!Ge(r,e.values[i],n):!n(r,e.values[i]))return e.values[i]=r,1}return 0},reconfigure:(e,o)=>{let s,c=o.config.address[r];if(c!=null){let r=at(o,c);if(this.dependencies.every(t=>t instanceof j?o.facet(t)===e.facet(t):t instanceof Ye?o.field(t,!1)==e.field(t,!1):!0)||(a?Ge(s=t(e),r,n):n(s=t(e),r)))return e.values[i]=r,0}else s=t(e);return e.values[i]=s,1}}}get extension(){return this}};function Ge(e,t,n){if(e.length!=t.length)return!1;for(let r=0;r<e.length;r++)if(!n(e[r],t[r]))return!1;return!0}function Ke(e,t){let n=!1;for(let r of t)it(e,r)&1&&(n=!0);return n}function qe(e,t,n){let r=n.map(t=>e[t.id]),i=n.map(e=>e.type),a=r.filter(e=>!(e&1)),o=e[t.id]>>1;function s(e){let n=[];for(let t=0;t<r.length;t++){let a=at(e,r[t]);if(i[t]==2)for(let e of a)n.push(e);else n.push(a)}return t.combine(n)}return{create(e){for(let t of r)it(e,t);return e.values[o]=s(e),1},update(e,n){if(!Ke(e,a))return 0;let r=s(e);return t.compare(r,e.values[o])?0:(e.values[o]=r,1)},reconfigure(e,i){let a=Ke(e,r),c=i.config.facets[t.id],l=i.facet(t);if(c&&!a&&Ue(n,c))return e.values[o]=l,0;let u=s(e);return t.compare(u,l)?(e.values[o]=l,0):(e.values[o]=u,1)}}}var Je=j.define({static:!0}),Ye=class e{constructor(e,t,n,r,i){this.id=e,this.createF=t,this.updateF=n,this.compareF=r,this.spec=i,this.provides=void 0}static define(t){let n=new e(He++,t.create,t.update,t.compare||((e,t)=>e===t),t);return t.provide&&(n.provides=t.provide(n)),n}create(e){return(e.facet(Je).find(e=>e.field==this)?.create||this.createF)(e)}slot(e){let t=e[this.id]>>1;return{create:e=>(e.values[t]=this.create(e),1),update:(e,n)=>{let r=e.values[t],i=this.updateF(r,n);return this.compareF(r,i)?0:(e.values[t]=i,1)},reconfigure:(e,n)=>{let r=e.facet(Je),i=n.facet(Je),a;return(a=r.find(e=>e.field==this))&&a!=i.find(e=>e.field==this)?(e.values[t]=a.create(e),1):n.config.address[this.id]==null?(e.values[t]=this.create(e),1):(e.values[t]=n.field(this),0)}}}init(e){return[this,Je.of({field:this,create:e})]}get extension(){return this}},Xe={lowest:4,low:3,default:2,high:1,highest:0};function Ze(e){return t=>new $e(t,e)}var Qe={highest:Ze(Xe.highest),high:Ze(Xe.high),default:Ze(Xe.default),low:Ze(Xe.low),lowest:Ze(Xe.lowest)},$e=class{constructor(e,t){this.inner=e,this.prec=t}get extension(){return this}},et=class e{of(e){return new tt(this,e)}reconfigure(t){return e.reconfigure.of({compartment:this,extension:t})}get(e){return e.config.compartments.get(this)}},tt=class{constructor(e,t){this.compartment=e,this.inner=t}get extension(){return this}},nt=class e{constructor(e,t,n,r,i,a){for(this.base=e,this.compartments=t,this.dynamicSlots=n,this.address=r,this.staticValues=i,this.facets=a,this.statusTemplate=[];this.statusTemplate.length<n.length;)this.statusTemplate.push(0)}staticFacet(e){let t=this.address[e.id];return t==null?e.default:this.staticValues[t>>1]}static resolve(t,n,r){let i=[],a=Object.create(null),o=new Map;for(let e of rt(t,n,o))e instanceof Ye?i.push(e):(a[e.facet.id]||(a[e.facet.id]=[])).push(e);let s=Object.create(null),c=[],l=[];for(let e of i)s[e.id]=l.length<<1,l.push(t=>e.slot(t));let u=r?.config.facets;for(let e in a){let t=a[e],n=t[0].facet,i=u&&u[e]||[];if(t.every(e=>e.type==0)){if(s[n.id]=c.length<<1|1,Ue(i,t))c.push(r.facet(n));else{let e=n.combine(t.map(e=>e.value));c.push(r&&n.compare(e,r.facet(n))?r.facet(n):e)}}else{for(let e of t)e.type==0?(s[e.id]=c.length<<1|1,c.push(e.value)):(s[e.id]=l.length<<1,l.push(t=>e.dynamicSlot(t)));s[n.id]=l.length<<1,l.push(e=>qe(e,n,t))}}let d=l.map(e=>e(s));return new e(t,o,d,s,c,a)}};function rt(e,t,n){let r=[[],[],[],[],[]],i=new Map;function a(e,o){let s=i.get(e);if(s!=null){if(s<=o)return;let t=r[s].indexOf(e);t>-1&&r[s].splice(t,1),e instanceof tt&&n.delete(e.compartment)}if(i.set(e,o),Array.isArray(e))for(let t of e)a(t,o);else if(e instanceof tt){if(n.has(e.compartment))throw RangeError(`Duplicate use of compartment in extensions`);let r=t.get(e.compartment)||e.inner;n.set(e.compartment,r),a(r,o)}else if(e instanceof $e)a(e.inner,e.prec);else if(e instanceof Ye)r[o].push(e),e.provides&&a(e.provides,o);else if(e instanceof We)r[o].push(e),e.facet.extensions&&a(e.facet.extensions,Xe.default);else{let t=e.extension;if(!t)throw Error(`Unrecognized extension value in extension set (${e}).`);if(t==e)throw Error(`Unrecognized extension value in extension set (${e}). This sometimes happens because multiple instances of @codemirror/state are loaded, breaking instanceof checks.`);a(t,o)}}return a(e,Xe.default),r.reduce((e,t)=>e.concat(t))}function it(e,t){if(t&1)return 2;let n=t>>1,r=e.status[n];if(r==4)throw Error(`Cyclic dependency between fields and/or facets`);if(r&2)return r;e.status[n]=4;let i=e.computeSlot(e,e.config.dynamicSlots[n]);return e.status[n]=2|i}function at(e,t){return t&1?e.config.staticValues[t>>1]:e.values[t>>1]}var ot=j.define(),st=j.define({combine:e=>e.some(e=>e),static:!0}),ct=j.define({combine:e=>e.length?e[0]:void 0,static:!0}),lt=j.define(),ut=j.define(),dt=j.define(),ft=j.define({combine:e=>e.length?e[0]:!1}),pt=class{constructor(e,t){this.type=e,this.value=t}static define(){return new mt}},mt=class{of(e){return new pt(this,e)}},ht=class{constructor(e){this.map=e}of(e){return new M(this,e)}},M=class e{constructor(e,t){this.type=e,this.value=t}map(t){let n=this.type.map(this.value,t);return n===void 0?void 0:n==this.value?this:new e(this.type,n)}is(e){return this.type==e}static define(e={}){return new ht(e.map||(e=>e))}static mapEffects(e,t){if(!e.length)return e;let n=[];for(let r of e){let e=r.map(t);e&&n.push(e)}return n}};M.reconfigure=M.define(),M.appendConfig=M.define();var gt=class e{constructor(t,n,r,i,a,o){this.startState=t,this.changes=n,this.selection=r,this.effects=i,this.annotations=a,this.scrollIntoView=o,this._doc=null,this._state=null,r&&Ve(r,n.newLength),a.some(t=>t.type==e.time)||(this.annotations=a.concat(e.time.of(Date.now())))}static create(t,n,r,i,a,o){return new e(t,n,r,i,a,o)}get newDoc(){return this._doc||=this.changes.apply(this.startState.doc)}get newSelection(){return this.selection||this.startState.selection.map(this.changes)}get state(){return this._state||this.startState.applyTransaction(this),this._state}annotation(e){for(let t of this.annotations)if(t.type==e)return t.value}get docChanged(){return!this.changes.empty}get reconfigured(){return this.startState.config!=this.state.config}isUserEvent(t){let n=this.annotation(e.userEvent);return!!(n&&(n==t||n.length>t.length&&n.slice(0,t.length)==t&&n[t.length]==`.`))}};gt.time=pt.define(),gt.userEvent=pt.define(),gt.addToHistory=pt.define(),gt.remote=pt.define();function _t(e,t){let n=[];for(let r=0,i=0;;){let a,o;if(r<e.length&&(i==t.length||t[i]>=e[r]))a=e[r++],o=e[r++];else if(i<t.length)a=t[i++],o=t[i++];else return n;!n.length||n[n.length-1]<a?n.push(a,o):n[n.length-1]<o&&(n[n.length-1]=o)}}function vt(e,t,n){let r,i,a;return n?(r=t.changes,i=Pe.empty(t.changes.length),a=e.changes.compose(t.changes)):(r=t.changes.map(e.changes),i=e.changes.mapDesc(t.changes,!0),a=e.changes.compose(r)),{changes:a,selection:t.selection?t.selection.map(i):e.selection?.map(r),effects:M.mapEffects(e.effects,r).concat(M.mapEffects(t.effects,i)),annotations:e.annotations.length?e.annotations.concat(t.annotations):t.annotations,scrollIntoView:e.scrollIntoView||t.scrollIntoView}}function yt(e,t,n){let r=t.selection,i=wt(t.annotations);return t.userEvent&&(i=i.concat(gt.userEvent.of(t.userEvent))),{changes:t.changes instanceof Pe?t.changes:Pe.of(t.changes||[],n,e.facet(ct)),selection:r&&(r instanceof A?r:A.single(r.anchor,r.head)),effects:wt(t.effects),annotations:i,scrollIntoView:!!t.scrollIntoView}}function bt(e,t,n){let r=yt(e,t.length?t[0]:{},e.doc.length);t.length&&t[0].filter===!1&&(n=!1);for(let i=1;i<t.length;i++){t[i].filter===!1&&(n=!1);let a=!!t[i].sequential;r=vt(r,yt(e,t[i],a?r.changes.newLength:e.doc.length),a)}let i=gt.create(e,r.changes,r.selection,r.effects,r.annotations,r.scrollIntoView);return St(n?xt(i):i)}function xt(e){let t=e.startState,n=!0;for(let r of t.facet(lt)){let t=r(e);if(t===!1){n=!1;break}Array.isArray(t)&&(n=n===!0?t:_t(n,t))}if(n!==!0){let r,i;if(n===!1)i=e.changes.invertedDesc,r=Pe.empty(t.doc.length);else{let t=e.changes.filter(n);r=t.changes,i=t.filtered.mapDesc(t.changes).invertedDesc}e=gt.create(t,r,e.selection&&e.selection.map(i),M.mapEffects(e.effects,i),e.annotations,e.scrollIntoView)}let r=t.facet(ut);for(let n=r.length-1;n>=0;n--){let i=r[n](e);e=i instanceof gt?i:Array.isArray(i)&&i.length==1&&i[0]instanceof gt?i[0]:bt(t,wt(i),!1)}return e}function St(e){let t=e.startState,n=t.facet(dt),r=e;for(let i=n.length-1;i>=0;i--){let a=n[i](e);a&&Object.keys(a).length&&(r=vt(r,yt(t,a,e.changes.newLength),!0))}return r==e?e:gt.create(t,e.changes,e.selection,r.effects,r.annotations,r.scrollIntoView)}var Ct=[];function wt(e){return e==null?Ct:Array.isArray(e)?e:[e]}var N=(function(e){return e[e.Word=0]=`Word`,e[e.Space=1]=`Space`,e[e.Other=2]=`Other`,e})(N||={}),Tt=/[\u00df\u0587\u0590-\u05f4\u0600-\u06ff\u3040-\u309f\u30a0-\u30ff\u3400-\u4db5\u4e00-\u9fcc\uac00-\ud7af]/,Et;try{Et=RegExp(`[\\p{Alphabetic}\\p{Number}_]`,`u`)}catch{}function Dt(e){if(Et)return Et.test(e);for(let t=0;t<e.length;t++){let n=e[t];if(/\w/.test(n)||n>``&&(n.toUpperCase()!=n.toLowerCase()||Tt.test(n)))return!0}return!1}function Ot(e){return t=>{if(!/\S/.test(t))return N.Space;if(Dt(t))return N.Word;for(let n=0;n<e.length;n++)if(t.indexOf(e[n])>-1)return N.Word;return N.Other}}var P=class e{constructor(e,t,n,r,i,a){this.config=e,this.doc=t,this.selection=n,this.values=r,this.status=e.statusTemplate.slice(),this.computeSlot=i,a&&(a._state=this);for(let e=0;e<this.config.dynamicSlots.length;e++)it(this,e<<1);this.computeSlot=null}field(e,t=!0){let n=this.config.address[e.id];if(n==null){if(t)throw RangeError(`Field is not present in this state`)}else return it(this,n),at(this,n)}update(...e){return bt(this,e,!0)}applyTransaction(t){let n=this.config,{base:r,compartments:i}=n;for(let e of t.effects)e.is(et.reconfigure)?(n&&=(i=new Map,n.compartments.forEach((e,t)=>i.set(t,e)),null),i.set(e.value.compartment,e.value.extension)):e.is(M.reconfigure)?(n=null,r=e.value):e.is(M.appendConfig)&&(n=null,r=wt(r).concat(e.value));let a;n?a=t.startState.values.slice():(n=nt.resolve(r,i,this),a=new e(n,this.doc,this.selection,n.dynamicSlots.map(()=>null),(e,t)=>t.reconfigure(e,this),null).values);let o=t.startState.facet(st)?t.newSelection:t.newSelection.asSingle();new e(n,t.newDoc,o,a,(e,n)=>n.update(e,t),t)}replaceSelection(e){return typeof e==`string`&&(e=this.toText(e)),this.changeByRange(t=>({changes:{from:t.from,to:t.to,insert:e},range:A.cursor(t.from+e.length,-1)}))}changeByRange(e){let t=this.selection,n=e(t.ranges[0]),r=this.changes(n.changes),i=[n.range],a=wt(n.effects);for(let n=1;n<t.ranges.length;n++){let o=e(t.ranges[n]),s=this.changes(o.changes),c=s.map(r);for(let e=0;e<n;e++)i[e]=i[e].map(c);let l=r.mapDesc(s,!0);i.push(o.range.map(l)),r=r.compose(c),a=M.mapEffects(a,c).concat(M.mapEffects(wt(o.effects),l))}return{changes:r,selection:A.create(i,t.mainIndex),effects:a}}changes(t=[]){return t instanceof Pe?t:Pe.of(t,this.doc.length,this.facet(e.lineSeparator))}toText(t){return E.of(t.split(this.facet(e.lineSeparator)||je))}sliceDoc(e=0,t=this.doc.length){return this.doc.sliceString(e,t,this.lineBreak)}facet(e){let t=this.config.address[e.id];return t==null?e.default:(it(this,t),at(this,t))}toJSON(e){let t={doc:this.sliceDoc(),selection:this.selection.toJSON()};if(e)for(let n in e){let r=e[n];r instanceof Ye&&this.config.address[r.id]!=null&&(t[n]=r.spec.toJSON(this.field(e[n]),this))}return t}static fromJSON(t,n={},r){if(!t||typeof t.doc!=`string`)throw RangeError(`Invalid JSON representation for EditorState`);let i=[];if(r){for(let e in r)if(Object.prototype.hasOwnProperty.call(t,e)){let n=r[e],a=t[e];i.push(n.init(e=>n.spec.fromJSON(a,e)))}}return e.create({doc:t.doc,selection:A.fromJSON(t.selection),extensions:n.extensions?i.concat([n.extensions]):i})}static create(t={}){let n=nt.resolve(t.extensions||[],new Map),r=t.doc instanceof E?t.doc:E.of((t.doc||``).split(n.staticFacet(e.lineSeparator)||je)),i=t.selection?t.selection instanceof A?t.selection:A.single(t.selection.anchor,t.selection.head):A.single(0);return Ve(i,r.length),n.staticFacet(st)||(i=i.asSingle()),new e(n,r,i,n.dynamicSlots.map(()=>null),(e,t)=>t.create(e),null)}get tabSize(){return this.facet(e.tabSize)}get lineBreak(){return this.facet(e.lineSeparator)||`
`}get readOnly(){return this.facet(ft)}phrase(t,...n){for(let n of this.facet(e.phrases))if(Object.prototype.hasOwnProperty.call(n,t)){t=n[t];break}return n.length&&(t=t.replace(/\$(\$|\d*)/g,(e,t)=>{if(t==`$`)return`$`;let r=+(t||1);return!r||r>n.length?e:n[r-1]})),t}languageDataAt(e,t,n=-1){let r=[];for(let i of this.facet(ot))for(let a of i(this,t,n))Object.prototype.hasOwnProperty.call(a,e)&&r.push(a[e]);return r}charCategorizer(e){let t=this.languageDataAt(`wordChars`,e);return Ot(t.length?t[0]:``)}wordAt(e){let{text:t,from:n,length:r}=this.doc.lineAt(e),i=this.charCategorizer(e),a=e-n,o=e-n;for(;a>0;){let e=O(t,a,!1);if(i(t.slice(e,a))!=N.Word)break;a=e}for(;o<r;){let e=O(t,o);if(i(t.slice(o,e))!=N.Word)break;o=e}return a==o?null:A.range(a+n,o+n)}};P.allowMultipleSelections=st,P.tabSize=j.define({combine:e=>e.length?e[0]:4}),P.lineSeparator=ct,P.readOnly=ft,P.phrases=j.define({compare(e,t){let n=Object.keys(e),r=Object.keys(t);return n.length==r.length&&n.every(n=>e[n]==t[n])}}),P.languageData=ot,P.changeFilter=lt,P.transactionFilter=ut,P.transactionExtender=dt,et.reconfigure=M.define();function kt(e,t,n={}){let r={};for(let t of e)for(let e of Object.keys(t)){let i=t[e],a=r[e];if(a===void 0)r[e]=i;else if(a!==i&&i!==void 0){if(Object.hasOwnProperty.call(n,e))r[e]=n[e](a,i);else throw Error(`Config merge conflict for field `+e)}}for(let e in t)r[e]===void 0&&(r[e]=t[e]);return r}var At=class{eq(e){return this==e}range(e,t=e){return Mt.create(e,t,this)}};At.prototype.startSide=At.prototype.endSide=0,At.prototype.point=!1,At.prototype.mapMode=Me.TrackDel;function jt(e,t){return e==t||e.constructor==t.constructor&&e.eq(t)}var Mt=class e{constructor(e,t,n){this.from=e,this.to=t,this.value=n}static create(t,n,r){return new e(t,n,r)}};function Nt(e,t){return e.from-t.from||e.value.startSide-t.value.startSide}var Pt=class e{constructor(e,t,n,r){this.from=e,this.to=t,this.value=n,this.maxPoint=r}get length(){return Ft(this.to)}findIndex(e,t,n,r=0){let i=n?this.to:this.from;for(let a=r,o=i.length;;){if(a==o)return a;let r=a+o>>1,s=i[r]-e||(n?this.value[r].endSide:this.value[r].startSide)-t;if(r==a)return s>=0?a:o;s>=0?o=r:a=r+1}}between(e,t,n,r){for(let i=this.findIndex(t,-1e9,!0),a=this.findIndex(n,1e9,!1,i);i<a;i++)if(r(this.from[i]+e,this.to[i]+e,this.value[i])===!1)return!1}map(t,n,r,i,a){let o=[],s=[],c=[],l=-1,u=-1;iter:for(let e=0;e<this.value.length;e++){let d=this.value[e],f=this.from[e]+t,p=this.to[e]+t,m,h;if(f==p){let e=n.mapPos(f,d.startSide,d.mapMode);if(e==null||(m=h=e,d.startSide!=d.endSide&&(h=n.mapPos(f,d.endSide),h<m)))continue}else if(m=n.mapPos(f,d.startSide),h=n.mapPos(p,d.endSide),m>h||m==h&&d.startSide>0&&d.endSide<=0)continue;if(!((h-m||d.endSide-d.startSide)<0)){if(l<0&&(l=m),d.point&&(u=Math.max(u,h-m)),(m-r||d.startSide-i)>=0)o.push(d),s.push(m-l),c.push(h-l),r=h,i=d.endSide;else{if(m==h)for(let e=o.length;e>0;e--){if((m-(c[e-1]+l)||d.startSide-o[e-1].endSide)>=0){o.splice(e,0,d),s.splice(e,0,m-l),c.splice(e,0,h-l);continue iter}if((m-(s[e-1]+l)||d.endSide-o[e-1].startSide)>0)break}a(m,h,d)}}}return{mapped:o.length?new e(s,c,o,u):null,pos:l}}},F=class e{constructor(e,t,n,r){this.chunkPos=e,this.chunk=t,this.nextLayer=n,this.maxPoint=r}static create(t,n,r,i){return new e(t,n,r,i)}get length(){let e=this.chunk.length-1;return e<0?0:Math.max(this.chunkEnd(e),this.nextLayer.length)}get size(){if(this.isEmpty)return 0;let e=this.nextLayer.size;for(let t of this.chunk)e+=t.value.length;return e}chunkEnd(e){return this.chunkPos[e]+this.chunk[e].length}update(t){let{add:n=[],sort:r=!1,filterFrom:i=0,filterTo:a=this.length}=t,o=t.filter;if(n.length==0&&!o)return this;if(r&&(n=n.slice().sort(Nt)),this.isEmpty)return n.length?e.of(n):this;let s=new zt(this,null,-1).goto(0),c=0,l=[],u=new Lt;for(;s.value||c<n.length;)if(c<n.length&&(s.from-n[c].from||s.startSide-n[c].value.startSide)>=0){let e=n[c++];u.addInner(e.from,e.to,e.value,!1)||l.push(e)}else s.rangeIndex==1&&s.chunkIndex<this.chunk.length&&(c==n.length||this.chunkEnd(s.chunkIndex)<n[c].from)&&(!o||i>this.chunkEnd(s.chunkIndex)||a<this.chunkPos[s.chunkIndex])&&u.addChunk(this.chunkPos[s.chunkIndex],this.chunk[s.chunkIndex])?s.nextChunk():((!o||i>s.to||a<s.from||o(s.from,s.to,s.value))&&(u.addInner(s.from,s.to,s.value,!1)||l.push(Mt.create(s.from,s.to,s.value))),s.next());return u.finishInner(this.nextLayer.isEmpty&&!l.length?e.empty:this.nextLayer.update({add:l,filter:o,filterFrom:i,filterTo:a}))}map(t){if(t.empty||this.isEmpty)return this;let n=[],r=[],i=-1,a,o=(e,t,n)=>{a||=new Lt,a.addRange(e,t,n,!1)};for(let e=0;e<this.chunk.length;e++){let a=this.chunkPos[e],s=this.chunk[e],c=t.touchesRange(a,a+s.length);if(c===!1)i=Math.max(i,s.maxPoint),n.push(s),r.push(t.mapPos(a));else if(c===!0){let[e,c]=n.length?[Ft(r)+Ft(n).length,Ft(Ft(n).value).endSide]:[-1,-1],{mapped:l,pos:u}=s.map(a,t,e,c,o);l&&(i=Math.max(i,l.maxPoint),n.push(l),r.push(u))}}let s=this.nextLayer.map(t);return a&&(s=a.finishInner(s)),n.length==0?s:new e(r,n,s||e.empty,i)}between(e,t,n){if(!this.isEmpty){for(let r=0;r<this.chunk.length;r++){let i=this.chunkPos[r],a=this.chunk[r];if(t>=i&&e<=i+a.length&&a.between(i,e-i,t-i,n)===!1)return}this.nextLayer.between(e,t,n)}}iter(e=0){return Bt.from([this]).goto(e)}get isEmpty(){return this.nextLayer==this}static iter(e,t=0){return Bt.from(e).goto(t)}static compare(e,t,n,r,i=-1){let a=e.filter(e=>e.maxPoint>0||!e.isEmpty&&e.maxPoint>=i),o=t.filter(e=>e.maxPoint>0||!e.isEmpty&&e.maxPoint>=i),s=Rt(a,o,n),c=new Ht(a,s,i),l=new Ht(o,s,i);n.iterGaps((e,t,n)=>Ut(c,e,l,t,n,r)),n.empty&&n.length==0&&Ut(c,0,l,0,0,r)}static eq(e,t,n=0,r){r??=1e9-1;let i=e.filter(e=>!e.isEmpty&&t.indexOf(e)<0),a=t.filter(t=>!t.isEmpty&&e.indexOf(t)<0);if(i.length!=a.length)return!1;if(!i.length)return!0;let o=Rt(i,a),s=new Ht(i,o,0).goto(n),c=new Ht(a,o,0).goto(n);for(;;){if(s.to!=c.to||!Wt(s.active,c.active)||s.point&&(!c.point||!jt(s.point,c.point)))return!1;if(s.to>r)return!0;s.next(),c.next()}}static spans(e,t,n,r,i=-1){let a=new Ht(e,null,i).goto(t),o=t,s=a.openStart;for(;;){let e=Math.min(a.to,n);if(a.point){let n=a.activeForPoint(a.to),i=a.pointFrom<t?n.length+1:a.point.startSide<0?n.length:Math.min(n.length,s);r.point(o,e,a.point,n,i,a.pointRank),s=Math.min(a.openEnd(e),n.length)}else e>o&&(r.span(o,e,a.active,s),s=a.openEnd(e));if(a.to>n)return s+(a.point&&a.to>n?1:0);o=a.to,a.next()}}static of(e,t=!1){let n=new Lt;for(let r of e instanceof Mt?[e]:t?It(e):e)n.add(r.from,r.to,r.value);return n.finish()}static join(t){if(!t.length)return e.empty;let n=Ft(t);for(let r=t.length-2;r>=0;r--)for(let i=t[r];i!=e.empty;i=i.nextLayer)n=new e(i.chunkPos,i.chunk,n,Math.max(i.maxPoint,n.maxPoint));return n}};F.empty=new F([],[],null,-1);function Ft(e){return e[e.length-1]}function It(e){if(e.length>1)for(let t=e[0],n=1;n<e.length;n++){let r=e[n];if(Nt(t,r)>0)return e.slice().sort(Nt);t=r}return e}F.empty.nextLayer=F.empty;var Lt=class e{finishChunk(e){this.chunks.push(new Pt(this.from,this.to,this.value,this.maxPoint)),this.chunkPos.push(this.chunkStart),this.chunkStart=-1,this.setMaxPoint=Math.max(this.setMaxPoint,this.maxPoint),this.maxPoint=-1,e&&(this.from=[],this.to=[],this.value=[])}constructor(){this.chunks=[],this.chunkPos=[],this.chunkStart=-1,this.last=null,this.lastFrom=-1e9,this.lastTo=-1e9,this.from=[],this.to=[],this.value=[],this.maxPoint=-1,this.setMaxPoint=-1,this.nextLayer=null}add(e,t,n){this.addRange(e,t,n,!0)}addRange(t,n,r,i){this.addInner(t,n,r,i)||(this.nextLayer||=new e).addRange(t,n,r,i)}addInner(e,t,n,r){let i=e-this.lastTo||n.startSide-this.last.endSide;if(r&&i<=0&&(e-this.lastFrom||n.startSide-this.last.startSide)<0)throw Error("Ranges must be added sorted by `from` position and `startSide`");return i<0?!1:(this.from.length==250&&this.finishChunk(!0),this.chunkStart<0&&(this.chunkStart=e),this.from.push(e-this.chunkStart),this.to.push(t-this.chunkStart),this.last=n,this.lastFrom=e,this.lastTo=t,this.value.push(n),n.point&&(this.maxPoint=Math.max(this.maxPoint,t-e)),!0)}addChunk(e,t){if((e-this.lastTo||t.value[0].startSide-this.last.endSide)<0)return!1;this.from.length&&this.finishChunk(!0),this.setMaxPoint=Math.max(this.setMaxPoint,t.maxPoint),this.chunks.push(t),this.chunkPos.push(e);let n=t.value.length-1;return this.last=t.value[n],this.lastFrom=t.from[n]+e,this.lastTo=t.to[n]+e,!0}finish(){return this.finishInner(F.empty)}finishInner(e){if(this.from.length&&this.finishChunk(!1),this.chunks.length==0)return e;let t=F.create(this.chunkPos,this.chunks,this.nextLayer?this.nextLayer.finishInner(e):e,this.setMaxPoint);return this.from=null,t}};function Rt(e,t,n){let r=new Map;for(let t of e)for(let e=0;e<t.chunk.length;e++)t.chunk[e].maxPoint<=0&&r.set(t.chunk[e],t.chunkPos[e]);let i=new Set;for(let e of t)for(let t=0;t<e.chunk.length;t++){let a=r.get(e.chunk[t]);a!=null&&(n?n.mapPos(a):a)==e.chunkPos[t]&&!n?.touchesRange(a,a+e.chunk[t].length)&&i.add(e.chunk[t])}return i}var zt=class{constructor(e,t,n,r=0){this.layer=e,this.skip=t,this.minPoint=n,this.rank=r}get startSide(){return this.value?this.value.startSide:0}get endSide(){return this.value?this.value.endSide:0}goto(e,t=-1e9){return this.chunkIndex=this.rangeIndex=0,this.gotoInner(e,t,!1),this}gotoInner(e,t,n){for(;this.chunkIndex<this.layer.chunk.length;){let t=this.layer.chunk[this.chunkIndex];if(!(this.skip&&this.skip.has(t)||this.layer.chunkEnd(this.chunkIndex)<e||t.maxPoint<this.minPoint))break;this.chunkIndex++,n=!1}if(this.chunkIndex<this.layer.chunk.length){let r=this.layer.chunk[this.chunkIndex].findIndex(e-this.layer.chunkPos[this.chunkIndex],t,!0);(!n||this.rangeIndex<r)&&this.setRangeIndex(r)}this.next()}forward(e,t){(this.to-e||this.endSide-t)<0&&this.gotoInner(e,t,!0)}next(){for(;;)if(this.chunkIndex==this.layer.chunk.length){this.from=this.to=1e9,this.value=null;break}else{let e=this.layer.chunkPos[this.chunkIndex],t=this.layer.chunk[this.chunkIndex],n=e+t.from[this.rangeIndex];if(this.from=n,this.to=e+t.to[this.rangeIndex],this.value=t.value[this.rangeIndex],this.setRangeIndex(this.rangeIndex+1),this.minPoint<0||this.value.point&&this.to-this.from>=this.minPoint)break}}setRangeIndex(e){if(e==this.layer.chunk[this.chunkIndex].value.length){if(this.chunkIndex++,this.skip)for(;this.chunkIndex<this.layer.chunk.length&&this.skip.has(this.layer.chunk[this.chunkIndex]);)this.chunkIndex++;this.rangeIndex=0}else this.rangeIndex=e}nextChunk(){this.chunkIndex++,this.rangeIndex=0,this.next()}compare(e){return this.from-e.from||this.startSide-e.startSide||this.rank-e.rank||this.to-e.to||this.endSide-e.endSide}},Bt=class e{constructor(e){this.heap=e}static from(t,n=null,r=-1){let i=[];for(let e=0;e<t.length;e++)for(let a=t[e];!a.isEmpty;a=a.nextLayer)a.maxPoint>=r&&i.push(new zt(a,n,r,e));return i.length==1?i[0]:new e(i)}get startSide(){return this.value?this.value.startSide:0}goto(e,t=-1e9){for(let n of this.heap)n.goto(e,t);for(let e=this.heap.length>>1;e>=0;e--)Vt(this.heap,e);return this.next(),this}forward(e,t){for(let n of this.heap)n.forward(e,t);for(let e=this.heap.length>>1;e>=0;e--)Vt(this.heap,e);(this.to-e||this.value.endSide-t)<0&&this.next()}next(){if(this.heap.length==0)this.from=this.to=1e9,this.value=null,this.rank=-1;else{let e=this.heap[0];this.from=e.from,this.to=e.to,this.value=e.value,this.rank=e.rank,e.value&&e.next(),Vt(this.heap,0)}}};function Vt(e,t){for(let n=e[t];;){let r=(t<<1)+1;if(r>=e.length)break;let i=e[r];if(r+1<e.length&&i.compare(e[r+1])>=0&&(i=e[r+1],r++),n.compare(i)<0)break;e[r]=n,e[t]=i,t=r}}var Ht=class{constructor(e,t,n){this.minPoint=n,this.active=[],this.activeTo=[],this.activeRank=[],this.minActive=-1,this.point=null,this.pointFrom=0,this.pointRank=0,this.to=-1e9,this.endSide=0,this.openStart=-1,this.cursor=Bt.from(e,t,n)}goto(e,t=-1e9){return this.cursor.goto(e,t),this.active.length=this.activeTo.length=this.activeRank.length=0,this.minActive=-1,this.to=e,this.endSide=t,this.openStart=-1,this.next(),this}forward(e,t){for(;this.minActive>-1&&(this.activeTo[this.minActive]-e||this.active[this.minActive].endSide-t)<0;)this.removeActive(this.minActive);this.cursor.forward(e,t)}removeActive(e){Gt(this.active,e),Gt(this.activeTo,e),Gt(this.activeRank,e),this.minActive=qt(this.active,this.activeTo)}addActive(e){let t=0,{value:n,to:r,rank:i}=this.cursor;for(;t<this.activeRank.length&&(i-this.activeRank[t]||r-this.activeTo[t])>0;)t++;Kt(this.active,t,n),Kt(this.activeTo,t,r),Kt(this.activeRank,t,i),e&&Kt(e,t,this.cursor.from),this.minActive=qt(this.active,this.activeTo)}next(){let e=this.to,t=this.point;this.point=null;let n=this.openStart<0?[]:null;for(;;){let r=this.minActive;if(r>-1&&(this.activeTo[r]-this.cursor.from||this.active[r].endSide-this.cursor.startSide)<0){if(this.activeTo[r]>e){this.to=this.activeTo[r],this.endSide=this.active[r].endSide;break}this.removeActive(r),n&&Gt(n,r)}else if(!this.cursor.value){this.to=this.endSide=1e9;break}else if(this.cursor.from>e){this.to=this.cursor.from,this.endSide=this.cursor.startSide;break}else{let e=this.cursor.value;if(!e.point)this.addActive(n),this.cursor.next();else if(t&&this.cursor.to==this.to&&this.cursor.from<this.cursor.to)this.cursor.next();else{this.point=e,this.pointFrom=this.cursor.from,this.pointRank=this.cursor.rank,this.to=this.cursor.to,this.endSide=e.endSide,this.cursor.next(),this.forward(this.to,this.endSide);break}}}if(n){this.openStart=0;for(let t=n.length-1;t>=0&&n[t]<e;t--)this.openStart++}}activeForPoint(e){if(!this.active.length)return this.active;let t=[];for(let n=this.active.length-1;n>=0&&!(this.activeRank[n]<this.pointRank);n--)(this.activeTo[n]>e||this.activeTo[n]==e&&this.active[n].endSide>=this.point.endSide)&&t.push(this.active[n]);return t.reverse()}openEnd(e){let t=0;for(let n=this.activeTo.length-1;n>=0&&this.activeTo[n]>e;n--)t++;return t}};function Ut(e,t,n,r,i,a){e.goto(t),n.goto(r);let o=r+i,s=r,c=r-t,l=!!a.boundChange;for(let t=!1;;){let r=e.to+c-n.to,i=r||e.endSide-n.endSide,u=i<0?e.to+c:n.to,d=Math.min(u,o);if(e.point||n.point?(e.point&&n.point&&jt(e.point,n.point)&&Wt(e.activeForPoint(e.to),n.activeForPoint(n.to))||a.comparePoint(s,d,e.point,n.point),t=!1):(t&&=(a.boundChange(s),!1),d>s&&!Wt(e.active,n.active)&&a.compareRange(s,d,e.active,n.active),l&&d<o&&(r||e.openEnd(u)!=n.openEnd(u))&&(t=!0)),u>o)break;s=u,i<=0&&e.next(),i>=0&&n.next()}}function Wt(e,t){if(e.length!=t.length)return!1;for(let n=0;n<e.length;n++)if(e[n]!=t[n]&&!jt(e[n],t[n]))return!1;return!0}function Gt(e,t){for(let n=t,r=e.length-1;n<r;n++)e[n]=e[n+1];e.pop()}function Kt(e,t,n){for(let n=e.length-1;n>=t;n--)e[n+1]=e[n];e[t]=n}function qt(e,t){let n=-1,r=1e9;for(let i=0;i<t.length;i++)(t[i]-r||e[i].endSide-e[n].endSide)<0&&(n=i,r=t[i]);return n}function Jt(e,t,n=e.length){let r=0;for(let i=0;i<n&&i<e.length;)e.charCodeAt(i)==9?(r+=t-r%t,i++):(r++,i=O(e,i));return r}function Yt(e,t,n,r){for(let r=0,i=0;;){if(i>=t)return r;if(r==e.length)break;i+=e.charCodeAt(r)==9?n-i%n:1,r=O(e,r)}return r===!0?-1:e.length}for(var Xt=`ͼ`,Zt=typeof Symbol>`u`?`__ͼ`:Symbol.for(Xt),Qt=typeof Symbol>`u`?`__styleSet`+Math.floor(Math.random()*1e8):Symbol(`styleSet`),$t=typeof globalThis<`u`?globalThis:typeof window<`u`?window:{},en=class{constructor(e,t){this.rules=[];let{finish:n}=t||{};function r(e){return/^@/.test(e)?[e]:e.split(/,\s*/)}function i(e,t,a,o){let s=[],c=/^@(\w+)\b/.exec(e[0]),l=c&&c[1]==`keyframes`;if(c&&t==null)return a.push(e[0]+`;`);for(let n in t){let o=t[n];if(/&/.test(n))i(n.split(/,\s*/).map(t=>e.map(e=>t.replace(/&/,e))).reduce((e,t)=>e.concat(t)),o,a);else if(o&&typeof o==`object`){if(!c)throw RangeError(`The value of a property (`+n+`) should be a primitive value.`);i(r(n),o,s,l)}else o!=null&&s.push(n.replace(/_.*/,``).replace(/[A-Z]/g,e=>`-`+e.toLowerCase())+`: `+o+`;`)}(s.length||l)&&a.push((n&&!c&&!o?e.map(n):e).join(`, `)+` {`+s.join(` `)+`}`)}for(let t in e)i(r(t),e[t],this.rules)}getRules(){return this.rules.join(`
`)}static newName(){let e=$t[Zt]||1;return $t[Zt]=e+1,Xt+e.toString(36)}static mount(e,t,n){let r=e[Qt],i=n&&n.nonce;r?i&&r.setNonce(i):r=new nn(e,i),r.mount(Array.isArray(t)?t:[t],e)}},tn=new Map,nn=class{constructor(e,t){let n=e.ownerDocument||e,r=n.defaultView;if(!e.head&&e.adoptedStyleSheets&&r.CSSStyleSheet){let t=tn.get(n);if(t)return e[Qt]=t;this.sheet=new r.CSSStyleSheet,tn.set(n,this)}else this.styleTag=n.createElement(`style`),t&&this.styleTag.setAttribute(`nonce`,t);this.modules=[],e[Qt]=this}mount(e,t){let n=this.sheet,r=0,i=0,a=!1;for(let t=0;t<e.length;t++){let o=e[t],s=this.modules.indexOf(o);if(s<i&&s>-1&&(this.modules.splice(s,1),a=!0,i--,s=-1),s==-1){if(this.modules.splice(i++,0,o),a=!0,n)for(let e=0;e<o.rules.length;e++)n.insertRule(o.rules[e],r++)}else{for(;i<s;)r+=this.modules[i++].rules.length;r+=o.rules.length,i++}}if(n)t.adoptedStyleSheets.indexOf(this.sheet)<0&&(t.adoptedStyleSheets=[this.sheet,...t.adoptedStyleSheets]);else{if(a){let e=``;for(let t=0;t<this.modules.length;t++)e+=this.modules[t].getRules()+`
`;this.styleTag.textContent=e}let e=t.head||t;this.styleTag.parentNode!=e&&e.insertBefore(this.styleTag,e.firstChild)}}setNonce(e){this.styleTag&&this.styleTag.getAttribute(`nonce`)!=e&&this.styleTag.setAttribute(`nonce`,e)}},rn={8:`Backspace`,9:`Tab`,10:`Enter`,12:`NumLock`,13:`Enter`,16:`Shift`,17:`Control`,18:`Alt`,20:`CapsLock`,27:`Escape`,32:` `,33:`PageUp`,34:`PageDown`,35:`End`,36:`Home`,37:`ArrowLeft`,38:`ArrowUp`,39:`ArrowRight`,40:`ArrowDown`,44:`PrintScreen`,45:`Insert`,46:`Delete`,59:`;`,61:`=`,91:`Meta`,92:`Meta`,106:`*`,107:`+`,108:`,`,109:`-`,110:`.`,111:`/`,144:`NumLock`,145:`ScrollLock`,160:`Shift`,161:`Shift`,162:`Control`,163:`Control`,164:`Alt`,165:`Alt`,173:`-`,186:`;`,187:`=`,188:`,`,189:`-`,190:`.`,191:`/`,192:"`",219:`[`,220:`\\`,221:`]`,222:`'`},an={48:`)`,49:`!`,50:`@`,51:`#`,52:`$`,53:`%`,54:`^`,55:`&`,56:`*`,57:`(`,59:`:`,61:`+`,173:`_`,186:`:`,187:`+`,188:`<`,189:`_`,190:`>`,191:`?`,192:`~`,219:`{`,220:`|`,221:`}`,222:`"`},on=typeof navigator<`u`&&/Mac/.test(navigator.platform),sn=typeof navigator<`u`&&/MSIE \d|Trident\/(?:[7-9]|\d{2,})\..*rv:(\d+)/.exec(navigator.userAgent),cn=0;cn<10;cn++)rn[48+cn]=rn[96+cn]=String(cn);for(var cn=1;cn<=24;cn++)rn[cn+111]=`F`+cn;for(var cn=65;cn<=90;cn++)rn[cn]=String.fromCharCode(cn+32),an[cn]=String.fromCharCode(cn);for(var ln in rn)an.hasOwnProperty(ln)||(an[ln]=rn[ln]);function un(e){var t=!(on&&e.metaKey&&e.shiftKey&&!e.ctrlKey&&!e.altKey||sn&&e.shiftKey&&e.key&&e.key.length==1||e.key==`Unidentified`)&&e.key||(e.shiftKey?an:rn)[e.keyCode]||e.key||`Unidentified`;return t==`Esc`&&(t=`Escape`),t==`Del`&&(t=`Delete`),t==`Left`&&(t=`ArrowLeft`),t==`Up`&&(t=`ArrowUp`),t==`Right`&&(t=`ArrowRight`),t==`Down`&&(t=`ArrowDown`),t}function I(){var e=arguments[0];typeof e==`string`&&(e=document.createElement(e));var t=1,n=arguments[1];if(n&&typeof n==`object`&&n.nodeType==null&&!Array.isArray(n)){for(var r in n)if(Object.prototype.hasOwnProperty.call(n,r)){var i=n[r];typeof i==`string`?e.setAttribute(r,i):i!=null&&(e[r]=i)}t++}for(;t<arguments.length;t++)dn(e,arguments[t]);return e}function dn(e,t){if(typeof t==`string`)e.appendChild(document.createTextNode(t));else if(t!=null){if(t.nodeType!=null)e.appendChild(t);else if(Array.isArray(t))for(var n=0;n<t.length;n++)dn(e,t[n]);else throw RangeError(`Unsupported child node: `+t)}}var fn=typeof navigator<`u`?navigator:{userAgent:``,vendor:``,platform:``},pn=typeof document<`u`?document:{documentElement:{style:{}}},mn=/Edge\/(\d+)/.exec(fn.userAgent),hn=/MSIE \d/.test(fn.userAgent),gn=/Trident\/(?:[7-9]|\d{2,})\..*rv:(\d+)/.exec(fn.userAgent),_n=!!(hn||gn||mn),vn=!_n&&/gecko\/(\d+)/i.test(fn.userAgent),yn=!_n&&/Chrome\/(\d+)/.exec(fn.userAgent),bn=`webkitFontSmoothing`in pn.documentElement.style,xn=!_n&&/Apple Computer/.test(fn.vendor),Sn=xn&&(/Mobile\/\w+/.test(fn.userAgent)||fn.maxTouchPoints>2),L={mac:Sn||/Mac/.test(fn.platform),windows:/Win/.test(fn.platform),linux:/Linux|X11/.test(fn.platform),ie:_n,ie_version:hn?pn.documentMode||6:gn?+gn[1]:mn?+mn[1]:0,gecko:vn,gecko_version:vn?+(/Firefox\/(\d+)/.exec(fn.userAgent)||[0,0])[1]:0,chrome:!!yn,chrome_version:yn?+yn[1]:0,ios:Sn,android:/Android\b/.test(fn.userAgent),webkit:bn,webkit_version:bn?+(/\bAppleWebKit\/(\d+)/.exec(fn.userAgent)||[0,0])[1]:0,safari:xn,safari_version:xn?+(/\bVersion\/(\d+(\.\d+)?)/.exec(fn.userAgent)||[0,0])[1]:0,tabSize:pn.documentElement.style.tabSize==null?`-moz-tab-size`:`tab-size`};function Cn(e,t){for(let n in e)n==`class`&&t.class?t.class+=` `+e.class:n==`style`&&t.style?t.style+=`;`+e.style:t[n]=e[n];return t}var wn=Object.create(null);function Tn(e,t,n){if(e==t)return!0;e||=wn,t||=wn;let r=Object.keys(e),i=Object.keys(t);if(r.length-0!=i.length-0)return!1;for(let a of r)if(a!=n&&(i.indexOf(a)==-1||e[a]!==t[a]))return!1;return!0}function En(e,t){for(let n=e.attributes.length-1;n>=0;n--){let r=e.attributes[n].name;t[r]??e.removeAttribute(r)}for(let n in t){let r=t[n];n==`style`?e.style.cssText=r:e.getAttribute(n)!=r&&e.setAttribute(n,r)}}function Dn(e,t,n){let r=!1;if(t)for(let i in t)n&&i in n||(r=!0,i==`style`?e.style.cssText=``:e.removeAttribute(i));if(n)for(let i in n)t&&t[i]==n[i]||(r=!0,i==`style`?e.style.cssText=n[i]:e.setAttribute(i,n[i]));return r}function On(e){let t=Object.create(null);for(let n=0;n<e.attributes.length;n++){let r=e.attributes[n];t[r.name]=r.value}return t}var kn=class{eq(e){return!1}updateDOM(e,t,n){return!1}compare(e){return this==e||this.constructor==e.constructor&&this.eq(e)}get estimatedHeight(){return-1}get lineBreaks(){return 0}ignoreEvent(e){return!0}coordsAt(e,t,n){return null}get isHidden(){return!1}get editable(){return!1}destroy(e){}},An=(function(e){return e[e.Text=0]=`Text`,e[e.WidgetBefore=1]=`WidgetBefore`,e[e.WidgetAfter=2]=`WidgetAfter`,e[e.WidgetRange=3]=`WidgetRange`,e})(An||={}),R=class extends At{constructor(e,t,n,r){super(),this.startSide=e,this.endSide=t,this.widget=n,this.spec=r}get heightRelevant(){return!1}static mark(e){return new jn(e)}static widget(e){let t=Math.max(-1e4,Math.min(1e4,e.side||0)),n=!!e.block;return t+=n&&!e.inlineOrder?t>0?3e8:-4e8:t>0?1e8:-1e8,new Nn(e,t,t,n,e.widget||null,!1)}static replace(e){let t=!!e.block,n,r;if(e.isBlockGap)n=-5e8,r=4e8;else{let{start:i,end:a}=Pn(e,t);n=(i?t?-3e8:-1:5e8)-1,r=(a?t?2e8:1:-6e8)+1}return new Nn(e,n,r,t,e.widget||null,!0)}static line(e){return new Mn(e)}static set(e,t=!1){return F.of(e,t)}hasHeight(){return this.widget?this.widget.estimatedHeight>-1:!1}};R.none=F.empty;var jn=class e extends R{constructor(e){let{start:t,end:n}=Pn(e);super(t?-1:5e8,n?1:-6e8,null,e),this.tagName=e.tagName||`span`,this.attrs=e.class&&e.attributes?Cn(e.attributes,{class:e.class}):e.class?{class:e.class}:e.attributes||wn}eq(t){return this==t||t instanceof e&&this.tagName==t.tagName&&Tn(this.attrs,t.attrs)}range(e,t=e){if(e>=t)throw RangeError(`Mark decorations may not be empty`);return super.range(e,t)}};jn.prototype.point=!1;var Mn=class e extends R{constructor(e){super(-2e8,-2e8,null,e)}eq(t){return t instanceof e&&this.spec.class==t.spec.class&&Tn(this.spec.attributes,t.spec.attributes)}range(e,t=e){if(t!=e)throw RangeError(`Line decoration ranges must be zero-length`);return super.range(e,t)}};Mn.prototype.mapMode=Me.TrackBefore,Mn.prototype.point=!0;var Nn=class e extends R{constructor(e,t,n,r,i,a){super(t,n,i,e),this.block=r,this.isReplace=a,this.mapMode=r?t<=0?Me.TrackBefore:Me.TrackAfter:Me.TrackDel}get type(){return this.startSide==this.endSide?this.startSide<=0?An.WidgetBefore:An.WidgetAfter:An.WidgetRange}get heightRelevant(){return this.block||!!this.widget&&(this.widget.estimatedHeight>=5||this.widget.lineBreaks>0)}eq(t){return t instanceof e&&Fn(this.widget,t.widget)&&this.block==t.block&&this.startSide==t.startSide&&this.endSide==t.endSide}range(e,t=e){if(this.isReplace&&(e>t||e==t&&this.startSide>0&&this.endSide<=0))throw RangeError(`Invalid range for replacement decoration`);if(!this.isReplace&&t!=e)throw RangeError(`Widget decorations can only have zero-length ranges`);return super.range(e,t)}};Nn.prototype.point=!0;function Pn(e,t=!1){let{inclusiveStart:n,inclusiveEnd:r}=e;return n??=e.inclusive,r??=e.inclusive,{start:n??t,end:r??t}}function Fn(e,t){return e==t||!!(e&&t&&e.compare(t))}function In(e,t,n,r=0){let i=n.length-1;i>=0&&n[i]+r>=e?n[i]=Math.max(n[i],t):n.push(e,t)}var Ln=class e extends At{constructor(e,t,n){super(),this.tagName=e,this.attributes=t,this.rank=n}eq(t){return t==this||t instanceof e&&this.tagName==t.tagName&&Tn(this.attributes,t.attributes)}static create(t){return new e(t.tagName,t.attributes||wn,t.rank==null?50:Math.max(0,Math.min(t.rank,100)))}static set(e,t=!1){return F.of(e,t)}};Ln.prototype.startSide=Ln.prototype.endSide=-1;var z=e=>e.nodeType==1,Rn=e=>e.nodeType==3;function zn(e){let t;return t=e.nodeType==11?e.getSelection?e:e.ownerDocument:e,t.getSelection()}function Bn(e,t){return t?e==t||e.contains(t.nodeType==1?t:t.parentNode):!1}function Vn(e,t){if(!t.anchorNode)return!1;try{return Bn(e,t.anchorNode)}catch{return!1}}function Hn(e){return Rn(e)?ar(e,0,e.nodeValue.length).getClientRects():z(e)?e.getClientRects():[]}function Un(e,t,n,r){return n?Kn(e,t,n,r,-1)||Kn(e,t,n,r,1):!1}function Wn(e){for(var t=0;;t++)if(e=e.previousSibling,!e)return t}function Gn(e){return z(e)&&/^(DIV|P|LI|UL|OL|BLOCKQUOTE|DD|DT|H\d|SECTION|PRE)$/.test(e.nodeName)}function Kn(e,t,n,r,i){for(;;){if(e==n&&t==r)return!0;if(t==(i<0?0:qn(e))){if(e.nodeName==`DIV`)return!1;let n=e.parentNode;if(!n||!z(n))return!1;t=Wn(e)+(i<0?0:1),e=n}else if(z(e)){if(e=e.childNodes[t+(i<0?-1:0)],z(e)&&(e.nodeName==`IMG`||e.nodeName==`BR`||e.contentEditable==`false`))return!1;t=i<0?qn(e):0}else return!1}}function qn(e){return Rn(e)?e.nodeValue.length:e.childNodes.length}function Jn(e,t){let{left:n,right:r}=e;if(n==r)return e;let i=t?n:r;return{left:i,right:i,top:e.top,bottom:e.bottom}}function Yn(e){let t=e.visualViewport;return t?{left:0,right:t.width,top:0,bottom:t.height}:{left:0,right:e.innerWidth,top:0,bottom:e.innerHeight}}function Xn(e,t){let n=t.width/e.offsetWidth,r=t.height/e.offsetHeight;return(n>.995&&n<1.005||!isFinite(n)||Math.abs(t.width-e.offsetWidth)<1)&&(n=1),(r>.995&&r<1.005||!isFinite(r)||Math.abs(t.height-e.offsetHeight)<1)&&(r=1),{scaleX:n,scaleY:r}}function Zn(e,t,n,r,i,a,o,s){let c=e.ownerDocument,l=c.defaultView||window;for(let u=e,d=!1;u&&!d;)if(z(u)){let e,f=u==c.body,p=1,m=1,h=!0,g=!0;if(f)e=Yn(l);else{let t=getComputedStyle(u);if(/^(fixed|sticky)$/.test(t.position)&&(d=!0),(u.scrollHeight<=u.clientHeight||!/auto|scroll/.test(t.overflowY))&&(g=!1),(u.scrollWidth<=u.clientWidth||!/auto|scroll/.test(t.overflowX))&&(h=!1),!h&&!g){u=u.assignedSlot||u.parentNode;continue}let n=u.getBoundingClientRect();({scaleX:p,scaleY:m}=Xn(u,n)),e={left:n.left,right:n.left+u.clientWidth*p,top:n.top,bottom:n.top+u.clientHeight*m}}let _=0,v=0;if(g){if(i==`nearest`)t.top<e.top+o?(v=t.top-(e.top+o),n>0&&t.bottom>e.bottom+v&&(v=t.bottom-e.bottom+o)):t.bottom>e.bottom-o&&(v=t.bottom-e.bottom+o,n<0&&t.top-v<e.top&&(v=t.top-(e.top+o)));else{let r=t.bottom-t.top,a=e.bottom-e.top;v=(i==`center`&&r<=a?t.top+r/2-a/2:i==`start`||i==`center`&&n<0?t.top-o:t.bottom-a+o)-e.top}}if(h&&(r==`nearest`?t.left<e.left+a?(_=t.left-(e.left+a),n>0&&t.right>e.right+_&&(_=t.right-e.right+a)):t.right>e.right-a&&(_=t.right-e.right+a,n<0&&t.left<e.left+_&&(_=t.left-(e.left+a))):_=(r==`center`?t.left+(t.right-t.left)/2-(e.right-e.left)/2:r==`start`==s?t.left-a:t.right-(e.right-e.left)+a)-e.left),_||v){if(f)l.scrollBy(_,v);else{let e=0,n=0;if(v){let e=u.scrollTop;u.scrollTop+=v/m,n=(u.scrollTop-e)*m}if(_){let t=u.scrollLeft;u.scrollLeft+=_/p,e=(u.scrollLeft-t)*p}t={left:t.left-e,top:t.top-n,right:t.right-e,bottom:t.bottom-n},e&&Math.abs(e-_)<1&&(r=`nearest`),n&&Math.abs(n-v)<1&&(i=`nearest`)}}if(f)break;(t.top<e.top||t.bottom>e.bottom||t.left<e.left||t.right>e.right)&&(t={left:Math.max(t.left,e.left),right:Math.min(t.right,e.right),top:Math.max(t.top,e.top),bottom:Math.min(t.bottom,e.bottom)}),u=u.assignedSlot||u.parentNode}else if(u.nodeType==11)u=u.host;else break}function Qn(e,t=!0){let n=e.ownerDocument,r=null,i=null;for(let a=e.parentNode;a&&!(a==n.body||(!t||r)&&i);)if(z(a))!i&&a.scrollHeight>a.clientHeight&&/scroll|auto/.test(getComputedStyle(a).overflowY)&&(i=a),t&&!r&&a.scrollWidth>a.clientWidth&&/scroll|auto/.test(getComputedStyle(a).overflowX)&&(r=a),a=a.assignedSlot||a.parentNode;else if(a.nodeType==11)a=a.host;else break;return{x:r,y:i}}var $n=class{constructor(){this.anchorNode=null,this.anchorOffset=0,this.focusNode=null,this.focusOffset=0}eq(e){return this.anchorNode==e.anchorNode&&this.anchorOffset==e.anchorOffset&&this.focusNode==e.focusNode&&this.focusOffset==e.focusOffset}setRange(e){let{anchorNode:t,focusNode:n}=e;this.set(t,Math.min(e.anchorOffset,t?qn(t):0),n,Math.min(e.focusOffset,n?qn(n):0))}set(e,t,n,r){this.anchorNode=e,this.anchorOffset=t,this.focusNode=n,this.focusOffset=r}};function er(e){let t=[];for(let n=e;n;n=n.nodeType==11?n.host:n.parentNode)z(n)&&t.push({node:n,left:n.scrollLeft,top:n.scrollTop});return t}function tr(e,t=!0){for(let{node:n,left:r,top:i}of e)t&&n.scrollTop!=i&&(n.scrollTop=i),n.scrollLeft!=r&&(n.scrollLeft=r)}var nr=null;L.safari&&L.safari_version>=26&&(nr=!1);function rr(e){if(e.setActive)return e.setActive();if(nr)return e.focus(nr);let t=er(e);e.focus(nr==null?{get preventScroll(){return nr={preventScroll:!0},!0}}:void 0),nr||(nr=!1,tr(t))}var ir;function ar(e,t,n=t){let r=ir||=document.createRange();return r.setEnd(e,n),r.setStart(e,t),r}function or(e,t,n,r){let i={key:t,code:t,keyCode:n,which:n,cancelable:!0};r&&({altKey:i.altKey,ctrlKey:i.ctrlKey,shiftKey:i.shiftKey,metaKey:i.metaKey}=r);let a=new KeyboardEvent(`keydown`,i);a.synthetic=!0,e.dispatchEvent(a);let o=new KeyboardEvent(`keyup`,i);return o.synthetic=!0,e.dispatchEvent(o),a.defaultPrevented||o.defaultPrevented}function sr(e){for(;e;){if(e&&(e.nodeType==9||e.nodeType==11&&e.host))return e;e=e.assignedSlot||e.parentNode}return null}function cr(e,t){let n=t.focusNode,r=t.focusOffset;if(!n||t.anchorNode!=n||t.anchorOffset!=r)return!1;for(r=Math.min(r,qn(n));;)if(r){if(!z(n))return!1;let e=n.childNodes[r-1];e.contentEditable==`false`?r--:(n=e,r=qn(n))}else if(n==e)return!0;else r=Wn(n),n=n.parentNode}function lr(e){return e instanceof Window?e.pageYOffset>Math.max(0,e.document.documentElement.scrollHeight-e.innerHeight-4):e.scrollTop>Math.max(1,e.scrollHeight-e.clientHeight-4)}function ur(e,t){for(let n=e,r=t;;)if(Rn(n)&&r>0)return{node:n,offset:r};else if(z(n)&&r>0){if(n.contentEditable==`false`)return null;n=n.childNodes[r-1],r=qn(n)}else if(n.parentNode&&!Gn(n))r=Wn(n),n=n.parentNode;else return null}function dr(e,t){for(let n=e,r=t;;)if(Rn(n)&&r<n.nodeValue.length)return{node:n,offset:r};else if(z(n)&&r<n.childNodes.length){if(n.contentEditable==`false`)return null;n=n.childNodes[r],r=0}else if(n.parentNode&&!Gn(n))r=Wn(n)+1,n=n.parentNode;else return null}var fr=class e{constructor(e,t,n=!0){this.node=e,this.offset=t,this.precise=n}static before(t,n){return new e(t.parentNode,Wn(t),n)}static after(t,n){return new e(t.parentNode,Wn(t)+1,n)}},B=(function(e){return e[e.LTR=0]=`LTR`,e[e.RTL=1]=`RTL`,e})(B||={}),pr=B.LTR,mr=B.RTL;function hr(e){let t=[];for(let n=0;n<e.length;n++)t.push(1<<e[n]);return t}var gr=hr(`88888888888888888888888888888888888666888888787833333333337888888000000000000000000000000008888880000000000000000000000000088888888888888888888888888888888888887866668888088888663380888308888800000000000000000000000800000000000000000000000000000008`),_r=hr(`4444448826627288999999999992222222222222222222222222222222222222222222222229999999999999999999994444444444644222822222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222999999949999999229989999223333333333`),vr=Object.create(null),yr=[];for(let e of[`()`,`[]`,`{}`]){let t=e.charCodeAt(0),n=e.charCodeAt(1);vr[t]=n,vr[n]=-t}function br(e){return e<=247?gr[e]:1424<=e&&e<=1524?2:1536<=e&&e<=1785?_r[e-1536]:1774<=e&&e<=2220?4:8192<=e&&e<=8204?256:64336<=e&&e<=65023?4:1}var xr=/[\u0590-\u05f4\u0600-\u06ff\u0700-\u08ac\ufb50-\ufdff]/,Sr=class{get dir(){return this.level%2?mr:pr}constructor(e,t,n){this.from=e,this.to=t,this.level=n}side(e,t){return this.dir==t==e?this.to:this.from}forward(e,t){return e==(this.dir==t)}static find(e,t,n,r){let i=-1;for(let a=0;a<e.length;a++){let o=e[a];if(o.from<=t&&o.to>=t){if(o.level==n)return a;(i<0||(r==0?e[i].level>o.level:r<0?o.from<t:o.to>t))&&(i=a)}}if(i<0)throw RangeError(`Index out of range`);return i}};function Cr(e,t){if(e.length!=t.length)return!1;for(let n=0;n<e.length;n++){let r=e[n],i=t[n];if(r.from!=i.from||r.to!=i.to||r.direction!=i.direction||!Cr(r.inner,i.inner))return!1}return!0}var V=[];function wr(e,t,n,r,i){for(let a=0;a<=r.length;a++){let o=a?r[a-1].to:t,s=a<r.length?r[a].from:n,c=a?256:i;for(let t=o,n=c,r=c;t<s;t++){let i=br(e.charCodeAt(t));i==512?i=n:i==8&&r==4&&(i=16),V[t]=i==4?2:i,i&7&&(r=i),n=i}for(let e=o,t=c,r=c;e<s;e++){let i=V[e];if(i==128)e<s-1&&t==V[e+1]&&t&24?i=V[e]=t:V[e]=256;else if(i==64){let i=e+1;for(;i<s&&V[i]==64;)i++;let a=e&&t==8||i<n&&V[i]==8?r==1?1:8:256;for(let t=e;t<i;t++)V[t]=a;e=i-1}else i==8&&r==1&&(V[e]=1);t=i,i&7&&(r=i)}}}function Tr(e,t,n,r,i){let a=i==1?2:1;for(let o=0,s=0,c=0;o<=r.length;o++){let l=o?r[o-1].to:t,u=o<r.length?r[o].from:n;for(let t=l,n,r,o;t<u;t++)if(r=vr[n=e.charCodeAt(t)]){if(r<0){for(let e=s-3;e>=0;e-=3)if(yr[e+1]==-r){let n=yr[e+2],r=n&2?i:n&4?n&1?a:i:0;r&&(V[t]=V[yr[e]]=r),s=e;break}}else if(yr.length==189)break;else yr[s++]=t,yr[s++]=n,yr[s++]=c}else if((o=V[t])==2||o==1){let e=o==i;c=+!e;for(let t=s-3;t>=0;t-=3){let n=yr[t+2];if(n&2)break;if(e)yr[t+2]|=2;else{if(n&4)break;yr[t+2]|=4}}}}}function Er(e,t,n,r){for(let i=0,a=r;i<=n.length;i++){let o=i?n[i-1].to:e,s=i<n.length?n[i].from:t;for(let c=o;c<s;){let o=V[c];if(o==256){let o=c+1;for(;;)if(o==s){if(i==n.length)break;o=n[i++].to,s=i<n.length?n[i].from:t}else if(V[o]==256)o++;else break;let l=a==1,u=l==((o<t?V[o]:r)==1)?l?1:2:r;for(let t=o,r=i,a=r?n[r-1].to:e;t>c;)t==a&&(t=n[--r].from,a=r?n[r-1].to:e),V[--t]=u;c=o}else a=o,c++}}}function Dr(e,t,n,r,i,a,o){let s=r%2?2:1;if(r%2==i%2)for(let c=t,l=0;c<n;){let t=!0,u=!1;if(l==a.length||c<a[l].from){let e=V[c];e!=s&&(t=!1,u=e==16)}let d=!t&&s==1?[]:null,f=t?r:r+1,p=c;run:for(;;)if(l<a.length&&p==a[l].from){if(u)break run;let m=a[l];if(!t)for(let e=m.to,t=l+1;;){if(e==n)break run;if(t<a.length&&a[t].from==e)e=a[t++].to;else if(V[e]==s)break run;else break}l++,d?d.push(m):(m.from>c&&o.push(new Sr(c,m.from,f)),Or(e,m.direction==pr==!(f%2)?r:r+1,i,m.inner,m.from,m.to,o),c=m.to),p=m.to}else if(p==n||(t?V[p]!=s:V[p]==s))break;else p++;d?Dr(e,c,p,r+1,i,d,o):c<p&&o.push(new Sr(c,p,f)),c=p}else for(let c=n,l=a.length;c>t;){let n=!0,u=!1;if(!l||c>a[l-1].to){let e=V[c-1];e!=s&&(n=!1,u=e==16)}let d=!n&&s==1?[]:null,f=n?r:r+1,p=c;run:for(;;)if(l&&p==a[l-1].to){if(u)break run;let m=a[--l];if(!n)for(let e=m.from,n=l;;){if(e==t)break run;if(n&&a[n-1].to==e)e=a[--n].from;else if(V[e-1]==s)break run;else break}d?d.push(m):(m.to<c&&o.push(new Sr(m.to,c,f)),Or(e,m.direction==pr==!(f%2)?r:r+1,i,m.inner,m.from,m.to,o),c=m.from),p=m.from}else if(p==t||(n?V[p-1]!=s:V[p-1]==s))break;else p--;d?Dr(e,p,c,r+1,i,d,o):p<c&&o.push(new Sr(p,c,f)),c=p}}function Or(e,t,n,r,i,a,o){let s=t%2?2:1;wr(e,i,a,r,s),Tr(e,i,a,r,s),Er(i,a,r,s),Dr(e,i,a,t,n,r,o)}function kr(e,t,n){if(!e)return[new Sr(0,0,+(t==mr))];if(t==pr&&!n.length&&!xr.test(e))return Ar(e.length);if(n.length)for(;e.length>V.length;)V[V.length]=256;let r=[],i=t==pr?0:1;return Or(e,i,i,n,0,e.length,r),r}function Ar(e){return[new Sr(0,e,0)]}var jr=``;function Mr(e,t,n,r,i){if(!e.length)return null;let a=r.head-e.from,o;if(r.head==e.from&&r.assoc<0){if(!i)return null;a=t[o=0].side(!1,n)}else if(r.head==e.to&&r.assoc>0){if(i)return null;a=t[o=t.length-1].side(!0,n)}else o=Sr.find(t,a,r.bidiLevel??-1,r.assoc);let s=t[o],c=s.side(i,n);if(a==c){let e=o+=i?1:-1;if(e<0||e>=t.length)return null;s=t[o=e],a=s.side(!i,n),c=s.side(i,n)}let l=O(e.text,a,s.forward(i,n));(l<s.from||l>s.to)&&(l=c),jr=e.text.slice(Math.min(a,l),Math.max(a,l));let u=o==(i?t.length-1:0)?null:t[o+(i?1:-1)];if(l==c){if(!u)return i?A.cursor(e.to,1):A.cursor(e.from,-1);if(u.level+ +!i<s.level)return A.cursor(u.side(!i,n)+e.from,u.forward(i,n)?1:-1,u.level)}return A.cursor(l+e.from,s.forward(i,n)?-1:1,s.level)}function Nr(e,t,n){for(let r=t;r<n;r++){let t=br(e.charCodeAt(r));if(t==1)return pr;if(t==2||t==4)return mr}}var Pr=j.define(),Fr=j.define(),Ir=j.define(),Lr=j.define(),Rr=j.define(),zr=j.define(),Br=j.define(),Vr=j.define(),Hr=j.define(),Ur=j.define({combine:e=>e.some(e=>e)}),Wr=j.define({combine:e=>e.some(e=>e)}),Gr=j.define(),Kr=class e{constructor(e,t,n,r,i,a=!1){this.range=e,this.y=t,this.x=n,this.yMargin=r,this.xMargin=i,this.isSnapshot=a}map(t){return t.empty?this:new e(this.range.map(t),this.y,this.x,this.yMargin,this.xMargin,this.isSnapshot)}clip(t){return this.range.to<=t.doc.length?this:new e(A.cursor(t.doc.length),this.y,this.x,this.yMargin,this.xMargin,this.isSnapshot)}},qr=M.define({map:(e,t)=>e.map(t)}),Jr=M.define();function Yr(e,t,n){let r=e.facet(Lr);r.length?r[0](t):window.onerror&&window.onerror(String(t),n,void 0,void 0,t)||(n?console.error(n+`:`,t):console.error(t))}var Xr=j.define({combine:e=>!e.length||e[0]}),Zr=0,Qr=j.define({combine(e){return e.filter((t,n)=>{for(let r=0;r<n;r++)if(e[r].plugin==t.plugin)return!1;return!0})}}),H=class e{constructor(e,t,n,r,i){this.id=e,this.create=t,this.domEventHandlers=n,this.domEventObservers=r,this.baseExtensions=i(this),this.extension=this.baseExtensions.concat(Qr.of({plugin:this,arg:void 0}))}of(e){return this.baseExtensions.concat(Qr.of({plugin:this,arg:e}))}static define(t,n){let{eventHandlers:r,eventObservers:i,provide:a,decorations:o}=n||{};return new e(Zr++,t,r,i,e=>{let t=[];return o&&t.push(ni.of(t=>{let n=t.plugin(e);return n?o(n):R.none})),a&&t.push(a(e)),t})}static fromClass(t,n){return e.define((e,n)=>new t(e,n),n)}},$r=class{constructor(e){this.spec=e,this.mustUpdate=null,this.value=null}get plugin(){return this.spec&&this.spec.plugin}update(e){if(!this.value){if(this.spec)try{this.value=this.spec.plugin.create(e,this.spec.arg)}catch(t){Yr(e.state,t,`CodeMirror plugin crashed`),this.deactivate()}}else if(this.mustUpdate){let e=this.mustUpdate;if(this.mustUpdate=null,this.value.update)try{this.value.update(e)}catch(t){if(Yr(e.state,t,`CodeMirror plugin crashed`),this.value.destroy)try{this.value.destroy()}catch{}this.deactivate()}}return this}destroy(e){if(this.value?.destroy)try{this.value.destroy()}catch(t){Yr(e.state,t,`CodeMirror plugin crashed`)}}deactivate(){this.spec=this.value=null}},ei=j.define(),ti=j.define(),ni=j.define(),ri=j.define(),ii=j.define(),ai=j.define(),oi=j.define();function si(e,t){let n=e.state.facet(oi);if(!n.length)return n;let r=n.map(t=>t instanceof Function?t(e):t),i=[];return F.spans(r,t.from,t.to,{point(){},span(e,n,r,a){let o=e-t.from,s=n-t.from,c=i;for(let e=r.length-1;e>=0;e--,a--){let t=r[e].spec.bidiIsolate,n;if(a>0&&c.length&&(n=c[c.length-1]).to==o&&n.direction==t)n.to=s,c=n.inner;else{let e={from:o,to:s,direction:t,inner:[]};c.push(e),c=e.inner}}}}),ci(t.text,i)}function ci(e,t){for(let n of t){if(n.direction==null){for(let t=n.from,r=0;t<n.to;r++){let i=r<n.inner.length?n.inner[r]:null,a=Nr(e,t,i?i.from:n.to);if(a){n.direction=a;break}if(!i)break;t=i.to}n.direction??=B.LTR}ci(e,n.inner)}return t}var li=j.define();function ui(e){let t=0,n=0,r=0,i=0;for(let a of e.state.facet(li)){let o=a(e);o&&(o.left!=null&&(t=Math.max(t,o.left)),o.right!=null&&(n=Math.max(n,o.right)),o.top!=null&&(r=Math.max(r,o.top)),o.bottom!=null&&(i=Math.max(i,o.bottom)))}return{left:t,right:n,top:r,bottom:i}}var di=j.define(),fi=class e{constructor(e,t,n,r){this.fromA=e,this.toA=t,this.fromB=n,this.toB=r}join(t){return new e(Math.min(this.fromA,t.fromA),Math.max(this.toA,t.toA),Math.min(this.fromB,t.fromB),Math.max(this.toB,t.toB))}addToSet(e){let t=e.length,n=this;for(;t>0;t--){let r=e[t-1];if(!(r.fromA>n.toA)){if(r.toA<n.fromA)break;n=n.join(r),e.splice(t-1,1)}}return e.splice(t,0,n),e}static extendWithRanges(t,n){if(n.length==0)return t;let r=[];for(let i=0,a=0,o=0;;){let s=i<t.length?t[i].fromB:1e9,c=a<n.length?n[a]:1e9,l=Math.min(s,c);if(l==1e9)break;let u=l+o,d=l,f=u;for(;;)if(a<n.length&&n[a]<=d){let e=n[a+1];a+=2,d=Math.max(d,e);for(let e=i;e<t.length&&t[e].fromB<=d;e++)o=t[e].toA-t[e].toB;f=Math.max(f,e+o)}else if(i<t.length&&t[i].fromB<=d){let e=t[i++];d=Math.max(d,e.toB),f=Math.max(f,e.toA),o=e.toA-e.toB}else break;r.push(new e(u,f,l,d))}return r}},pi=class e{constructor(e,t,n){this.view=e,this.state=t,this.transactions=n,this.flags=0,this.startState=e.state,this.changes=Pe.empty(this.startState.doc.length);for(let e of n)this.changes=this.changes.compose(e.changes);let r=[];this.changes.iterChangedRanges((e,t,n,i)=>r.push(new fi(e,t,n,i))),this.changedRanges=r}static create(t,n,r){return new e(t,n,r)}get viewportChanged(){return(this.flags&4)>0}get viewportMoved(){return(this.flags&8)>0}get heightChanged(){return(this.flags&2)>0}get geometryChanged(){return this.docChanged||(this.flags&18)>0}get focusChanged(){return(this.flags&1)>0}get docChanged(){return!this.changes.empty}get selectionSet(){return this.transactions.some(e=>e.selection)}get empty(){return this.flags==0&&this.transactions.length==0}},mi=[],U=class{constructor(e,t,n=0){this.dom=e,this.length=t,this.flags=n,this.parent=null,e.cmTile=this}get breakAfter(){return this.flags&1}get children(){return mi}isWidget(){return!1}get isHidden(){return!1}isComposite(){return!1}isLine(){return!1}isText(){return!1}isBlock(){return!1}get domAttrs(){return null}sync(e){if(this.flags|=2,this.flags&4){this.flags&=-5;let e=this.domAttrs;e&&En(this.dom,e)}}toString(){return this.constructor.name+(this.children.length?`(${this.children})`:``)+(this.breakAfter?`#`:``)}destroy(){this.parent=null}setDOM(e){this.dom=e,e.cmTile=this}get posAtStart(){return this.parent?this.parent.posBefore(this):0}get posAtEnd(){return this.posAtStart+this.length}posBefore(e,t=this.posAtStart){let n=t;for(let t of this.children){if(t==e)return n;n+=t.length+t.breakAfter}throw RangeError(`Invalid child in posBefore`)}posAfter(e){return this.posBefore(e)+e.length}covers(e){return!0}coordsIn(e,t,n){return null}domPosFor(e,t){let n=Wn(this.dom),r=this.length?e>0:t>0;return new fr(this.parent.dom,n+ +!!r,e==0||e==this.length)}markDirty(e){this.flags&=-3,e&&(this.flags|=4),this.parent&&this.parent.flags&2&&this.parent.markDirty(!1)}get overrideDOMText(){return null}get root(){for(let e=this;e;e=e.parent)if(e instanceof _i)return e;return null}static get(e){return e.cmTile}},hi=class extends U{constructor(e){super(e,0),this._children=[]}isComposite(){return!0}get children(){return this._children}get lastChild(){return this.children.length?this.children[this.children.length-1]:null}append(e){this.children.push(e),e.parent=this}sync(e){if(this.flags&2)return;super.sync(e);let t=this.dom,n=null,r,i=e?.node==t?e:null,a=0;for(let o of this.children){if(o.sync(e),a+=o.length+o.breakAfter,r=n?n.nextSibling:t.firstChild,i&&r!=o.dom&&(i.written=!0),o.dom.parentNode==t)for(;r&&r!=o.dom;)r=gi(r);else t.insertBefore(o.dom,r);n=o.dom}for(r=n?n.nextSibling:t.firstChild,i&&r&&(i.written=!0);r;)r=gi(r);this.length=a}};function gi(e){let t=e.nextSibling;return e.parentNode.removeChild(e),t}var _i=class extends hi{constructor(e,t){super(t),this.view=e}owns(e){for(;e;e=e.parent)if(e==this)return!0;return!1}isBlock(){return!0}nearest(e){for(;;){if(!e)return null;let t=U.get(e);if(t&&this.owns(t))return t;e=e.parentNode}}blockTiles(e){for(let t=[],n=this,r=0,i=0;;)if(r==n.children.length){if(!t.length)return;n=n.parent,n.breakAfter&&i++,r=t.pop()}else{let a=n.children[r++];if(a instanceof vi)t.push(r),n=a,r=0;else{let t=i+a.length,n=e(a,i);if(n!==void 0)return n;i=t+a.breakAfter}}}resolveBlock(e,t){let n,r=-1,i,a=-1;if(this.blockTiles((o,s)=>{let c=s+o.length;if(e>=s&&e<=c){if(o.isWidget()&&t>=-1&&t<=1){if(o.flags&32)return!0;o.flags&16&&(n=void 0)}(s<e||e==c&&(t<-1?o.length:o.covers(1)))&&(!n||!o.isWidget()&&n.isWidget())&&(n=o,r=e-s),(c>e||e==s&&(t>1?o.length:o.covers(-1)))&&(!i||!o.isWidget()&&i.isWidget())&&(i=o,a=e-s)}}),!n&&!i)throw Error(`No tile at position `+e);return n&&t<0||!i?{tile:n,offset:r}:{tile:i,offset:a}}},vi=class e extends hi{constructor(e,t){super(e),this.wrapper=t}isBlock(){return!0}covers(e){return this.children.length?e<0?this.children[0].covers(-1):this.lastChild.covers(1):!1}get domAttrs(){return this.wrapper.attributes}static of(t,n){let r=new e(n||document.createElement(t.tagName),t);return n||(r.flags|=4),r}},yi=class e extends hi{constructor(e,t){super(e),this.attrs=t}isLine(){return!0}static start(t,n,r){let i=new e(n||document.createElement(`div`),t);return n&&r||(i.flags|=4),i}get domAttrs(){return this.attrs}resolveInline(e,t,n=!1){let r=null,i=-1,a=null,o=-1;function s(e,c){for(let l=0,u=0;l<e.children.length&&u<=c;l++){let d=e.children[l],f=u+d.length;f>=c&&(d.isComposite()?s(d,c-u):(!a||n&&a.isHidden&&(t>0&&!(a.flags&32)||n&&xi(a,d)))&&(f>c||d.flags&32&&t<=1)?(a=d,o=c-u):(u<c||d.flags&16&&!(n&&d.isHidden)&&t>=-1)&&(r=d,i=c-u)),u=f}}s(this,e);let c=(t<0?r:a)||r||a;return c?{tile:c,offset:c==r?i:o}:null}coordsIn(e,t,n){let r=this.resolveInline(e,t,!0);return r?r.tile.coordsIn(Math.max(0,r.offset),t,n):bi(this)}domIn(e,t){let n=this.resolveInline(e,t);if(n){let{tile:e,offset:r}=n;if(this.dom.contains(e.dom))return e.isText()?new fr(e.dom,Math.min(e.dom.nodeValue.length,r)):e.domPosFor(r,e.flags&16?1:e.flags&32?-1:t);let i=n.tile.parent,a=!1;for(let e of i.children){if(a)return new fr(e.dom,0);e==n.tile&&(a=!0)}}return new fr(this.dom,0)}};function bi(e){let t=e.dom.lastChild;if(!t)return e.dom.getBoundingClientRect();let n=Hn(t);return n[n.length-1]||null}function xi(e,t){let n=e.coordsIn(0,1),r=t.coordsIn(0,1);return n&&r&&r.top<n.bottom}var Si=class e extends hi{constructor(e,t){super(e),this.mark=t}get domAttrs(){return this.mark.attrs}static of(t,n){let r=new e(n||document.createElement(t.tagName),t);return n||(r.flags|=4),r}},Ci=class e extends U{constructor(e,t){super(e,t.length),this.text=t}sync(e){this.flags&2||(super.sync(e),this.dom.nodeValue!=this.text&&(e&&e.node==this.dom&&(e.written=!0),this.dom.nodeValue=this.text))}isText(){return!0}toString(){return JSON.stringify(this.text)}coordsIn(e,t,n){let r=this.dom.nodeValue.length;e>r&&(e=r);let i=e,a=e,o=0;e==0&&t<0||e==r&&t>=0?L.chrome||L.gecko||(e?(i--,o=1):a<r&&(a++,o=-1)):t<0?i--:a<r&&a++;let s=ar(this.dom,i,a).getClientRects();if(!s.length)return null;let c=s[(o?o<0:t>=0)?0:s.length-1];return L.safari&&!o&&c.width==0&&(c=Array.prototype.find.call(s,e=>e.width)||c),n==null?c:Jn(c,(o?o>0:t<0)==n)}static of(t,n){let r=new e(n||document.createTextNode(t),t);return n||(r.flags|=2),r}},wi=class e extends U{constructor(e,t,n,r){super(e,t,r),this.widget=n}isWidget(){return!0}get isHidden(){return this.widget.isHidden}covers(e){return this.flags&48?!1:(this.flags&(e<0?64:128))>0}coordsIn(e,t){return this.coordsInWidget(e,t,!1)}coordsInWidget(e,t,n){let r=this.widget.coordsAt(this.dom,e,t);if(r)return r;if(n)return Jn(this.dom.getBoundingClientRect(),this.length?e==0:t<=0);{let t=this.dom.getClientRects(),n=null;if(!t.length)return null;let r=this.flags&16?!0:this.flags&32?!1:e>0;for(let i=r?t.length-1:0;n=t[i],!(e>0?i==0:i==t.length-1||n.top<n.bottom);i+=r?-1:1);return Jn(n,!r)}}get overrideDOMText(){if(!this.length)return E.empty;let{root:e}=this;if(!e)return E.empty;let t=this.posAtStart;return e.view.state.doc.slice(t,t+this.length)}destroy(){super.destroy(),this.widget.destroy(this.dom)}static of(t,n,r,i,a){return a||(a=t.toDOM(n),t.editable||(a.contentEditable=`false`)),new e(a,r,t,i)}},Ti=class extends U{constructor(e){let t=document.createElement(`img`);t.className=`cm-widgetBuffer`,t.setAttribute(`aria-hidden`,`true`),super(t,0,e)}get isHidden(){return!0}get overrideDOMText(){return E.empty}coordsIn(e,t,n){let r=this.dom.getBoundingClientRect();return n==null?r:Jn(r,t>0==n)}},Ei=class{constructor(e){this.index=0,this.beforeBreak=!1,this.parents=[],this.tile=e}advance(e,t,n){let{tile:r,index:i,beforeBreak:a,parents:o}=this;for(;e||t>0;)if(!r.isComposite()){let t=r.length;if(i<t&&e){let a=Math.min(e,t-i);n&&n.skip(r,i,i+a),e-=a,i+=a}if(i==t)a=!!r.breakAfter,{tile:r,index:i}=o.pop(),i++;else if(!e)break}else if(a){if(!e)break;n&&n.break(),e--,a=!1}else if(i==r.children.length){if(!e&&!o.length)break;n&&n.leave(r),a=!!r.breakAfter,{tile:r,index:i}=o.pop(),i++}else{let s=r.children[i],c=s.breakAfter;(t>0?s.length<=e:s.length<e)&&(!n||n.skip(s,0,s.length)!==!1||!s.isComposite)?(a=!!c,i++,e-=s.length):(o.push({tile:r,index:i}),r=s,i=0,n&&s.isComposite()&&n.enter(s))}return this.tile=r,this.index=i,this.beforeBreak=a,this}get root(){return this.parents.length?this.parents[0].tile:this.tile}},Di=class{constructor(e,t,n,r){this.from=e,this.to=t,this.wrapper=n,this.rank=r}},Oi=class{constructor(e,t,n){this.cache=e,this.root=t,this.blockWrappers=n,this.curLine=null,this.lastBlock=null,this.afterWidget=null,this.pos=0,this.wrappers=[],this.wrapperPos=0}addText(e,t,n,r){this.flushBuffer();let i=this.ensureMarks(t,n),a=i.lastChild;if(a&&a.isText()&&!(a.flags&8)&&a.length+e.length<512){this.cache.reused.set(a,2);let t=i.children[i.children.length-1]=new Ci(a.dom,a.text+e);t.parent=i}else i.append(r||Ci.of(e,this.cache.find(Ci)?.dom));this.pos+=e.length,this.afterWidget=null}addComposition(e,t){let n=this.curLine;n.dom!=t.line.dom&&(n.setDOM(this.cache.reused.has(t.line)?Ri(t.line.dom):t.line.dom),this.cache.reused.set(t.line,2));let r=n;for(let e=t.marks.length-1;e>=0;e--){let n=t.marks[e],i=r.lastChild;if(i instanceof Si&&i.mark.eq(n.mark))i.dom!=n.dom&&i.setDOM(Ri(n.dom)),r=i;else{let{dom:e}=n;this.cache.reused.get(n)&&U.get(n.dom)&&(e=Ri(n.dom));let t=Si.of(n.mark,e);r.append(t),r=t}this.cache.reused.set(n,2)}let i=U.get(e.text);i&&this.cache.reused.set(i,2);let a=new Ci(e.text,e.text.nodeValue);a.flags|=8,this.pos=e.range.toB,r.append(a)}addInlineWidget(e,t,n){let r=this.afterWidget&&e.flags&48&&(this.afterWidget.flags&48)==(e.flags&48);r||this.flushBuffer();let i=this.ensureMarks(t,n);!r&&!(e.flags&16)&&i.append(this.getBuffer(1)),i.append(e),this.pos+=e.length,this.afterWidget=e}addMark(e,t,n){this.flushBuffer(),this.ensureMarks(t,n).append(e),this.pos+=e.length,this.afterWidget=null}addBlockWidget(e){this.getBlockPos().append(e),this.pos+=e.length,this.lastBlock=e,this.endLine()}continueWidget(e){let t=this.afterWidget||this.lastBlock;t.length+=e,this.pos+=e}addLineStart(e,t){e||=Fi;let n=yi.start(e,t||this.cache.find(yi)?.dom,!!t);this.getBlockPos().append(this.lastBlock=this.curLine=n)}addLine(e){this.getBlockPos().append(e),this.pos+=e.length,this.lastBlock=e,this.endLine()}addBreak(){this.lastBlock.flags|=1,this.endLine(),this.pos++}addLineStartIfNotCovered(e){this.blockPosCovered()||this.addLineStart(e)}ensureLine(e){this.curLine||this.addLineStart(e)}ensureMarks(e,t){let n=this.curLine;for(let r=e.length-1;r>=0;r--){let i=e[r],a;if(t>0&&(a=n.lastChild)&&a instanceof Si&&a.mark.eq(i))n=a,t--;else{let e=Si.of(i,this.cache.find(Si,e=>e.mark.eq(i))?.dom);n.append(e),n=e,t=0}}return n}endLine(){if(this.curLine){this.flushBuffer();let e=this.curLine.lastChild;(!e||!Ni(this.curLine,!1)||e.dom.nodeName!=`BR`&&e.isWidget()&&!(L.ios&&Ni(this.curLine,!0)))&&this.curLine.append(this.cache.findWidget(Bi,0,32)||new wi(Bi.toDOM(),0,Bi,32)),this.curLine=this.afterWidget=null}}updateBlockWrappers(){this.wrapperPos>this.pos+1e4&&(this.blockWrappers.goto(this.pos),this.wrappers.length=0);for(let e=this.wrappers.length-1;e>=0;e--)this.wrappers[e].to<this.pos&&this.wrappers.splice(e,1);for(let e=this.blockWrappers;e.value&&e.from<=this.pos;e.next())if(e.to>=this.pos){let t=e.rank*102+e.value.rank,n=new Di(e.from,e.to,e.value,t),r=this.wrappers.length;for(;r>0&&(this.wrappers[r-1].rank-n.rank||this.wrappers[r-1].to-n.to)<0;)r--;this.wrappers.splice(r,0,n)}this.wrapperPos=this.pos}getBlockPos(){this.updateBlockWrappers();let e=this.root;for(let t of this.wrappers){let n=e.lastChild;if(t.from<this.pos&&n instanceof vi&&n.wrapper.eq(t.wrapper))e=n;else{let n=vi.of(t.wrapper,this.cache.find(vi,e=>e.wrapper.eq(t.wrapper))?.dom);e.append(n),e=n}}return e}blockPosCovered(){let e=this.lastBlock;return e!=null&&!e.breakAfter&&(!e.isWidget()||(e.flags&160)>0)}getBuffer(e){let t=2|(e<0?16:32),n=this.cache.find(Ti,void 0,1);return n&&(n.flags=t),n||new Ti(t)}flushBuffer(){this.afterWidget&&!(this.afterWidget.flags&32)&&(this.afterWidget.parent.append(this.getBuffer(-1)),this.afterWidget=null)}},ki=class{constructor(e){this.skipCount=0,this.text=``,this.textOff=0,this.cursor=e.iter()}skip(e){this.textOff+e<=this.text.length?this.textOff+=e:(this.skipCount+=e-(this.text.length-this.textOff),this.text=``,this.textOff=0)}next(e){if(this.textOff==this.text.length){let{value:t,lineBreak:n,done:r}=this.cursor.next(this.skipCount);if(this.skipCount=0,r)throw Error(`Ran out of text content when drawing inline views`);this.text=t;let i=this.textOff=Math.min(e,t.length);return n?null:t.slice(0,i)}let t=Math.min(this.text.length,this.textOff+e),n=this.text.slice(this.textOff,t);return this.textOff=t,n}},Ai=[wi,yi,Ci,Si,Ti,vi,_i];for(let e=0;e<Ai.length;e++)Ai[e].bucket=e;var ji=class{constructor(e){this.view=e,this.buckets=Ai.map(()=>[]),this.index=Ai.map(()=>0),this.reused=new Map}add(e){let t=e.constructor.bucket,n=this.buckets[t];n.length<6?n.push(e):n[this.index[t]=(this.index[t]+1)%6]=e}find(e,t,n=2){let r=e.bucket,i=this.buckets[r],a=this.index[r];for(let e=0;e<i.length;e++){let o=(e+a)%i.length,s=i[o];if((!t||t(s))&&!this.reused.has(s))return i.splice(o,1),o<a&&this.index[r]--,this.reused.set(s,n),s}return null}findWidget(e,t,n){let r=this.buckets[0];if(r.length)for(let i=0,a=0;;i++){if(i==r.length){if(a)return null;a=1,i=0}let o=r[i];if(!this.reused.has(o)&&(a==0?o.widget.compare(e):o.widget.constructor==e.constructor&&e.updateDOM(o.dom,this.view,o.widget)))return r.splice(i,1),i<this.index[0]&&this.index[0]--,o.widget==e&&o.length==t&&(o.flags&497)==n?(this.reused.set(o,1),o):(this.reused.set(o,2),new wi(o.dom,t,e,o.flags&-498|n))}}reuse(e){return this.reused.set(e,1),e}maybeReuse(e,t=2){if(!this.reused.has(e))return this.reused.set(e,t),e.dom}clear(){for(let e=0;e<this.buckets.length;e++)this.buckets[e].length=this.index[e]=0}},Mi=class{constructor(e,t,n,r,i){this.view=e,this.decorations=r,this.disallowBlockEffectsFor=i,this.openWidget=!1,this.openMarks=0,this.cache=new ji(e),this.text=new ki(e.state.doc),this.builder=new Oi(this.cache,new _i(e,e.contentDOM),F.iter(n)),this.cache.reused.set(t,2),this.old=new Ei(t),this.reuseWalker={skip:(e,t,n)=>{if(this.cache.add(e),e.isComposite())return!1},enter:e=>this.cache.add(e),leave:()=>{},break:()=>{}}}run(e,t){let n=t&&this.getCompositionContext(t.text);for(let r=0,i=0,a=0;;){let o=a<e.length?e[a++]:null,s=o?o.fromA:this.old.root.length;if(s>r){let e=s-r;this.preserve(e,!a,!o),r=s,i+=e}if(!o)break;t&&o.fromA<=t.range.fromA&&o.toA>=t.range.toA?(this.forward(o.fromA,t.range.fromA,t.range.fromA<t.range.toA?1:-1),this.emit(i,t.range.fromB),this.builder.flushBuffer(),this.cache.clear(),this.builder.addComposition(t,n),this.text.skip(t.range.toB-t.range.fromB),this.forward(t.range.fromA,o.toA),this.emit(t.range.toB,o.toB)):(this.forward(o.fromA,o.toA),this.emit(i,o.toB)),i=o.toB,r=o.toA}return this.builder.curLine&&this.builder.endLine(),this.builder.root}preserve(e,t,n){let r=Li(this.old),i=this.openMarks;this.old.advance(e,n?1:-1,{skip:(e,t,n)=>{if(e.isWidget()){if(this.openWidget)this.builder.continueWidget(n-t);else{let a=n>0||t<e.length?wi.of(e.widget,this.view,n-t,e.flags&496,this.cache.maybeReuse(e)):this.cache.reuse(e);a.flags&256?(a.flags&=-2,this.builder.addBlockWidget(a)):(this.builder.ensureLine(null),this.builder.addInlineWidget(a,r,i),i=r.length)}}else if(e.isText())this.builder.ensureLine(null),!t&&n==e.length&&!this.cache.reused.has(e)?this.builder.addText(e.text,r,i,this.cache.reuse(e)):(this.cache.add(e),this.builder.addText(e.text.slice(t,n),r,i)),i=r.length;else if(e.isLine())e.flags&=-2,this.cache.reused.set(e,1),this.builder.addLine(e);else if(e instanceof Ti)this.cache.add(e);else if(e instanceof Si)this.builder.ensureLine(null),this.builder.addMark(e,r,i),this.cache.reused.set(e,1),i=r.length;else return!1;this.openWidget=!1},enter:e=>{e.isLine()?this.builder.addLineStart(e.attrs,this.cache.maybeReuse(e)):(this.cache.add(e),e instanceof Si&&r.unshift(e.mark)),this.openWidget=!1},leave:e=>{e.isLine()?r.length&&=i=0:e instanceof Si&&(r.shift(),i=Math.min(i,r.length))},break:()=>{this.builder.addBreak(),this.openWidget=!1}}),this.text.skip(e)}emit(e,t){let n=null,r=this.builder,i=-1,a=F.spans(this.decorations,e,t,{point:(e,t,a,o,s,c)=>{if(a instanceof Nn){if(this.disallowBlockEffectsFor[c]){if(a.block)throw RangeError(`Block decorations may not be specified via plugins`);if(t>this.view.state.doc.lineAt(e).to)throw RangeError(`Decorations that replace line breaks may not be specified via plugins`)}if(i=o.length,s>o.length)r.continueWidget(t-e);else{let i=a.widget||(a.block?zi.block:zi.inline),c=Pi(a),l=this.cache.findWidget(i,t-e,c)||wi.of(i,this.view,t-e,c);a.block?(a.startSide>0&&r.addLineStartIfNotCovered(n),r.addBlockWidget(l)):(r.ensureLine(n),r.addInlineWidget(l,o,s))}n=null}else n=Ii(n,a);t>e&&this.text.skip(t-e)},span:(e,t,a,o)=>{for(let i=e;i<t;){let s=this.text.next(Math.min(512,t-i));s==null?(r.addLineStartIfNotCovered(n),r.addBreak(),i++):(r.ensureLine(n),r.addText(s,a,i==e?o:a.length),i+=s.length),n=null}i=a.length}});i>-1&&(this.openWidget=a>i),this.openWidget||r.addLineStartIfNotCovered(n),this.openMarks=a}forward(e,t,n=1){t-e<=10?this.old.advance(t-e,n,this.reuseWalker):(this.old.advance(5,-1,this.reuseWalker),this.old.advance(t-e-10,-1),this.old.advance(5,n,this.reuseWalker))}getCompositionContext(e){let t=[],n=null;for(let r=e.parentNode;;r=r.parentNode){let e=U.get(r);if(r==this.view.contentDOM)break;e instanceof Si?t.push(e):e?.isLine()?n=e:e instanceof vi||(r.nodeName==`DIV`&&!n?n=new yi(r,Fi):n||t.push(Si.of(new jn({tagName:r.nodeName.toLowerCase(),attributes:On(r)}),r)))}return n?{line:n,marks:t}:null}};function Ni(e,t){let n=e=>{for(let r of e.children)if((t?r.isText():r.length)||n(r))return!0;return!1};return n(e)}function Pi(e){let t=e.isReplace?(e.startSide<0?64:0)|(e.endSide>0?128:0):e.startSide>0?32:16;return e.block&&(t|=256),t}var Fi={class:`cm-line`};function Ii(e,t){let n=t.spec.attributes,r=t.spec.class;return!n&&!r||(e||={class:`cm-line`},n&&Cn(n,e),r&&(e.class+=` `+r)),e}function Li(e){let t=[];for(let n=e.parents.length;n>1;n--){let r=n==e.parents.length?e.tile:e.parents[n].tile;r instanceof Si&&t.push(r.mark)}return t}function Ri(e){let t=U.get(e);return t&&t.setDOM(e.cloneNode()),e}var zi=class extends kn{constructor(e){super(),this.tag=e}eq(e){return e.tag==this.tag}toDOM(){return document.createElement(this.tag)}updateDOM(e){return e.nodeName.toLowerCase()==this.tag}get isHidden(){return!0}};zi.inline=new zi(`span`),zi.block=new zi(`div`);var Bi=new class extends kn{toDOM(){return document.createElement(`br`)}get isHidden(){return!0}get editable(){return!0}},Vi=class{constructor(e){this.view=e,this.decorations=[],this.blockWrappers=[],this.dynamicDecorationMap=[!1],this.domChanged=null,this.hasComposition=null,this.editContextFormatting=R.none,this.lastCompositionAfterCursor=!1,this.minWidth=0,this.minWidthFrom=0,this.minWidthTo=0,this.impreciseAnchor=null,this.impreciseHead=null,this.forceSelection=!1,this.lastUpdate=Date.now(),this.updateDeco(),this.tile=new _i(e,e.contentDOM),this.updateInner([new fi(0,0,0,e.state.doc.length)],null)}update(e){let t=e.changedRanges;this.minWidth>0&&t.length&&(t.every(({fromA:e,toA:t})=>t<this.minWidthFrom||e>this.minWidthTo)?(this.minWidthFrom=e.changes.mapPos(this.minWidthFrom,1),this.minWidthTo=e.changes.mapPos(this.minWidthTo,1)):this.minWidth=this.minWidthFrom=this.minWidthTo=0),this.updateEditContextFormatting(e);let n=-1;this.view.inputState.composing>=0&&!this.view.observer.editContext&&(this.domChanged?.newSel?n=this.domChanged.newSel.head:!Qi(e.changes,this.hasComposition)&&!e.selectionSet&&(n=e.state.selection.main.head));let r=n>-1?Gi(this.view,e.changes,n):null;if(this.domChanged=null,this.hasComposition){let{from:n,to:r}=this.hasComposition;t=new fi(n,r,e.changes.mapPos(n,-1),e.changes.mapPos(r,1)).addToSet(t.slice())}this.hasComposition=r?{from:r.range.fromB,to:r.range.toB}:null,(L.ie||L.chrome)&&!r&&e&&e.state.doc.lines!=e.startState.doc.lines&&(this.forceSelection=!0);let i=this.decorations,a=this.blockWrappers;this.updateDeco();let o=Ji(i,this.decorations,e.changes);o.length&&(t=fi.extendWithRanges(t,o));let s=Xi(a,this.blockWrappers,e.changes);return s.length&&(t=fi.extendWithRanges(t,s)),r&&!t.some(e=>e.fromA<=r.range.fromA&&e.toA>=r.range.toA)&&(t=r.range.addToSet(t.slice())),this.tile.flags&2&&t.length==0?!1:(this.updateInner(t,r),e.transactions.length&&(this.lastUpdate=Date.now()),!0)}updateInner(e,t){this.view.viewState.mustMeasureContent=!0;let{observer:n}=this.view;n.ignore(()=>{if(t||e.length){let n=this.tile,r=new Mi(this.view,n,this.blockWrappers,this.decorations,this.dynamicDecorationMap);t&&U.get(t.text)&&r.cache.reused.set(U.get(t.text),2),this.tile=r.run(e,t),Hi(n,r.cache.reused)}this.tile.dom.style.height=this.view.viewState.contentHeight/this.view.scaleY+`px`,this.tile.dom.style.flexBasis=this.minWidth?this.minWidth+`px`:``;let r=L.chrome||L.ios?{node:n.selectionRange.focusNode,written:!1}:void 0;this.tile.sync(r),r&&(r.written||n.selectionRange.focusNode!=r.node||!this.tile.dom.contains(r.node))&&(this.forceSelection=!0),this.tile.dom.style.height=``});let r=[];if(this.view.viewport.from||this.view.viewport.to<this.view.state.doc.length)for(let e of this.tile.children)e.isWidget()&&e.widget instanceof $i&&r.push(e.dom);n.updateGaps(r)}updateEditContextFormatting(e){this.editContextFormatting=this.editContextFormatting.map(e.changes);for(let t of e.transactions)for(let e of t.effects)e.is(Jr)&&(this.editContextFormatting=e.value)}updateSelection(e=!1,t=!1){(e||!this.view.observer.selectionRange.focusNode)&&this.view.observer.readSelectionRange();let{dom:n}=this.tile,r=this.view.root.activeElement,i=r==n,a=!i&&!(this.view.state.facet(Xr)||n.tabIndex>-1)&&Vn(n,this.view.observer.selectionRange)&&!(r&&n.contains(r));if(!(i||t||a))return;let o=this.forceSelection;this.forceSelection=!1;let s=this.view.state.selection.main,c,l;if(s.empty?l=c=this.inlineDOMNearPos(s.anchor,s.assoc||1):(l=this.inlineDOMNearPos(s.head,s.head==s.from?1:-1),c=this.inlineDOMNearPos(s.anchor,s.anchor==s.from?1:-1)),L.gecko&&s.empty&&!this.hasComposition&&Ui(c)){let e=document.createTextNode(``);this.view.observer.ignore(()=>c.node.insertBefore(e,c.node.childNodes[c.offset]||null)),c=l=new fr(e,0),o=!0}let u=this.view.observer.selectionRange;!o&&u.focusNode&&(Un(c.node,c.offset,u.anchorNode,u.anchorOffset)&&Un(l.node,l.offset,u.focusNode,u.focusOffset)||this.suppressWidgetCursorChange(u,s))||(this.view.observer.ignore(()=>{L.android&&L.chrome&&n.contains(u.focusNode)&&Zi(u.focusNode,n)&&(n.blur(),n.focus({preventScroll:!0}));let e=zn(this.view.root);if(e){if(s.empty){if(L.gecko){let e=Ki(c.node,c.offset);if(e&&e!=3){let t=(e==1?ur:dr)(c.node,c.offset);t&&(c=new fr(t.node,t.offset))}}e.collapse(c.node,c.offset),s.bidiLevel!=null&&e.caretBidiLevel!==void 0&&(e.caretBidiLevel=s.bidiLevel)}else if(e.extend){e.collapse(c.node,c.offset);try{e.extend(l.node,l.offset)}catch{}}else{let t=document.createRange();s.anchor>s.head&&([c,l]=[l,c]),t.setEnd(l.node,l.offset),t.setStart(c.node,c.offset),e.removeAllRanges(),e.addRange(t)}}a&&this.view.root.activeElement==n&&(n.blur(),r&&r.focus())}),this.view.observer.setSelectionRange(c,l)),this.impreciseAnchor=c.precise?null:new fr(u.anchorNode,u.anchorOffset),this.impreciseHead=l.precise?null:new fr(u.focusNode,u.focusOffset)}suppressWidgetCursorChange(e,t){return this.hasComposition&&t.empty&&Un(e.focusNode,e.focusOffset,e.anchorNode,e.anchorOffset)&&this.posFromDOM(e.focusNode,e.focusOffset)==t.head}enforceCursorAssoc(){if(this.hasComposition)return;let{view:e}=this,t=e.state.selection.main,n=zn(e.root),{anchorNode:r,anchorOffset:i}=e.observer.selectionRange;if(!n||!t.empty||!t.assoc||!n.modify)return;let a=this.lineAt(t.head,t.assoc);if(!a)return;let o=a.posAtStart;if(t.head==o||t.head==o+a.length)return;let s=this.coordsAt(t.head,-1),c=this.coordsAt(t.head,1);if(!s||!c||s.bottom>c.top)return;let l=this.domAtPos(t.head+t.assoc,t.assoc);n.collapse(l.node,l.offset),n.modify(`move`,t.assoc<0?`forward`:`backward`,`lineboundary`),e.observer.readSelectionRange();let u=e.observer.selectionRange;e.docView.posFromDOM(u.anchorNode,u.anchorOffset)!=t.from&&n.collapse(r,i)}posFromDOM(e,t){let n=this.tile.nearest(e);if(!n)return this.tile.dom.compareDocumentPosition(e)&2?0:this.view.state.doc.length;let r=n.posAtStart;if(n.isComposite()){let i;if(e==n.dom)i=n.dom.childNodes[t];else{let r=qn(e)==0?0:t==0?-1:1;for(;;){let t=e.parentNode;if(t==n.dom)break;r==0&&t.firstChild!=t.lastChild&&(r=e==t.firstChild?-1:1),e=t}i=r<0?e:e.nextSibling}if(i==n.dom.firstChild)return r;for(;i&&!U.get(i);)i=i.nextSibling;if(!i)return r+n.length;for(let e=0,t=r;;e++){let r=n.children[e];if(r.dom==i)return t;t+=r.length+r.breakAfter}}else if(n.isText())return e==n.dom?r+t:r+(t?n.length:0);else return r}domAtPos(e,t){let{tile:n,offset:r}=this.tile.resolveBlock(e,t);return n.isWidget()?n.domPosFor(r,t):n.domIn(r,t)}inlineDOMNearPos(e,t){let n,r=-1,i=!1,a,o=-1,s=!1;return this.tile.blockTiles((t,c)=>{if(t.isWidget()){if(t.flags&32&&c>=e)return!0;t.flags&16&&(i=!0)}else{let l=c+t.length;if(c<=e&&(n=t,r=e-c,i=l<e),l>=e&&!a&&(a=t,o=e-c,s=c>e),c>e&&a)return!0}}),!n&&!a?this.domAtPos(e,t):(i&&a?n=null:s&&n&&(a=null),n&&t<0||!a?n.domIn(r,t):a.domIn(o,t))}coordsAt(e,t,n){let{tile:r,offset:i}=this.tile.resolveBlock(e,t);return r.isWidget()?r.widget instanceof $i?null:r.coordsInWidget(i,t,!0):r.coordsIn(i,t,n)}lineAt(e,t){let{tile:n}=this.tile.resolveBlock(e,t);return n.isLine()?n:null}coordsForChar(e){let{tile:t,offset:n}=this.tile.resolveBlock(e,1);if(!t.isLine())return null;function r(e,t){if(e.isComposite())for(let n of e.children){if(n.length>=t){let e=r(n,t);if(e)return e}if(t-=n.length,t<0)break}else if(e.isText()&&t<e.length){let n=O(e.text,t);if(n==t)return null;let r=ar(e.dom,t,n).getClientRects();for(let e=0;e<r.length;e++){let t=r[e];if(e==r.length-1||t.top<t.bottom&&t.left<t.right)return t}}return null}return r(t,n)}measureVisibleLineHeights(e){let t=[],{from:n,to:r}=e,i=this.view.contentDOM.clientWidth,a=i>Math.max(this.view.scrollDOM.clientWidth,this.minWidth)+1,o=-1,s=this.view.textDirection==B.LTR,c=0,l=(e,u,d)=>{for(let f=0;f<e.children.length&&!(u>r);f++){let r=e.children[f],p=u+r.length,m=r.dom.getBoundingClientRect(),{height:h}=m;if(d&&!f&&(c+=m.top-d.top),r instanceof vi)p>n&&l(r,u,m);else if(u>=n&&(c>0&&t.push(-c),t.push(h+c),c=0,a)){let e=r.dom.lastChild,t=e?Hn(e):[];if(t.length){let e=t[t.length-1],n=s?e.right-m.left:m.right-e.left;n>o&&(o=n,this.minWidth=i,this.minWidthFrom=u,this.minWidthTo=p)}}d&&f==e.children.length-1&&(c+=d.bottom-m.bottom),u=p+r.breakAfter}};return l(this.tile,0,null),t}textDirectionAt(e){let{tile:t}=this.tile.resolveBlock(e,1);return getComputedStyle(t.dom).direction==`rtl`?B.RTL:B.LTR}measureTextSize(){let e=this.tile.blockTiles(e=>{if(e.isLine()&&e.children.length&&e.length<=20){let t=0,n;for(let r of e.children){if(!r.isText()||/[^ -~]/.test(r.text))return;let e=Hn(r.dom);if(e.length!=1)return;t+=e[0].width,n=e[0].height}if(t)return{lineHeight:e.dom.getBoundingClientRect().height,charWidth:t/e.length,textHeight:n}}});if(e)return e;let t=document.createElement(`div`),n,r,i;return t.className=`cm-line`,t.style.width=`99999px`,t.style.position=`absolute`,t.textContent=`abc def ghi jkl mno pqr stu`,this.view.observer.ignore(()=>{this.tile.dom.appendChild(t);let e=Hn(t.firstChild)[0];n=t.getBoundingClientRect().height,r=e&&e.width?e.width/27:7,i=e&&e.height?e.height:n,t.remove()}),{lineHeight:n,charWidth:r,textHeight:i}}computeBlockGapDeco(){let e=[],t=this.view.viewState;for(let n=0,r=0;;r++){let i=r==t.viewports.length?null:t.viewports[r],a=i?i.from-1:this.view.state.doc.length;if(a>n){let r=(t.lineBlockAt(a).bottom-t.lineBlockAt(n).top)/this.view.scaleY;e.push(R.replace({widget:new $i(r),block:!0,inclusive:!0,isBlockGap:!0}).range(n,a))}if(!i)break;n=i.to+1}return R.set(e)}updateDeco(){let e=1,t=this.view.state.facet(ni).map(t=>(this.dynamicDecorationMap[e++]=typeof t==`function`)?t(this.view):t),n=!1,r=this.view.state.facet(ii).map((e,t)=>{let r=typeof e==`function`;return r&&(n=!0),r?e(this.view):e});for(r.length&&(this.dynamicDecorationMap[e++]=n,t.push(F.join(r))),this.decorations=[this.editContextFormatting,...t,this.computeBlockGapDeco(),this.view.viewState.lineGapDeco];e<this.decorations.length;)this.dynamicDecorationMap[e++]=!1;this.blockWrappers=this.view.state.facet(ri).map(e=>typeof e==`function`?e(this.view):e)}scrollIntoView(e){if(e.isSnapshot){let t=this.view.viewState.lineBlockAt(e.range.head);this.view.scrollDOM.scrollTop=t.top-e.yMargin,this.view.scrollDOM.scrollLeft=e.xMargin;return}for(let t of this.view.state.facet(Gr))try{if(t(this.view,e.range,e))return!0}catch(e){Yr(this.view.state,e,`scroll handler`)}let{range:t}=e,n=this.coordsAt(t.head,t.assoc||(t.head>t.anchor?-1:1)),r;if(!n)return;!t.empty&&(r=this.coordsAt(t.anchor,t.anchor>t.head?-1:1))&&(n={left:Math.min(n.left,r.left),top:Math.min(n.top,r.top),right:Math.max(n.right,r.right),bottom:Math.max(n.bottom,r.bottom)});let i=ui(this.view),a={left:n.left-i.left,top:n.top-i.top,right:n.right+i.right,bottom:n.bottom+i.bottom},{offsetWidth:o,offsetHeight:s}=this.view.scrollDOM;if(Zn(this.view.scrollDOM,a,t.head<t.anchor?-1:1,e.x,e.y,Math.max(Math.min(e.xMargin,o),-o),Math.max(Math.min(e.yMargin,s),-s),this.view.textDirection==B.LTR),window.visualViewport&&window.innerHeight-window.visualViewport.height>1&&(n.top>window.visualViewport.offsetTop+window.visualViewport.height||n.bottom<window.visualViewport.offsetTop)){let e=this.view.docView.lineAt(t.head,1);if(e){let t=er(e.dom);e.dom.scrollIntoView({block:`nearest`}),tr(t,!1)}}}lineHasWidget(e){let t=e=>e.isWidget()||e.children.some(t);return t(this.tile.resolveBlock(e,1).tile)}destroy(){Hi(this.tile)}};function Hi(e,t){let n=t?.get(e);if(n!=1){n??e.destroy();for(let n of e.children)Hi(n,t)}}function Ui(e){return z(e.node)&&e.node.firstChild&&(e.offset==0||e.node.childNodes[e.offset-1].contentEditable==`false`)&&(e.offset==e.node.childNodes.length||e.node.childNodes[e.offset].contentEditable==`false`)}function Wi(e,t){let n=e.observer.selectionRange;if(!n.focusNode)return null;let r=ur(n.focusNode,n.focusOffset),i=dr(n.focusNode,n.focusOffset),a=r||i;if(i&&r&&i.node!=r.node){let t=U.get(i.node);if(!t||t.isText()&&t.text!=i.node.nodeValue)a=i;else if(e.docView.lastCompositionAfterCursor){let e=U.get(r.node);!e||e.isText()&&e.text!=r.node.nodeValue||(a=i)}}if(e.docView.lastCompositionAfterCursor=a!=r,!a)return null;let o=t-a.offset;return{from:o,to:o+a.node.nodeValue.length,node:a.node}}function Gi(e,t,n){let r=Wi(e,n);if(!r)return null;let{node:i,from:a,to:o}=r,s=i.nodeValue;if(/[\n\r]/.test(s)||e.state.doc.sliceString(r.from,r.to)!=s)return null;let c=t.invertedDesc;return{range:new fi(c.mapPos(a),c.mapPos(o),a,o),text:i}}function Ki(e,t){return z(e)?(t&&e.childNodes[t-1].contentEditable==`false`?1:0)|(t<e.childNodes.length&&e.childNodes[t].contentEditable==`false`?2:0):0}var qi=class{constructor(){this.changes=[]}compareRange(e,t){In(e,t,this.changes)}comparePoint(e,t){In(e,t,this.changes)}boundChange(e){In(e,e,this.changes)}};function Ji(e,t,n){let r=new qi;return F.compare(e,t,n,r),r.changes}var Yi=class{constructor(){this.changes=[]}compareRange(e,t){In(e,t,this.changes)}comparePoint(){}boundChange(e){In(e,e,this.changes)}};function Xi(e,t,n){let r=new Yi;return F.compare(e,t,n,r),r.changes}function Zi(e,t){for(let n=e;n&&n!=t;n=n.assignedSlot||n.parentNode)if(z(n)&&n.contentEditable==`false`)return!0;return!1}function Qi(e,t){let n=!1;return t&&e.iterChangedRanges((e,r)=>{e<t.to&&r>t.from&&(n=!0)}),n}var $i=class extends kn{constructor(e){super(),this.height=e}toDOM(){let e=document.createElement(`div`);return e.className=`cm-gap`,this.updateDOM(e),e}eq(e){return e.height==this.height}updateDOM(e){return e.style.height=this.height+`px`,!0}get editable(){return!0}get estimatedHeight(){return this.height}ignoreEvent(){return!1}};function ea(e,t,n=1){let r=e.charCategorizer(t),i=e.doc.lineAt(t),a=t-i.from;if(i.length==0)return A.cursor(t);a==0?n=1:a==i.length&&(n=-1);let o=a,s=a;n<0?o=O(i.text,a,!1):s=O(i.text,a);let c=r(i.text.slice(o,s));for(;o>0;){let e=O(i.text,o,!1);if(r(i.text.slice(e,o))!=c)break;o=e}for(;s<i.length;){let e=O(i.text,s);if(r(i.text.slice(s,e))!=c)break;s=e}return A.undirectionalRange(o+i.from,s+i.from)}function ta(e,t,n,r,i){let a=Math.round((r-t.left)*e.defaultCharacterWidth);if(e.lineWrapping&&n.height>e.defaultLineHeight*1.5){let t=e.viewState.heightOracle.textHeight,r=Math.floor((i-n.top-(e.defaultLineHeight-t)*.5)/t);a+=r*e.viewState.heightOracle.lineLength}let o=e.state.sliceDoc(n.from,n.to);return n.from+Yt(o,a,e.state.tabSize)}function na(e,t,n){let r=e.lineBlockAt(t);if(Array.isArray(r.type)){let e;for(let i of r.type){if(i.from>t)break;if(!(i.to<t)){if(i.from<t&&i.to>t)return i;(!e||i.type==An.Text&&(e.type!=i.type||(n<0?i.from<t:i.to>t)))&&(e=i)}}return e||r}return r}function ra(e,t,n,r){let i=na(e,t.head,t.assoc||-1),a=!r||i.type!=An.Text||!(e.lineWrapping||i.widgetLineBreaks)?null:e.coordsAtPos(t.assoc<0&&t.head>i.from?t.head-1:t.head);if(a){let t=e.dom.getBoundingClientRect(),r=e.textDirectionAt(i.from),o=e.posAndSideAtCoords({x:n==(r==B.LTR)?t.right-1:t.left+1,y:(a.top+a.bottom)/2});if(o)return A.cursor(o.pos,o.assoc)}return A.cursor(n?i.to:i.from,n?1:-1)}function ia(e,t,n,r){let i=e.state.doc.lineAt(t.head),a=e.bidiSpans(i),o=e.textDirectionAt(i.from);for(let s=t,c=null;;){let t=Mr(i,a,o,s,n),l=jr;if(!t){if(i.number==(n?e.state.doc.lines:1))return s;l=`
`,i=e.state.doc.line(i.number+(n?1:-1)),a=e.bidiSpans(i),t=n?A.cursor(i.from,-1):A.cursor(i.to,1)}if(!c){if(!r)return t;c=r(l)}else if(!c(l))return s;s=t}}function aa(e,t,n){let r=e.state.charCategorizer(t),i=r(n);return e=>{let t=r(e);return i==N.Space&&(i=t),i==t}}function oa(e,t,n,r){let i=t.head,a=n?1:-1;if(i==(n?e.state.doc.length:0))return A.cursor(i,t.assoc);let o=t.goalColumn,s,c=e.contentDOM.getBoundingClientRect(),l=e.coordsAtPos(i,t.assoc||((t.empty?n:t.head==t.from)?1:-1)),u=e.documentTop;if(l)o??=l.left-c.left,s=a<0?l.top:l.bottom;else{let t=e.viewState.lineBlockAt(i);o??=Math.min(c.right-c.left,e.defaultCharacterWidth*(i-t.from)),s=(a<0?t.top:t.bottom)+u}let d=c.left+o,f=e.viewState.heightOracle.textHeight>>1,p=r??f;for(let t=0;;t+=f){let r=s+(p+t)*a,i=da(e,{x:d,y:r},!1,a);if(n?r>c.bottom:r<c.top)return A.cursor(i.pos,i.assoc);let l=e.coordsAtPos(i.pos,i.assoc),u=l?(l.top+l.bottom)/2:0;if(!l||(n?u>s:u<s))return A.cursor(i.pos,i.assoc,void 0,o)}}function sa(e,t,n){for(;;){let r=0;for(let i of e)i.between(t-1,t+1,(e,i,a)=>{if(t>e&&t<i){let a=r||n||(t-e<i-t?-1:1);t=a<0?e:i,r=a}});if(!r)return t}}function ca(e,t){let n=null;for(let r=0;r<t.ranges.length;r++){let i=t.ranges[r],a=null;if(i.empty){let t=sa(e,i.from,0);t!=i.from&&(a=A.cursor(t,-1))}else{let t=sa(e,i.from,-1),n=sa(e,i.to,1);(t!=i.from||n!=i.to)&&(a=i.undirectional?A.undirectionalRange(i.from,i.to):A.range(i.from==i.anchor?t:n,i.from==i.head?t:n))}a&&(n||=t.ranges.slice(),n[r]=a)}return n?A.create(n,t.mainIndex):t}function la(e,t,n){let r=sa(e.state.facet(ai).map(t=>t(e)),n.from,t.head>n.from?-1:1);return r==n.from?n:A.cursor(r,r<n.from?1:-1)}var ua=class{constructor(e,t){this.pos=e,this.assoc=t}};function da(e,t,n,r){let i=e.contentDOM.getBoundingClientRect(),a=i.top+e.viewState.paddingTop,{x:o,y:s}=t,c=s-a,l;for(;;){if(c<0)return new ua(0,1);if(c>e.viewState.docHeight)return new ua(e.state.doc.length,-1);if(l=e.elementAtHeight(c),r==null)break;if(l.type==An.Text){if(r<0?l.to<e.viewport.from:l.from>e.viewport.to)break;let t=e.docView.coordsAt(r<0?l.from:l.to,r>0?-1:1);if(t&&(r<0?t.top<=c+a:t.bottom>=c+a))break}let t=e.viewState.heightOracle.textHeight/2;c=r>0?l.bottom+t:l.top-t}if(e.viewport.from>=l.to||e.viewport.to<=l.from){if(n)return null;if(l.type==An.Text){let t=ta(e,i,l,o,s);return new ua(t,t==l.from?1:-1)}}if(l.type!=An.Text)return c<(l.top+l.bottom)/2?new ua(l.from,1):new ua(l.to,-1);let u=e.docView.lineAt(l.from,2);return u&&u.length==l.length||(u=e.docView.lineAt(l.from,-2)),new fa(e,o,s,e.textDirectionAt(l.from)).scanTile(u,l.from)}var fa=class{constructor(e,t,n,r){this.view=e,this.x=t,this.y=n,this.baseDir=r,this.line=null,this.spans=null}bidiSpansAt(e){return(!this.line||this.line.from>e||this.line.to<e)&&(this.line=this.view.state.doc.lineAt(e),this.spans=this.view.bidiSpans(this.line)),this}baseDirAt(e,t){let{line:n,spans:r}=this.bidiSpansAt(e);return r[Sr.find(r,e-n.from,-1,t)].level==this.baseDir}dirAt(e,t){let{line:n,spans:r}=this.bidiSpansAt(e);return r[Sr.find(r,e-n.from,-1,t)].dir}bidiIn(e,t){let{spans:n,line:r}=this.bidiSpansAt(e);return n.length>1||n.length&&(n[0].level!=this.baseDir||n[0].to+r.from<t)}scan(e,t,n=!1){let r=0,i=e.length-1,a=new Set,o=this.bidiIn(e[0],e[i]),s,c,l=-1,u=1e9,d;search:for(;r<i;){let n=i-r,f=r+i>>1;adjust:if(a.has(f)){for(let e=1;e<n;e++){let t=f+e;if(t>=i&&(t-=n),!a.has(t)){f=t;break adjust}}break search}a.add(f);let p=t(f),m=0;if(p)for(let e=0;e<p.length;e++){let t=p[e];if(!(t.width==0&&p.length>1)){if(t.bottom<this.y)(!s||s.bottom<t.bottom)&&(s=t),m=1;else if(t.top>this.y)(!c||c.top>t.top)&&(c=t),m=-1;else{let e=t.left>this.x?this.x-t.left:t.right<this.x?this.x-t.right:0,n=Math.abs(e);n<u&&(l=f,u=n,d=t),e&&(m=e<0==(this.baseDir==B.LTR)?-1:1)}}}m==-1&&(!o||this.baseDirAt(e[f],1))?i=f:m==1&&(!o||this.baseDirAt(e[f+1],-1))&&(r=f+1)}if(!d){if(!c&&!s)return{i:0,after:!1};let n=s&&(!c||this.y-s.bottom<c.top-this.y)?s:c;return this.y=(n.top+n.bottom)/2,this.scan(e,t,!0)}if(u&&!n){let{top:n,bottom:r}=d;if(s&&s.bottom>(n+n+r)/3)return this.y=s.bottom-1,this.scan(e,t,!0);if(c&&c.top<(n+r+r)/3)return this.y=c.top+1,this.scan(e,t,!0)}let f=(o?this.dirAt(e[l],1):this.baseDir)==B.LTR;return{i:l,after:this.x>(d.left+d.right)/2==f}}scanText(e,t){let n=[];for(let r=0;r<e.length;r=O(e.text,r))n.push(t+r);n.push(t+e.length);let r=this.scan(n,r=>{let i=n[r]-t,a=n[r+1]-t;return ar(e.dom,i,a).getClientRects()});return r.after?new ua(n[r.i+1],-1):new ua(n[r.i],1)}scanTile(e,t){if(!e.length)return new ua(t,1);if(e.children.length==1){let n=e.children[0];if(n.isText())return this.scanText(n,t);if(n.isComposite())return this.scanTile(n,t)}let n=[t];for(let r=0,i=t;r<e.children.length;r++)n.push(i+=e.children[r].length);let r=this.scan(n,t=>{let n=e.children[t];return n.flags&48?null:(z(n.dom)?n.dom:ar(n.dom,0,n.length)).getClientRects()}),i=e.children[r.i],a=n[r.i];return i.isText()?this.scanText(i,a):i.isComposite()?this.scanTile(i,a):r.after?new ua(n[r.i+1],-1):new ua(a,1)}},pa=`￿`,ma=class{constructor(e,t){this.points=e,this.view=t,this.text=``,this.lineSeparator=t.state.facet(P.lineSeparator)}append(e){this.text+=e}lineBreak(){this.text+=pa}readRange(e,t){if(!e)return this;let n=e.parentNode;for(let r=e;;){this.findPointBefore(n,r);let e=this.text.length;this.readNode(r);let i=U.get(r),a=r.nextSibling;if(a==t){i?.breakAfter&&!a&&n!=this.view.contentDOM&&this.lineBreak();break}let o=U.get(a);(i&&o?i.breakAfter:(i?i.breakAfter:Gn(r))||Gn(a)&&(r.nodeName!=`BR`||i?.isWidget())&&this.text.length>e)&&!ga(a,t)&&this.lineBreak(),r=a}return this.findPointBefore(n,t),this}readTextNode(e){let t=e.nodeValue;for(let n of this.points)n.node==e&&(n.pos=this.text.length+Math.min(n.offset,t.length));for(let n=0,r=this.lineSeparator?null:/\r\n?|\n/g;;){let i=-1,a=1,o;if(this.lineSeparator?(i=t.indexOf(this.lineSeparator,n),a=this.lineSeparator.length):(o=r.exec(t))&&(i=o.index,a=o[0].length),this.append(t.slice(n,i<0?t.length:i)),i<0)break;if(this.lineBreak(),a>1)for(let t of this.points)t.node==e&&t.pos>this.text.length&&(t.pos-=a-1);n=i+a}}readNode(e){let t=U.get(e),n=t&&t.overrideDOMText;if(n!=null){this.findPointInside(e,n.length);for(let e=n.iter();!e.next().done;)e.lineBreak?this.lineBreak():this.append(e.value)}else Rn(e)?this.readTextNode(e):e.nodeName==`BR`?e.nextSibling&&this.lineBreak():z(e)&&this.readRange(e.firstChild,null)}findPointBefore(e,t){for(let n of this.points)n.node==e&&e.childNodes[n.offset]==t&&(n.pos=this.text.length)}findPointInside(e,t){for(let n of this.points)(Rn(e)?n.node==e:e.contains(n.node))&&(n.pos=this.text.length+(ha(e,n.node,n.offset)?t:0))}};function ha(e,t,n){for(;;){if(!t||n<qn(t))return!1;if(t==e)return!0;n=Wn(t)+1,t=t.parentNode}}function ga(e,t){let n;for(;e!=t&&e;e=e.nextSibling){let t=U.get(e);if(!t?.isWidget())return!1;t&&(n||=[]).push(t)}if(n){for(let e of n)if(e.overrideDOMText?.length)return!1}return!0}var _a=class{constructor(e,t){this.node=e,this.offset=t,this.pos=-1}},va=class{constructor(e,t,n,r){this.typeOver=r,this.bounds=null,this.text=``,this.domChanged=t>-1;let{impreciseHead:i,impreciseAnchor:a}=e.docView,o=e.state.selection;if(e.state.readOnly&&t>-1)this.newSel=null;else if(t>-1&&(this.bounds=ya(e.docView.tile,t,n,0))){let t=i||a?[]:wa(e),n=new ma(t,e);n.readRange(this.bounds.startDOM,this.bounds.endDOM),this.text=n.text,this.newSel=Ta(t,this.bounds.from)}else{let t=e.observer.selectionRange,n=i&&i.node==t.focusNode&&i.offset==t.focusOffset||!Bn(e.contentDOM,t.focusNode)?o.main.head:e.docView.posFromDOM(t.focusNode,t.focusOffset),r=a&&a.node==t.anchorNode&&a.offset==t.anchorOffset||!Bn(e.contentDOM,t.anchorNode)?o.main.anchor:e.docView.posFromDOM(t.anchorNode,t.anchorOffset),s=e.viewport;if((L.ios||L.chrome)&&n!=r&&Math.min(n,r)<=o.main.from&&Math.max(n,r)>=o.main.to&&(s.from>0||s.to<e.state.doc.length)){let t=Math.min(n,r),i=Math.max(n,r),a=s.from-t,o=s.to-i;a!=0&&a!=1&&t!=0||o!=0&&o!=-1&&i!=e.state.doc.length||(n=0,r=e.state.doc.length)}if(e.inputState.composing>-1&&o.ranges.length>1)this.newSel=o.replaceRange(A.range(r,n));else if(e.lineWrapping&&r==n&&!(o.main.empty&&o.main.head==n)&&e.inputState.lastTouchTime>Date.now()-100){let t=e.coordsAtPos(n,-1),r=0;t&&(r=e.inputState.lastTouchY<=t.bottom?-1:1),this.newSel=A.create([A.cursor(n,r)])}else this.newSel=A.single(r,n)}}};function ya(e,t,n,r){if(e.isComposite()){let i=-1,a=-1,o=-1,s=-1;for(let c=0,l=r,u=r;c<e.children.length;c++){let r=e.children[c],d=l+r.length;if(l<t&&d>n)return ya(r,t,n,l);if(d>=t&&i==-1&&(i=c,a=l),l>n&&r.dom.parentNode==e.dom){o=c,s=u;break}u=d,l=d+r.breakAfter}return{from:a,to:s<0?r+e.length:s,startDOM:(i?e.children[i-1].dom.nextSibling:null)||e.dom.firstChild,endDOM:o<e.children.length&&o>=0?e.children[o].dom:null}}return e.isText()?{from:r,to:r+e.length,startDOM:e.dom,endDOM:e.dom.nextSibling}:null}function ba(e,t){let n,{newSel:r}=t,{state:i}=e,a=i.selection.main,o=e.inputState.lastKeyTime>Date.now()-100?e.inputState.lastKeyCode:-1;if(t.bounds){let{from:e,to:r}=t.bounds,s=a.from,c=null;(o===8||L.android&&t.text.length<r-e)&&(s=a.to,c=`end`);let l=i.doc.sliceString(e,r,pa),u,d;!a.empty&&a.from>=e&&a.to<=r&&(t.typeOver||l!=t.text)&&l.slice(0,a.from-e)==t.text.slice(0,a.from-e)&&l.slice(a.to-e)==t.text.slice(u=t.text.length-(l.length-(a.to-e)))?n={from:a.from,to:a.to,insert:E.of(t.text.slice(a.from-e,u).split(pa))}:(d=Ca(l,t.text,s-e,c))&&(L.chrome&&o==13&&d.toB==d.from+2&&t.text.slice(d.from,d.toB)==`￿￿`&&d.toB--,n={from:e+d.from,to:e+d.toA,insert:E.of(t.text.slice(d.from,d.toB).split(pa))})}else r&&(!e.hasFocus&&i.facet(Xr)||Ea(r,a))&&(r=null);if(!n&&!r)return!1;if((L.mac||L.android)&&n&&n.from==n.to&&n.from==a.head-1&&/^\. ?$/.test(n.insert.toString())&&e.contentDOM.getAttribute(`autocorrect`)==`off`?(r&&n.insert.length==2&&(r=A.single(r.main.anchor-1,r.main.head-1)),n={from:n.from,to:n.to,insert:E.of([n.insert.toString().replace(`.`,` `)])}):i.doc.lineAt(a.from).to<a.to&&e.docView.lineHasWidget(a.to)&&e.inputState.insertingTextAt>Date.now()-50?n={from:a.from,to:a.to,insert:i.toText(e.inputState.insertingText)}:L.chrome&&n&&n.from==n.to&&n.from==a.head&&n.insert.toString()==`
 `&&e.lineWrapping&&(r&&=A.single(r.main.anchor-1,r.main.head-1),n={from:a.from,to:a.to,insert:E.of([` `])}),n)return xa(e,n,r,o);if(r&&!Ea(r,a)){let t=!1,n=`select`;return e.inputState.lastSelectionTime>Date.now()-50&&(e.inputState.lastSelectionOrigin==`select`&&(t=!0),n=e.inputState.lastSelectionOrigin,n==`select.pointer`&&(r=ca(i.facet(ai).map(t=>t(e)),r))),e.dispatch({selection:r,scrollIntoView:t,userEvent:n}),!0}return!1}function xa(e,t,n,r=-1){if(L.ios&&e.inputState.flushIOSKey(t))return!0;let i=e.state.selection.main;if(L.android&&(t.to==i.to&&(t.from==i.from||t.from==i.from-1&&e.state.sliceDoc(t.from,i.from)==` `)&&t.insert.length==1&&t.insert.lines==2&&or(e.contentDOM,`Enter`,13)||(t.from==i.from-1&&t.to==i.to&&t.insert.length==0||r==8&&t.insert.length<t.to-t.from&&t.to>i.head)&&or(e.contentDOM,`Backspace`,8)||t.from==i.from&&t.to==i.to+1&&t.insert.length==0&&or(e.contentDOM,`Delete`,46)))return!0;let a=t.insert.toString();e.inputState.composing>=0&&e.inputState.composing++;let o,s=()=>o||=Sa(e,t,n);return e.state.facet(zr).some(n=>n(e,t.from,t.to,a,s))||e.dispatch(s()),!0}function Sa(e,t,n){let r,i=e.state,a=i.selection.main,o=-1;if(t.from==t.to&&t.from<a.from||t.from>a.to){let n=t.from<a.from?-1:1,r=n<0?a.from:a.to,s=sa(i.facet(ai).map(t=>t(e)),r,n);t.from==s&&(o=s)}if(o>-1)r={changes:t,selection:A.cursor(t.from+t.insert.length,-1)};else if(t.from>=a.from&&t.to<=a.to&&t.to-t.from>=(a.to-a.from)/3&&(!n||n.main.empty&&n.main.from==t.from+t.insert.length)&&e.inputState.composing<0){let n=a.from<t.from?i.sliceDoc(a.from,t.from):``,o=a.to>t.to?i.sliceDoc(t.to,a.to):``;r=i.replaceSelection(e.state.toText(n+t.insert.sliceString(0,void 0,e.state.lineBreak)+o))}else{let o=i.changes(t),s=n&&n.main.to<=o.newLength?n.main:void 0;if(i.selection.ranges.length>1&&(e.inputState.composing>=0||e.inputState.compositionPendingChange)&&t.to<=a.to+10&&t.to>=a.to-10){let c=e.state.sliceDoc(t.from,t.to),l,u=n&&Wi(e,n.main.head);if(u){let e=t.insert.length-(t.to-t.from);l={from:u.from,to:u.to-e}}else l=e.state.doc.lineAt(a.head);let d=a.to-t.to;r=i.changeByRange(n=>{if(n.from==a.from&&n.to==a.to)return{changes:o,range:s||n.map(o)};let r=n.to-d,u=r-c.length;if(e.state.sliceDoc(u,r)!=c||r>=l.from&&u<=l.to)return{range:n};let f=i.changes({from:u,to:r,insert:t.insert}),p=n.to-a.to;return{changes:f,range:s?A.range(Math.max(0,s.anchor+p),Math.max(0,s.head+p)):n.map(f)}})}else r={changes:o,selection:s&&i.selection.replaceRange(s)}}let s=`input.type`;return(e.composing||e.inputState.compositionPendingChange&&e.inputState.compositionEndedAt>Date.now()-50)&&(e.inputState.compositionPendingChange=!1,s+=`.compose`,e.inputState.compositionFirstChange&&(s+=`.start`,e.inputState.compositionFirstChange=!1)),i.update(r,{userEvent:s,scrollIntoView:!0})}function Ca(e,t,n,r){let i=Math.min(e.length,t.length),a=0;for(;a<i&&e.charCodeAt(a)==t.charCodeAt(a);)a++;if(a==i&&e.length==t.length)return null;let o=e.length,s=t.length;for(;o>0&&s>0&&e.charCodeAt(o-1)==t.charCodeAt(s-1);)o--,s--;if(r==`end`){let e=Math.max(0,a-Math.min(o,s));n-=o+e-a}if(o<a&&e.length<t.length){let e=n<=a&&n>=o?a-n:0;a-=e,s=a+(s-o),o=a}else if(s<a){let e=n<=a&&n>=s?a-n:0;a-=e,o=a+(o-s),s=a}return{from:a,toA:o,toB:s}}function wa(e){let t=[];if(e.root.activeElement!=e.contentDOM)return t;let{anchorNode:n,anchorOffset:r,focusNode:i,focusOffset:a}=e.observer.selectionRange;return n&&(t.push(new _a(n,r)),(i!=n||a!=r)&&t.push(new _a(i,a))),t}function Ta(e,t){if(e.length==0)return null;let n=e[0].pos,r=e.length==2?e[1].pos:n;return n<0||r<0?null:n==r?A.create([A.cursor(r+t,-1)]):A.single(n+t,r+t)}function Ea(e,t){return t.head==e.main.head&&t.anchor==e.main.anchor}var Da=class{setSelectionOrigin(e){this.lastSelectionOrigin=e,this.lastSelectionTime=Date.now()}constructor(e){this.view=e,this.lastKeyCode=0,this.lastKeyTime=0,this.touchActive=!1,this.lastTouchTime=0,this.lastTouchX=0,this.lastTouchY=0,this.lastFocusTime=0,this.lastScrollTop=0,this.lastScrollLeft=0,this.lastWheelEvent=0,this.pendingIOSKey=void 0,this.lastIOSMomentumScroll=0,this.tabFocusMode=-1,this.lastSelectionOrigin=null,this.lastSelectionTime=0,this.lastContextMenu=0,this.scrollHandlers=[],this.handlers=Object.create(null),this.composing=-1,this.compositionFirstChange=null,this.compositionEndedAt=0,this.compositionPendingKey=!1,this.compositionPendingChange=!1,this.insertingText=``,this.insertingTextAt=0,this.mouseSelection=null,this.draggedContent=null,this.handleEvent=this.handleEvent.bind(this),this.notifiedFocused=e.hasFocus,L.safari&&e.contentDOM.addEventListener(`input`,()=>null),L.gecko&&uo(e.contentDOM.ownerDocument)}handleEvent(e){Va(this.view,e)&&!this.ignoreDuringComposition(e)&&(e.type==`keydown`&&this.keydown(e)||(this.view.updateState==0?this.runHandlers(e.type,e):Promise.resolve().then(()=>this.runHandlers(e.type,e))))}runHandlers(e,t){let n=this.handlers[e];if(n){for(let e of n.observers)e(this.view,t);for(let e of n.handlers){if(t.defaultPrevented)break;if(e(this.view,t)){t.preventDefault();break}}}}ensureHandlers(e){let t=Aa(e),n=this.handlers,r=this.view.contentDOM;for(let e in t)if(e!=`scroll`){let i=!t[e].handlers.length,a=n[e];a&&i!=!a.handlers.length&&(r.removeEventListener(e,this.handleEvent),a=null),a||r.addEventListener(e,this.handleEvent,{passive:i})}for(let e in n)e!=`scroll`&&!t[e]&&r.removeEventListener(e,this.handleEvent);this.handlers=t}keydown(e){if(this.lastKeyCode=e.keyCode,this.lastKeyTime=Date.now(),e.keyCode==9&&this.tabFocusMode>-1&&(!this.tabFocusMode||Date.now()<=this.tabFocusMode))return!0;if(this.tabFocusMode>0&&e.keyCode!=27&&Na.indexOf(e.keyCode)<0&&(this.tabFocusMode=-1),L.android&&L.chrome&&!e.synthetic&&(e.keyCode==13||e.keyCode==8))return this.view.observer.delayAndroidKey(e.key,e.keyCode),!0;if(L.ios&&!e.synthetic&&!e.altKey&&!e.metaKey&&(ja.some(t=>t.keyCode==e.keyCode)&&!e.ctrlKey||Ma.indexOf(e.key)>-1&&e.ctrlKey)){let t={ctrlKey:e.ctrlKey,altKey:e.altKey,metaKey:e.metaKey,shiftKey:e.shiftKey};t.shiftKey&&L.ios&&!/^(off|none)$/.test(this.view.contentDOM.autocapitalize)&&Oa(this.view.win)&&(t.shiftKey=!1);let n=this.pendingIOSKey={key:e.key,keyCode:e.keyCode,mods:t};return setTimeout(()=>{this.pendingIOSKey==n&&this.flushIOSKey()},50),!0}return e.keyCode!=229&&this.view.observer.forceFlush(),!1}flushIOSKey(e){let t=this.pendingIOSKey;return!t||this.view.observer.pendingRecords().length||t.key==`Enter`&&e&&e.from<e.to&&/^\S+$/.test(e.insert.toString())?!1:(this.pendingIOSKey=void 0,or(this.view.contentDOM,t.key,t.keyCode,t.mods))}ignoreDuringComposition(e){return!/^key/.test(e.type)||e.synthetic?!1:this.composing>0?!0:L.safari&&!L.ios&&this.compositionPendingKey&&Date.now()-this.compositionEndedAt<100?(this.compositionPendingKey=!1,!0):!1}startMouseSelection(e){this.mouseSelection&&this.mouseSelection.destroy(),this.mouseSelection=e}update(e){this.view.observer.update(e),this.mouseSelection&&this.mouseSelection.update(e),this.draggedContent&&e.docChanged&&(this.draggedContent=this.draggedContent.map(e.changes)),e.transactions.length&&(this.lastKeyCode=this.lastSelectionTime=0)}destroy(){this.mouseSelection&&this.mouseSelection.destroy()}};function Oa(e){return e.visualViewport?e.visualViewport.height*e.visualViewport.scale/e.document.documentElement.clientHeight<.85:!1}function ka(e,t){return(n,r)=>{try{return t.call(e,r,n)}catch(e){Yr(n.state,e)}}}function Aa(e){let t=Object.create(null);function n(e){return t[e]||(t[e]={observers:[],handlers:[]})}for(let t of e){let e=t.spec,r=e&&e.plugin.domEventHandlers,i=e&&e.plugin.domEventObservers;if(r)for(let e in r){let i=r[e];i&&n(e).handlers.push(ka(t.value,i))}if(i)for(let e in i){let r=i[e];r&&n(e).observers.push(ka(t.value,r))}}for(let e in Ha)n(e).handlers.push(Ha[e]);for(let e in Ua)n(e).observers.push(Ua[e]);return t}var ja=[{key:`Backspace`,keyCode:8,inputType:`deleteContentBackward`},{key:`Enter`,keyCode:13,inputType:`insertParagraph`},{key:`Enter`,keyCode:13,inputType:`insertLineBreak`},{key:`Delete`,keyCode:46,inputType:`deleteContentForward`}],Ma=`dthko`,Na=[16,17,18,20,91,92,224,225],Pa=6;function Fa(e){return Math.max(0,e)*.7+8}function Ia(e,t){return Math.max(Math.abs(e.clientX-t.clientX),Math.abs(e.clientY-t.clientY))}var La=class{constructor(e,t,n,r){this.view=e,this.startEvent=t,this.style=n,this.mustSelect=r,this.scrollSpeed={x:0,y:0},this.scrolling=-1,this.lastEvent=t,this.scrollParents=Qn(e.contentDOM),this.atoms=e.state.facet(ai).map(t=>t(e));let i=e.contentDOM.ownerDocument;i.addEventListener(`mousemove`,this.move=this.move.bind(this)),i.addEventListener(`mouseup`,this.up=this.up.bind(this)),this.extend=t.shiftKey,this.multiple=e.state.facet(P.allowMultipleSelections)&&Ra(e,t),this.dragging=Ba(e,t)&&$a(t)==1?null:!1}start(e){this.dragging===!1&&this.select(e)}move(e){if(e.buttons==0)return this.destroy();if(this.dragging||this.dragging==null&&Ia(this.startEvent,e)<10)return;this.select(this.lastEvent=e);let t=0,n=0,r=0,i=0,a=this.view.win.innerWidth,o=this.view.win.innerHeight;this.scrollParents.x&&({left:r,right:a}=this.scrollParents.x.getBoundingClientRect()),this.scrollParents.y&&({top:i,bottom:o}=this.scrollParents.y.getBoundingClientRect());let s=ui(this.view);e.clientX-s.left<=r+Pa?t=-Fa(r-e.clientX):e.clientX+s.right>=a-Pa&&(t=Fa(e.clientX-a)),e.clientY-s.top<=i+Pa?n=-Fa(i-e.clientY):e.clientY+s.bottom>=o-Pa&&(n=Fa(e.clientY-o)),this.setScrollSpeed(t,n)}up(e){this.dragging??this.select(this.lastEvent),this.dragging||e.preventDefault(),this.destroy()}destroy(){this.setScrollSpeed(0,0);let e=this.view.contentDOM.ownerDocument;e.removeEventListener(`mousemove`,this.move),e.removeEventListener(`mouseup`,this.up),this.view.inputState.mouseSelection=this.view.inputState.draggedContent=null}setScrollSpeed(e,t){this.scrollSpeed={x:e,y:t},e||t?this.scrolling<0&&(this.scrolling=setInterval(()=>this.scroll(),50)):this.scrolling>-1&&(clearInterval(this.scrolling),this.scrolling=-1)}scroll(){let{x:e,y:t}=this.scrollSpeed;e&&this.scrollParents.x&&(this.scrollParents.x.scrollLeft+=e,e=0),t&&this.scrollParents.y&&(this.scrollParents.y.scrollTop+=t,t=0),(e||t)&&this.view.win.scrollBy(e,t),this.dragging===!1&&this.select(this.lastEvent)}select(e){let{view:t}=this,n=ca(this.atoms,this.style.get(e,this.extend,this.multiple));(this.mustSelect||!n.eq(t.state.selection,this.dragging===!1))&&this.view.dispatch({selection:n,userEvent:`select.pointer`}),this.mustSelect=!1}update(e){e.transactions.some(e=>e.isUserEvent(`input.type`))?this.destroy():this.style.update(e)&&setTimeout(()=>this.select(this.lastEvent),20)}};function Ra(e,t){let n=e.state.facet(Pr);return n.length?n[0](t):L.mac?t.metaKey:t.ctrlKey}function za(e,t){let n=e.state.facet(Fr);return n.length?n[0](t):L.mac?!t.altKey:!t.ctrlKey}function Ba(e,t){let{main:n}=e.state.selection;if(n.empty)return!1;let r=zn(e.root);if(!r||r.rangeCount==0)return!0;let i=r.getRangeAt(0).getClientRects();for(let e=0;e<i.length;e++){let n=i[e];if(n.left<=t.clientX&&n.right>=t.clientX&&n.top<=t.clientY&&n.bottom>=t.clientY)return!0}return!1}function Va(e,t){if(!t.bubbles)return!0;if(t.defaultPrevented)return!1;for(let n=t.target,r;n!=e.contentDOM;n=n.parentNode)if(!n||n.nodeType==11||(r=U.get(n))&&r.isWidget()&&!r.isHidden&&r.widget.ignoreEvent(t))return!1;return!0}var Ha=Object.create(null),Ua=Object.create(null),Wa=L.ie&&L.ie_version<15||L.ios&&L.webkit_version<604;function Ga(e){let t=e.dom.parentNode;if(!t)return;let n=t.appendChild(document.createElement(`textarea`));n.style.cssText=`position: fixed; left: -10000px; top: 10px`,n.focus(),setTimeout(()=>{e.focus(),n.remove(),qa(e,n.value)},50)}function Ka(e,t,n){for(let r of e.facet(t))n=r(n,e);return n}function qa(e,t){t=Ka(e.state,Vr,t);let{state:n}=e,r,i=1,a=n.toText(t),o=a.lines==n.selection.ranges.length;if(ao!=null&&n.selection.ranges.every(e=>e.empty)&&ao==a.toString()){let e=-1;r=n.changeByRange(r=>{let s=n.doc.lineAt(r.from);if(s.from==e)return{range:r};e=s.from;let c=n.toText((o?a.line(i++).text:t)+n.lineBreak);return{changes:{from:s.from,insert:c},range:A.cursor(r.from+c.length,-1)}})}else r=o?n.changeByRange(e=>{let t=a.line(i++);return{changes:{from:e.from,to:e.to,insert:t.text},range:A.cursor(e.from+t.length,-1)}}):n.replaceSelection(a);e.dispatch(r,{userEvent:`input.paste`,scrollIntoView:!0})}Ua.scroll=e=>{let t=e.inputState;t.lastScrollTop=e.scrollDOM.scrollTop,t.lastScrollLeft=e.scrollDOM.scrollLeft,L.ios&&!t.touchActive&&(t.lastIOSMomentumScroll=Date.now())},Ua.wheel=Ua.mousewheel=e=>{e.inputState.lastWheelEvent=Date.now()},Ha.keydown=(e,t)=>(e.inputState.setSelectionOrigin(`select`),t.keyCode==27&&e.inputState.tabFocusMode!=0&&(e.inputState.tabFocusMode=Date.now()+2e3),!1),Ua.touchstart=(e,t)=>{let n=e.inputState,r=t.targetTouches[0];n.touchActive=!0,n.lastTouchTime=Date.now(),r&&(n.lastTouchX=r.clientX,n.lastTouchY=r.clientY),n.setSelectionOrigin(`select.pointer`)},Ua.touchmove=e=>{e.inputState.setSelectionOrigin(`select.pointer`)},Ua.touchend=(e,t)=>{e.inputState.touchActive=!1},Ha.mousedown=(e,t)=>{if(e.observer.flush(),e.inputState.lastTouchTime>Date.now()-2e3)return!1;let n=null;for(let r of e.state.facet(Ir))if(n=r(e,t),n)break;if(!n&&t.button==0&&(n=eo(e,t)),n){let r=!e.hasFocus;e.inputState.startMouseSelection(new La(e,t,n,r)),r&&e.observer.ignore(()=>{rr(e.contentDOM);let t=e.root.activeElement;t&&!t.contains(e.contentDOM)&&t.blur()});let i=e.inputState.mouseSelection;if(i)return i.start(t),i.dragging===!1}else e.inputState.setSelectionOrigin(`select.pointer`);return!1};function Ja(e,t,n,r){if(r==1)return A.cursor(t,n);if(r==2)return ea(e.state,t,n);{let r=e.docView.lineAt(t,n),i=e.state.doc.lineAt(r?r.posAtEnd:t),a=r?r.posAtStart:i.from,o=r?r.posAtEnd:i.to;return o<e.state.doc.length&&o==i.to&&o++,A.undirectionalRange(a,o)}}var Ya=L.ie&&L.ie_version<=11,Xa=null,Za=0,Qa=0;function $a(e){if(!Ya)return e.detail;let t=Xa,n=Qa;return Xa=e,Qa=Date.now(),Za=!t||n>Date.now()-400&&Math.abs(t.clientX-e.clientX)<2&&Math.abs(t.clientY-e.clientY)<2?(Za+1)%3:1}function eo(e,t){let n=e.posAndSideAtCoords({x:t.clientX,y:t.clientY},!1),r=$a(t),i=e.state.selection;return{update(e){e.docChanged&&(n.pos=e.changes.mapPos(n.pos),i=i.map(e.changes))},get(t,a,o){let s=e.posAndSideAtCoords({x:t.clientX,y:t.clientY},!1),c,l=Ja(e,s.pos,s.assoc,r);if(n.pos!=s.pos&&!a){let t=Ja(e,n.pos,n.assoc,r),i=Math.min(t.from,l.from),a=Math.max(t.to,l.to);l=i<l.from?A.range(i,a,l.assoc):A.range(a,i,l.assoc)}return a?i.replaceRange(i.main.extend(l.from,l.to,l.assoc)):o&&r==1&&i.ranges.length>1&&(c=to(i,s.pos))?c:o?i.addRange(l):A.create([l])}}}function to(e,t){for(let n=0;n<e.ranges.length;n++){let{from:r,to:i}=e.ranges[n];if(r<=t&&i>=t)return A.create(e.ranges.slice(0,n).concat(e.ranges.slice(n+1)),e.mainIndex==n?0:e.mainIndex-+(e.mainIndex>n))}return null}Ha.dragstart=(e,t)=>{let{selection:{main:n}}=e.state;if(t.target.draggable){let r=e.docView.tile.nearest(t.target);if(r&&r.isWidget()){let e=r.posAtStart,t=e+r.length;(e>=n.to||t<=n.from)&&(n=A.undirectionalRange(e,t))}}let{inputState:r}=e;return r.mouseSelection&&(r.mouseSelection.dragging=!0),r.draggedContent=n,t.dataTransfer&&(t.dataTransfer.setData(`Text`,Ka(e.state,Hr,e.state.sliceDoc(n.from,n.to))),t.dataTransfer.effectAllowed=`copyMove`),!1},Ha.dragend=e=>(e.inputState.draggedContent=null,!1);function no(e,t,n,r){if(n=Ka(e.state,Vr,n),!n)return;let i=e.posAtCoords({x:t.clientX,y:t.clientY},!1),{draggedContent:a}=e.inputState,o=r&&a&&za(e,t)?{from:a.from,to:a.to}:null,s={from:i,insert:n},c=e.state.changes(o?[o,s]:s);e.focus(),e.dispatch({changes:c,selection:{anchor:c.mapPos(i,-1),head:c.mapPos(i,1)},userEvent:o?`move.drop`:`input.drop`}),e.inputState.draggedContent=null}Ha.drop=(e,t)=>{if(!t.dataTransfer)return!1;if(e.state.readOnly)return!0;let n=t.dataTransfer.files;if(n&&n.length){let r=Array(n.length),i=0,a=()=>{++i==n.length&&no(e,t,r.filter(e=>e!=null).join(e.state.lineBreak),!1)};for(let e=0;e<n.length;e++){let t=new FileReader;t.onerror=a,t.onload=()=>{/[\x00-\x08\x0e-\x1f]{2}/.test(t.result)||(r[e]=t.result),a()},t.readAsText(n[e])}return!0}{let n=t.dataTransfer.getData(`Text`);if(n)return no(e,t,n,!0),!0}return!1},Ha.paste=(e,t)=>{if(e.state.readOnly)return!0;e.observer.flush();let n=Wa?null:t.clipboardData;return n?(qa(e,n.getData(`text/plain`)||n.getData(`text/uri-list`)),!0):(Ga(e),!1)};function ro(e,t){let n=e.dom.parentNode;if(!n)return;let r=n.appendChild(document.createElement(`textarea`));r.style.cssText=`position: fixed; left: -10000px; top: 10px`,r.value=t,r.focus(),r.selectionEnd=t.length,r.selectionStart=0,setTimeout(()=>{r.remove(),e.focus()},50)}function io(e){let t=[],n=[],r=!1;for(let r of e.selection.ranges)r.empty||(t.push(e.sliceDoc(r.from,r.to)),n.push(r));if(!t.length){let i=-1;for(let{from:r}of e.selection.ranges){let a=e.doc.lineAt(r);a.number>i&&(t.push(a.text),n.push({from:a.from,to:Math.min(e.doc.length,a.to+1)})),i=a.number}r=!0}return{text:Ka(e,Hr,t.join(e.lineBreak)),ranges:n,linewise:r}}var ao=null;Ha.copy=Ha.cut=(e,t)=>{if(!Vn(e.contentDOM,e.observer.selectionRange))return!1;let{text:n,ranges:r,linewise:i}=io(e.state);if(!n&&!i)return!1;ao=i?n:null,t.type==`cut`&&!e.state.readOnly&&e.dispatch({changes:r,scrollIntoView:!0,userEvent:`delete.cut`});let a=Wa?null:t.clipboardData;return a?(a.clearData(),a.setData(`text/plain`,n),!0):(ro(e,n),!1)};var oo=pt.define();function so(e,t){let n=[];for(let r of e.facet(Br)){let i=r(e,t);i&&n.push(i)}return n.length?e.update({effects:n,annotations:oo.of(!0)}):null}function co(e){setTimeout(()=>{let t=e.hasFocus;if(t!=e.inputState.notifiedFocused){let n=so(e.state,t);n?e.dispatch(n):e.update([])}},10)}Ua.focus=e=>{e.inputState.lastFocusTime=Date.now(),!e.scrollDOM.scrollTop&&(e.inputState.lastScrollTop||e.inputState.lastScrollLeft)&&(e.scrollDOM.scrollTop=e.inputState.lastScrollTop,e.scrollDOM.scrollLeft=e.inputState.lastScrollLeft),co(e)},Ua.blur=e=>{e.observer.clearSelectionRange(),co(e)},Ua.compositionstart=Ua.compositionupdate=(e,t)=>{if(!e.observer.editContext&&(e.inputState.compositionFirstChange??(e.inputState.compositionFirstChange=!0),e.inputState.composing<0)){let{main:t}=e.state.selection;!t.empty&&e.lineBlockAt(t.from).from!=e.lineBlockAt(t.to).from&&!e.state.readOnly&&e.dispatch({changes:e.state.selection.ranges.filter(e=>!e.empty).map(e=>({from:e.from,to:e.to})),userEvent:`input`}),e.inputState.composing=0}},Ua.compositionend=e=>{e.observer.editContext||(e.inputState.composing=-1,e.inputState.compositionEndedAt=Date.now(),e.inputState.compositionPendingKey=!0,e.inputState.compositionPendingChange=e.observer.pendingRecords().length>0,e.inputState.compositionFirstChange=null,L.chrome&&L.android?e.observer.flushSoon():e.inputState.compositionPendingChange?Promise.resolve().then(()=>e.observer.flush()):setTimeout(()=>{e.inputState.composing<0&&e.docView.hasComposition&&e.update([])},50))},Ua.contextmenu=e=>{e.inputState.lastContextMenu=Date.now()},Ha.beforeinput=(e,t)=>{if((t.inputType==`insertText`||t.inputType==`insertCompositionText`)&&(e.inputState.insertingText=t.data,e.inputState.insertingTextAt=Date.now()),t.inputType==`insertReplacementText`&&e.observer.editContext){let n=t.dataTransfer?.getData(`text/plain`),r=t.getTargetRanges();if(n&&r.length){let t=r[0];return xa(e,{from:e.posAtDOM(t.startContainer,t.startOffset),to:e.posAtDOM(t.endContainer,t.endOffset),insert:e.state.toText(n)},null),!0}}let n;if(L.chrome&&L.android&&(n=ja.find(e=>e.inputType==t.inputType))&&(e.observer.delayAndroidKey(n.key,n.keyCode),n.key==`Backspace`||n.key==`Delete`)){let t=window.visualViewport?.height||0;setTimeout(()=>{(window.visualViewport?.height||0)>t+10&&e.hasFocus&&(e.contentDOM.blur(),e.focus())},100)}return L.ios&&t.inputType==`deleteContentForward`&&e.observer.flushSoon(),L.safari&&t.inputType==`insertText`&&e.inputState.composing>=0&&setTimeout(()=>Ua.compositionend(e,t),20),!1};var lo=new Set;function uo(e){lo.has(e)||(lo.add(e),e.addEventListener(`copy`,()=>{}),e.addEventListener(`cut`,()=>{}))}var fo=[`pre-wrap`,`normal`,`pre-line`,`break-spaces`],po=!1;function mo(){po=!1}var ho=class{constructor(e){this.lineWrapping=e,this.doc=E.empty,this.heightSamples={},this.lineHeight=14,this.charWidth=7,this.textHeight=14,this.lineLength=30}heightForGap(e,t){let n=this.doc.lineAt(t).number-this.doc.lineAt(e).number+1;return this.lineWrapping&&(n+=Math.max(0,Math.ceil((t-e-n*this.lineLength*.5)/this.lineLength))),this.lineHeight*n}heightForLine(e){return this.lineWrapping?(1+Math.max(0,Math.ceil((e-this.lineLength)/Math.max(1,this.lineLength-5))))*this.lineHeight:this.lineHeight}setDoc(e){return this.doc=e,this}mustRefreshForWrapping(e){return fo.indexOf(e)>-1!=this.lineWrapping}mustRefreshForHeights(e){let t=!1;for(let n=0;n<e.length;n++){let r=e[n];r<0?n++:this.heightSamples[Math.floor(r*10)]||(t=!0,this.heightSamples[Math.floor(r*10)]=!0)}return t}refresh(e,t,n,r,i,a){let o=fo.indexOf(e)>-1,s=Math.abs(t-this.lineHeight)>.3||this.lineWrapping!=o;if(this.lineWrapping=o,this.lineHeight=t,this.charWidth=n,this.textHeight=r,this.lineLength=i,s){this.heightSamples={};for(let e=0;e<a.length;e++){let t=a[e];t<0?e++:this.heightSamples[Math.floor(t*10)]=!0}}return s}},go=class{constructor(e,t){this.from=e,this.heights=t,this.index=0}get more(){return this.index<this.heights.length}},_o=class e{constructor(e,t,n,r,i){this.from=e,this.length=t,this.top=n,this.height=r,this._content=i}get type(){return typeof this._content==`number`?An.Text:Array.isArray(this._content)?this._content:this._content.type}get to(){return this.from+this.length}get bottom(){return this.top+this.height}get widget(){return this._content instanceof Nn?this._content.widget:null}get widgetLineBreaks(){return typeof this._content==`number`?this._content:0}join(t){let n=(Array.isArray(this._content)?this._content:[this]).concat(Array.isArray(t._content)?t._content:[t]);return new e(this.from,this.length+t.length,this.top,this.height+t.height,n)}},W=(function(e){return e[e.ByPos=0]=`ByPos`,e[e.ByHeight=1]=`ByHeight`,e[e.ByPosNoHeight=2]=`ByPosNoHeight`,e})(W||={}),vo=.001,yo=class e{constructor(e,t,n=2){this.length=e,this.height=t,this.flags=n}get outdated(){return(this.flags&2)>0}set outdated(e){this.flags=(e?2:0)|this.flags&-3}setHeight(e){this.height!=e&&(Math.abs(this.height-e)>vo&&(po=!0),this.height=e)}replace(t,n,r){return e.of(r)}decomposeLeft(e,t){t.push(this)}decomposeRight(e,t){t.push(this)}applyChanges(e,t,n,r){let i=this,a=n.doc;for(let o=r.length-1;o>=0;o--){let{fromA:s,toA:c,fromB:l,toB:u}=r[o],d=i.lineAt(s,W.ByPosNoHeight,n.setDoc(t),0,0),f=d.to>=c?d:i.lineAt(c,W.ByPosNoHeight,n,0,0);for(u+=f.to-c,c=f.to;o>0&&d.from<=r[o-1].toA;)s=r[o-1].fromA,l=r[o-1].fromB,o--,s<d.from&&(d=i.lineAt(s,W.ByPosNoHeight,n,0,0));l+=d.from-s,s=d.from;let p=Oo.build(n.setDoc(a),e,l,u);i=bo(i,i.replace(s,c,p))}return i.updateHeight(n,0)}static empty(){return new Co(0,0,0)}static of(t){if(t.length==1)return t[0];let n=0,r=t.length,i=0,a=0;for(;;)if(n==r){if(i>a*2){let e=t[n-1];e.break?t.splice(--n,1,e.left,null,e.right):t.splice(--n,1,e.left,e.right),r+=1+e.break,i-=e.size}else if(a>i*2){let e=t[r];e.break?t.splice(r,1,e.left,null,e.right):t.splice(r,1,e.left,e.right),r+=2+e.break,a-=e.size}else break}else if(i<a){let e=t[n++];e&&(i+=e.size)}else{let e=t[--r];e&&(a+=e.size)}let o=!1;return t[n-1]==null?(o=!0,n--):t[n]??(o=!0,r++),new To(e.of(t.slice(0,n)),o,e.of(t.slice(r)))}};function bo(e,t){return e==t?e:(e.constructor!=t.constructor&&(po=!0),t)}yo.prototype.size=1;var xo=R.replace({}),So=class extends yo{constructor(e,t,n){super(e,t),this.deco=n,this.spaceAbove=0}mainBlock(e,t){return new _o(t,this.length,e+this.spaceAbove,this.height-this.spaceAbove,this.deco||0)}blockAt(e,t,n,r){return this.spaceAbove&&e<n+this.spaceAbove?new _o(r,0,n,this.spaceAbove,xo):this.mainBlock(n,r)}lineAt(e,t,n,r,i){let a=this.mainBlock(r,i);return this.spaceAbove?this.blockAt(0,n,r,i).join(a):a}forEachLine(e,t,n,r,i,a){e<=i+this.length&&t>=i&&a(this.lineAt(0,W.ByPos,n,r,i))}setMeasuredHeight(e){let t=e.heights[e.index++];t<0?(this.spaceAbove=-t,t=e.heights[e.index++]):this.spaceAbove=0,this.setHeight(t)}updateHeight(e,t=0,n=!1,r){return r&&r.from<=t&&r.more&&this.setMeasuredHeight(r),this.outdated=!1,this}toString(){return`block(${this.length})`}},Co=class e extends So{constructor(e,t,n){super(e,t,null),this.collapsed=0,this.widgetHeight=0,this.breaks=0,this.spaceAbove=n}mainBlock(e,t){return new _o(t,this.length,e+this.spaceAbove,this.height-this.spaceAbove,this.breaks)}replace(t,n,r){let i=r[0];return r.length==1&&(i instanceof e||i instanceof wo&&i.flags&4)&&Math.abs(this.length-i.length)<10?(i instanceof wo?i=new e(i.length,this.height,this.spaceAbove):i.height=this.height,this.outdated||(i.outdated=!1),i):yo.of(r)}updateHeight(e,t=0,n=!1,r){return r&&r.from<=t&&r.more?this.setMeasuredHeight(r):(n||this.outdated)&&(this.spaceAbove=0,this.setHeight(Math.max(this.widgetHeight,e.heightForLine(this.length-this.collapsed))+this.breaks*e.lineHeight)),this.outdated=!1,this}toString(){return`line(${this.length}${this.collapsed?-this.collapsed:``}${this.widgetHeight?`:`+this.widgetHeight:``})`}},wo=class e extends yo{constructor(e){super(e,0)}heightMetrics(e,t){let n=e.doc.lineAt(t).number,r=e.doc.lineAt(t+this.length).number,i=r-n+1,a,o=0;if(e.lineWrapping){let t=Math.min(this.height,e.lineHeight*i);a=t/i,this.length>i+1&&(o=(this.height-t)/(this.length-i-1))}else a=this.height/i;return{firstLine:n,lastLine:r,perLine:a,perChar:o}}blockAt(e,t,n,r){let{firstLine:i,lastLine:a,perLine:o,perChar:s}=this.heightMetrics(t,r);if(t.lineWrapping){let i=r+(e<t.lineHeight?0:Math.round(Math.max(0,Math.min(1,(e-n)/this.height))*this.length)),a=t.doc.lineAt(i),c=o+a.length*s,l=Math.max(n,e-c/2);return new _o(a.from,a.length,l,c,0)}{let r=Math.max(0,Math.min(a-i,Math.floor((e-n)/o))),{from:s,length:c}=t.doc.line(i+r);return new _o(s,c,n+o*r,o,0)}}lineAt(e,t,n,r,i){if(t==W.ByHeight)return this.blockAt(e,n,r,i);if(t==W.ByPosNoHeight){let{from:t,to:r}=n.doc.lineAt(e);return new _o(t,r-t,0,0,0)}let{firstLine:a,perLine:o,perChar:s}=this.heightMetrics(n,i),c=n.doc.lineAt(e),l=o+c.length*s,u=c.number-a,d=r+o*u+s*(c.from-i-u);return new _o(c.from,c.length,Math.max(r,Math.min(d,r+this.height-l)),l,0)}forEachLine(e,t,n,r,i,a){e=Math.max(e,i),t=Math.min(t,i+this.length);let{firstLine:o,perLine:s,perChar:c}=this.heightMetrics(n,i);for(let l=e,u=r;l<=t;){let t=n.doc.lineAt(l);if(l==e){let n=t.number-o;u+=s*n+c*(e-i-n)}let r=s+c*t.length;a(new _o(t.from,t.length,u,r,0)),u+=r,l=t.to+1}}replace(t,n,r){let i=this.length-n;if(i>0){let t=r[r.length-1];t instanceof e?r[r.length-1]=new e(t.length+i):r.push(null,new e(i-1))}if(t>0){let n=r[0];n instanceof e?r[0]=new e(t+n.length):r.unshift(new e(t-1),null)}return yo.of(r)}decomposeLeft(t,n){n.push(new e(t-1),null)}decomposeRight(t,n){n.push(null,new e(this.length-t-1))}updateHeight(t,n=0,r=!1,i){let a=n+this.length;if(i&&i.from<=n+this.length&&i.more){let r=[],o=Math.max(n,i.from),s=-1;for(i.from>n&&r.push(new e(i.from-n-1).updateHeight(t,n));o<=a&&i.more;){let e=t.doc.lineAt(o).length;r.length&&r.push(null);let n=i.heights[i.index++],a=0;n<0&&(a=-n,n=i.heights[i.index++]),s==-1?s=n:Math.abs(n-s)>=vo&&(s=-2);let c=new Co(e,n,a);c.outdated=!1,r.push(c),o+=e+1}o<=a&&r.push(null,new e(a-o).updateHeight(t,o));let c=yo.of(r);return(s<0||Math.abs(c.height-this.height)>=vo||Math.abs(s-this.heightMetrics(t,n).perLine)>=vo)&&(po=!0),bo(this,c)}return(r||this.outdated)&&(this.setHeight(t.heightForGap(n,n+this.length)),this.outdated=!1),this}toString(){return`gap(${this.length})`}},To=class extends yo{constructor(e,t,n){super(e.length+ +!!t+n.length,e.height+n.height,+!!t|(e.outdated||n.outdated?2:0)),this.left=e,this.right=n,this.size=e.size+n.size}get break(){return this.flags&1}blockAt(e,t,n,r){let i=n+this.left.height;return e<i?this.left.blockAt(e,t,n,r):this.right.blockAt(e,t,i,r+this.left.length+this.break)}lineAt(e,t,n,r,i){let a=r+this.left.height,o=i+this.left.length+this.break,s=t==W.ByHeight?e<a:e<o,c=s?this.left.lineAt(e,t,n,r,i):this.right.lineAt(e,t,n,a,o);if(this.break||(s?c.to<o:c.from>o))return c;let l=t==W.ByPosNoHeight?W.ByPosNoHeight:W.ByPos;return s?c.join(this.right.lineAt(o,l,n,a,o)):this.left.lineAt(o,l,n,r,i).join(c)}forEachLine(e,t,n,r,i,a){let o=r+this.left.height,s=i+this.left.length+this.break;if(this.break)e<s&&this.left.forEachLine(e,t,n,r,i,a),t>=s&&this.right.forEachLine(e,t,n,o,s,a);else{let c=this.lineAt(s,W.ByPos,n,r,i);e<c.from&&this.left.forEachLine(e,Math.min(t,c.from-1),n,r,i,a),c.to>=e&&c.from<=t&&a(c),t>c.to&&this.right.forEachLine(Math.max(e,c.to+1),t,n,o,s,a)}}replace(e,t,n){let r=this.left.length+this.break;if(t<r)return this.balanced(this.left.replace(e,t,n),this.right);if(e>this.left.length)return this.balanced(this.left,this.right.replace(e-r,t-r,n));let i=[];e>0&&this.decomposeLeft(e,i);let a=i.length;for(let e of n)i.push(e);if(e>0&&Eo(i,a-1),t<this.length){let e=i.length;this.decomposeRight(t,i),Eo(i,e)}return yo.of(i)}decomposeLeft(e,t){let n=this.left.length;if(e<=n)return this.left.decomposeLeft(e,t);t.push(this.left),this.break&&(n++,e>=n&&t.push(null)),e>n&&this.right.decomposeLeft(e-n,t)}decomposeRight(e,t){let n=this.left.length,r=n+this.break;if(e>=r)return this.right.decomposeRight(e-r,t);e<n&&this.left.decomposeRight(e,t),this.break&&e<r&&t.push(null),t.push(this.right)}balanced(e,t){return e.size>2*t.size||t.size>2*e.size?yo.of(this.break?[e,null,t]:[e,t]):(this.left=bo(this.left,e),this.right=bo(this.right,t),this.setHeight(e.height+t.height),this.outdated=e.outdated||t.outdated,this.size=e.size+t.size,this.length=e.length+this.break+t.length,this)}updateHeight(e,t=0,n=!1,r){let{left:i,right:a}=this,o=t+i.length+this.break,s=null;return r&&r.from<=t+i.length&&r.more?s=i=i.updateHeight(e,t,n,r):i.updateHeight(e,t,n),r&&r.from<=o+a.length&&r.more?s=a=a.updateHeight(e,o,n,r):a.updateHeight(e,o,n),s?this.balanced(i,a):(this.height=this.left.height+this.right.height,this.outdated=!1,this)}toString(){return this.left+(this.break?` `:`-`)+this.right}};function Eo(e,t){let n,r;e[t]==null&&(n=e[t-1])instanceof wo&&(r=e[t+1])instanceof wo&&e.splice(t-1,3,new wo(n.length+1+r.length))}var Do=5,Oo=class e{constructor(e,t){this.pos=e,this.oracle=t,this.nodes=[],this.lineStart=-1,this.lineEnd=-1,this.covering=null,this.writtenTo=e}get isCovered(){return this.covering&&this.nodes[this.nodes.length-1]==this.covering}span(e,t){if(this.lineStart>-1){let e=Math.min(t,this.lineEnd),n=this.nodes[this.nodes.length-1];n instanceof Co?n.length+=e-this.pos:(e>this.pos||!this.isCovered)&&this.nodes.push(new Co(e-this.pos,-1,0)),this.writtenTo=e,t>e&&(this.nodes.push(null),this.writtenTo++,this.lineStart=-1)}this.pos=t}point(e,t,n){if(e<t||n.heightRelevant){let r=n.widget?n.widget.estimatedHeight:0,i=n.widget?n.widget.lineBreaks:0;r<0&&(r=this.oracle.lineHeight);let a=t-e;n.block?this.addBlock(new So(a,r,n)):(a||i||r>=Do)&&this.addLineDeco(r,i,a)}else t>e&&this.span(e,t);this.lineEnd>-1&&this.lineEnd<this.pos&&(this.lineEnd=this.oracle.doc.lineAt(this.pos).to)}enterLine(){if(this.lineStart>-1)return;let{from:e,to:t}=this.oracle.doc.lineAt(this.pos);this.lineStart=e,this.lineEnd=t,this.writtenTo<e&&((this.writtenTo<e-1||this.nodes[this.nodes.length-1]==null)&&this.nodes.push(this.blankContent(this.writtenTo,e-1)),this.nodes.push(null)),this.pos>e&&this.nodes.push(new Co(this.pos-e,-1,0)),this.writtenTo=this.pos}blankContent(e,t){let n=new wo(t-e);return this.oracle.doc.lineAt(e).to==t&&(n.flags|=4),n}ensureLine(){this.enterLine();let e=this.nodes.length?this.nodes[this.nodes.length-1]:null;if(e instanceof Co)return e;let t=new Co(0,-1,0);return this.nodes.push(t),t}addBlock(e){this.enterLine();let t=e.deco;t&&t.startSide>0&&!this.isCovered&&this.ensureLine(),this.nodes.push(e),this.writtenTo=this.pos+=e.length,t&&t.endSide>0&&(this.covering=e)}addLineDeco(e,t,n){let r=this.ensureLine();r.length+=n,r.collapsed+=n,r.widgetHeight=Math.max(r.widgetHeight,e),r.breaks+=t,this.writtenTo=this.pos+=n}finish(e){let t=this.nodes.length==0?null:this.nodes[this.nodes.length-1];this.lineStart>-1&&!(t instanceof Co)&&!this.isCovered?this.nodes.push(new Co(0,-1,0)):(this.writtenTo<this.pos||t==null)&&this.nodes.push(this.blankContent(this.writtenTo,this.pos));let n=e;for(let e of this.nodes)e instanceof Co&&e.updateHeight(this.oracle,n),n+=e?e.length:1;return this.nodes}static build(t,n,r,i){let a=new e(r,t);return F.spans(n,r,i,a,0),a.finish(r)}};function ko(e,t,n){let r=new Ao;return F.compare(e,t,n,r,0),r.changes}var Ao=class{constructor(){this.changes=[]}compareRange(){}comparePoint(e,t,n,r){(e<t||n&&n.heightRelevant||r&&r.heightRelevant)&&In(e,t,this.changes,5)}};function jo(e,t){let n=e.getBoundingClientRect(),r=e.ownerDocument,i=r.defaultView||window,a=Math.max(0,n.left),o=Math.min(i.innerWidth,n.right),s=Math.max(0,n.top),c=Math.min(i.innerHeight,n.bottom);for(let t=e.parentNode;t&&t!=r.body;)if(z(t)){let n=window.getComputedStyle(t);if((t.scrollHeight>t.clientHeight||t.scrollWidth>t.clientWidth)&&n.overflow!=`visible`){let n=t.getBoundingClientRect();a=Math.max(a,n.left),o=Math.min(o,n.right),s=Math.max(s,n.top),c=Math.min(t==e.parentNode?i.innerHeight:c,n.bottom)}t=n.position==`absolute`||n.position==`fixed`?t.offsetParent:t.parentNode}else if(t.nodeType==11)t=t.host;else break;return{left:a-n.left,right:Math.max(a,o)-n.left,top:s-(n.top+t),bottom:Math.max(s,c)-(n.top+t)}}function Mo(e){let t=e.getBoundingClientRect(),n=e.ownerDocument.defaultView||window;return t.left<n.innerWidth&&t.right>0&&t.top<n.innerHeight&&t.bottom>0}function No(e,t){let n=e.getBoundingClientRect();return{left:0,right:n.right-n.left,top:t,bottom:n.bottom-(n.top+t)}}var Po=class{constructor(e,t,n,r){this.from=e,this.to=t,this.size=n,this.displaySize=r}static same(e,t){if(e.length!=t.length)return!1;for(let n=0;n<e.length;n++){let r=e[n],i=t[n];if(r.from!=i.from||r.to!=i.to||r.size!=i.size)return!1}return!0}draw(e,t){return R.replace({widget:new Fo(this.displaySize*(t?e.scaleY:e.scaleX),t)}).range(this.from,this.to)}},Fo=class extends kn{constructor(e,t){super(),this.size=e,this.vertical=t}eq(e){return e.size==this.size&&e.vertical==this.vertical}toDOM(){let e=document.createElement(`div`);return this.vertical?e.style.height=this.size+`px`:(e.style.width=this.size+`px`,e.style.height=`2px`,e.style.display=`inline-block`),e}get estimatedHeight(){return this.vertical?this.size:-1}},Io=class{constructor(e,t){this.view=e,this.state=t,this.pixelViewport={left:0,right:window.innerWidth,top:0,bottom:0},this.inView=!0,this.paddingTop=0,this.paddingBottom=0,this.contentDOMWidth=0,this.contentDOMHeight=0,this.editorHeight=0,this.editorWidth=0,this.scaleX=1,this.scaleY=1,this.scrollOffset=0,this.scrolledToBottom=!1,this.scrollAnchorPos=0,this.scrollAnchorHeight=-1,this.scaler=Ho,this.scrollTarget=null,this.printing=!1,this.mustMeasureContent=!0,this.defaultTextDirection=B.LTR,this.visibleRanges=[],this.mustEnforceCursorAssoc=!1;let n=t.facet(ti).some(e=>typeof e!=`function`&&e.class==`cm-lineWrapping`);this.heightOracle=new ho(n),this.stateDeco=Uo(t),this.heightMap=yo.empty().applyChanges(this.stateDeco,E.empty,this.heightOracle.setDoc(t.doc),[new fi(0,0,0,t.doc.length)]);for(let e=0;e<2&&(this.viewport=this.getViewport(0,null),this.updateForViewport());e++);this.updateViewportLines(),this.lineGaps=this.ensureLineGaps([]),this.lineGapDeco=R.set(this.lineGaps.map(e=>e.draw(this,!1))),this.scrollParent=e.scrollDOM,this.computeVisibleRanges()}updateForViewport(){let e=[this.viewport],{main:t}=this.state.selection;for(let n=0;n<=1;n++){let r=n?t.head:t.anchor;if(!e.some(({from:e,to:t})=>r>=e&&r<=t)){let{from:t,to:n}=this.lineBlockAt(r);e.push(new Lo(t,n))}}return this.viewports=e.sort((e,t)=>e.from-t.from),this.updateScaler()}updateScaler(){let e=this.scaler;return this.scaler=this.heightMap.height<=7e6?Ho:Wo.get(this.heightOracle,this.heightMap,this.viewports),e.eq(this.scaler)?0:2}updateViewportLines(){this.viewportLines=[],this.heightMap.forEachLine(this.viewport.from,this.viewport.to,this.heightOracle.setDoc(this.state.doc),0,0,e=>{this.viewportLines.push(Go(e,this.scaler))})}update(e,t=null){this.state=e.state;let n=this.stateDeco;this.stateDeco=Uo(this.state);let r=e.changedRanges,i=fi.extendWithRanges(r,ko(n,this.stateDeco,e?e.changes:Pe.empty(this.state.doc.length))),a=this.heightMap.height,o=this.scrolledToBottom?null:this.scrollAnchorAt(this.scrollOffset);mo(),this.heightMap=this.heightMap.applyChanges(this.stateDeco,e.startState.doc,this.heightOracle.setDoc(this.state.doc),i),(this.heightMap.height!=a||po)&&(e.flags|=2),o?(this.scrollAnchorPos=e.changes.mapPos(o.from,-1),this.scrollAnchorHeight=o.top):(this.scrollAnchorPos=-1,this.scrollAnchorHeight=a);let s=i.length?this.mapViewport(this.viewport,e.changes):this.viewport;(t&&(t.range.head<s.from||t.range.head>s.to)||!this.viewportIsAppropriate(s))&&(s=this.getViewport(0,t));let c=s.from!=this.viewport.from||s.to!=this.viewport.to;this.viewport=s,e.flags|=this.updateForViewport(),(c||!e.changes.empty||e.flags&2)&&this.updateViewportLines(),(this.lineGaps.length||this.viewport.to-this.viewport.from>4e3)&&this.updateLineGaps(this.ensureLineGaps(this.mapLineGaps(this.lineGaps,e.changes))),e.flags|=this.computeVisibleRanges(e.changes),t&&(this.scrollTarget=t),!this.mustEnforceCursorAssoc&&(e.selectionSet||e.focusChanged)&&e.view.lineWrapping&&e.state.selection.main.empty&&e.state.selection.main.assoc&&!e.state.facet(Wr)&&(this.mustEnforceCursorAssoc=!0)}measure(){let{view:e}=this,t=e.contentDOM,n=window.getComputedStyle(t),r=this.heightOracle,i=n.whiteSpace;this.defaultTextDirection=n.direction==`rtl`?B.RTL:B.LTR;let a=this.heightOracle.mustRefreshForWrapping(i)||this.mustMeasureContent===`refresh`,o=t.getBoundingClientRect(),s=a||this.mustMeasureContent||this.contentDOMHeight!=o.height;this.contentDOMHeight=o.height,this.mustMeasureContent=!1;let c=0,l=0;if(o.width&&o.height){let{scaleX:e,scaleY:n}=Xn(t,o);(e>.005&&Math.abs(this.scaleX-e)>.005||n>.005&&Math.abs(this.scaleY-n)>.005)&&(this.scaleX=e,this.scaleY=n,c|=16,a=s=!0)}let u=(parseInt(n.paddingTop)||0)*this.scaleY,d=(parseInt(n.paddingBottom)||0)*this.scaleY;(this.paddingTop!=u||this.paddingBottom!=d)&&(this.paddingTop=u,this.paddingBottom=d,c|=18),this.editorWidth!=e.scrollDOM.clientWidth&&(r.lineWrapping&&(s=!0),this.editorWidth=e.scrollDOM.clientWidth,c|=16);let f=Qn(this.view.contentDOM,!1).y;f!=this.scrollParent&&(this.scrollParent=f,this.scrollAnchorHeight=-1,this.scrollOffset=0);let p=this.getScrollOffset();this.scrollOffset!=p&&(this.scrollAnchorHeight=-1,this.scrollOffset=p),this.scrolledToBottom=lr(this.scrollParent||e.win);let m=(this.printing?No:jo)(t,this.paddingTop),h=m.top-this.pixelViewport.top,g=m.bottom-this.pixelViewport.bottom;this.pixelViewport=m;let _=this.pixelViewport.bottom>this.pixelViewport.top&&this.pixelViewport.right>this.pixelViewport.left;if(_!=this.inView&&(this.inView=_,_&&(s=!0)),!this.inView&&!this.scrollTarget&&!Mo(e.dom))return 0;let v=o.width;if((this.contentDOMWidth!=v||this.editorHeight!=e.scrollDOM.clientHeight)&&(this.contentDOMWidth=o.width,this.editorHeight=e.scrollDOM.clientHeight,c|=16),s){let t=e.docView.measureVisibleLineHeights(this.viewport);if(r.mustRefreshForHeights(t)&&(a=!0),a||r.lineWrapping&&Math.abs(v-this.contentDOMWidth)>r.charWidth){let{lineHeight:n,charWidth:o,textHeight:s}=e.docView.measureTextSize();a=n>0&&r.refresh(i,n,o,s,Math.max(5,v/o),t),a&&(e.docView.minWidth=0,c|=16)}h>0&&g>0?l=Math.max(h,g):h<0&&g<0&&(l=Math.min(h,g)),mo();for(let n of this.viewports){let i=n.from==this.viewport.from?t:e.docView.measureVisibleLineHeights(n);this.heightMap=(a?yo.empty().applyChanges(this.stateDeco,E.empty,this.heightOracle,[new fi(0,0,0,e.state.doc.length)]):this.heightMap).updateHeight(r,0,a,new go(n.from,i))}po&&(c|=2)}let y=!this.viewportIsAppropriate(this.viewport,l)||this.scrollTarget&&(this.scrollTarget.range.head<this.viewport.from||this.scrollTarget.range.head>this.viewport.to);return y&&(c&2&&(c|=this.updateScaler()),this.viewport=this.getViewport(l,this.scrollTarget),c|=this.updateForViewport()),(c&2||y)&&this.updateViewportLines(),(this.lineGaps.length||this.viewport.to-this.viewport.from>4e3)&&this.updateLineGaps(this.ensureLineGaps(a?[]:this.lineGaps,e)),c|=this.computeVisibleRanges(),this.mustEnforceCursorAssoc&&(this.mustEnforceCursorAssoc=!1,e.docView.enforceCursorAssoc()),c}get visibleTop(){return this.scaler.fromDOM(this.pixelViewport.top)}get visibleBottom(){return this.scaler.fromDOM(this.pixelViewport.bottom)}getViewport(e,t){let n=.5-Math.max(-.5,Math.min(.5,e/1e3/2)),r=this.heightMap,i=this.heightOracle,{visibleTop:a,visibleBottom:o}=this,s=new Lo(r.lineAt(a-n*1e3,W.ByHeight,i,0,0).from,r.lineAt(o+(1-n)*1e3,W.ByHeight,i,0,0).to);if(t){let{head:e}=t.range;if(e<s.from||e>s.to){let n=Math.min(this.editorHeight,this.pixelViewport.bottom-this.pixelViewport.top),a=r.lineAt(e,W.ByPos,i,0,0),o;o=t.y==`center`?(a.top+a.bottom)/2-n/2:t.y==`start`||t.y==`nearest`&&e<s.from?a.top:a.bottom-n,s=new Lo(r.lineAt(o-500,W.ByHeight,i,0,0).from,r.lineAt(o+n+500,W.ByHeight,i,0,0).to)}}return s}mapViewport(e,t){let n=t.mapPos(e.from,-1),r=t.mapPos(e.to,1);return new Lo(this.heightMap.lineAt(n,W.ByPos,this.heightOracle,0,0).from,this.heightMap.lineAt(r,W.ByPos,this.heightOracle,0,0).to)}viewportIsAppropriate({from:e,to:t},n=0){if(!this.inView)return!0;let{top:r}=this.heightMap.lineAt(e,W.ByPos,this.heightOracle,0,0),{bottom:i}=this.heightMap.lineAt(t,W.ByPos,this.heightOracle,0,0),{visibleTop:a,visibleBottom:o}=this;return(e==0||r<=a-Math.max(10,Math.min(-n,250)))&&(t==this.state.doc.length||i>=o+Math.max(10,Math.min(n,250)))&&r>a-2e3&&i<o+2e3}mapLineGaps(e,t){if(!e.length||t.empty)return e;let n=[];for(let r of e)t.touchesRange(r.from,r.to)||n.push(new Po(t.mapPos(r.from),t.mapPos(r.to),r.size,r.displaySize));return n}ensureLineGaps(e,t){let n=this.heightOracle.lineWrapping,r=n?1e4:2e3,i=r>>1,a=r<<1;if(this.defaultTextDirection!=B.LTR&&!n)return[];let o=[],s=(r,a,c,l)=>{if(a-r<i)return;let u=this.state.selection.main,d=[u.from];u.empty||d.push(u.to);for(let e of d)if(e>r&&e<a){s(r,e-10,c,l),s(e+10,a,c,l);return}let f=Vo(e,e=>e.from>=c.from&&e.to<=c.to&&Math.abs(e.from-r)<i&&Math.abs(e.to-a)<i&&!d.some(t=>e.from<t&&e.to>t));if(!f){if(a<c.to&&t&&n&&t.visibleRanges.some(e=>e.from<=a&&e.to>=a)){let e=t.moveToLineBoundary(A.cursor(a),!1,!0).head;e>r&&(a=e)}let e=this.gapSize(c,r,a,l);f=new Po(r,a,e,n||e<2e6?e:2e6)}o.push(f)},c=t=>{if(t.length<a||t.type!=An.Text)return;let i=Ro(t.from,t.to,this.stateDeco);if(i.total<a)return;let o=this.scrollTarget?this.scrollTarget.range.head:null,c,l;if(n){let e=r/this.heightOracle.lineLength*this.heightOracle.lineHeight,n,a;if(o!=null){let r=Bo(i,o),s=((this.visibleBottom-this.visibleTop)/2+e)/t.height;n=r-s,a=r+s}else n=(this.visibleTop-t.top-e)/t.height,a=(this.visibleBottom-t.top+e)/t.height;c=zo(i,n),l=zo(i,a)}else{let n=i.total*this.heightOracle.charWidth,a=r*this.heightOracle.charWidth,s=0;if(n>2e6)for(let n of e)n.from>=t.from&&n.from<t.to&&n.size!=n.displaySize&&n.from*this.heightOracle.charWidth+s<this.pixelViewport.left&&(s=n.size-n.displaySize);let u=this.pixelViewport.left+s,d=this.pixelViewport.right+s,f,p;if(o!=null){let e=Bo(i,o),t=((d-u)/2+a)/n;f=e-t,p=e+t}else f=(u-a)/n,p=(d+a)/n;c=zo(i,f),l=zo(i,p)}c>t.from&&s(t.from,c,t,i),l<t.to&&s(l,t.to,t,i)};for(let e of this.viewportLines)Array.isArray(e.type)?e.type.forEach(c):c(e);return o}gapSize(e,t,n,r){let i=Bo(r,n)-Bo(r,t);return this.heightOracle.lineWrapping?e.height*i:r.total*this.heightOracle.charWidth*i}updateLineGaps(e){Po.same(e,this.lineGaps)||(this.lineGaps=e,this.lineGapDeco=R.set(e.map(e=>e.draw(this,this.heightOracle.lineWrapping))))}computeVisibleRanges(e){let t=this.stateDeco;this.lineGaps.length&&(t=t.concat(this.lineGapDeco));let n=[];F.spans(t,this.viewport.from,this.viewport.to,{span(e,t){n.push({from:e,to:t})},point(){}},20);let r=0;if(n.length!=this.visibleRanges.length)r=12;else for(let t=0;t<n.length&&!(r&8);t++){let i=this.visibleRanges[t],a=n[t];(i.from!=a.from||i.to!=a.to)&&(r|=4,e&&e.mapPos(i.from,-1)==a.from&&e.mapPos(i.to,1)==a.to||(r|=8))}return this.visibleRanges=n,r}lineBlockAt(e){return e>=this.viewport.from&&e<=this.viewport.to&&this.viewportLines.find(t=>t.from<=e&&t.to>=e)||Go(this.heightMap.lineAt(e,W.ByPos,this.heightOracle,0,0),this.scaler)}lineBlockAtHeight(e){return e>=this.viewportLines[0].top&&e<=this.viewportLines[this.viewportLines.length-1].bottom&&this.viewportLines.find(t=>t.top<=e&&t.bottom>=e)||Go(this.heightMap.lineAt(this.scaler.fromDOM(e),W.ByHeight,this.heightOracle,0,0),this.scaler)}getScrollOffset(){return this.scrollParent==this.view.scrollDOM?this.scrollParent.scrollTop*this.scaleY:(this.scrollParent?this.scrollParent.getBoundingClientRect().top:0)-this.view.contentDOM.getBoundingClientRect().top}scrollAnchorAt(e){let t=this.lineBlockAtHeight(e+12-this.paddingTop);return t.from>=this.viewport.from||this.viewportLines[0].top-e>200?t:this.viewportLines[0]}elementAtHeight(e){return Go(this.heightMap.blockAt(this.scaler.fromDOM(e),this.heightOracle,0,0),this.scaler)}get docHeight(){return this.scaler.toDOM(this.heightMap.height)}get contentHeight(){return this.docHeight+this.paddingTop+this.paddingBottom}},Lo=class{constructor(e,t){this.from=e,this.to=t}};function Ro(e,t,n){let r=[],i=e,a=0;return F.spans(n,e,t,{span(){},point(e,t){e>i&&(r.push({from:i,to:e}),a+=e-i),i=t}},20),i<t&&(r.push({from:i,to:t}),a+=t-i),{total:a,ranges:r}}function zo({total:e,ranges:t},n){if(n<=0)return t[0].from;if(n>=1)return t[t.length-1].to;let r=Math.floor(e*n);for(let e=0;;e++){let{from:n,to:i}=t[e],a=i-n;if(r<=a)return n+r;r-=a}}function Bo(e,t){let n=0;for(let{from:r,to:i}of e.ranges){if(t<=i){n+=t-r;break}n+=i-r}return n/e.total}function Vo(e,t){for(let n of e)if(t(n))return n}var Ho={toDOM(e){return e},fromDOM(e){return e},scale:1,eq(e){return e==this}};function Uo(e){let t=e.facet(ni).filter(e=>typeof e!=`function`),n=e.facet(ii).filter(e=>typeof e!=`function`);return n.length&&t.push(F.join(n)),t}var Wo=class e{constructor(e,t){this.scale=e,this.viewports=t}static get(t,n,r){let i=0,a=0,o=0,s=r.map(({from:e,to:r})=>{let a=n.lineAt(e,W.ByPos,t,0,0).top,o=n.lineAt(r,W.ByPos,t,0,0).bottom;return i+=o-a,{from:e,to:r,top:a,bottom:o,domTop:0,domBottom:0}});if(n.height-i<10)return Ho;let c=(7e6-i)/(n.height-i);for(let e of s)e.domTop=o+(e.top-a)*c,o=e.domBottom=e.domTop+(e.bottom-e.top),a=e.bottom;return new e(c,s)}toDOM(e){for(let t=0,n=0,r=0;;t++){let i=t<this.viewports.length?this.viewports[t]:null;if(!i||e<i.top)return r+(e-n)*this.scale;if(e<=i.bottom)return i.domTop+(e-i.top);n=i.bottom,r=i.domBottom}}fromDOM(e){for(let t=0,n=0,r=0;;t++){let i=t<this.viewports.length?this.viewports[t]:null;if(!i||e<i.domTop)return n+(e-r)/this.scale;if(e<=i.domBottom)return i.top+(e-i.domTop);n=i.bottom,r=i.domBottom}}eq(t){return t instanceof e&&this.scale==t.scale&&this.viewports.length==t.viewports.length&&this.viewports.every((e,n)=>e.from==t.viewports[n].from&&e.to==t.viewports[n].to)}};function Go(e,t){if(t.scale==1)return e;let n=t.toDOM(e.top),r=t.toDOM(e.bottom);return new _o(e.from,e.length,n,r-n,Array.isArray(e._content)?e._content.map(e=>Go(e,t)):e._content)}var Ko=j.define({combine:e=>e.join(` `)}),qo=j.define({combine:e=>e.indexOf(!0)>-1}),Jo=en.newName(),Yo=en.newName(),Xo=en.newName(),Zo={"&light":`.`+Yo,"&dark":`.`+Xo};function Qo(e,t,n){return new en(t,{finish(t){return/&/.test(t)?t.replace(/&\w*/,t=>{if(t==`&`)return e;if(!n||!n[t])throw RangeError(`Unsupported selector: ${t}`);return n[t]}):e+` `+t}})}var $o=Qo(`.`+Jo,{"&":{position:`relative !important`,boxSizing:`border-box`,"&.cm-focused":{outline:`1px dotted #212121`},display:`flex !important`,flexDirection:`column`},".cm-scroller":{display:`flex !important`,alignItems:`flex-start !important`,fontFamily:`monospace`,lineHeight:1.4,height:`100%`,overflowX:`auto`,position:`relative`,zIndex:0,overflowAnchor:`none`},".cm-content":{margin:0,flexGrow:2,flexShrink:0,display:`block`,whiteSpace:`pre`,wordWrap:`normal`,boxSizing:`border-box`,minHeight:`100%`,padding:`4px 0`,outline:`none`,"&[contenteditable=true]":{WebkitUserModify:`read-write-plaintext-only`}},".cm-lineWrapping":{whiteSpace_fallback:`pre-wrap`,whiteSpace:`break-spaces`,wordBreak:`break-word`,overflowWrap:`anywhere`,flexShrink:1},"&light .cm-content":{caretColor:`black`},"&dark .cm-content":{caretColor:`white`},".cm-line":{display:`block`,padding:`0 2px 0 6px`},".cm-layer":{userSelect:`none`,position:`absolute`,left:0,top:0,contain:`size style`,"& > *":{position:`absolute`}},"&light .cm-selectionBackground":{background:`#d9d9d9`},"&dark .cm-selectionBackground":{background:`#222`},"&light.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground":{background:`#d7d4f0`},"&dark.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground":{background:`#233`},".cm-cursorLayer":{pointerEvents:`none`},"&.cm-focused > .cm-scroller > .cm-cursorLayer":{animation:`steps(1) cm-blink 1.2s infinite`},"@keyframes cm-blink":{"0%":{},"50%":{opacity:0},"100%":{}},"@keyframes cm-blink2":{"0%":{},"50%":{opacity:0},"100%":{}},".cm-cursor, .cm-dropCursor":{borderLeft:`1.2px solid black`,marginLeft:`-0.6px`,pointerEvents:`none`},".cm-cursor":{display:`none`},"&dark .cm-cursor":{borderLeftColor:`#ddd`},".cm-selectionHandle":{backgroundColor:`currentColor`,width:`1.5px`},".cm-selectionHandle-start::before, .cm-selectionHandle-end::before":{content:`""`,backgroundColor:`inherit`,borderRadius:`50%`,width:`8px`,height:`8px`,position:`absolute`,left:`-3.25px`},".cm-selectionHandle-start::before":{top:`-8px`},".cm-selectionHandle-end::before":{bottom:`-8px`},".cm-dropCursor":{position:`absolute`},"&.cm-focused > .cm-scroller > .cm-cursorLayer .cm-cursor":{display:`block`},".cm-iso":{unicodeBidi:`isolate`},".cm-announced":{position:`fixed`,top:`-10000px`},"@media print":{".cm-announced":{display:`none`}},"&light .cm-activeLine":{backgroundColor:`#cceeff44`},"&dark .cm-activeLine":{backgroundColor:`#99eeff33`},"&light .cm-specialChar":{color:`red`},"&dark .cm-specialChar":{color:`#f78`},".cm-gutters":{flexShrink:0,display:`flex`,height:`100%`,boxSizing:`border-box`,zIndex:200},".cm-gutters-before":{insetInlineStart:0},".cm-gutters-after":{insetInlineEnd:0},"&light .cm-gutters":{backgroundColor:`#f5f5f5`,color:`#6c6c6c`,border:`0px solid #ddd`,"&.cm-gutters-before":{borderRightWidth:`1px`},"&.cm-gutters-after":{borderLeftWidth:`1px`}},"&dark .cm-gutters":{backgroundColor:`#333338`,color:`#ccc`},".cm-gutter":{display:`flex !important`,flexDirection:`column`,flexShrink:0,boxSizing:`border-box`,minHeight:`100%`,overflow:`hidden`},".cm-gutterElement":{boxSizing:`border-box`},".cm-lineNumbers .cm-gutterElement":{padding:`0 3px 0 5px`,minWidth:`20px`,textAlign:`right`,whiteSpace:`nowrap`},"&light .cm-activeLineGutter":{backgroundColor:`#e2f2ff`},"&dark .cm-activeLineGutter":{backgroundColor:`#222227`},".cm-panels":{boxSizing:`border-box`,position:`sticky`,left:0,right:0,zIndex:300},"&light .cm-panels":{backgroundColor:`#f5f5f5`,color:`black`},".cm-panels-top":{top:`0`},".cm-panels-bottom":{bottom:`0`},"&light .cm-panels-top":{borderBottom:`1px solid #ddd`},"&light .cm-panels-bottom":{borderTop:`1px solid #ddd`},"&dark .cm-panels":{backgroundColor:`#333338`,color:`white`},".cm-dialog":{padding:`2px 19px 4px 6px`,position:`relative`,"& label":{fontSize:`80%`}},".cm-dialog-close":{position:`absolute`,top:`3px`,right:`4px`,backgroundColor:`inherit`,border:`none`,font:`inherit`,fontSize:`14px`,padding:`0`},".cm-tab":{display:`inline-block`,overflow:`hidden`,verticalAlign:`bottom`},".cm-widgetBuffer":{verticalAlign:`baseline`,height:`1em`,width:0,display:`inline`},".cm-placeholder":{color:`#888`,display:`inline-block`,verticalAlign:`top`,userSelect:`none`},".cm-highlightSpace":{background:`radial-gradient(circle at 50% 55%, #aaa 20%, transparent 0) no-repeat`,backgroundSize:`.4em`,backgroundPosition:`calc(min(50%, 0px)) center`},".cm-highlightTab":{backgroundImage:`url('data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="200" height="20"><path stroke="%23888" stroke-width="1" fill="none" d="M1 10H196L190 5M190 15L196 10M197 4L197 16"/></svg>')`,backgroundSize:`auto 100%`,backgroundPosition:`right 90%`,backgroundRepeat:`no-repeat`},".cm-trailingSpace":{backgroundColor:`#ff332255`},".cm-button":{verticalAlign:`middle`,color:`inherit`,fontSize:`70%`,padding:`.2em 1em`,borderRadius:`1px`},"&light .cm-button":{backgroundImage:`linear-gradient(#eff1f5, #d9d9df)`,border:`1px solid #888`,"&:active":{backgroundImage:`linear-gradient(#b4b4b4, #d0d3d6)`}},"&dark .cm-button":{backgroundImage:`linear-gradient(#393939, #111)`,border:`1px solid #888`,"&:active":{backgroundImage:`linear-gradient(#111, #333)`}},".cm-textfield":{verticalAlign:`middle`,color:`inherit`,fontSize:`70%`,border:`1px solid silver`,padding:`.2em .5em`},"&light .cm-textfield":{backgroundColor:`white`},"&dark .cm-textfield":{border:`1px solid #555`,backgroundColor:`inherit`}},Zo),es={childList:!0,characterData:!0,subtree:!0,attributes:!0,characterDataOldValue:!0},ts=L.ie&&L.ie_version<=11,ns=class{constructor(e){this.view=e,this.active=!1,this.editContext=null,this.selectionRange=new $n,this.selectionChanged=!1,this.delayedFlush=-1,this.resizeTimeout=-1,this.queue=[],this.delayedAndroidKey=null,this.flushingAndroidKey=-1,this.lastChange=0,this.scrollTargets=[],this.intersection=null,this.resizeScroll=null,this.intersecting=!1,this.gapIntersection=null,this.gaps=[],this.printQuery=null,this.parentCheck=-1,this.dom=e.contentDOM,this.observer=new MutationObserver(t=>{for(let e of t)this.queue.push(e);(L.ie&&L.ie_version<=11||L.ios&&e.composing)&&t.some(e=>e.type==`childList`&&e.removedNodes.length||e.type==`characterData`&&e.oldValue.length>e.target.nodeValue.length)?this.flushSoon():this.flush()}),window.EditContext&&L.android&&e.constructor.EDIT_CONTEXT!==!1&&!(L.chrome&&L.chrome_version<126)&&(this.editContext=new os(e),e.state.facet(Xr)&&(e.contentDOM.editContext=this.editContext.editContext)),ts&&(this.onCharData=e=>{this.queue.push({target:e.target,type:`characterData`,oldValue:e.prevValue}),this.flushSoon()}),this.onSelectionChange=this.onSelectionChange.bind(this),this.onResize=this.onResize.bind(this),this.onPrint=this.onPrint.bind(this),this.onScroll=this.onScroll.bind(this),window.matchMedia&&(this.printQuery=window.matchMedia(`print`)),typeof ResizeObserver==`function`&&(this.resizeScroll=new ResizeObserver(()=>{this.view.docView?.lastUpdate<Date.now()-75&&this.onResize()}),this.resizeScroll.observe(e.scrollDOM)),this.addWindowListeners(this.win=e.win),this.start(),typeof IntersectionObserver==`function`&&(this.intersection=new IntersectionObserver(e=>{this.parentCheck<0&&(this.parentCheck=setTimeout(this.listenForScroll.bind(this),1e3)),e.length>0&&e[e.length-1].intersectionRatio>0!=this.intersecting&&(this.intersecting=!this.intersecting,this.intersecting!=this.view.inView&&this.onScrollChanged(document.createEvent(`Event`)))},{threshold:[0,.001]}),this.intersection.observe(this.dom),this.gapIntersection=new IntersectionObserver(e=>{e.length>0&&e[e.length-1].intersectionRatio>0&&this.onScrollChanged(document.createEvent(`Event`))},{})),this.listenForScroll(),this.readSelectionRange()}onScrollChanged(e){this.view.inputState.runHandlers(`scroll`,e),this.intersecting&&this.view.measure()}onScroll(e){this.intersecting&&this.flush(this.pendingRecords().length>0),this.editContext&&this.view.requestMeasure(this.editContext.measureReq),this.onScrollChanged(e)}onResize(){this.resizeTimeout<0&&(this.resizeTimeout=setTimeout(()=>{this.resizeTimeout=-1,this.view.requestMeasure()},50))}onPrint(e){(e.type!=`change`&&e.type||e.matches)&&(this.view.viewState.printing=!0,this.view.measure(),setTimeout(()=>{this.view.viewState.printing=!1,this.view.requestMeasure()},500))}updateGaps(e){if(this.gapIntersection&&(e.length!=this.gaps.length||this.gaps.some((t,n)=>t!=e[n]))){this.gapIntersection.disconnect();for(let t of e)this.gapIntersection.observe(t);this.gaps=e}}onSelectionChange(e){let t=this.selectionChanged;if(!this.readSelectionRange()||this.delayedAndroidKey)return;let{view:n}=this,r=this.selectionRange;if(n.state.facet(Xr)?n.root.activeElement!=this.dom:!Vn(this.dom,r))return;let i=r.anchorNode&&n.docView.tile.nearest(r.anchorNode);i&&i.isWidget()&&i.widget.ignoreEvent(e)?t||(this.selectionChanged=!1):(L.ie&&L.ie_version<=11||L.android&&L.chrome)&&!n.state.selection.main.empty&&r.focusNode&&Un(r.focusNode,r.focusOffset,r.anchorNode,r.anchorOffset)?this.flushSoon():this.flush(!1)}readSelectionRange(){let{view:e}=this,t=zn(e.root);if(!t)return!1;let n=L.safari&&e.root.nodeType==11&&e.root.activeElement==this.dom&&as(this.view,t)||t;if(!n||this.selectionRange.eq(n))return!1;let r=Vn(this.dom,n);return r&&!this.selectionChanged&&e.inputState.lastFocusTime>Date.now()-200&&e.inputState.lastTouchTime<Date.now()-300&&cr(this.dom,n)?(this.view.inputState.lastFocusTime=0,e.docView.updateSelection(),!1):(this.selectionRange.setRange(n),r&&(this.selectionChanged=!0),!0)}setSelectionRange(e,t){this.selectionRange.set(e.node,e.offset,t.node,t.offset),this.selectionChanged=!1}clearSelectionRange(){this.selectionRange.set(null,0,null,0)}listenForScroll(){this.parentCheck=-1;let e=0,t=null;for(let n=this.dom;n;)if(z(n))!t&&e<this.scrollTargets.length&&this.scrollTargets[e]==n?e++:t||=this.scrollTargets.slice(0,e),t&&t.push(n),n=n.assignedSlot||n.parentNode;else if(n.nodeType==11)n=n.host;else break;if(e<this.scrollTargets.length&&!t&&(t=this.scrollTargets.slice(0,e)),t){for(let e of this.scrollTargets)e.removeEventListener(`scroll`,this.onScroll);for(let e of this.scrollTargets=t)e.addEventListener(`scroll`,this.onScroll)}}ignore(e){if(!this.active)return e();try{return this.stop(),e()}finally{this.start(),this.clear()}}start(){this.active||=(this.observer.observe(this.dom,es),ts&&this.dom.addEventListener(`DOMCharacterDataModified`,this.onCharData),!0)}stop(){this.active&&(this.active=!1,this.observer.disconnect(),ts&&this.dom.removeEventListener(`DOMCharacterDataModified`,this.onCharData))}clear(){this.processRecords(),this.queue.length=0,this.selectionChanged=!1}delayAndroidKey(e,t){if(!this.delayedAndroidKey){let e=()=>{let e=this.delayedAndroidKey;e&&(this.clearDelayedAndroidKey(),this.view.inputState.lastKeyCode=e.keyCode,this.view.inputState.lastKeyTime=Date.now(),!this.flush()&&e.force&&or(this.dom,e.key,e.keyCode))};this.flushingAndroidKey=this.view.win.requestAnimationFrame(e)}this.delayedAndroidKey&&e!=`Enter`||(this.delayedAndroidKey={key:e,keyCode:t,force:this.lastChange<Date.now()-50||!!this.delayedAndroidKey?.force})}clearDelayedAndroidKey(){this.win.cancelAnimationFrame(this.flushingAndroidKey),this.delayedAndroidKey=null,this.flushingAndroidKey=-1}flushSoon(){this.delayedFlush<0&&(this.delayedFlush=this.view.win.requestAnimationFrame(()=>{this.delayedFlush=-1,this.flush()}))}forceFlush(){this.delayedFlush>=0&&(this.view.win.cancelAnimationFrame(this.delayedFlush),this.delayedFlush=-1),this.flush()}pendingRecords(){for(let e of this.observer.takeRecords())this.queue.push(e);return this.queue}processRecords(){let e=this.pendingRecords();e.length&&(this.queue=[]);let t=-1,n=-1,r=!1;for(let i of e){let e=this.readMutation(i);e&&(e.typeOver&&(r=!0),t==-1?{from:t,to:n}=e:(t=Math.min(e.from,t),n=Math.max(e.to,n)))}return{from:t,to:n,typeOver:r}}readChange(){let{from:e,to:t,typeOver:n}=this.processRecords(),r=this.selectionChanged&&Vn(this.dom,this.selectionRange);if(e<0&&!r)return null;e>-1&&(this.lastChange=Date.now()),this.view.inputState.lastFocusTime=0,this.selectionChanged=!1;let i=new va(this.view,e,t,n);return this.view.docView.domChanged={newSel:i.newSel?i.newSel.main:null},i}flush(e=!0){if(this.delayedFlush>=0||this.delayedAndroidKey)return!1;e&&this.readSelectionRange();let t=this.selectionChanged,n=this.readChange();if(!n)return this.view.requestMeasure(),!1;let r=this.view.state,i=ba(this.view,n);return this.view.state==r&&(n.domChanged||t)&&this.view.update([]),i}readMutation(e){let t=this.view.docView.tile.nearest(e.target);if(!t||t.isWidget())return null;if(t.markDirty(e.type==`attributes`),e.type==`childList`){let n=rs(t,e.previousSibling||e.target.previousSibling,-1),r=rs(t,e.nextSibling||e.target.nextSibling,1);return{from:n?t.posAfter(n):t.posAtStart,to:r?t.posBefore(r):t.posAtEnd,typeOver:!1}}return e.type==`characterData`?{from:t.posAtStart,to:t.posAtEnd,typeOver:e.target.nodeValue==e.oldValue}:null}setWindow(e){e!=this.win&&(this.removeWindowListeners(this.win),this.win=e,this.addWindowListeners(this.win))}addWindowListeners(e){e.addEventListener(`resize`,this.onResize),this.printQuery?this.printQuery.addEventListener?this.printQuery.addEventListener(`change`,this.onPrint):this.printQuery.addListener(this.onPrint):e.addEventListener(`beforeprint`,this.onPrint),e.addEventListener(`scroll`,this.onScroll),e.document.addEventListener(`selectionchange`,this.onSelectionChange)}removeWindowListeners(e){e.removeEventListener(`scroll`,this.onScroll),e.removeEventListener(`resize`,this.onResize),this.printQuery?this.printQuery.removeEventListener?this.printQuery.removeEventListener(`change`,this.onPrint):this.printQuery.removeListener(this.onPrint):e.removeEventListener(`beforeprint`,this.onPrint),e.document.removeEventListener(`selectionchange`,this.onSelectionChange)}update(e){this.editContext&&(this.editContext.update(e),e.startState.facet(Xr)!=e.state.facet(Xr)&&(e.view.contentDOM.editContext=e.state.facet(Xr)?this.editContext.editContext:null))}destroy(){var e,t,n;this.stop(),(e=this.intersection)==null||e.disconnect(),(t=this.gapIntersection)==null||t.disconnect(),(n=this.resizeScroll)==null||n.disconnect();for(let e of this.scrollTargets)e.removeEventListener(`scroll`,this.onScroll);this.removeWindowListeners(this.win),clearTimeout(this.parentCheck),clearTimeout(this.resizeTimeout),this.win.cancelAnimationFrame(this.delayedFlush),this.win.cancelAnimationFrame(this.flushingAndroidKey),this.editContext&&(this.view.contentDOM.editContext=null,this.editContext.destroy())}};function rs(e,t,n){for(;t;){let r=U.get(t);if(r&&r.parent==e)return r;let i=t.parentNode;t=i==e.dom?n>0?t.nextSibling:t.previousSibling:i}return null}function is(e,t){let n=t.startContainer,r=t.startOffset,i=t.endContainer,a=t.endOffset,o=e.docView.domAtPos(e.state.selection.main.anchor,1);return Un(o.node,o.offset,i,a)&&([n,r,i,a]=[i,a,n,r]),{anchorNode:n,anchorOffset:r,focusNode:i,focusOffset:a}}function as(e,t){if(t.getComposedRanges){let n=t.getComposedRanges(e.root)[0];if(n)return is(e,n)}let n=null;function r(e){e.preventDefault(),e.stopImmediatePropagation(),n=e.getTargetRanges()[0]}return e.contentDOM.addEventListener(`beforeinput`,r,!0),e.dom.ownerDocument.execCommand(`indent`),e.contentDOM.removeEventListener(`beforeinput`,r,!0),n?is(e,n):null}var os=class{constructor(e){this.from=0,this.to=0,this.pendingContextChange=null,this.handlers=Object.create(null),this.composing=null,this.resetRange(e.state);let t=this.editContext=new window.EditContext({text:e.state.doc.sliceString(this.from,this.to),selectionStart:this.toContextPos(Math.max(this.from,Math.min(this.to,e.state.selection.main.anchor))),selectionEnd:this.toContextPos(e.state.selection.main.head)});this.handlers.textupdate=n=>{let r=e.state.selection.main,{anchor:i,head:a}=r,o=this.toEditorPos(n.updateRangeStart),s=this.toEditorPos(n.updateRangeEnd);e.inputState.composing>=0&&!this.composing&&(this.composing={contextBase:n.updateRangeStart,editorBase:o,drifted:!1});let c=s-o>n.text.length;o==this.from&&i<this.from?o=i:s==this.to&&i>this.to&&(s=i);let l=Ca(e.state.sliceDoc(o,s),n.text,(c?r.from:r.to)-o,c?`end`:null);if(!l){let t=A.single(this.toEditorPos(n.selectionStart),this.toEditorPos(n.selectionEnd));Ea(t,r)||e.dispatch({selection:t,userEvent:`select`});return}let u={from:l.from+o,to:l.toA+o,insert:E.of(n.text.slice(l.from,l.toB).split(`
`))};if((L.mac||L.android)&&u.from==a-1&&/^\. ?$/.test(n.text)&&e.contentDOM.getAttribute(`autocorrect`)==`off`&&(u={from:o,to:s,insert:E.of([n.text.replace(`.`,` `)])}),this.pendingContextChange=u,!e.state.readOnly){let t=this.to-this.from+(u.to-u.from+u.insert.length);xa(e,u,A.single(this.toEditorPos(n.selectionStart,t),this.toEditorPos(n.selectionEnd,t)))}this.pendingContextChange&&(this.revertPending(e.state),this.setSelection(e.state)),u.from<u.to&&!u.insert.length&&e.inputState.composing>=0&&!/[\\p{Alphabetic}\\p{Number}_]/.test(t.text.slice(Math.max(0,n.updateRangeStart-1),Math.min(t.text.length,n.updateRangeStart+1)))&&this.handlers.compositionend(n)},this.handlers.characterboundsupdate=n=>{let r=[],i=null;for(let t=this.toEditorPos(n.rangeStart),a=this.toEditorPos(n.rangeEnd);t<a;t++){let n=e.coordsForChar(t);i=n&&new DOMRect(n.left,n.top,n.right-n.left,n.bottom-n.top)||i||new DOMRect,r.push(i)}t.updateCharacterBounds(n.rangeStart,r)},this.handlers.textformatupdate=t=>{let n=[];for(let e of t.getTextFormats()){let t=e.underlineStyle,r=e.underlineThickness;if(!/none/i.test(t)&&!/none/i.test(r)){let i=this.toEditorPos(e.rangeStart),a=this.toEditorPos(e.rangeEnd);if(i<a){let e=`text-decoration: underline ${/^[a-z]/.test(t)?t+` `:t==`Dashed`?`dashed `:t==`Squiggle`?`wavy `:``}${/thin/i.test(r)?1:2}px`;n.push(R.mark({attributes:{style:e}}).range(i,a))}}}e.dispatch({effects:Jr.of(R.set(n))})},this.handlers.compositionstart=()=>{e.inputState.composing<0&&(e.inputState.composing=0,e.inputState.compositionFirstChange=!0)},this.handlers.compositionend=()=>{if(e.inputState.composing=-1,e.inputState.compositionFirstChange=null,this.composing){let{drifted:t}=this.composing;this.composing=null,t&&this.reset(e.state)}};for(let e in this.handlers)t.addEventListener(e,this.handlers[e]);this.measureReq={read:e=>{let t=zn(e.root);t&&t.rangeCount&&this.editContext.updateSelectionBounds(t.getRangeAt(0).getBoundingClientRect())}}}applyEdits(e){let t=0,n=!1,r=this.pendingContextChange;return e.changes.iterChanges((i,a,o,s,c)=>{if(n)return;let l=c.length-(a-i);if(r&&a>=r.to){if(r.from==i&&r.to==a&&r.insert.eq(c)){r=this.pendingContextChange=null,t+=l,this.to+=l;return}r=null,this.revertPending(e.state)}if(i+=t,a+=t,a<=this.from)this.from+=l,this.to+=l;else if(i<this.to){if(i<this.from||a>this.to||this.to-this.from+c.length>3e4){n=!0;return}this.editContext.updateText(this.toContextPos(i),this.toContextPos(a),c.toString()),this.to+=l}t+=l}),r&&!n&&this.revertPending(e.state),!n}update(e){let t=this.pendingContextChange,n=e.startState.selection.main;this.composing&&(this.composing.drifted||!e.changes.touchesRange(n.from,n.to)&&e.transactions.some(e=>!e.isUserEvent(`input.type`)&&e.changes.touchesRange(this.from,this.to)))?(this.composing.drifted=!0,this.composing.editorBase=e.changes.mapPos(this.composing.editorBase)):!this.applyEdits(e)||!this.rangeIsValid(e.state)?(this.pendingContextChange=null,this.reset(e.state)):(e.docChanged||e.selectionSet||t)&&this.setSelection(e.state),(e.geometryChanged||e.docChanged||e.selectionSet)&&e.view.requestMeasure(this.measureReq)}resetRange(e){let{head:t}=e.selection.main;this.from=Math.max(0,t-1e4),this.to=Math.min(e.doc.length,t+1e4)}reset(e){this.resetRange(e),this.editContext.updateText(0,this.editContext.text.length,e.doc.sliceString(this.from,this.to)),this.setSelection(e)}revertPending(e){let t=this.pendingContextChange;this.pendingContextChange=null,this.editContext.updateText(this.toContextPos(t.from),this.toContextPos(t.from+t.insert.length),e.doc.sliceString(t.from,t.to))}setSelection(e){let{main:t}=e.selection,n=this.toContextPos(Math.max(this.from,Math.min(this.to,t.anchor))),r=this.toContextPos(t.head);(this.editContext.selectionStart!=n||this.editContext.selectionEnd!=r)&&this.editContext.updateSelection(n,r)}rangeIsValid(e){let{head:t}=e.selection.main;return!(this.from>0&&t-this.from<500||this.to<e.doc.length&&this.to-t<500||this.to-this.from>3e4)}toEditorPos(e,t=this.to-this.from){e=Math.min(e,t);let n=this.composing;return n&&n.drifted?n.editorBase+(e-n.contextBase):e+this.from}toContextPos(e){let t=this.composing;return t&&t.drifted?t.contextBase+(e-t.editorBase):e-this.from}destroy(){for(let e in this.handlers)this.editContext.removeEventListener(e,this.handlers[e])}},G=class e{get state(){return this.viewState.state}get viewport(){return this.viewState.viewport}get visibleRanges(){return this.viewState.visibleRanges}get inView(){return this.viewState.inView}get composing(){return!!this.inputState&&this.inputState.composing>0}get compositionStarted(){return!!this.inputState&&this.inputState.composing>=0}get root(){return this._root}get win(){return this.dom.ownerDocument.defaultView||window}constructor(e={}){this.plugins=[],this.pluginMap=new Map,this.editorAttrs={},this.contentAttrs={},this.bidiCache=[],this.destroyed=!1,this.updateState=2,this.measureScheduled=-1,this.measureRequests=[],this.clearAnnouncement=-1,this.contentDOM=document.createElement(`div`),this.scrollDOM=document.createElement(`div`),this.scrollDOM.tabIndex=-1,this.scrollDOM.className=`cm-scroller`,this.scrollDOM.appendChild(this.contentDOM),this.announceDOM=document.createElement(`div`),this.announceDOM.className=`cm-announced`,this.announceDOM.setAttribute(`aria-live`,`polite`),this.dom=document.createElement(`div`),this.dom.appendChild(this.announceDOM),this.dom.appendChild(this.scrollDOM),e.parent&&e.parent.appendChild(this.dom);let{dispatch:t}=e;this.dispatchTransactions=e.dispatchTransactions||t&&(e=>e.forEach(e=>t(e,this)))||(e=>this.update(e)),this.dispatch=this.dispatch.bind(this),this._root=e.root||sr(e.parent)||document,this.viewState=new Io(this,e.state||P.create(e)),e.scrollTo&&e.scrollTo.is(qr)&&(this.viewState.scrollTarget=e.scrollTo.value.clip(this.viewState.state)),this.plugins=this.state.facet(Qr).map(e=>new $r(e));for(let e of this.plugins)e.update(this);this.observer=new ns(this),this.inputState=new Da(this),this.inputState.ensureHandlers(this.plugins),this.docView=new Vi(this),this.mountStyles(),this.updateAttrs(),this.updateState=0,this.requestMeasure(),document.fonts?.ready&&document.fonts.ready.then(()=>{this.viewState.mustMeasureContent=`refresh`,this.requestMeasure()})}dispatch(...e){let t=e.length==1&&e[0]instanceof gt?e:e.length==1&&Array.isArray(e[0])?e[0]:[this.state.update(...e)];this.dispatchTransactions(t,this)}update(t){if(this.updateState!=0)throw Error(`Calls to EditorView.update are not allowed while an update is in progress`);let n=!1,r=!1,i,a=this.state;for(let e of t){if(e.startState!=a)throw RangeError(`Trying to update state with a transaction that doesn't start from the previous state.`);a=e.state}if(this.destroyed){this.viewState.state=a;return}let o=this.hasFocus,s=0,c=null;t.some(e=>e.annotation(oo))?(this.inputState.notifiedFocused=o,s=1):o!=this.inputState.notifiedFocused&&(this.inputState.notifiedFocused=o,c=so(a,o),c||(s=1));let l=this.observer.delayedAndroidKey,u=null;if(l?(this.observer.clearDelayedAndroidKey(),u=this.observer.readChange(),(u&&!this.state.doc.eq(a.doc)||!this.state.selection.eq(a.selection))&&(u=null)):this.observer.clear(),a.facet(P.phrases)!=this.state.facet(P.phrases))return this.setState(a);i=pi.create(this,a,t),i.flags|=s;let d=this.viewState.scrollTarget;try{this.updateState=2;for(let n of t){if(d&&=d.map(n.changes),n.scrollIntoView){let{main:t}=n.state.selection,{x:r,y:i}=this.state.facet(e.cursorScrollMargin);d=new Kr(t.empty?t:A.cursor(t.head,t.head>t.anchor?-1:1),`nearest`,`nearest`,i,r)}for(let e of n.effects)e.is(qr)&&(d=e.value.clip(this.state))}this.viewState.update(i,d),this.bidiCache=ls.update(this.bidiCache,i.changes),i.empty||(this.updatePlugins(i),this.inputState.update(i)),n=this.docView.update(i),this.state.facet(di)!=this.styleModules&&this.mountStyles(),r=this.updateAttrs(),this.showAnnouncements(t),this.docView.updateSelection(n,t.some(e=>e.isUserEvent(`select.pointer`)))}finally{this.updateState=0}if(i.startState.facet(Ko)!=i.state.facet(Ko)&&(this.viewState.mustMeasureContent=!0),(n||r||d||this.viewState.mustEnforceCursorAssoc||this.viewState.mustMeasureContent)&&this.requestMeasure(),n&&this.docViewUpdate(),!i.empty)for(let e of this.state.facet(Rr))try{e(i)}catch(e){Yr(this.state,e,`update listener`)}(c||u)&&Promise.resolve().then(()=>{c&&this.state==c.startState&&this.dispatch(c),u&&!ba(this,u)&&l.force&&or(this.contentDOM,l.key,l.keyCode)})}setState(e){if(this.updateState!=0)throw Error(`Calls to EditorView.setState are not allowed while an update is in progress`);if(this.destroyed){this.viewState.state=e;return}this.updateState=2;let t=this.hasFocus;try{for(let e of this.plugins)e.destroy(this);this.viewState=new Io(this,e),this.plugins=e.facet(Qr).map(e=>new $r(e)),this.pluginMap.clear();for(let e of this.plugins)e.update(this);this.docView.destroy(),this.docView=new Vi(this),this.inputState.ensureHandlers(this.plugins),this.mountStyles(),this.updateAttrs(),this.bidiCache=[]}finally{this.updateState=0}t&&this.focus(),this.requestMeasure()}updatePlugins(e){let t=e.startState.facet(Qr),n=e.state.facet(Qr);if(t!=n){let r=[];for(let i of n){let n=t.indexOf(i);if(n<0)r.push(new $r(i));else{let t=this.plugins[n];t.mustUpdate=e,r.push(t)}}for(let t of this.plugins)t.mustUpdate!=e&&t.destroy(this);this.plugins=r,this.pluginMap.clear()}else for(let t of this.plugins)t.mustUpdate=e;for(let e=0;e<this.plugins.length;e++)this.plugins[e].update(this);t!=n&&this.inputState.ensureHandlers(this.plugins)}docViewUpdate(){for(let e of this.plugins){let t=e.value;if(t&&t.docViewUpdate)try{t.docViewUpdate(this)}catch(e){Yr(this.state,e,`doc view update listener`)}}}measure(e=!0){if(this.destroyed)return;if(this.measureScheduled>-1&&this.win.cancelAnimationFrame(this.measureScheduled),this.observer.delayedAndroidKey){this.measureScheduled=-1,this.requestMeasure();return}this.measureScheduled=0,e&&this.observer.forceFlush();let t=null,n=this.viewState.scrollParent,r=this.viewState.getScrollOffset(),{scrollAnchorPos:i,scrollAnchorHeight:a,scaleY:o}=this.viewState;Math.abs(r-this.viewState.scrollOffset)>1&&(a=-1),this.viewState.scrollAnchorHeight=-1;try{for(let e=0;;e++){if(a<0){if(lr(n||this.win))i=-1,a=this.viewState.heightMap.height/this.viewState.scaleY;else{let e=this.viewState.scrollAnchorAt(r);i=e.from,a=e.top}o=this.viewState.scaleY}this.updateState=1;let s=this.viewState.measure();if(!s&&!this.measureRequests.length&&this.viewState.scrollTarget==null)break;if(e>5){console.warn(this.measureRequests.length?`Measure loop restarted more than 5 times`:`Viewport failed to stabilize`);break}let c=[];s&4||([this.measureRequests,c]=[c,this.measureRequests]);let l=c.map(e=>{try{return e.read(this)}catch(e){return Yr(this.state,e),cs}}),u=pi.create(this,this.state,[]),d=!1;u.flags|=s,t?t.flags|=s:t=u,this.updateState=2,u.empty||(this.updatePlugins(u),this.inputState.update(u),this.updateAttrs(),d=this.docView.update(u),d&&this.docViewUpdate());for(let e=0;e<c.length;e++)if(l[e]!=cs)try{let t=c[e];t.write&&t.write(l[e],this)}catch(e){Yr(this.state,e)}if(d&&this.docView.updateSelection(!0),!u.viewportChanged&&this.measureRequests.length==0){if(this.viewState.editorHeight){if(this.viewState.scrollTarget){this.docView.scrollIntoView(this.viewState.scrollTarget),this.viewState.scrollTarget=null,a=-1;continue}{let e=(i<0?this.viewState.heightMap.height:this.viewState.lineBlockAt(i).top)/this.viewState.scaleY-a/o;if((e>1||e<-1)&&!(L.ios&&this.inputState.lastIOSMomentumScroll>Date.now()-100)&&(n==this.scrollDOM||this.hasFocus||Math.max(this.inputState.lastWheelEvent,this.inputState.lastTouchTime)>Date.now()-100)){r+=e,n?i<0?n.scrollTop=n.scrollHeight:n.scrollTop+=e:this.win.scrollBy(0,e),a=-1;continue}}}break}}}finally{this.updateState=0,this.measureScheduled=-1}if(t&&!t.empty)for(let e of this.state.facet(Rr))e(t)}get themeClasses(){return Jo+` `+(this.state.facet(qo)?Xo:Yo)+` `+this.state.facet(Ko)}updateAttrs(){let e=us(this,ei,{class:`cm-editor`+(this.hasFocus?` cm-focused `:` `)+this.themeClasses}),t={spellcheck:`false`,autocorrect:`off`,autocapitalize:`off`,writingsuggestions:`false`,translate:`no`,contenteditable:this.state.facet(Xr)?`true`:`false`,class:`cm-content`,style:`${L.tabSize}: ${this.state.tabSize}`,role:`textbox`,"aria-multiline":`true`};this.state.readOnly&&(t[`aria-readonly`]=`true`),us(this,ti,t);let n=this.observer.ignore(()=>{let n=Dn(this.contentDOM,this.contentAttrs,t),r=Dn(this.dom,this.editorAttrs,e);return n||r});return this.editorAttrs=e,this.contentAttrs=t,n}showAnnouncements(t){let n=!0;for(let r of t)for(let t of r.effects)if(t.is(e.announce)){n&&=(this.announceDOM.textContent=``,this.win.clearTimeout(this.clearAnnouncement),this.clearAnnouncement=this.win.setTimeout(()=>{this.announceDOM.textContent=`\xA0`},200),!1);let e=this.announceDOM.appendChild(document.createElement(`div`));e.textContent=t.value}}mountStyles(){this.styleModules=this.state.facet(di);let t=this.state.facet(e.cspNonce);en.mount(this.root,this.styleModules.concat($o).reverse(),t?{nonce:t}:void 0)}readMeasured(){if(this.updateState==2)throw Error(`Reading the editor layout isn't allowed during an update`);this.updateState==0&&this.measureScheduled>-1&&this.measure(!1)}requestMeasure(e){if(this.measureScheduled<0&&(this.measureScheduled=this.win.requestAnimationFrame(()=>this.measure())),e){if(this.measureRequests.indexOf(e)>-1)return;if(e.key!=null){for(let t=0;t<this.measureRequests.length;t++)if(this.measureRequests[t].key===e.key){this.measureRequests[t]=e;return}}this.measureRequests.push(e)}}plugin(e){let t=this.pluginMap.get(e);return(t===void 0||t&&t.plugin!=e)&&this.pluginMap.set(e,t=this.plugins.find(t=>t.plugin==e)||null),t&&t.update(this).value}get documentTop(){return this.contentDOM.getBoundingClientRect().top+this.viewState.paddingTop}get documentPadding(){return{top:this.viewState.paddingTop,bottom:this.viewState.paddingBottom}}get scaleX(){return this.viewState.scaleX}get scaleY(){return this.viewState.scaleY}elementAtHeight(e){return this.readMeasured(),this.viewState.elementAtHeight(e)}lineBlockAtHeight(e){return this.readMeasured(),this.viewState.lineBlockAtHeight(e)}get viewportLineBlocks(){return this.viewState.viewportLines}lineBlockAt(e){return this.viewState.lineBlockAt(e)}get contentHeight(){return this.viewState.contentHeight}moveByChar(e,t,n){return la(this,e,ia(this,e,t,n))}moveByGroup(e,t){return la(this,e,ia(this,e,t,t=>aa(this,e.head,t)))}visualLineSide(e,t){return t?A.cursor(e.to,1):A.cursor(e.from,-1)}moveToLineBoundary(e,t,n=!0){return ra(this,e,t,n)}moveVertically(e,t,n){return la(this,e,oa(this,e,t,n))}domAtPos(e,t=1){return this.docView.domAtPos(e,t)}posAtDOM(e,t=0){return this.docView.posFromDOM(e,t)}posAtCoords(e,t=!0){this.readMeasured();let n=da(this,e,t);return n&&n.pos}posAndSideAtCoords(e,t=!0){return this.readMeasured(),da(this,e,t)}coordsAtPos(e,t=1){this.readMeasured();let n=this.state.doc.lineAt(e),r=this.bidiSpans(n),i=r[Sr.find(r,e-n.from,-1,t)];return n.length&&(e==n.from&&t<0||e==n.to&&t>0)&&i.dir!=this.textDirectionAt(n.from)&&(e==n.to?(e=n.from+i.from,t=1):(e=n.from+i.to,t=-1)),this.docView.coordsAt(e,t,i.dir==B.RTL)}coordsForChar(e){return this.readMeasured(),this.docView.coordsForChar(e)}get defaultCharacterWidth(){return this.viewState.heightOracle.charWidth}get defaultLineHeight(){return this.viewState.heightOracle.lineHeight}get textDirection(){return this.viewState.defaultTextDirection}textDirectionAt(e){return!this.state.facet(Ur)||e<this.viewport.from||e>this.viewport.to?this.textDirection:(this.readMeasured(),this.docView.textDirectionAt(e))}get lineWrapping(){return this.viewState.heightOracle.lineWrapping}bidiSpans(e){if(e.length>ss)return Ar(e.length);let t=this.textDirectionAt(e.from),n;for(let r of this.bidiCache)if(r.from==e.from&&r.dir==t&&(r.fresh||Cr(r.isolates,n=si(this,e))))return r.order;n||=si(this,e);let r=kr(e.text,t,n);return this.bidiCache.push(new ls(e.from,e.to,t,n,!0,r)),r}get hasFocus(){return(this.dom.ownerDocument.hasFocus()||L.safari&&this.inputState?.lastContextMenu>Date.now()-3e4)&&this.root.activeElement==this.contentDOM}focus(){this.observer.ignore(()=>{rr(this.contentDOM),this.docView.updateSelection()})}setRoot(e){this._root!=e&&(this._root=e,this.observer.setWindow((e.nodeType==9?e:e.ownerDocument).defaultView||window),this.mountStyles())}destroy(){this.root.activeElement==this.contentDOM&&this.contentDOM.blur();for(let e of this.plugins)e.destroy(this);this.plugins=[],this.inputState.destroy(),this.docView.destroy(),this.dom.remove(),this.observer.destroy(),this.win.clearTimeout(this.clearAnnouncement),this.measureScheduled>-1&&this.win.cancelAnimationFrame(this.measureScheduled),this.destroyed=!0}static scrollIntoView(e,t={}){return qr.of(new Kr(typeof e==`number`?A.cursor(e):e,t.y??`nearest`,t.x??`nearest`,t.yMargin??5,t.xMargin??5))}scrollSnapshot(){let{scrollTop:e,scrollLeft:t}=this.scrollDOM,n=this.viewState.scrollAnchorAt(e);return qr.of(new Kr(A.cursor(n.from),`start`,`start`,n.top-e,t,!0))}setTabFocusMode(e){e==null?this.inputState.tabFocusMode=this.inputState.tabFocusMode<0?0:-1:typeof e==`boolean`?this.inputState.tabFocusMode=e?0:-1:this.inputState.tabFocusMode!=0&&(this.inputState.tabFocusMode=Date.now()+e)}static domEventHandlers(e){return H.define(()=>({}),{eventHandlers:e})}static domEventObservers(e){return H.define(()=>({}),{eventObservers:e})}static theme(e,t){let n=en.newName(),r=[Ko.of(n),di.of(Qo(`.${n}`,e))];return t&&t.dark&&r.push(qo.of(!0)),r}static baseTheme(e){return Qe.lowest(di.of(Qo(`.`+Jo,e,Zo)))}static findFromDOM(e){let t=e.querySelector(`.cm-content`);return(t&&U.get(t)||U.get(e))?.root?.view||null}};G.styleModule=di,G.inputHandler=zr,G.clipboardInputFilter=Vr,G.clipboardOutputFilter=Hr,G.scrollHandler=Gr,G.focusChangeEffect=Br,G.perLineTextDirection=Ur,G.exceptionSink=Lr,G.updateListener=Rr,G.editable=Xr,G.mouseSelectionStyle=Ir,G.dragMovesSelection=Fr,G.clickAddsSelectionRange=Pr,G.decorations=ni,G.blockWrappers=ri,G.outerDecorations=ii,G.atomicRanges=ai,G.bidiIsolatedRanges=oi,G.cursorScrollMargin=j.define({combine:e=>{let t=5,n=5;for(let r of e)typeof r==`number`?t=n=r:{x:t,y:n}=r;return{x:t,y:n}}}),G.scrollMargins=li,G.darkTheme=qo,G.cspNonce=j.define({combine:e=>e.length?e[0]:``}),G.contentAttributes=ti,G.editorAttributes=ei,G.lineWrapping=G.contentAttributes.of({class:`cm-lineWrapping`}),G.announce=M.define();var ss=4096,cs={},ls=class e{constructor(e,t,n,r,i,a){this.from=e,this.to=t,this.dir=n,this.isolates=r,this.fresh=i,this.order=a}static update(t,n){if(n.empty&&!t.some(e=>e.fresh))return t;let r=[],i=t.length?t[t.length-1].dir:B.LTR;for(let a=Math.max(0,t.length-10);a<t.length;a++){let o=t[a];o.dir==i&&!n.touchesRange(o.from,o.to)&&r.push(new e(n.mapPos(o.from,1),n.mapPos(o.to,-1),o.dir,o.isolates,!1,o.order))}return r}};function us(e,t,n){for(let r=e.state.facet(t),i=r.length-1;i>=0;i--){let t=r[i],a=typeof t==`function`?t(e):t;a&&Cn(a,n)}return n}var ds=L.mac?`mac`:L.windows?`win`:L.linux?`linux`:`key`;function fs(e,t){let n=e.split(/-(?!$)/),r=n[n.length-1];r==`Space`&&(r=` `);let i,a,o,s;for(let e=0;e<n.length-1;++e){let r=n[e];if(/^(cmd|meta|m)$/i.test(r))s=!0;else if(/^a(lt)?$/i.test(r))i=!0;else if(/^(c|ctrl|control)$/i.test(r))a=!0;else if(/^s(hift)?$/i.test(r))o=!0;else if(/^mod$/i.test(r))t==`mac`?s=!0:a=!0;else throw Error(`Unrecognized modifier name: `+r)}return i&&(r=`Alt-`+r),a&&(r=`Ctrl-`+r),s&&(r=`Meta-`+r),o&&(r=`Shift-`+r),r}function ps(e,t,n){return t.altKey&&(e=`Alt-`+e),t.ctrlKey&&(e=`Ctrl-`+e),t.metaKey&&(e=`Meta-`+e),n!==!1&&t.shiftKey&&(e=`Shift-`+e),e}var ms=Qe.default(G.domEventHandlers({keydown(e,t){return Cs(_s(t.state),e,t,`editor`)}})),hs=j.define({enables:ms}),gs=new WeakMap;function _s(e){let t=e.facet(hs),n=gs.get(t);return n||gs.set(t,n=xs(t.reduce((e,t)=>e.concat(t),[]))),n}function vs(e,t,n){return Cs(_s(e.state),t,e,n)}var ys=null,bs=4e3;function xs(e,t=ds){let n=Object.create(null),r=Object.create(null),i=(e,t)=>{let n=r[e];if(n==null)r[e]=t;else if(n!=t)throw Error(`Key binding `+e+` is used both as a regular binding and as a multi-stroke prefix`)},a=(e,r,a,o,s)=>{let c=n[e]||(n[e]=Object.create(null)),l=r.split(/ (?!$)/).map(e=>fs(e,t));for(let t=1;t<l.length;t++){let n=l.slice(0,t).join(` `);i(n,!0),c[n]||(c[n]={preventDefault:!0,stopPropagation:!1,run:[t=>{let r=ys={view:t,prefix:n,scope:e};return setTimeout(()=>{ys==r&&(ys=null)},bs),!0}]})}let u=l.join(` `);i(u,!1);let d=c[u]||(c[u]={preventDefault:!1,stopPropagation:!1,run:(c._any?.run)?.slice()||[]});a&&d.run.push(a),o&&(d.preventDefault=!0),s&&(d.stopPropagation=!0)};for(let r of e){let e=r.scope?r.scope.split(` `):[`editor`];if(r.any)for(let t of e){let e=n[t]||(n[t]=Object.create(null));e._any||={preventDefault:!1,stopPropagation:!1,run:[]};let{any:i}=r;for(let t in e)e[t].run.push(e=>i(e,Ss))}let i=r[t]||r.key;if(i)for(let t of e)a(t,i,r.run,r.preventDefault,r.stopPropagation),r.shift&&a(t,`Shift-`+i,r.shift,r.preventDefault,r.stopPropagation)}return n}var Ss=null;function Cs(e,t,n,r){Ss=t;let i=un(t),a=Ae(Oe(i,0))==i.length&&i!=` `,o=``,s=!1,c=!1,l=!1;ys&&ys.view==n&&ys.scope==r&&(o=ys.prefix+` `,Na.indexOf(t.keyCode)<0&&(c=!0,ys=null));let u=new Set,d=e=>{if(e){for(let t of e.run)if(!u.has(t)&&(u.add(t),t(n)))return e.stopPropagation&&(l=!0),!0;e.preventDefault&&(e.stopPropagation&&(l=!0),c=!0)}return!1},f=e[r],p,m;return f&&(d(f[o+ps(i,t,!a)])?s=!0:a&&(t.altKey||t.metaKey||t.ctrlKey)&&!(L.windows&&t.ctrlKey&&t.altKey)&&!(L.mac&&t.altKey&&!(t.ctrlKey||t.metaKey))&&(p=rn[t.keyCode])&&p!=i?(d(f[o+ps(p,t,!0)])||t.shiftKey&&(m=an[t.keyCode])!=i&&m!=p&&d(f[o+ps(m,t,!1)]))&&(s=!0):a&&t.shiftKey&&d(f[o+ps(i,t,!0)])&&(s=!0),!s&&d(f._any)&&(s=!0)),c&&(s=!0),s&&l&&t.stopPropagation(),Ss=null,s}var ws=class e{constructor(e,t,n,r,i){this.className=e,this.left=t,this.top=n,this.width=r,this.height=i}draw(){let e=document.createElement(`div`);return e.className=this.className,this.adjust(e),e}update(e,t){return t.className==this.className&&(this.adjust(e),!0)}adjust(e){e.style.left=this.left+`px`,e.style.top=this.top+`px`,this.width!=null&&(e.style.width=this.width+`px`),e.style.height=this.height+`px`}eq(e){return this.left==e.left&&this.top==e.top&&this.width==e.width&&this.height==e.height&&this.className==e.className}static forRange(t,n,r){if(r.empty){let i=t.coordsAtPos(r.head,r.assoc||1);if(!i)return[];let a=Ts(t);return[new e(n,i.left-a.left,i.top-a.top,null,i.bottom-i.top)]}return Ds(t,n,r)}};function Ts(e){let t=e.scrollDOM.getBoundingClientRect();return{left:(e.textDirection==B.LTR?t.left:t.right-e.scrollDOM.clientWidth*e.scaleX)-e.scrollDOM.scrollLeft*e.scaleX,top:t.top-e.scrollDOM.scrollTop*e.scaleY}}function Es(e,t,n,r){let i=e.coordsAtPos(t,n*2);if(!i)return r;let a=e.dom.getBoundingClientRect(),o=(i.top+i.bottom)/2,s=e.posAtCoords({x:a.left+1,y:o}),c=e.posAtCoords({x:a.right-1,y:o});return s==null||c==null?r:{from:Math.max(r.from,Math.min(s,c)),to:Math.min(r.to,Math.max(s,c))}}function Ds(e,t,n){if(n.to<=e.viewport.from||n.from>=e.viewport.to)return[];let r=Math.max(n.from,e.viewport.from),i=Math.min(n.to,e.viewport.to),a=e.textDirection==B.LTR,o=e.contentDOM,s=o.getBoundingClientRect(),c=Ts(e),l=o.querySelector(`.cm-line`),u=l&&window.getComputedStyle(l),d=s.left+(u?parseInt(u.paddingLeft)+Math.min(0,parseInt(u.textIndent)):0),f=s.right-(u?parseInt(u.paddingRight):0),p=na(e,r,1),m=na(e,i,-1),h=p.type==An.Text?p:null,g=m.type==An.Text?m:null;if(h&&(e.lineWrapping||p.widgetLineBreaks)&&(h=Es(e,r,1,h)),g&&(e.lineWrapping||m.widgetLineBreaks)&&(g=Es(e,i,-1,g)),h&&g&&h.from==g.from&&h.to==g.to)return v(y(n.from,n.to,h));{let t=h?y(n.from,null,h):ee(p,!1),r=g?y(null,n.to,g):ee(m,!0),i=[];return(h||p).to<(g||m).from-(h&&g?1:0)||p.widgetLineBreaks>1&&t.bottom+e.defaultLineHeight/2<r.top?i.push(_(d,t.bottom,f,r.top)):t.bottom<r.top&&e.elementAtHeight((t.bottom+r.top)/2).type==An.Text&&(t.bottom=r.top=(t.bottom+r.top)/2),v(t).concat(i).concat(v(r))}function _(e,n,r,i){return new ws(t,e-c.left,n-c.top,Math.max(0,r-e),i-n)}function v({top:e,bottom:t,horizontal:n}){let r=[];for(let i=0;i<n.length;i+=2)r.push(_(n[i],e,n[i+1],t));return r}function y(t,n,r){let i=1e9,o=-1e9,s=[];function c(t,n,c,l,u){let p=e.coordsAtPos(t,t==r.to?-2:2),m=e.coordsAtPos(c,c==r.from?2:-2);p&&m&&(i=Math.min(p.top,m.top,i),o=Math.max(p.bottom,m.bottom,o),u==B.LTR?s.push(a&&n?d:p.left,a&&l?f:m.right):s.push(!a&&l?d:m.left,!a&&n?f:p.right))}let l=t??r.from,u=n??r.to;for(let r of e.visibleRanges)if(r.to>l&&r.from<u)for(let i=Math.max(r.from,l),a=Math.min(r.to,u);;){let r=e.state.doc.lineAt(i);for(let o of e.bidiSpans(r)){let e=o.from+r.from,s=o.to+r.from;if(e>=a)break;s>i&&c(Math.max(e,i),t==null&&e<=l,Math.min(s,a),n==null&&s>=u,o.dir)}if(i=r.to+1,i>=a)break}return s.length==0&&c(l,t==null,u,n==null,e.textDirection),{top:i,bottom:o,horizontal:s}}function ee(e,t){let n=s.top+(t?e.top:e.bottom);return{top:n,bottom:n,horizontal:[]}}}function Os(e,t){return e.constructor==t.constructor&&e.eq(t)}var ks=class{constructor(e,t){this.view=e,this.layer=t,this.drawn=[],this.scaleX=1,this.scaleY=1,this.measureReq={read:this.measure.bind(this),write:this.draw.bind(this)},this.dom=e.scrollDOM.appendChild(document.createElement(`div`)),this.dom.classList.add(`cm-layer`),t.above&&this.dom.classList.add(`cm-layer-above`),t.class&&this.dom.classList.add(t.class),this.scale(),this.dom.setAttribute(`aria-hidden`,`true`),this.setOrder(e.state),e.requestMeasure(this.measureReq),t.mount&&t.mount(this.dom,e)}update(e){e.startState.facet(As)!=e.state.facet(As)&&this.setOrder(e.state),(this.layer.update(e,this.dom)||e.geometryChanged)&&(this.scale(),e.view.requestMeasure(this.measureReq))}docViewUpdate(e){this.layer.updateOnDocViewUpdate!==!1&&e.requestMeasure(this.measureReq)}setOrder(e){let t=0,n=e.facet(As);for(;t<n.length&&n[t]!=this.layer;)t++;this.dom.style.zIndex=String((this.layer.above?150:-1)-t)}measure(){return this.layer.markers(this.view)}scale(){let{scaleX:e,scaleY:t}=this.view;(e!=this.scaleX||t!=this.scaleY)&&(this.scaleX=e,this.scaleY=t,this.dom.style.transform=`scale(${1/e}, ${1/t})`)}draw(e){if(e.length!=this.drawn.length||e.some((e,t)=>!Os(e,this.drawn[t]))){let t=this.dom.firstChild,n=0;for(let r of e)r.update&&t&&r.constructor&&this.drawn[n].constructor&&r.update(t,this.drawn[n])?(t=t.nextSibling,n++):this.dom.insertBefore(r.draw(),t);for(;t;){let e=t.nextSibling;t.remove(),t=e}this.drawn=e,L.webkit&&(this.dom.style.display=this.dom.firstChild?``:`none`)}}destroy(){this.layer.destroy&&this.layer.destroy(this.dom,this.view),this.dom.remove()}},As=j.define();function js(e){return[H.define(t=>new ks(t,e)),As.of(e)]}var Ms=!L.ios,Ns=j.define({combine(e){return kt(e,{cursorBlinkRate:1200,drawRangeCursor:!0,iosSelectionHandles:!0},{cursorBlinkRate:(e,t)=>Math.min(e,t),drawRangeCursor:(e,t)=>e||t})}});function Ps(e={}){return[Ns.of(e),Is,Rs,Bs,Wr.of(!0)]}function Fs(e){return e.startState.facet(Ns)!=e.state.facet(Ns)}var Is=js({above:!0,markers(e){let{state:t}=e,n=t.facet(Ns),r=[];for(let i of t.selection.ranges){let a=i==t.selection.main;if(i.empty||n.drawRangeCursor&&!(a&&L.ios&&n.iosSelectionHandles)){let t=a?`cm-cursor cm-cursor-primary`:`cm-cursor cm-cursor-secondary`,n=i.empty?i:A.cursor(i.head,i.assoc);for(let i of ws.forRange(e,t,n))r.push(i)}}return r},update(e,t){e.transactions.some(e=>e.selection)&&(t.style.animationName=t.style.animationName==`cm-blink`?`cm-blink2`:`cm-blink`);let n=Fs(e);return n&&Ls(e.state,t),e.docChanged||e.selectionSet||n},mount(e,t){Ls(t.state,e)},class:`cm-cursorLayer`});function Ls(e,t){t.style.animationDuration=e.facet(Ns).cursorBlinkRate+`ms`}var Rs=js({above:!1,markers(e){let t=[],{main:n,ranges:r}=e.state.selection;for(let i of r)if(!i.empty&&(Ms||i!=n))for(let n of ws.forRange(e,`cm-selectionBackground`,i))t.push(n);if(L.ios&&!n.empty&&e.state.facet(Ns).iosSelectionHandles){for(let r of ws.forRange(e,`cm-selectionHandle cm-selectionHandle-start`,A.cursor(n.from,1)))t.push(r);for(let r of ws.forRange(e,`cm-selectionHandle cm-selectionHandle-end`,A.cursor(n.to,1)))t.push(r)}return t},update(e,t){return e.docChanged||e.selectionSet||e.viewportChanged||Fs(e)},class:`cm-selectionLayer`}),zs=L.gecko&&L.gecko_version==153?`#ffffff01`:`transparent`,Bs=Qe.highest(G.theme({".cm-line":{...Ms?{"& ::selection, &::selection":{backgroundColor:`${zs} !important`}}:{},caretColor:`transparent !important`},".cm-content":{caretColor:`transparent !important`,"& :focus":{caretColor:`initial !important`,...Ms?{"&::selection, & ::selection":{backgroundColor:`Highlight !important`}}:{}}}})),Vs=M.define({map(e,t){return e==null?null:t.mapPos(e)}}),Hs=Ye.define({create(){return null},update(e,t){return e!=null&&(e=t.changes.mapPos(e)),t.effects.reduce((e,t)=>t.is(Vs)?t.value:e,e)}}),Us=H.fromClass(class{constructor(e){this.view=e,this.cursor=null,this.measureReq={read:this.readPos.bind(this),write:this.drawCursor.bind(this)}}update(e){var t;let n=e.state.field(Hs);n==null?this.cursor!=null&&((t=this.cursor)==null||t.remove(),this.cursor=null):(this.cursor||(this.cursor=this.view.scrollDOM.appendChild(document.createElement(`div`)),this.cursor.className=`cm-dropCursor`),(e.startState.field(Hs)!=n||e.docChanged||e.geometryChanged)&&this.view.requestMeasure(this.measureReq))}readPos(){let{view:e}=this,t=e.state.field(Hs),n=t!=null&&e.coordsAtPos(t);if(!n)return null;let r=e.scrollDOM.getBoundingClientRect();return{left:n.left-r.left+e.scrollDOM.scrollLeft*e.scaleX,top:n.top-r.top+e.scrollDOM.scrollTop*e.scaleY,height:n.bottom-n.top}}drawCursor(e){if(this.cursor){let{scaleX:t,scaleY:n}=this.view;e?(this.cursor.style.left=e.left/t+`px`,this.cursor.style.top=e.top/n+`px`,this.cursor.style.height=e.height/n+`px`):this.cursor.style.left=`-100000px`}}destroy(){this.cursor&&this.cursor.remove()}setDropPos(e){this.view.state.field(Hs)!=e&&this.view.dispatch({effects:Vs.of(e)})}},{eventObservers:{dragover(e){this.setDropPos(this.view.posAtCoords({x:e.clientX,y:e.clientY}))},dragleave(e){e.target!=this.view.contentDOM&&this.view.contentDOM.contains(e.relatedTarget)||this.setDropPos(null)},dragend(){this.setDropPos(null)},drop(){this.setDropPos(null)}}});function Ws(){return[Hs,Us]}function Gs(e,t,n,r,i){t.lastIndex=0;for(let a=e.iterRange(n,r),o=n,s;!a.next().done;o+=a.value.length)if(!a.lineBreak)for(;s=t.exec(a.value);)i(o+s.index,s)}function Ks(e,t){let n=e.visibleRanges;if(n.length==1&&n[0].from==e.viewport.from&&n[0].to==e.viewport.to)return n;let r=[];for(let{from:i,to:a}of n)i=Math.max(e.state.doc.lineAt(i).from,i-t),a=Math.min(e.state.doc.lineAt(a).to,a+t),r.length&&r[r.length-1].to>=i?r[r.length-1].to=a:r.push({from:i,to:a});return r}var qs=class{constructor(e){let{regexp:t,decoration:n,decorate:r,boundary:i,maxLength:a=1e3}=e;if(!t.global)throw RangeError(`The regular expression given to MatchDecorator should have its 'g' flag set`);if(this.regexp=t,r)this.addMatch=(e,t,n,i)=>r(i,n,n+e[0].length,e,t);else if(typeof n==`function`)this.addMatch=(e,t,r,i)=>{let a=n(e,t,r);a&&i(r,r+e[0].length,a)};else if(n)this.addMatch=(e,t,r,i)=>i(r,r+e[0].length,n);else throw RangeError(`Either 'decorate' or 'decoration' should be provided to MatchDecorator`);this.boundary=i,this.maxLength=a}createDeco(e){let t=new Lt,n=t.add.bind(t);for(let{from:t,to:r}of Ks(e,this.maxLength))Gs(e.state.doc,this.regexp,t,r,(t,r)=>this.addMatch(r,e,t,n));return t.finish()}updateDeco(e,t){let n=1e9,r=-1;return e.docChanged&&e.changes.iterChanges((t,i,a,o)=>{o>=e.view.viewport.from&&a<=e.view.viewport.to&&(n=Math.min(a,n),r=Math.max(o,r))}),e.viewportMoved||r-n>1e3?this.createDeco(e.view):r>-1?this.updateRange(e.view,t.map(e.changes),n,r):t}updateRange(e,t,n,r){for(let i of e.visibleRanges){let a=Math.max(i.from,n),o=Math.min(i.to,r);if(o>=a){let n=e.state.doc.lineAt(a),r=n.to<o?e.state.doc.lineAt(o):n,s=Math.max(i.from,n.from),c=Math.min(i.to,r.to);if(this.boundary){for(;a>n.from;a--)if(this.boundary.test(n.text[a-1-n.from])){s=a;break}for(;o<r.to;o++)if(this.boundary.test(r.text[o-r.from])){c=o;break}}let l=[],u,d=(e,t,n)=>l.push(n.range(e,t));if(n==r)for(this.regexp.lastIndex=s-n.from;(u=this.regexp.exec(n.text))&&u.index<c-n.from;)this.addMatch(u,e,u.index+n.from,d);else Gs(e.state.doc,this.regexp,s,c,(t,n)=>this.addMatch(n,e,t,d));t=t.update({filterFrom:s,filterTo:c,filter:(e,t)=>e<s||t>c,add:l})}}return t}},Js=/x/.unicode==null?`g`:`gu`,Ys=RegExp(`[\0-\b
--­؜​‎‏\u2028\u2029‭‮⁦⁧⁩﻿￹-￼]`,Js),Xs={0:`null`,7:`bell`,8:`backspace`,10:`newline`,11:`vertical tab`,13:`carriage return`,27:`escape`,8203:`zero width space`,8204:`zero width non-joiner`,8205:`zero width joiner`,8206:`left-to-right mark`,8207:`right-to-left mark`,8232:`line separator`,8237:`left-to-right override`,8238:`right-to-left override`,8294:`left-to-right isolate`,8295:`right-to-left isolate`,8297:`pop directional isolate`,8233:`paragraph separator`,65279:`zero width no-break space`,65532:`object replacement`},Zs=null;function Qs(){if(Zs==null&&typeof document<`u`&&document.body){let e=document.body.style;Zs=(e.tabSize??e.MozTabSize)!=null}return Zs||!1}var $s=j.define({combine(e){let t=kt(e,{render:null,specialChars:Ys,addSpecialChars:null});return(t.replaceTabs=!Qs())&&(t.specialChars=RegExp(`	|`+t.specialChars.source,Js)),t.addSpecialChars&&(t.specialChars=RegExp(t.specialChars.source+`|`+t.addSpecialChars.source,Js)),t}});function ec(e={}){return[$s.of(e),nc()]}var tc=null;function nc(){return tc||=H.fromClass(class{constructor(e){this.view=e,this.decorations=R.none,this.decorationCache=Object.create(null),this.decorator=this.makeDecorator(e.state.facet($s)),this.decorations=this.decorator.createDeco(e)}makeDecorator(e){return new qs({regexp:e.specialChars,decoration:(t,n,r)=>{let{doc:i}=n.state,a=Oe(t[0],0);if(a==9){let e=i.lineAt(r),t=n.state.tabSize,a=Jt(e.text,t,r-e.from);return R.replace({widget:new oc((t-a%t)*this.view.defaultCharacterWidth/this.view.scaleX)})}return this.decorationCache[a]||(this.decorationCache[a]=R.replace({widget:new ac(e,a)}))},boundary:e.replaceTabs?void 0:/[^]/})}update(e){let t=e.state.facet($s);e.startState.facet($s)==t?this.decorations=this.decorator.updateDeco(e,this.decorations):(this.decorator=this.makeDecorator(t),this.decorations=this.decorator.createDeco(e.view))}},{decorations:e=>e.decorations})}var rc=`•`;function ic(e){return e>=32?rc:e==10?`␤`:String.fromCharCode(9216+e)}var ac=class extends kn{constructor(e,t){super(),this.options=e,this.code=t}eq(e){return e.code==this.code}toDOM(e){let t=ic(this.code),n=e.state.phrase(`Control character`)+` `+(Xs[this.code]||`0x`+this.code.toString(16)),r=this.options.render&&this.options.render(this.code,n,t);if(r)return r;let i=document.createElement(`span`);return i.textContent=t,i.title=n,i.setAttribute(`aria-label`,n),i.className=`cm-specialChar`,i}ignoreEvent(){return!1}},oc=class extends kn{constructor(e){super(),this.width=e}eq(e){return e.width==this.width}toDOM(){let e=document.createElement(`span`);return e.textContent=`	`,e.className=`cm-tab`,e.style.width=this.width+`px`,e}ignoreEvent(){return!1}};function sc(){return lc}var cc=R.line({class:`cm-activeLine`}),lc=H.fromClass(class{constructor(e){this.decorations=this.getDeco(e)}update(e){(e.docChanged||e.selectionSet)&&(this.decorations=this.getDeco(e.view))}getDeco(e){let t=-1,n=[];for(let r of e.state.selection.ranges){let i=e.lineBlockAt(r.head);i.from>t&&(n.push(cc.range(i.from)),t=i.from)}return R.set(n)}},{decorations:e=>e.decorations}),uc=2e3;function dc(e,t,n){let r=Math.min(t.line,n.line),i=Math.max(t.line,n.line),a=[];if(t.off>uc||n.off>uc||t.col<0||n.col<0){let o=Math.min(t.off,n.off),s=Math.max(t.off,n.off);for(let t=r;t<=i;t++){let n=e.doc.line(t);n.length<=s&&a.push(A.range(n.from+o,n.to+s))}}else{let o=Math.min(t.col,n.col),s=Math.max(t.col,n.col);for(let t=r;t<=i;t++){let n=e.doc.line(t),r=Yt(n.text,o,e.tabSize,!0);if(r<0)a.push(A.cursor(n.to));else{let t=Yt(n.text,s,e.tabSize);a.push(A.range(n.from+r,n.from+t))}}}return a}function fc(e,t){let n=e.coordsAtPos(e.viewport.from);return n?Math.round(Math.abs((n.left-t)/e.defaultCharacterWidth)):-1}function pc(e,t){let n=e.posAtCoords({x:t.clientX,y:t.clientY},!1),r=e.state.doc.lineAt(n),i=n-r.from,a=i>uc?-1:i==r.length?fc(e,t.clientX):Jt(r.text,e.state.tabSize,n-r.from);return{line:r.number,col:a,off:i}}function mc(e,t){let n=pc(e,t),r=e.state.selection;return n?{update(e){if(e.docChanged){let t=e.changes.mapPos(e.startState.doc.line(n.line).from),i=e.state.doc.lineAt(t);n={line:i.number,col:n.col,off:Math.min(n.off,i.length)},r=r.map(e.changes)}},get(t,i,a){let o=pc(e,t);if(!o)return r;let s=dc(e.state,n,o);return s.length?a?A.create(s.concat(r.ranges)):A.create(s):r}}:null}function hc(e){let t=e?.eventFilter||(e=>e.altKey&&e.button==0);return G.mouseSelectionStyle.of((e,n)=>t(n)?mc(e,n):null)}var gc={Alt:[18,e=>!!e.altKey],Control:[17,e=>!!e.ctrlKey],Shift:[16,e=>!!e.shiftKey],Meta:[91,e=>!!e.metaKey]},_c={style:`cursor: crosshair`};function vc(e={}){let[t,n]=gc[e.key||`Alt`],r=H.fromClass(class{constructor(e){this.view=e,this.isDown=!1}set(e){this.isDown!=e&&(this.isDown=e,this.view.update([]))}},{eventObservers:{keydown(e){this.set(e.keyCode==t||n(e))},keyup(e){e.keyCode!=t&&n(e)||this.set(!1)},mousemove(e){this.set(n(e))}}});return[r,G.contentAttributes.of(e=>e.plugin(r)?.isDown?_c:null)]}var yc=`-10000px`,bc=class{constructor(e,t,n,r){this.facet=t,this.createTooltipView=n,this.removeTooltipView=r,this.input=e.state.facet(t),this.tooltips=this.input.filter(e=>e);let i=null;this.tooltipViews=this.tooltips.map(e=>i=n(e,i))}update(e,t){var n;let r=e.state.facet(this.facet),i=r.filter(e=>e);if(r===this.input){for(let t of this.tooltipViews)t.update&&t.update(e);return!1}let a=[],o=t?[]:null;for(let n=0;n<i.length;n++){let r=i[n],s=-1;if(r){for(let e=0;e<this.tooltips.length;e++){let t=this.tooltips[e];t&&t.create==r.create&&(s=e)}if(s<0)a[n]=this.createTooltipView(r,n?a[n-1]:null),o&&(o[n]=!!r.above);else{let r=a[n]=this.tooltipViews[s];o&&(o[n]=t[s]),r.update&&r.update(e)}}}for(let e of this.tooltipViews)a.indexOf(e)<0&&(this.removeTooltipView(e),(n=e.destroy)==null||n.call(e));return t&&(o.forEach((e,n)=>t[n]=e),t.length=o.length),this.input=r,this.tooltips=i,this.tooltipViews=a,!0}};function xc(e){let t=e.dom.ownerDocument.documentElement,n=t.clientWidth,r=getComputedStyle(t);if(!/stable/.test(r.scrollbarGutter||``))return{left:0,right:n};let i=t.getBoundingClientRect(),a=e.win.scrollX;return{left:Math.max(0,i.left+a-parseFloat(r.marginLeft)),right:Math.min(n,i.right+a+parseFloat(r.marginRight))}}function Sc(e){let{left:t,right:n}=xc(e);return{top:0,left:t,right:n,bottom:e.dom.ownerDocument.documentElement.clientHeight}}var Cc=j.define({combine:e=>({position:L.ios?`absolute`:e.find(e=>e.position)?.position||`fixed`,parent:e.find(e=>e.parent)?.parent||null,tooltipSpace:e.find(e=>e.tooltipSpace)?.tooltipSpace||Sc})}),wc=new WeakMap,Tc=H.fromClass(class{constructor(e){this.view=e,this.above=[],this.inView=!0,this.madeAbsolute=!1,this.lastTransaction=0,this.measureTimeout=-1;let t=e.state.facet(Cc);this.position=t.position,this.parent=t.parent,this.classes=e.themeClasses,this.createContainer(),this.measureReq={read:this.readMeasure.bind(this),write:this.writeMeasure.bind(this),key:this},this.resizeObserver=typeof ResizeObserver==`function`?new ResizeObserver(()=>this.measureSoon()):null,this.manager=new bc(e,kc,(e,t)=>this.createTooltip(e,t),e=>{this.resizeObserver&&this.resizeObserver.unobserve(e.dom),e.dom.remove()}),this.above=this.manager.tooltips.map(e=>!!e.above),this.intersectionObserver=typeof IntersectionObserver==`function`?new IntersectionObserver(e=>{Date.now()>this.lastTransaction-50&&e.length>0&&e[e.length-1].intersectionRatio<1&&this.measureSoon()},{threshold:[1]}):null,this.observeIntersection(),e.win.addEventListener(`resize`,this.measureSoon=this.measureSoon.bind(this)),this.maybeMeasure()}createContainer(){this.parent?(this.container=document.createElement(`div`),this.container.style.position=`relative`,this.container.className=this.view.themeClasses,this.parent.appendChild(this.container)):this.container=this.view.dom}observeIntersection(){if(this.intersectionObserver){this.intersectionObserver.disconnect();for(let e of this.manager.tooltipViews)this.intersectionObserver.observe(e.dom)}}measureSoon(){this.measureTimeout<0&&(this.measureTimeout=setTimeout(()=>{this.measureTimeout=-1,this.maybeMeasure()},50))}update(e){e.transactions.length&&(this.lastTransaction=Date.now());let t=this.manager.update(e,this.above);t&&this.observeIntersection();let n=t||e.geometryChanged,r=e.state.facet(Cc);if(r.position!=this.position&&!this.madeAbsolute){this.position=r.position;for(let e of this.manager.tooltipViews)e.dom.style.position=this.position;n=!0}if(r.parent!=this.parent){this.parent&&this.container.remove(),this.parent=r.parent,this.createContainer();for(let e of this.manager.tooltipViews)this.container.appendChild(e.dom);n=!0}else this.parent&&this.view.themeClasses!=this.classes&&(this.classes=this.container.className=this.view.themeClasses);n&&this.maybeMeasure()}createTooltip(e,t){let n=e.create(this.view),r=t?t.dom:null;if(n.dom.classList.add(`cm-tooltip`),e.arrow&&!n.dom.querySelector(`.cm-tooltip > .cm-tooltip-arrow`)){let e=document.createElement(`div`);e.className=`cm-tooltip-arrow`,n.dom.appendChild(e)}return n.dom.style.position=this.position,n.dom.style.top=yc,n.dom.style.left=`0px`,this.container.insertBefore(n.dom,r),n.mount&&n.mount(this.view),this.resizeObserver&&this.resizeObserver.observe(n.dom),n}destroy(){var e,t,n;this.view.win.removeEventListener(`resize`,this.measureSoon);for(let t of this.manager.tooltipViews)t.dom.remove(),(e=t.destroy)==null||e.call(t);this.parent&&this.container.remove(),(t=this.resizeObserver)==null||t.disconnect(),(n=this.intersectionObserver)==null||n.disconnect(),clearTimeout(this.measureTimeout)}readMeasure(){let e=1,t=1,n=!1,r=0;if(this.position==`fixed`&&this.manager.tooltipViews.length){let{dom:e}=this.manager.tooltipViews[0];if(L.safari){let t=e.getBoundingClientRect();n=Math.abs(t.top+1e4)>1||Math.abs(t.left)>1}else n=!!e.offsetParent&&e.offsetParent!=this.container.ownerDocument.body}if(n||this.position==`absolute`){if(this.parent){let n=this.parent.getBoundingClientRect();n.width&&n.height&&(e=n.width/this.parent.offsetWidth,t=n.height/this.parent.offsetHeight)}else({scaleX:e,scaleY:t}=this.view.viewState)}else r=xc(this.view).left;let i=this.view.scrollDOM.getBoundingClientRect(),a=ui(this.view);return{visible:{left:i.left+a.left,top:i.top+a.top,right:i.right-a.right,bottom:i.bottom-a.bottom},parent:this.parent?this.container.getBoundingClientRect():this.view.dom.getBoundingClientRect(),pos:this.manager.tooltips.map((e,t)=>{let n=this.manager.tooltipViews[t];return n.getCoords?n.getCoords(e.pos):this.view.coordsAtPos(e.pos)}),size:this.manager.tooltipViews.map(({dom:e})=>e.getBoundingClientRect()),space:this.view.state.facet(Cc).tooltipSpace(this.view),scaleX:e,scaleY:t,makeAbsolute:n,fixedLeft:r}}writeMeasure(e){if(e.makeAbsolute){this.madeAbsolute=!0,this.position=`absolute`;for(let e of this.manager.tooltipViews)e.dom.style.position=`absolute`}let{visible:t,space:n,scaleX:r,scaleY:i}=e,a=[];for(let o=0;o<this.manager.tooltips.length;o++){let s=this.manager.tooltips[o],c=this.manager.tooltipViews[o],{dom:l}=c,u=e.pos[o],d=e.size[o];if(!u||s.clip!==!1&&(u.bottom<=Math.max(t.top,n.top)||u.top>=Math.min(t.bottom,n.bottom)||u.right<Math.max(t.left,n.left)-.1||u.left>Math.min(t.right,n.right)+.1)){l.style.top=yc;continue}let f=s.arrow?c.dom.querySelector(`.cm-tooltip-arrow`):null,p=f?7:0,m=d.right-d.left,h=wc.get(c)??d.bottom-d.top,g=c.offset||Oc,_=this.view.textDirection==B.LTR,v=d.width>n.right-n.left?_?n.left:n.right-d.width:_?Math.max(n.left,Math.min(u.left-(f?14:0)+g.x,n.right-m)):Math.min(Math.max(n.left,u.left-m+(f?14:0)-g.x),n.right-m),y=this.above[o];!s.strictSide&&(y?u.top-h-p-g.y<n.top:u.bottom+h+p+g.y>n.bottom)&&y==n.bottom-u.bottom>u.top-n.top&&(y=this.above[o]=!y);let ee=(y?u.top-n.top:n.bottom-u.bottom)-p;if(ee<h&&c.resize!==!1){if(ee<this.view.defaultLineHeight){l.style.top=yc;continue}wc.set(c,h),l.style.height=(h=ee)/i+`px`}else l.style.height&&(l.style.height=``);let b=y?u.top-h-p-g.y:u.bottom+p+g.y,x=v+m;if(c.overlap!==!0)for(let e of a)e.left<x&&e.right>v&&e.top<b+h&&e.bottom>b&&(b=y?e.top-h-2-p:e.bottom+p+2);if(this.position==`absolute`?(l.style.top=(b-e.parent.top)/i+`px`,Ec(l,(v-e.parent.left)/r)):(l.style.top=b/i+`px`,Ec(l,(v-e.fixedLeft)/r)),f){let e=u.left+(_?g.x:-g.x)-(v+14-7);f.style.left=e/r+`px`}c.overlap!==!0&&a.push({left:v,top:b,right:x,bottom:b+h}),l.classList.toggle(`cm-tooltip-above`,y),l.classList.toggle(`cm-tooltip-below`,!y),c.positioned&&c.positioned(e.space)}}maybeMeasure(){if(this.manager.tooltips.length&&(this.view.inView&&this.view.requestMeasure(this.measureReq),this.inView!=this.view.inView&&(this.inView=this.view.inView,!this.inView)))for(let e of this.manager.tooltipViews)e.dom.style.top=yc}},{eventObservers:{scroll(){this.maybeMeasure()}}});function Ec(e,t){let n=parseInt(e.style.left,10);(isNaN(n)||Math.abs(t-n)>1)&&(e.style.left=t+`px`)}var Dc=G.baseTheme({".cm-tooltip":{zIndex:500,boxSizing:`border-box`},"&light .cm-tooltip":{border:`1px solid #bbb`,backgroundColor:`#f5f5f5`},"&light .cm-tooltip-section:not(:first-child)":{borderTop:`1px solid #bbb`},"&dark .cm-tooltip":{backgroundColor:`#333338`,color:`white`},".cm-tooltip-arrow":{height:`7px`,width:`14px`,position:`absolute`,zIndex:-1,overflow:`hidden`,"&:before, &:after":{content:`''`,position:`absolute`,width:0,height:0,borderLeft:`7px solid transparent`,borderRight:`7px solid transparent`},".cm-tooltip-above &":{bottom:`-7px`,"&:before":{borderTop:`7px solid #bbb`},"&:after":{borderTop:`7px solid #f5f5f5`,bottom:`1px`}},".cm-tooltip-below &":{top:`-7px`,"&:before":{borderBottom:`7px solid #bbb`},"&:after":{borderBottom:`7px solid #f5f5f5`,top:`1px`}}},"&dark .cm-tooltip .cm-tooltip-arrow":{"&:before":{borderTopColor:`#333338`,borderBottomColor:`#333338`},"&:after":{borderTopColor:`transparent`,borderBottomColor:`transparent`}}}),Oc={x:0,y:0},kc=j.define({enables:[Tc,Dc]}),Ac=j.define({combine:e=>e.reduce((e,t)=>e.concat(t),[])}),jc=class e{static create(t){return new e(t)}constructor(e){this.view=e,this.mounted=!1,this.dom=document.createElement(`div`),this.dom.classList.add(`cm-tooltip-hover`),this.manager=new bc(e,Ac,(e,t)=>this.createHostedView(e,t),e=>e.dom.remove())}createHostedView(e,t){let n=e.create(this.view);return n.dom.classList.add(`cm-tooltip-section`),this.dom.insertBefore(n.dom,t?t.dom.nextSibling:this.dom.firstChild),this.mounted&&n.mount&&n.mount(this.view),n}mount(e){for(let t of this.manager.tooltipViews)t.mount&&t.mount(e);this.mounted=!0}positioned(e){for(let t of this.manager.tooltipViews)t.positioned&&t.positioned(e)}update(e){this.manager.update(e)}destroy(){var e;for(let t of this.manager.tooltipViews)(e=t.destroy)==null||e.call(t)}passProp(e){let t;for(let n of this.manager.tooltipViews){let r=n[e];if(r!==void 0){if(t===void 0)t=r;else if(t!==r)return}}return t}get offset(){return this.passProp(`offset`)}get getCoords(){return this.passProp(`getCoords`)}get overlap(){return this.passProp(`overlap`)}get resize(){return this.passProp(`resize`)}},Mc=kc.compute([Ac],e=>{let t=e.facet(Ac);return t.length===0?null:{pos:Math.min(...t.map(e=>e.pos)),end:Math.max(...t.map(e=>e.end??e.pos)),create:jc.create,above:t[0].above,arrow:t.some(e=>e.arrow)}}),Nc=j.define(),Pc=class{constructor(e,t,n,r,i,a){this.view=e,this.source=t,this.field=n,this.locked=r,this.setHover=i,this.hoverTime=a,this.hoverTimeout=-1,this.restartTimeout=-1,this.pending=null,this.lastMove={x:0,y:0,target:e.dom,time:0},this.checkHover=this.checkHover.bind(this),e.dom.addEventListener(`mouseleave`,this.mouseleave=this.mouseleave.bind(this)),e.dom.addEventListener(`mousemove`,this.mousemove=this.mousemove.bind(this))}update(e){this.pending&&(this.pending=null,clearTimeout(this.restartTimeout),this.restartTimeout=setTimeout(()=>this.startHover(),20))}get active(){return this.view.state.field(this.field)}checkHover(){if(this.hoverTimeout=-1,this.active.length)return;let e=Date.now()-this.lastMove.time;e<this.hoverTime?this.hoverTimeout=setTimeout(this.checkHover,this.hoverTime-e):this.startHover()}startHover(){clearTimeout(this.restartTimeout);let{view:e,lastMove:t}=this,n=e.docView.tile.nearest(t.target);if(!n)return;let r,i=1;if(n.isWidget())r=n.posAtStart;else{if(r=e.posAtCoords(t),r==null)return;let n=e.coordsAtPos(r);if(!n||t.y<n.top||t.y>n.bottom||t.x<n.left-e.defaultCharacterWidth||t.x>n.right+e.defaultCharacterWidth)return;let a=e.bidiSpans(e.state.doc.lineAt(r)).find(e=>e.from<=r&&e.to>=r),o=a&&a.dir==B.RTL?-1:1;i=t.x<n.left?-o:o}this.activateHover(e,r,i)}activateHover(e,t,n,r){let i=this.source(e,t,n),a=t=>{if(t&&(!Array.isArray(t)||t.length)){let n=Array.isArray(t)?t:[t];r&&this.locked.set(n,r),e.dispatch({effects:this.setHover.of(n)})}};if(i&&`then`in i){let n=this.pending={pos:t};i.then(e=>{this.pending==n&&(this.pending=null,a(e))},t=>Yr(e.state,t,`hover tooltip`))}else a(i)}get tooltip(){let e=this.view.plugin(Tc),t=e?e.manager.tooltips.findIndex(e=>e.create==jc.create):-1;return t>-1?e.manager.tooltipViews[t]:null}mousemove(e){this.lastMove={x:e.clientX,y:e.clientY,target:e.target,time:Date.now()},this.hoverTimeout<0&&(this.hoverTimeout=setTimeout(this.checkHover,this.hoverTime));let{active:t,tooltip:n}=this;if(t.length&&!this.locked.has(t)&&n&&!Ic(n.dom,e)||this.pending){let{pos:n}=t[0]||this.pending,r=t[0]?.end??n;(n==r?this.view.posAtCoords(this.lastMove)!=n:!Lc(this.view,n,r,e.clientX,e.clientY))&&(this.view.dispatch({effects:this.setHover.of([])}),this.pending=null)}}mouseleave(e){clearTimeout(this.hoverTimeout),this.hoverTimeout=-1;let{active:t}=this;if(t.length&&!this.locked.has(t)){let{tooltip:t}=this;t&&t.dom.contains(e.relatedTarget)?this.watchTooltipLeave(t.dom):this.view.dispatch({effects:this.setHover.of([])})}}watchTooltipLeave(e){let t=n=>{e.removeEventListener(`mouseleave`,t);let{active:r}=this;r.length&&!this.locked.has(r)&&!this.view.dom.contains(n.relatedTarget)&&this.view.dispatch({effects:this.setHover.of([])})};e.addEventListener(`mouseleave`,t)}destroy(){clearTimeout(this.hoverTimeout),clearTimeout(this.restartTimeout),this.view.dom.removeEventListener(`mouseleave`,this.mouseleave),this.view.dom.removeEventListener(`mousemove`,this.mousemove)}},Fc=4;function Ic(e,t){let{left:n,right:r,top:i,bottom:a}=e.getBoundingClientRect(),o;if(o=e.querySelector(`.cm-tooltip-arrow`)){let e=o.getBoundingClientRect();i=Math.min(e.top,i),a=Math.max(e.bottom,a)}return t.clientX>=n-Fc&&t.clientX<=r+Fc&&t.clientY>=i-Fc&&t.clientY<=a+Fc}function Lc(e,t,n,r,i,a){let o=e.scrollDOM.getBoundingClientRect(),s=e.documentTop+e.documentPadding.top+e.contentHeight;if(o.left>r||o.right<r||o.top>i||Math.min(o.bottom,s)<i)return!1;let c=e.posAtCoords({x:r,y:i},!1);return c>=t&&c<=n}function Rc(e,t={}){let n=M.define(),r=new WeakMap,i=Ye.define({create(){return[]},update(e,a){let o=r.get(e);if(e.length&&(t.hideOnChange&&(a.docChanged||a.selection)||o&&o(a)?e=[]:t.hideOn&&(e=e.filter(e=>!t.hideOn(a,e)))),a.docChanged&&e.length){let t=[];for(let n of e){let e=a.changes.mapPos(n.pos,-1,Me.TrackDel);if(e!=null){let r=Object.assign(Object.create(null),n);r.pos=e,r.end!=null&&(r.end=a.changes.mapPos(r.end)),t.push(r)}}e=t}for(let t of a.effects)t.is(n)&&(e=t.value,o=void 0),(t.is(Vc)&&!t.value||t.value==i)&&(e=[]);return e.length&&o&&r.set(e,o),e},provide:e=>Ac.from(e)}),a=H.define(a=>new Pc(a,e,i,r,n,t.hoverTime||300));return{active:i,extension:[i,a,Nc.of(a),Mc]}}function zc(e,t,n,r={}){let i=e.state.facet(Nc).map(t=>e.plugin(t)).filter(e=>!!e);if(r.tooltip&&r.tooltip.active){let e=i.find(e=>e.field==r.tooltip.active);e&&(i=[e])}for(let a of i)a.activateHover(e,t,n,r.until??(()=>!1))}function Bc(e,t){let n=e.plugin(Tc);if(!n)return null;let r=n.manager.tooltips.indexOf(t);return r<0?null:n.manager.tooltipViews[r]}var Vc=M.define(),Hc=j.define({combine(e){let t,n;for(let r of e)t||=r.topContainer,n||=r.bottomContainer;return{topContainer:t,bottomContainer:n}}});function Uc(e,t){let n=e.plugin(Wc),r=n?n.specs.indexOf(t):-1;return r>-1?n.panels[r]:null}var Wc=H.fromClass(class{constructor(e){this.input=e.state.facet(qc),this.specs=this.input.filter(e=>e),this.panels=this.specs.map(t=>t(e));let t=e.state.facet(Hc);this.top=new Gc(e,!0,t.topContainer),this.bottom=new Gc(e,!1,t.bottomContainer),this.top.sync(this.panels.filter(e=>e.top)),this.bottom.sync(this.panels.filter(e=>!e.top));for(let e of this.panels)e.dom.classList.add(`cm-panel`),e.mount&&e.mount()}update(e){let t=e.state.facet(Hc);this.top.container!=t.topContainer&&(this.top.sync([]),this.top=new Gc(e.view,!0,t.topContainer)),this.bottom.container!=t.bottomContainer&&(this.bottom.sync([]),this.bottom=new Gc(e.view,!1,t.bottomContainer)),this.top.syncClasses(),this.bottom.syncClasses();let n=e.state.facet(qc);if(n!=this.input){let t=n.filter(e=>e),r=[],i=[],a=[],o=[];for(let n of t){let t=this.specs.indexOf(n),s;t<0?(s=n(e.view),o.push(s)):(s=this.panels[t],s.update&&s.update(e)),r.push(s),(s.top?i:a).push(s)}this.specs=t,this.panels=r,this.top.sync(i),this.bottom.sync(a);for(let e of o)e.dom.classList.add(`cm-panel`),e.mount&&e.mount()}else for(let t of this.panels)t.update&&t.update(e)}destroy(){this.top.sync([]),this.bottom.sync([])}},{provide:e=>G.scrollMargins.of(t=>{let n=t.plugin(e);return n&&{top:n.top.scrollMargin(),bottom:n.bottom.scrollMargin()}})}),Gc=class{constructor(e,t,n){this.view=e,this.top=t,this.container=n,this.dom=void 0,this.classes=``,this.panels=[],this.syncClasses()}sync(e){for(let t of this.panels)t.destroy&&e.indexOf(t)<0&&t.destroy();this.panels=e,this.syncDOM()}syncDOM(){if(this.panels.length==0){this.dom&&=(this.dom.remove(),void 0);return}if(!this.dom){this.dom=document.createElement(`div`),this.dom.className=this.top?`cm-panels cm-panels-top`:`cm-panels cm-panels-bottom`;let e=this.container||this.view.dom;e.insertBefore(this.dom,this.top?e.firstChild:null)}let e=this.dom.firstChild;for(let t of this.panels)if(t.dom.parentNode==this.dom){for(;e!=t.dom;)e=Kc(e);e=e.nextSibling}else this.dom.insertBefore(t.dom,e);for(;e;)e=Kc(e)}scrollMargin(){return!this.dom||this.container?0:Math.max(0,this.top?this.dom.getBoundingClientRect().bottom-Math.max(0,this.view.scrollDOM.getBoundingClientRect().top):Math.min(innerHeight,this.view.scrollDOM.getBoundingClientRect().bottom)-this.dom.getBoundingClientRect().top)}syncClasses(){if(this.container&&this.classes!=this.view.themeClasses){for(let e of this.classes.split(` `))e&&this.container.classList.remove(e);for(let e of(this.classes=this.view.themeClasses).split(` `))e&&this.container.classList.add(e)}}};function Kc(e){let t=e.nextSibling;return e.remove(),t}var qc=j.define({enables:Wc});function Jc(e,t){let n,r=new Promise(e=>n=e),i=e=>$c(e,t,n);e.state.field(Xc,!1)?e.dispatch({effects:Zc.of(i)}):e.dispatch({effects:M.appendConfig.of(Xc.init(()=>[i]))});let a=Qc.of(i);return{close:a,result:r.then(t=>((e.win.queueMicrotask||(t=>e.win.setTimeout(t,10)))(()=>{e.state.field(Xc).indexOf(i)>-1&&e.dispatch({effects:a})}),t))}}function Yc(e,t){let n=e.state.field(Xc,!1)||[];for(let r of n){let n=Uc(e,r);if(n&&n.dom.classList.contains(t))return n}return null}var Xc=Ye.define({create(){return[]},update(e,t){for(let n of t.effects)n.is(Zc)?e=[n.value].concat(e):n.is(Qc)&&(e=e.filter(e=>e!=n.value));return e},provide:e=>qc.computeN([e],t=>t.field(e))}),Zc=M.define(),Qc=M.define();function $c(e,t,n){let r=t.content?t.content(e,()=>s(null)):null;if(!r){if(r=I(`form`),t.input){let e=I(`input`,t.input);/^(text|password|number|email|tel|url)$/.test(e.type)&&e.classList.add(`cm-textfield`),e.name||=`input`,r.appendChild(I(`label`,(t.label||``)+`: `,e))}else r.appendChild(document.createTextNode(t.label||``));r.appendChild(document.createTextNode(` `)),r.appendChild(I(`button`,{class:`cm-button`,type:`submit`},t.submitLabel||`OK`))}let i=r.nodeName==`FORM`?[r]:r.querySelectorAll(`form`);for(let e=0;e<i.length;e++){let t=i[e];t.addEventListener(`keydown`,e=>{e.keyCode==27?(e.preventDefault(),s(null)):e.keyCode==13&&(e.preventDefault(),s(t))}),t.addEventListener(`submit`,e=>{e.preventDefault(),s(t)})}let a=I(`div`,r,I(`button`,{onclick:()=>s(null),"aria-label":e.state.phrase(`close`),class:`cm-dialog-close`,type:`button`},[`×`]));t.class&&(a.className=t.class),a.classList.add(`cm-dialog`);let o=typeof t.focus==`string`?r.querySelector(t.focus):t.focus?r.querySelector(`input`)||r.querySelector(`button`):null;function s(t){a.contains(a.ownerDocument.activeElement)&&e.focus(),n(t)}return{dom:a,top:t.top,mount:()=>{o&&(`select`in o?o.select():`focus`in o&&o.focus())},destroy:()=>n(null)}}var el=class extends At{compare(e){return this==e||this.constructor==e.constructor&&this.eq(e)}eq(e){return!1}destroy(e){}};el.prototype.elementClass=``,el.prototype.toDOM=void 0,el.prototype.mapMode=Me.TrackBefore,el.prototype.startSide=el.prototype.endSide=-1,el.prototype.point=!0;var tl=j.define(),nl=j.define(),rl={class:``,renderEmptyElements:!1,elementStyle:``,markers:()=>F.empty,lineMarker:()=>null,widgetMarker:()=>null,lineMarkerChange:null,initialSpacer:null,updateSpacer:null,domEventHandlers:{},side:`before`},il=j.define();function al(e){return[sl(),il.of({...rl,...e})]}var ol=j.define({combine:e=>e.some(e=>e)});function sl(e){let t=[cl];return e&&e.fixed===!1&&t.push(ol.of(!0)),t}var cl=H.fromClass(class{constructor(e){this.view=e,this.domAfter=null,this.prevViewport=e.viewport,this.dom=document.createElement(`div`),this.dom.className=`cm-gutters cm-gutters-before`,this.dom.setAttribute(`aria-hidden`,`true`),this.dom.style.minHeight=this.view.contentHeight/this.view.scaleY+`px`,this.gutters=e.state.facet(il).map(t=>new fl(e,t)),this.fixed=!e.state.facet(ol);for(let e of this.gutters)e.config.side==`after`?this.getDOMAfter().appendChild(e.dom):this.dom.appendChild(e.dom);this.fixed&&(this.dom.style.position=`sticky`),this.syncGutters(!1),e.scrollDOM.insertBefore(this.dom,e.contentDOM)}getDOMAfter(){return this.domAfter||(this.domAfter=document.createElement(`div`),this.domAfter.className=`cm-gutters cm-gutters-after`,this.domAfter.setAttribute(`aria-hidden`,`true`),this.domAfter.style.minHeight=this.view.contentHeight/this.view.scaleY+`px`,this.domAfter.style.position=this.fixed?`sticky`:``,this.view.scrollDOM.appendChild(this.domAfter)),this.domAfter}update(e){if(this.updateGutters(e)){let t=this.prevViewport,n=e.view.viewport,r=Math.min(t.to,n.to)-Math.max(t.from,n.from);this.syncGutters(r<(n.to-n.from)*.8)}if(e.geometryChanged){let e=this.view.contentHeight/this.view.scaleY+`px`;this.dom.style.minHeight=e,this.domAfter&&(this.domAfter.style.minHeight=e)}this.view.state.facet(ol)!=!this.fixed&&(this.fixed=!this.fixed,this.dom.style.position=this.fixed?`sticky`:``,this.domAfter&&(this.domAfter.style.position=this.fixed?`sticky`:``)),this.prevViewport=e.view.viewport}syncGutters(e){let t=this.dom.nextSibling;e&&(this.dom.remove(),this.domAfter&&this.domAfter.remove());let n=F.iter(this.view.state.facet(tl),this.view.viewport.from),r=[],i=this.gutters.map(e=>new dl(e,this.view.viewport,-this.view.documentPadding.top));for(let e of this.view.viewportLineBlocks)if(r.length&&(r=[]),Array.isArray(e.type)){let t=!0;for(let a of e.type)if(a.type==An.Text&&t){ul(n,r,a.from);for(let e of i)e.line(this.view,a,r);t=!1}else if(a.widget)for(let e of i)e.widget(this.view,a)}else if(e.type==An.Text){ul(n,r,e.from);for(let t of i)t.line(this.view,e,r)}else if(e.widget)for(let t of i)t.widget(this.view,e);for(let e of i)e.finish();e&&(this.view.scrollDOM.insertBefore(this.dom,t),this.domAfter&&this.view.scrollDOM.appendChild(this.domAfter))}updateGutters(e){let t=e.startState.facet(il),n=e.state.facet(il),r=e.docChanged||e.heightChanged||e.viewportChanged||!F.eq(e.startState.facet(tl),e.state.facet(tl),e.view.viewport.from,e.view.viewport.to);if(t==n)for(let t of this.gutters)t.update(e)&&(r=!0);else{r=!0;let i=[];for(let r of n){let n=t.indexOf(r);n<0?i.push(new fl(this.view,r)):(this.gutters[n].update(e),i.push(this.gutters[n]))}for(let e of this.gutters)e.dom.remove(),i.indexOf(e)<0&&e.destroy();for(let e of i)e.config.side==`after`?this.getDOMAfter().appendChild(e.dom):this.dom.appendChild(e.dom);this.gutters=i}return r}destroy(){for(let e of this.gutters)e.destroy();this.dom.remove(),this.domAfter&&this.domAfter.remove()}},{provide:e=>G.scrollMargins.of(t=>{let n=t.plugin(e);if(!n||n.gutters.length==0||!n.fixed)return null;let r=n.dom.offsetWidth*t.scaleX,i=n.domAfter?n.domAfter.offsetWidth*t.scaleX:0;return t.textDirection==B.LTR?{left:r,right:i}:{right:r,left:i}})});function ll(e){return Array.isArray(e)?e:[e]}function ul(e,t,n){for(;e.value&&e.from<=n;)e.from==n&&t.push(e.value),e.next()}var dl=class{constructor(e,t,n){this.gutter=e,this.height=n,this.i=0,this.cursor=F.iter(e.markers,t.from)}addElement(e,t,n){let{gutter:r}=this,i=(t.top-this.height)/e.scaleY,a=t.height/e.scaleY;if(this.i==r.elements.length){let t=new pl(e,a,i,n);r.elements.push(t),r.dom.appendChild(t.dom)}else r.elements[this.i].update(e,a,i,n);this.height=t.bottom,this.i++}line(e,t,n){let r=[];ul(this.cursor,r,t.from),n.length&&(r=r.concat(n));let i=this.gutter.config.lineMarker(e,t,r);i&&r.unshift(i);let a=this.gutter;(r.length!=0||a.config.renderEmptyElements)&&this.addElement(e,t,r)}widget(e,t){let n=this.gutter.config.widgetMarker(e,t.widget,t),r=n?[n]:null;for(let n of e.state.facet(nl)){let i=n(e,t.widget,t);i&&(r||=[]).push(i)}r&&this.addElement(e,t,r)}finish(){let e=this.gutter;for(;e.elements.length>this.i;){let t=e.elements.pop();e.dom.removeChild(t.dom),t.destroy()}}},fl=class{constructor(e,t){this.view=e,this.config=t,this.elements=[],this.spacer=null,this.dom=document.createElement(`div`),this.dom.className=`cm-gutter`+(this.config.class?` `+this.config.class:``);for(let n in t.domEventHandlers)this.dom.addEventListener(n,r=>{let i=r.target,a;if(i!=this.dom&&this.dom.contains(i)){for(;i.parentNode!=this.dom;)i=i.parentNode;let e=i.getBoundingClientRect();a=(e.top+e.bottom)/2}else a=r.clientY;let o=e.lineBlockAtHeight(a-e.documentTop);t.domEventHandlers[n](e,o,r)&&r.preventDefault()});this.markers=ll(t.markers(e)),t.initialSpacer&&(this.spacer=new pl(e,0,0,[t.initialSpacer(e)]),this.dom.appendChild(this.spacer.dom),this.spacer.dom.style.cssText+=`visibility: hidden; pointer-events: none`)}update(e){let t=this.markers;if(this.markers=ll(this.config.markers(e.view)),this.spacer&&this.config.updateSpacer){let t=this.config.updateSpacer(this.spacer.markers[0],e);t!=this.spacer.markers[0]&&this.spacer.update(e.view,0,0,[t])}let n=e.view.viewport;return!F.eq(this.markers,t,n.from,n.to)||(this.config.lineMarkerChange?this.config.lineMarkerChange(e):!1)}destroy(){for(let e of this.elements)e.destroy()}},pl=class{constructor(e,t,n,r){this.height=-1,this.above=0,this.markers=[],this.dom=document.createElement(`div`),this.dom.className=`cm-gutterElement`,this.update(e,t,n,r)}update(e,t,n,r){this.height!=t&&(this.height=t,this.dom.style.height=t+`px`),this.above!=n&&(this.dom.style.marginTop=(this.above=n)?n+`px`:``),ml(this.markers,r)||this.setMarkers(e,r)}setMarkers(e,t){let n=`cm-gutterElement`,r=this.dom.firstChild;for(let i=0,a=0;;){let o=a,s=i<t.length?t[i++]:null,c=!1;if(s){let e=s.elementClass;e&&(n+=` `+e);for(let e=a;e<this.markers.length;e++)if(this.markers[e].compare(s)){o=e,c=!0;break}}else o=this.markers.length;for(;a<o;){let e=this.markers[a++];if(e.toDOM){e.destroy(r);let t=r.nextSibling;r.remove(),r=t}}if(!s)break;s.toDOM&&(c?r=r.nextSibling:this.dom.insertBefore(s.toDOM(e),r)),c&&a++}this.dom.className=n,this.markers=t}destroy(){this.setMarkers(null,[])}};function ml(e,t){if(e.length!=t.length)return!1;for(let n=0;n<e.length;n++)if(!e[n].compare(t[n]))return!1;return!0}var hl=j.define(),gl=j.define(),_l=j.define({combine(e){return kt(e,{formatNumber:String,domEventHandlers:{}},{domEventHandlers(e,t){let n=Object.assign({},e);for(let e in t){let r=n[e],i=t[e];n[e]=r?(e,t,n)=>r(e,t,n)||i(e,t,n):i}return n}})}}),vl=class extends el{constructor(e){super(),this.number=e}eq(e){return this.number==e.number}toDOM(){return document.createTextNode(this.number)}};function yl(e,t){return e.state.facet(_l).formatNumber(t,e.state)}var bl=il.compute([_l],e=>({class:`cm-lineNumbers`,renderEmptyElements:!1,markers(e){return e.state.facet(hl)},lineMarker(e,t,n){return n.some(e=>e.toDOM)?null:new vl(yl(e,e.state.doc.lineAt(t.from).number))},widgetMarker:(e,t,n)=>{for(let r of e.state.facet(gl)){let i=r(e,t,n);if(i)return i}return null},lineMarkerChange:e=>e.startState.facet(_l)!=e.state.facet(_l),initialSpacer(e){return new vl(yl(e,Sl(e.state.doc.lines)))},updateSpacer(e,t){let n=yl(t.view,Sl(t.view.state.doc.lines));return n==e.number?e:new vl(n)},domEventHandlers:e.facet(_l).domEventHandlers,side:`before`}));function xl(e={}){return[_l.of(e),sl(),bl]}function Sl(e){let t=9;for(;t<e;)t=t*10+9;return t}var Cl=new class extends el{constructor(){super(...arguments),this.elementClass=`cm-activeLineGutter`}},wl=tl.compute([`selection`],e=>{let t=[],n=-1;for(let r of e.selection.ranges){let i=e.doc.lineAt(r.head).from;i>n&&(n=i,t.push(Cl.range(i)))}return F.of(t)});function Tl(){return wl}var El=1024,Dl=0,Ol=class{constructor(e,t){this.from=e,this.to=t}},K=class{constructor(e={}){this.id=Dl++,this.perNode=!!e.perNode,this.deserialize=e.deserialize||(()=>{throw Error(`This node type doesn't define a deserialize function`)}),this.combine=e.combine||null}add(e){if(this.perNode)throw RangeError(`Can't add per-node props to node types`);return typeof e!=`function`&&(e=jl.match(e)),t=>{let n=e(t);return n===void 0?null:[this,n]}}};K.closedBy=new K({deserialize:e=>e.split(` `)}),K.openedBy=new K({deserialize:e=>e.split(` `)}),K.group=new K({deserialize:e=>e.split(` `)}),K.isolate=new K({deserialize:e=>{if(e&&e!=`rtl`&&e!=`ltr`&&e!=`auto`)throw RangeError(`Invalid value for isolate: `+e);return e||`auto`}}),K.contextHash=new K({perNode:!0}),K.lookAhead=new K({perNode:!0}),K.mounted=new K({perNode:!0});var kl=class{constructor(e,t,n,r=!1){this.tree=e,this.overlay=t,this.parser=n,this.bracketed=r}static get(e){return e&&e.props&&e.props[K.mounted.id]}},Al=Object.create(null),jl=class e{constructor(e,t,n,r=0){this.name=e,this.props=t,this.id=n,this.flags=r}static define(t){let n=t.props&&t.props.length?Object.create(null):Al,r=+!!t.top|(t.skipped?2:0)|(t.error?4:0)|(t.name==null?8:0),i=new e(t.name||``,n,t.id,r);if(t.props){for(let e of t.props)if(Array.isArray(e)||(e=e(i)),e){if(e[0].perNode)throw RangeError(`Can't store a per-node prop on a node type`);n[e[0].id]=e[1]}}return i}prop(e){return this.props[e.id]}get isTop(){return(this.flags&1)>0}get isSkipped(){return(this.flags&2)>0}get isError(){return(this.flags&4)>0}get isAnonymous(){return(this.flags&8)>0}is(e){if(typeof e==`string`){if(this.name==e)return!0;let t=this.prop(K.group);return t?t.indexOf(e)>-1:!1}return this.id==e}static match(e){let t=Object.create(null);for(let n in e)for(let r of n.split(` `))t[r]=e[n];return e=>{for(let n=e.prop(K.group),r=-1;r<(n?n.length:0);r++){let i=t[r<0?e.name:n[r]];if(i)return i}}}};jl.none=new jl(``,Object.create(null),0,8);var Ml=class e{constructor(e){this.types=e;for(let t=0;t<e.length;t++)if(e[t].id!=t)throw RangeError(`Node type ids should correspond to array positions when creating a node set`)}extend(...t){let n=[];for(let e of this.types){let r=null;for(let n of t){let t=n(e);if(t){r||=Object.assign({},e.props);let n=t[1],i=t[0];i.combine&&i.id in r&&(n=i.combine(r[i.id],n)),r[i.id]=n}}n.push(r?new jl(e.name,r,e.id,e.flags):e)}return new e(n)}},Nl=new WeakMap,Pl=new WeakMap,q;(function(e){e[e.ExcludeBuffers=1]=`ExcludeBuffers`,e[e.IncludeAnonymous=2]=`IncludeAnonymous`,e[e.IgnoreMounts=4]=`IgnoreMounts`,e[e.IgnoreOverlays=8]=`IgnoreOverlays`,e[e.EnterBracketed=16]=`EnterBracketed`})(q||={});var J=class e{constructor(e,t,n,r,i){if(this.type=e,this.children=t,this.positions=n,this.length=r,this.props=null,i&&i.length){this.props=Object.create(null);for(let[e,t]of i)this.props[typeof e==`number`?e:e.id]=t}}toString(){let e=kl.get(this);if(e&&!e.overlay)return e.tree.toString();let t=``;for(let e of this.children){let n=e.toString();n&&(t&&(t+=`,`),t+=n)}return this.type.name?(/\W/.test(this.type.name)&&!this.type.isError?JSON.stringify(this.type.name):this.type.name)+(t.length?`(`+t+`)`:``):t}cursor(e=0){return new Jl(this.topNode,e)}cursorAt(e,t=0,n=0){let r=new Jl(Nl.get(this)||this.topNode);return r.moveTo(e,t),Nl.set(this,r._tree),r}get topNode(){return new Bl(this,0,0,null)}resolve(e,t=0){let n=Rl(Nl.get(this)||this.topNode,e,t,!1);return Nl.set(this,n),n}resolveInner(e,t=0){let n=Rl(Pl.get(this)||this.topNode,e,t,!0);return Pl.set(this,n),n}resolveStack(e,t=0){return ql(this,e,t)}iterate(e){let{enter:t,leave:n,from:r=0,to:i=this.length}=e,a=e.mode||0,o=(a&q.IncludeAnonymous)>0;for(let e=this.cursor(a|q.IncludeAnonymous);;){let a=!1;if(e.from<=i&&e.to>=r&&(!o&&e.type.isAnonymous||t(e)!==!1)){if(e.firstChild())continue;a=!0}for(;a&&n&&(o||!e.type.isAnonymous)&&n(e),!e.nextSibling();){if(!e.parent())return;a=!0}}}prop(e){return e.perNode?this.props?this.props[e.id]:void 0:this.type.prop(e)}get propValues(){let e=[];if(this.props)for(let t in this.props)e.push([+t,this.props[t]]);return e}balance(t={}){return this.children.length<=8?this:$l(jl.none,this.children,this.positions,0,this.children.length,0,this.length,(t,n,r)=>new e(this.type,t,n,r,this.propValues),t.makeTree||((t,n,r)=>new e(jl.none,t,n,r)))}static build(e){return Xl(e)}};J.empty=new J(jl.none,[],[],0);var Fl=class e{constructor(e,t){this.buffer=e,this.index=t}get id(){return this.buffer[this.index-4]}get start(){return this.buffer[this.index-3]}get end(){return this.buffer[this.index-2]}get size(){return this.buffer[this.index-1]}get pos(){return this.index}next(){this.index-=4}fork(){return new e(this.buffer,this.index)}},Il=class e{constructor(e,t,n){this.buffer=e,this.length=t,this.set=n}get type(){return jl.none}toString(){let e=[];for(let t=0;t<this.buffer.length;)e.push(this.childString(t)),t=this.buffer[t+3];return e.join(`,`)}childString(e){let t=this.buffer[e],n=this.buffer[e+3],r=this.set.types[t],i=r.name;if(/\W/.test(i)&&!r.isError&&(i=JSON.stringify(i)),e+=4,n==e)return i;let a=[];for(;e<n;)a.push(this.childString(e)),e=this.buffer[e+3];return i+`(`+a.join(`,`)+`)`}findChild(e,t,n,r,i){let{buffer:a}=this,o=-1;for(let s=e;s!=t&&!(Ll(i,r,a[s+1],a[s+2])&&(o=s,n>0));s=a[s+3]);return o}slice(t,n,r){let i=this.buffer,a=new Uint16Array(n-t),o=0;for(let e=t,s=0;e<n;){a[s++]=i[e++],a[s++]=i[e++]-r;let n=a[s++]=i[e++]-r;a[s++]=i[e++]-t,o=Math.max(o,n)}return new e(a,o,this.set)}};function Ll(e,t,n,r){switch(e){case-2:return n<t;case-1:return r>=t&&n<t;case 0:return n<t&&r>t;case 1:return n<=t&&r>t;case 2:return r>t;case 4:return!0}}function Rl(e,t,n,r){for(;e.from==e.to||(n<1?e.from>=t:e.from>t)||(n>-1?e.to<=t:e.to<t);){let t=!r&&e instanceof Bl&&e.index<0?null:e.parent;if(!t)return e;e=t}let i=r?0:q.IgnoreOverlays;if(r)for(let r=e,a=r.parent;a;r=a,a=r.parent)r instanceof Bl&&r.index<0&&a.enter(t,n,i)?.from!=r.from&&(e=a);for(;;){let r=e.enter(t,n,i);if(!r)return e;e=r}}var zl=class{cursor(e=0){return new Jl(this,e)}getChild(e,t=null,n=null){let r=Vl(this,e,t,n);return r.length?r[0]:null}getChildren(e,t=null,n=null){return Vl(this,e,t,n)}resolve(e,t=0){return Rl(this,e,t,!1)}resolveInner(e,t=0){return Rl(this,e,t,!0)}matchContext(e){return Hl(this.parent,e)}enterUnfinishedNodesBefore(e){let t=this.childBefore(e),n=this;for(;t;){let e=t.lastChild;if(!e||e.to!=t.to)break;e.type.isError&&e.from==e.to?(n=t,t=e.prevSibling):t=e}return n}get node(){return this}get next(){return this.parent}},Bl=class e extends zl{constructor(e,t,n,r){super(),this._tree=e,this.from=t,this.index=n,this._parent=r}get type(){return this._tree.type}get name(){return this._tree.type.name}get to(){return this.from+this._tree.length}nextChild(t,n,r,i,a=0){for(let o=this;;){for(let{children:s,positions:c}=o._tree,l=n>0?s.length:-1;t!=l;t+=n){let l=s[t],u=c[t]+o.from,d;if(a&q.EnterBracketed&&l instanceof J&&(d=kl.get(l))&&!d.overlay&&d.bracketed&&r>=u&&r<=u+l.length||Ll(i,r,u,u+l.length)){if(l instanceof Il){if(a&q.ExcludeBuffers)continue;let e=l.findChild(0,l.buffer.length,n,r-u,i);if(e>-1)return new Wl(new Ul(o,l,t,u),null,e)}else if(a&q.IncludeAnonymous||!l.type.isAnonymous||Yl(l)){let s;if(!(a&q.IgnoreMounts)&&(s=kl.get(l))&&!s.overlay)return new e(s.tree,u,t,o);let c=new e(l,u,t,o);return a&q.IncludeAnonymous||!c.type.isAnonymous?c:c.nextChild(n<0?l.children.length-1:0,n,r,i,a)}}}if(a&q.IncludeAnonymous||!o.type.isAnonymous||(t=o.index>=0?o.index+n:n<0?-1:o._parent._tree.children.length,o=o._parent,!o))return null}}get firstChild(){return this.nextChild(0,1,0,4)}get lastChild(){return this.nextChild(this._tree.children.length-1,-1,0,4)}childAfter(e){return this.nextChild(0,1,e,2)}childBefore(e){return this.nextChild(this._tree.children.length-1,-1,e,-2)}prop(e){return this._tree.prop(e)}enter(t,n,r=0){let i;if(!(r&q.IgnoreOverlays)&&(i=kl.get(this._tree))&&i.overlay){let a=t-this.from,o=r&q.EnterBracketed&&i.bracketed;for(let{from:t,to:r}of i.overlay)if((n>0||o?t<=a:t<a)&&(n<0||o?r>=a:r>a))return new e(i.tree,i.overlay[0].from+this.from,-1,this)}return this.nextChild(0,1,t,n,r)}nextSignificantParent(){let e=this;for(;e.type.isAnonymous&&e._parent;)e=e._parent;return e}get parent(){return this._parent?this._parent.nextSignificantParent():null}get nextSibling(){return this._parent&&this.index>=0?this._parent.nextChild(this.index+1,1,0,4):null}get prevSibling(){return this._parent&&this.index>=0?this._parent.nextChild(this.index-1,-1,0,4):null}get tree(){return this._tree}toTree(){return this._tree}toString(){return this._tree.toString()}};function Vl(e,t,n,r){let i=e.cursor(),a=[];if(!i.firstChild())return a;if(n!=null){for(let e=!1;!e;)if(e=i.type.is(n),!i.nextSibling())return a}for(;;){if(r!=null&&i.type.is(r))return a;if(i.type.is(t)&&a.push(i.node),!i.nextSibling())return r==null?a:[]}}function Hl(e,t,n=t.length-1){for(let r=e;n>=0;r=r.parent){if(!r)return!1;if(!r.type.isAnonymous){if(t[n]&&t[n]!=r.name)return!1;n--}}return!0}var Ul=class{constructor(e,t,n,r){this.parent=e,this.buffer=t,this.index=n,this.start=r}},Wl=class e extends zl{get name(){return this.type.name}get from(){return this.context.start+this.context.buffer.buffer[this.index+1]}get to(){return this.context.start+this.context.buffer.buffer[this.index+2]}constructor(e,t,n){super(),this.context=e,this._parent=t,this.index=n,this.type=e.buffer.set.types[e.buffer.buffer[n]]}child(t,n,r){let{buffer:i}=this.context,a=i.findChild(this.index+4,i.buffer[this.index+3],t,n-this.context.start,r);return a<0?null:new e(this.context,this,a)}get firstChild(){return this.child(1,0,4)}get lastChild(){return this.child(-1,0,4)}childAfter(e){return this.child(1,e,2)}childBefore(e){return this.child(-1,e,-2)}prop(e){return this.type.prop(e)}enter(t,n,r=0){if(r&q.ExcludeBuffers)return null;let{buffer:i}=this.context,a=i.findChild(this.index+4,i.buffer[this.index+3],n>0?1:-1,t-this.context.start,n);return a<0?null:new e(this.context,this,a)}get parent(){return this._parent||this.context.parent.nextSignificantParent()}externalSibling(e){return this._parent?null:this.context.parent.nextChild(this.context.index+e,e,0,4)}get nextSibling(){let{buffer:t}=this.context,n=t.buffer[this.index+3];return n<(this._parent?t.buffer[this._parent.index+3]:t.buffer.length)?new e(this.context,this._parent,n):this.externalSibling(1)}get prevSibling(){let{buffer:t}=this.context,n=this._parent?this._parent.index+4:0;return this.index==n?this.externalSibling(-1):new e(this.context,this._parent,t.findChild(n,this.index,-1,0,4))}get tree(){return null}toTree(){let e=[],t=[],{buffer:n}=this.context,r=this.index+4,i=n.buffer[this.index+3];if(i>r){let a=n.buffer[this.index+1];e.push(n.slice(r,i,a)),t.push(0)}return new J(this.type,e,t,this.to-this.from)}toString(){return this.context.buffer.childString(this.index)}};function Gl(e){if(!e.length)return null;let t=0,n=e[0];for(let r=1;r<e.length;r++){let i=e[r];(i.from>n.from||i.to<n.to)&&(n=i,t=r)}let r=n instanceof Bl&&n.index<0?null:n.parent,i=e.slice();return r?i[t]=r:i.splice(t,1),new Kl(i,n)}var Kl=class{constructor(e,t){this.heads=e,this.node=t}get next(){return Gl(this.heads)}};function ql(e,t,n){let r=e.resolveInner(t,n),i=null;for(let e=r instanceof Bl?r:r.context.parent;e;e=e.parent)if(e.index<0){let a=e.parent;(i||=[r]).push(a.resolve(t,n)),e=a}else{let a=kl.get(e.tree);if(a&&a.overlay&&a.overlay[0].from<=t&&a.overlay[a.overlay.length-1].to>=t){let o=new Bl(a.tree,a.overlay[0].from+e.from,-1,e);(i||=[r]).push(Rl(o,t,n,!1))}}return i?Gl(i):r}var Jl=class{get name(){return this.type.name}constructor(e,t=0){if(this.buffer=null,this.stack=[],this.index=0,this.bufferNode=null,this.mode=t&~q.EnterBracketed,e instanceof Bl)this.yieldNode(e);else{this._tree=e.context.parent,this.buffer=e.context;for(let t=e._parent;t;t=t._parent)this.stack.unshift(t.index);this.bufferNode=e,this.yieldBuf(e.index)}}yieldNode(e){return e?(this._tree=e,this.type=e.type,this.from=e.from,this.to=e.to,!0):!1}yieldBuf(e,t){this.index=e;let{start:n,buffer:r}=this.buffer;return this.type=t||r.set.types[r.buffer[e]],this.from=n+r.buffer[e+1],this.to=n+r.buffer[e+2],!0}yield(e){return e?e instanceof Bl?(this.buffer=null,this.yieldNode(e)):(this.buffer=e.context,this.yieldBuf(e.index,e.type)):!1}toString(){return this.buffer?this.buffer.buffer.childString(this.index):this._tree.toString()}enterChild(e,t,n){if(!this.buffer)return this.yield(this._tree.nextChild(e<0?this._tree._tree.children.length-1:0,e,t,n,this.mode));let{buffer:r}=this.buffer,i=r.findChild(this.index+4,r.buffer[this.index+3],e,t-this.buffer.start,n);return i<0?!1:(this.stack.push(this.index),this.yieldBuf(i))}firstChild(){return this.enterChild(1,0,4)}lastChild(){return this.enterChild(-1,0,4)}childAfter(e){return this.enterChild(1,e,2)}childBefore(e){return this.enterChild(-1,e,-2)}enter(e,t,n=this.mode){return this.buffer?n&q.ExcludeBuffers?!1:this.enterChild(1,e,t):this.yield(this._tree.enter(e,t,n))}parent(){if(!this.buffer)return this.yieldNode(this.mode&q.IncludeAnonymous?this._tree._parent:this._tree.parent);if(this.stack.length)return this.yieldBuf(this.stack.pop());let e=this.mode&q.IncludeAnonymous?this.buffer.parent:this.buffer.parent.nextSignificantParent();return this.buffer=null,this.yieldNode(e)}sibling(e){if(!this.buffer)return this._tree._parent?this.yield(this._tree.index<0?null:this._tree._parent.nextChild(this._tree.index+e,e,0,4,this.mode)):!1;let{buffer:t}=this.buffer,n=this.stack.length-1;if(e<0){let e=n<0?0:this.stack[n]+4;if(this.index!=e)return this.yieldBuf(t.findChild(e,this.index,-1,0,4))}else{let e=t.buffer[this.index+3];if(e<(n<0?t.buffer.length:t.buffer[this.stack[n]+3]))return this.yieldBuf(e)}return n<0&&this.yield(this.buffer.parent.nextChild(this.buffer.index+e,e,0,4,this.mode))}nextSibling(){return this.sibling(1)}prevSibling(){return this.sibling(-1)}atLastNode(e){let t,n,{buffer:r}=this;if(r){if(e>0){if(this.index<r.buffer.buffer.length)return!1}else for(let e=0;e<this.index;e++)if(r.buffer.buffer[e+3]<this.index)return!1;({index:t,parent:n}=r)}else({index:t,_parent:n}=this._tree);for(;n;{index:t,_parent:n}=n)if(t>-1)for(let r=t+e,i=e<0?-1:n._tree.children.length;r!=i;r+=e){let e=n._tree.children[r];if(this.mode&q.IncludeAnonymous||e instanceof Il||!e.type.isAnonymous||Yl(e))return!1}return!0}move(e,t){if(t&&this.enterChild(e,0,4))return!0;for(;;){if(this.sibling(e))return!0;if(this.atLastNode(e)||!this.parent())return!1}}next(e=!0){return this.move(1,e)}prev(e=!0){return this.move(-1,e)}moveTo(e,t=0){for(;(this.from==this.to||(t<1?this.from>=e:this.from>e)||(t>-1?this.to<=e:this.to<e))&&this.parent(););for(;this.enterChild(1,e,t););return this}get node(){if(!this.buffer)return this._tree;let e=this.bufferNode,t=null,n=0;if(e&&e.context==this.buffer)scan:for(let r=this.index,i=this.stack.length;i>=0;){for(let a=e;a;a=a._parent)if(a.index==r){if(r==this.index)return a;t=a,n=i+1;break scan}r=this.stack[--i]}for(let e=n;e<this.stack.length;e++)t=new Wl(this.buffer,t,this.stack[e]);return this.bufferNode=new Wl(this.buffer,t,this.index)}get tree(){return this.buffer?null:this._tree._tree}iterate(e,t){for(let n=0;;){let r=!1;if(this.type.isAnonymous||e(this)!==!1){if(this.firstChild()){n++;continue}this.type.isAnonymous||(r=!0)}for(;;){if(r&&t&&t(this),r=this.type.isAnonymous,!n)return;if(this.nextSibling())break;this.parent(),n--,r=!0}}}matchContext(e){if(!this.buffer)return Hl(this.node.parent,e);let{buffer:t}=this.buffer,{types:n}=t.set;for(let r=e.length-1,i=this.stack.length-1;r>=0;i--){if(i<0)return Hl(this._tree,e,r);let a=n[t.buffer[this.stack[i]]];if(!a.isAnonymous){if(e[r]&&e[r]!=a.name)return!1;r--}}return!0}};function Yl(e){return e.children.some(e=>e instanceof Il||!e.type.isAnonymous||Yl(e))}function Xl(e){let{buffer:t,nodeSet:n,maxBufferLength:r=El,reused:i=[],minRepeatType:a=n.types.length}=e,o=Array.isArray(t)?new Fl(t,t.length):t,s=n.types,c=0,l=0;function u(e,t,_,v,y,ee){let{id:b,start:x,end:te,size:ne}=o,re=l,ie=c;if(ne<0){if(o.next(),ne==-1){let t=i[b];_.push(t),v.push(x-e);return}if(ne==-3){c=b;return}if(ne==-4){l=b;return}throw RangeError(`Unrecognized record size: ${ne}`)}let S=s[b],C,w,ae=x-e;if(te-x<=r&&(w=h(o.pos-t,y))){let t=new Uint16Array(w.size-w.skip),r=o.pos-w.size,i=t.length;for(;o.pos>r;)i=g(w.start,t,i);C=new Il(t,te-w.start,n),ae=w.start-e}else{let e=o.pos-ne;o.next();let t=[],n=[],i=b>=a?b:-1,s=0,c=te;for(;o.pos>e;)i>=0&&o.id==i&&o.size>=0?(o.end<=c-r&&(p(t,n,x,s,o.end,c,i,re,ie),s=t.length,c=o.end),o.next()):ee>2500?d(x,e,t,n):u(x,e,t,n,i,ee+1);if(i>=0&&s>0&&s<t.length&&p(t,n,x,s,x,c,i,re,ie),t.reverse(),n.reverse(),i>-1&&s>0){let e=f(S,ie);C=$l(S,t,n,0,t.length,0,te-x,e,e)}else C=m(S,t,n,te-x,re-te,ie)}_.push(C),v.push(ae)}function d(e,t,i,a){let s=[],c=0,l=-1;for(;o.pos>t;){let{id:e,start:t,end:n,size:i}=o;if(i>4)o.next();else if(l>-1&&t<l)break;else l<0&&(l=n-r),s.push(e,t,n),c++,o.next()}if(c){let t=new Uint16Array(c*4),r=s[s.length-2];for(let e=s.length-3,n=0;e>=0;e-=3)t[n++]=s[e],t[n++]=s[e+1]-r,t[n++]=s[e+2]-r,t[n++]=n;i.push(new Il(t,s[2]-r,n)),a.push(r-e)}}function f(e,t){return(n,r,i)=>{let a=0,o=n.length-1,s,c;if(o>=0&&(s=n[o])instanceof J){if(!o&&s.type==e&&s.length==i)return s;(c=s.prop(K.lookAhead))&&(a=r[o]+s.length+c)}return m(e,n,r,i,a,t)}}function p(e,t,r,i,a,o,s,c,l){let u=[],d=[];for(;e.length>i;)u.push(e.pop()),d.push(t.pop()+r-a);e.push(m(n.types[s],u,d,o-a,c-o,l)),t.push(a-r)}function m(e,t,n,r,i,a,o){if(a){let e=[K.contextHash,a];o=o?[e].concat(o):[e]}if(i>25){let e=[K.lookAhead,i];o=o?[e].concat(o):[e]}return new J(e,t,n,r,o)}function h(e,t){let n=o.fork(),i=0,s=0,c=0,l=n.end-r,u={size:0,start:0,skip:0};scan:for(let r=n.pos-e;n.pos>r;){let e=n.size;if(n.id==t&&e>=0){u.size=i,u.start=s,u.skip=c,c+=4,i+=4,n.next();continue}let o=n.pos-e;if(e<0||o<r||n.start<l)break;let d=n.id>=a?4:0,f=n.start;for(n.next();n.pos>o;){if(n.size<0){if(n.size==-3||n.size==-4)d+=4;else break scan}else n.id>=a&&(d+=4);n.next()}s=f,i+=e,c+=d}return(t<0||i==e)&&(u.size=i,u.start=s,u.skip=c),u.size>4?u:void 0}function g(e,t,n){let{id:r,start:i,end:s,size:u}=o;if(o.next(),u>=0&&r<a){let a=n;if(u>4){let r=o.pos-(u-4);for(;o.pos>r;)n=g(e,t,n)}t[--n]=a,t[--n]=s-e,t[--n]=i-e,t[--n]=r}else u==-3?c=r:u==-4&&(l=r);return n}let _=[],v=[];for(;o.pos>0;)u(e.start||0,e.bufferStart||0,_,v,-1,0);let y=e.length??(_.length?v[0]+_[0].length:0);return new J(s[e.topID],_.reverse(),v.reverse(),y)}var Zl=new WeakMap;function Ql(e,t){if(!e.isAnonymous||t instanceof Il||t.type!=e)return 1;let n=Zl.get(t);if(n==null){n=1;for(let r of t.children){if(r.type!=e||!(r instanceof J)){n=1;break}n+=Ql(e,r)}Zl.set(t,n)}return n}function $l(e,t,n,r,i,a,o,s,c){let l=0;for(let n=r;n<i;n++)l+=Ql(e,t[n]);let u=Math.ceil(l*1.5/8),d=[],f=[];function p(t,n,r,i,o){for(let s=r;s<i;){let r=s,l=n[s],m=Ql(e,t[s]);for(s++;s<i;s++){let n=Ql(e,t[s]);if(m+n>=u)break;m+=n}if(s==r+1){if(m>u){let e=t[r];p(e.children,e.positions,0,e.children.length,n[r]+o);continue}d.push(t[r])}else{let i=n[s-1]+t[s-1].length-l;d.push($l(e,t,n,r,s,l,i,null,c))}f.push(l+o-a)}}return p(t,n,r,i,0),(s||c)(d,f,o)}var eu=class e{constructor(e,t,n,r,i=!1,a=!1){this.from=e,this.to=t,this.tree=n,this.offset=r,this.open=!!i|(a?2:0)}get openStart(){return(this.open&1)>0}get openEnd(){return(this.open&2)>0}static addTree(t,n=[],r=!1){let i=[new e(0,t.length,t,0,!1,r)];for(let e of n)e.to>t.length&&i.push(e);return i}static applyChanges(t,n,r=128){if(!n.length)return t;let i=[],a=1,o=t.length?t[0]:null;for(let s=0,c=0,l=0;;s++){let u=s<n.length?n[s]:null,d=u?u.fromA:1e9;if(d-c>=r)for(;o&&o.from<d;){let n=o;if(c>=n.from||d<=n.to||l){let t=Math.max(n.from,c)-l,r=Math.min(n.to,d)-l;n=t>=r?null:new e(t,r,n.tree,n.offset+l,s>0,!!u)}if(n&&i.push(n),o.to>d)break;o=a<t.length?t[a++]:null}if(!u)break;c=u.toA,l=u.toA-u.toB}return i}},tu=class{startParse(e,t,n){return typeof e==`string`&&(e=new nu(e)),n=n?n.length?n.map(e=>new Ol(e.from,e.to)):[new Ol(0,0)]:[new Ol(0,e.length)],this.createParse(e,t||[],n)}parse(e,t,n){let r=this.startParse(e,t,n);for(;;){let e=r.advance();if(e)return e}}},nu=class{constructor(e){this.string=e}get length(){return this.string.length}chunk(e){return this.string.slice(e)}get lineChunks(){return!1}read(e,t){return this.string.slice(e,t)}};new K({perNode:!0});var ru=0,iu=class e{constructor(e,t,n,r){this.name=e,this.set=t,this.base=n,this.modified=r,this.id=ru++}toString(){let{name:e}=this;for(let t of this.modified)t.name&&(e=`${t.name}(${e})`);return e}static define(t,n){let r=typeof t==`string`?t:`?`;if(t instanceof e&&(n=t),n?.base)throw Error(`Can not derive from a modified tag`);let i=new e(r,[],null,[]);if(i.set.push(i),n)for(let e of n.set)i.set.push(e);return i}static defineModifier(e){let t=new ou(e);return e=>e.modified.indexOf(t)>-1?e:ou.get(e.base||e,e.modified.concat(t).sort((e,t)=>e.id-t.id))}},au=0,ou=class e{constructor(e){this.name=e,this.instances=[],this.id=au++}static get(t,n){if(!n.length)return t;let r=n[0].instances.find(e=>e.base==t&&su(n,e.modified));if(r)return r;let i=[],a=new iu(t.name,i,t,n);for(let e of n)e.instances.push(a);let o=cu(n);for(let n of t.set)if(!n.modified.length)for(let t of o)i.push(e.get(n,t));return a}};function su(e,t){return e.length==t.length&&e.every((e,n)=>e==t[n])}function cu(e){let t=[[]];for(let n=0;n<e.length;n++)for(let r=0,i=t.length;r<i;r++)t.push(t[r].concat(e[n]));return t.sort((e,t)=>t.length-e.length)}function lu(e){let t=Object.create(null);for(let n in e){let r=e[n];Array.isArray(r)||(r=[r]);for(let e of n.split(` `))if(e){let n=[],i=2,a=e;for(let t=0;;){if(a==`...`&&t>0&&t+3==e.length){i=1;break}let r=/^"(?:[^"\\]|\\.)*?"|[^\/!]+/.exec(a);if(!r)throw RangeError(`Invalid path: `+e);if(n.push(r[0]==`*`?``:r[0][0]==`"`?JSON.parse(r[0]):r[0]),t+=r[0].length,t==e.length)break;let o=e[t++];if(t==e.length&&o==`!`){i=0;break}if(o!=`/`)throw RangeError(`Invalid path: `+e);a=e.slice(t)}let o=n.length-1,s=n[o];if(!s)throw RangeError(`Invalid path: `+e);t[s]=new du(r,i,o>0?n.slice(0,o):null).sort(t[s])}}return uu.add(t)}var uu=new K({combine(e,t){let n,r,i;for(;e||t;){if(!e||t&&e.depth<=t.depth?(i=t,t=t.next):(i=e,e=e.next),n&&n.mode==i.mode&&!i.context&&!n.context)continue;let a=new du(i.tags,i.mode,i.context);n?n.next=a:r=a,n=a}return r}}),du=class{constructor(e,t,n,r){this.tags=e,this.mode=t,this.context=n,this.next=r}get opaque(){return this.mode==0}get inherit(){return this.mode==1}sort(e){return!e||e.depth<this.depth?(this.next=e,this):(e.next=this.sort(e.next),e)}get depth(){return this.context?this.context.length:0}};du.empty=new du([],2,null);function fu(e,t){let n=Object.create(null);for(let t of e)if(!Array.isArray(t.tag))n[t.tag.id]=t.class;else for(let e of t.tag)n[e.id]=t.class;let{scope:r,all:i=null}=t||{};return{style:e=>{let t=i;for(let r of e)for(let e of r.set){let r=n[e.id];if(r){t=t?t+` `+r:r;break}}return t},scope:r}}function pu(e,t){let n=null;for(let r of e){let e=r.style(t);e&&(n=n?n+` `+e:e)}return n}function mu(e,t,n,r=0,i=e.length){let a=new hu(r,Array.isArray(t)?t:[t],n);a.highlightRange(e.cursor(),r,i,``,a.highlighters),a.flush(i)}var hu=class{constructor(e,t,n){this.at=e,this.highlighters=t,this.span=n,this.class=``}startSpan(e,t){t!=this.class&&(this.flush(e),e>this.at&&(this.at=e),this.class=t)}flush(e){e>this.at&&this.class&&this.span(this.at,e,this.class)}highlightRange(e,t,n,r,i){let{type:a,from:o,to:s}=e;if(o>=n||s<=t)return;a.isTop&&(i=this.highlighters.filter(e=>!e.scope||e.scope(a)));let c=r,l=gu(e)||du.empty,u=pu(i,l.tags);if(u&&(c&&(c+=` `),c+=u,l.mode==1&&(r+=(r?` `:``)+u)),this.startSpan(Math.max(t,o),c),l.opaque)return;let d=e.tree&&e.tree.prop(K.mounted);if(d&&d.overlay){let a=e.node.enter(d.overlay[0].from+o,1),l=this.highlighters.filter(e=>!e.scope||e.scope(d.tree.type)),u=e.firstChild();for(let f=0,p=o;;f++){let m=f<d.overlay.length?d.overlay[f]:null,h=m?m.from+o:s,g=Math.max(t,p),_=Math.min(n,h);if(g<_&&u)for(;e.from<_&&(this.highlightRange(e,g,_,r,i),this.startSpan(Math.min(_,e.to),c),!(e.to>=h||!e.nextSibling())););if(!m||h>n)break;p=m.to+o,p>t&&(this.highlightRange(a.cursor(),Math.max(t,m.from+o),Math.min(n,p),``,l),this.startSpan(Math.min(n,p),c))}u&&e.parent()}else if(e.firstChild()){d&&(r=``);do if(!(e.to<=t)){if(e.from>=n)break;this.highlightRange(e,t,n,r,i),this.startSpan(Math.min(n,e.to),c)}while(e.nextSibling());e.parent()}}};function gu(e){let t=e.type.prop(uu);for(;t&&t.context&&!e.matchContext(t.context);)t=t.next;return t||null}var Y=iu.define,_u=Y(),vu=Y(),yu=Y(vu),bu=Y(vu),xu=Y(),Su=Y(xu),Cu=Y(xu),wu=Y(),Tu=Y(wu),Eu=Y(),Du=Y(),Ou=Y(),ku=Y(Ou),Au=Y(),X={comment:_u,lineComment:Y(_u),blockComment:Y(_u),docComment:Y(_u),name:vu,variableName:Y(vu),typeName:yu,tagName:Y(yu),propertyName:bu,attributeName:Y(bu),className:Y(vu),labelName:Y(vu),namespace:Y(vu),macroName:Y(vu),literal:xu,string:Su,docString:Y(Su),character:Y(Su),attributeValue:Y(Su),number:Cu,integer:Y(Cu),float:Y(Cu),bool:Y(xu),regexp:Y(xu),escape:Y(xu),color:Y(xu),url:Y(xu),keyword:Eu,self:Y(Eu),null:Y(Eu),atom:Y(Eu),unit:Y(Eu),modifier:Y(Eu),operatorKeyword:Y(Eu),controlKeyword:Y(Eu),definitionKeyword:Y(Eu),moduleKeyword:Y(Eu),operator:Du,derefOperator:Y(Du),arithmeticOperator:Y(Du),logicOperator:Y(Du),bitwiseOperator:Y(Du),compareOperator:Y(Du),updateOperator:Y(Du),definitionOperator:Y(Du),typeOperator:Y(Du),controlOperator:Y(Du),punctuation:Ou,separator:Y(Ou),bracket:ku,angleBracket:Y(ku),squareBracket:Y(ku),paren:Y(ku),brace:Y(ku),content:wu,heading:Tu,heading1:Y(Tu),heading2:Y(Tu),heading3:Y(Tu),heading4:Y(Tu),heading5:Y(Tu),heading6:Y(Tu),contentSeparator:Y(wu),list:Y(wu),quote:Y(wu),emphasis:Y(wu),strong:Y(wu),link:Y(wu),monospace:Y(wu),strikethrough:Y(wu),inserted:Y(),deleted:Y(),changed:Y(),invalid:Y(),meta:Au,documentMeta:Y(Au),annotation:Y(Au),processingInstruction:Y(Au),definition:iu.defineModifier(`definition`),constant:iu.defineModifier(`constant`),function:iu.defineModifier(`function`),standard:iu.defineModifier(`standard`),local:iu.defineModifier(`local`),special:iu.defineModifier(`special`)};for(let e in X){let t=X[e];t instanceof iu&&(t.name=e)}fu([{tag:X.link,class:`tok-link`},{tag:X.heading,class:`tok-heading`},{tag:X.emphasis,class:`tok-emphasis`},{tag:X.strong,class:`tok-strong`},{tag:X.keyword,class:`tok-keyword`},{tag:X.atom,class:`tok-atom`},{tag:X.bool,class:`tok-bool`},{tag:X.url,class:`tok-url`},{tag:X.labelName,class:`tok-labelName`},{tag:X.inserted,class:`tok-inserted`},{tag:X.deleted,class:`tok-deleted`},{tag:X.literal,class:`tok-literal`},{tag:X.string,class:`tok-string`},{tag:X.number,class:`tok-number`},{tag:[X.regexp,X.escape,X.special(X.string)],class:`tok-string2`},{tag:X.variableName,class:`tok-variableName`},{tag:X.local(X.variableName),class:`tok-variableName tok-local`},{tag:X.definition(X.variableName),class:`tok-variableName tok-definition`},{tag:X.special(X.variableName),class:`tok-variableName2`},{tag:X.definition(X.propertyName),class:`tok-propertyName tok-definition`},{tag:X.typeName,class:`tok-typeName`},{tag:X.namespace,class:`tok-namespace`},{tag:X.className,class:`tok-className`},{tag:X.macroName,class:`tok-macroName`},{tag:X.propertyName,class:`tok-propertyName`},{tag:X.operator,class:`tok-operator`},{tag:X.comment,class:`tok-comment`},{tag:X.meta,class:`tok-meta`},{tag:X.invalid,class:`tok-invalid`},{tag:X.punctuation,class:`tok-punctuation`}]);function ju(e,t,n,r=0,i=0){t??(t=e.search(/[^\s\u00a0]/),t==-1&&(t=e.length));let a=i;for(let i=r;i<t;i++)e.charCodeAt(i)==9?a+=n-a%n:a++;return a}var Mu=class{constructor(e,t,n,r){this.string=e,this.tabSize=t,this.indentUnit=n,this.overrideIndent=r,this.pos=0,this.start=0,this.lastColumnPos=0,this.lastColumnValue=0}eol(){return this.pos>=this.string.length}sol(){return this.pos==0}peek(){return this.string.charAt(this.pos)||void 0}next(){if(this.pos<this.string.length)return this.string.charAt(this.pos++)}eat(e){let t=this.string.charAt(this.pos),n;if(n=typeof e==`string`?t==e:t&&(e instanceof RegExp?e.test(t):e(t)),n)return++this.pos,t}eatWhile(e){let t=this.pos;for(;this.eat(e););return this.pos>t}eatSpace(){let e=this.pos;for(;/[\s\u00a0]/.test(this.string.charAt(this.pos));)++this.pos;return this.pos>e}skipToEnd(){this.pos=this.string.length}skipTo(e){let t=this.string.indexOf(e,this.pos);if(t>-1)return this.pos=t,!0}backUp(e){this.pos-=e}column(){return this.lastColumnPos<this.start&&(this.lastColumnValue=ju(this.string,this.start,this.tabSize,this.lastColumnPos,this.lastColumnValue),this.lastColumnPos=this.start),this.lastColumnValue}indentation(){return this.overrideIndent??ju(this.string,null,this.tabSize)}match(e,t,n){if(typeof e==`string`){let r=e=>n?e.toLowerCase():e;return r(this.string.substr(this.pos,e.length))==r(e)?(t!==!1&&(this.pos+=e.length),!0):null}{let n=this.string.slice(this.pos).match(e);return n&&n.index>0?null:(n&&t!==!1&&(this.pos+=n[0].length),n)}}current(){return this.string.slice(this.start,this.pos)}},Nu=new K;function Pu(e){return j.define({combine:e?t=>t.concat(e):void 0})}var Fu=new K,Iu=class{constructor(e,t,n=[],r=``){this.data=e,this.name=r,P.prototype.hasOwnProperty(`tree`)||Object.defineProperty(P.prototype,"tree",{get(){return Ru(this)}}),this.parser=t,this.extension=[qu.of(this),P.languageData.of((e,t,n)=>{let r=Lu(e,t,n),i=r.type.prop(Nu);if(!i)return[];let a=e.facet(i),o=r.type.prop(Fu);if(o){let i=r.resolve(t-r.from,n);for(let t of o)if(t.test(i,e)){let n=e.facet(t.facet);return t.type==`replace`?n:n.concat(a)}}return a})].concat(n)}isActiveAt(e,t,n=-1){return Lu(e,t,n).type.prop(Nu)==this.data}findRegions(e){let t=e.facet(qu);if(t?.data==this.data)return[{from:0,to:e.doc.length}];if(!t||!t.allowsNesting)return[];let n=[],r=(e,t)=>{if(e.prop(Nu)==this.data){n.push({from:t,to:t+e.length});return}let i=e.prop(K.mounted);if(i){if(i.tree.prop(Nu)==this.data){if(i.overlay)for(let e of i.overlay)n.push({from:e.from+t,to:e.to+t});else n.push({from:t,to:t+e.length});return}if(i.overlay){let e=n.length;if(r(i.tree,i.overlay[0].from+t),n.length>e)return}}for(let n=0;n<e.children.length;n++){let i=e.children[n];i instanceof J&&r(i,e.positions[n]+t)}};return r(Ru(e),0),n}get allowsNesting(){return!0}};Iu.setState=M.define();function Lu(e,t,n){let r=e.facet(qu),i=Ru(e).topNode;if(!r||r.allowsNesting)for(let e=i;e;e=e.enter(t,n,q.ExcludeBuffers|q.EnterBracketed))e.type.isTop&&(i=e);return i}function Ru(e){let t=e.field(Iu.state,!1);return t?t.tree:J.empty}var zu=class{constructor(e){this.doc=e,this.cursorPos=0,this.string=``,this.cursor=e.iter()}get length(){return this.doc.length}syncTo(e){return this.string=this.cursor.next(e-this.cursorPos).value,this.cursorPos=e+this.string.length,this.cursorPos-this.string.length}chunk(e){return this.syncTo(e),this.string}get lineChunks(){return!0}read(e,t){let n=this.cursorPos-this.string.length;return e<n||t>=this.cursorPos?this.doc.sliceString(e,t):this.string.slice(e-n,t-n)}},Bu=null,Vu=class e{constructor(e,t,n=[],r,i,a,o,s){this.parser=e,this.state=t,this.fragments=n,this.tree=r,this.treeLen=i,this.viewport=a,this.skipped=o,this.scheduleOn=s,this.parse=null,this.tempSkipped=[]}static create(t,n,r){return new e(t,n,[],J.empty,0,r,[],null)}startParse(){return this.parser.startParse(new zu(this.state.doc),this.fragments)}work(e,t){return t!=null&&t>=this.state.doc.length&&(t=void 0),this.tree!=J.empty&&this.isDone(t??this.state.doc.length)?(this.takeTree(),!0):this.withContext(()=>{if(typeof e==`number`){let t=Date.now()+e;e=()=>Date.now()>t}for(this.parse||=this.startParse(),t!=null&&(this.parse.stoppedAt==null||this.parse.stoppedAt>t)&&t<this.state.doc.length&&this.parse.stopAt(t);;){let n=this.parse.advance();if(n){if(this.fragments=this.withoutTempSkipped(eu.addTree(n,this.fragments,this.parse.stoppedAt!=null)),this.treeLen=this.parse.stoppedAt??this.state.doc.length,this.tree=n,this.parse=null,this.treeLen<(t??this.state.doc.length))this.parse=this.startParse();else return!0}if(e())return!1}})}takeTree(){let e,t;this.parse&&(e=this.parse.parsedPos)>=this.treeLen&&((this.parse.stoppedAt==null||this.parse.stoppedAt>e)&&this.parse.stopAt(e),this.withContext(()=>{for(;!(t=this.parse.advance()););}),this.treeLen=e,this.tree=t,this.fragments=this.withoutTempSkipped(eu.addTree(this.tree,this.fragments,!0)),this.parse=null)}withContext(e){let t=Bu;Bu=this;try{return e()}finally{Bu=t}}withoutTempSkipped(e){for(let t;t=this.tempSkipped.pop();)e=Hu(e,t.from,t.to);return e}changes(t,n){let{fragments:r,tree:i,treeLen:a,viewport:o,skipped:s}=this;if(this.takeTree(),!t.empty){let e=[];if(t.iterChangedRanges((t,n,r,i)=>e.push({fromA:t,toA:n,fromB:r,toB:i})),r=eu.applyChanges(r,e),i=J.empty,a=0,o={from:t.mapPos(o.from,-1),to:t.mapPos(o.to,1)},this.skipped.length){s=[];for(let e of this.skipped){let n=t.mapPos(e.from,1),r=t.mapPos(e.to,-1);n<r&&s.push({from:n,to:r})}}}return new e(this.parser,n,r,i,a,o,s,this.scheduleOn)}updateViewport(e){if(this.viewport.from==e.from&&this.viewport.to==e.to)return!1;this.viewport=e;let t=this.skipped.length;for(let t=0;t<this.skipped.length;t++){let{from:n,to:r}=this.skipped[t];n<e.to&&r>e.from&&(this.fragments=Hu(this.fragments,n,r),this.skipped.splice(t--,1))}return this.skipped.length>=t?!1:(this.reset(),!0)}reset(){this.parse&&=(this.takeTree(),null)}skipUntilInView(e,t){this.skipped.push({from:e,to:t})}static getSkippingParser(e){return new class extends tu{createParse(t,n,r){let i=r[0].from,a=r[r.length-1].to;return{parsedPos:i,advance(){let t=Bu;if(t){for(let e of r)t.tempSkipped.push(e);e&&(t.scheduleOn=t.scheduleOn?Promise.all([t.scheduleOn,e]):e)}return this.parsedPos=a,new J(jl.none,[],[],a-i)},stoppedAt:null,stopAt(){}}}}}isDone(e){e=Math.min(e,this.state.doc.length);let t=this.fragments;return this.treeLen>=e&&t.length&&t[0].from==0&&t[0].to>=e}static get(){return Bu}};function Hu(e,t,n){return eu.applyChanges(e,[{fromA:t,toA:n,fromB:t,toB:n}])}var Uu=class e{constructor(e){this.context=e,this.tree=e.tree}apply(t){if(!t.docChanged&&this.tree==this.context.tree)return this;let n=this.context.changes(t.changes,t.state),r=this.context.treeLen==t.startState.doc.length?void 0:Math.max(t.changes.mapPos(this.context.treeLen),n.viewport.to);return n.work(20,r)||n.takeTree(),new e(n)}static init(t){let n=Math.min(3e3,t.doc.length),r=Vu.create(t.facet(qu).parser,t,{from:0,to:n});return r.work(20,n)||r.takeTree(),new e(r)}};Iu.state=Ye.define({create:Uu.init,update(e,t){for(let e of t.effects)if(e.is(Iu.setState))return e.value;return t.startState.facet(qu)==t.state.facet(qu)?e.apply(t):Uu.init(t.state)}});var Wu=e=>{let t=setTimeout(()=>e(),500);return()=>clearTimeout(t)};typeof requestIdleCallback<`u`&&(Wu=e=>{let t=-1,n=setTimeout(()=>{t=requestIdleCallback(e,{timeout:400})},100);return()=>t<0?clearTimeout(n):cancelIdleCallback(t)});var Gu=typeof navigator<`u`&&navigator.scheduling?.isInputPending?()=>navigator.scheduling.isInputPending():null,Ku=H.fromClass(class{constructor(e){this.view=e,this.working=null,this.workScheduled=0,this.chunkEnd=-1,this.chunkBudget=-1,this.work=this.work.bind(this),this.scheduleWork()}update(e){let t=this.view.state.field(Iu.state).context;(t.updateViewport(e.view.viewport)||this.view.viewport.to>t.treeLen)&&this.scheduleWork(),(e.docChanged||e.selectionSet)&&(this.view.hasFocus&&(this.chunkBudget+=50),this.scheduleWork()),this.checkAsyncSchedule(t)}scheduleWork(){if(this.working)return;let{state:e}=this.view,t=e.field(Iu.state);t.tree==t.context.tree&&t.context.isDone(e.doc.length)||(this.working=Wu(this.work))}work(e){this.working=null;let t=Date.now();if(this.chunkEnd<t&&(this.chunkEnd<0||this.view.hasFocus)&&(this.chunkEnd=t+3e4,this.chunkBudget=3e3),this.chunkBudget<=0)return;let{state:n,viewport:{to:r}}=this.view,i=n.field(Iu.state);if(i.tree==i.context.tree&&i.context.isDone(r+1e5))return;let a=Date.now()+Math.min(this.chunkBudget,100,e&&!Gu?Math.max(25,e.timeRemaining()-5):1e9),o=i.context.treeLen<r&&n.doc.length>r+1e3,s=i.context.work(()=>Gu&&Gu()||Date.now()>a,r+(o?0:1e5));this.chunkBudget-=Date.now()-t,(s||this.chunkBudget<=0)&&(i.context.takeTree(),this.view.dispatch({effects:Iu.setState.of(new Uu(i.context))})),this.chunkBudget>0&&(!s||o)&&this.scheduleWork(),this.checkAsyncSchedule(i.context)}checkAsyncSchedule(e){e.scheduleOn&&=(this.workScheduled++,e.scheduleOn.then(()=>this.scheduleWork()).catch(e=>Yr(this.view.state,e)).then(()=>this.workScheduled--),null)}destroy(){this.working&&this.working()}isWorking(){return!!(this.working||this.workScheduled>0)}},{eventHandlers:{focus(){this.scheduleWork()}}}),qu=j.define({combine(e){return e.length?e[0]:null},enables:e=>[Iu.state,Ku,G.contentAttributes.compute([e],t=>{let n=t.facet(e);return n&&n.name?{"data-language":n.name}:{}})]}),Ju=j.define(),Yu=j.define({combine:e=>{if(!e.length)return`  `;let t=e[0];if(!t||/\S/.test(t)||Array.from(t).some(e=>e!=t[0]))throw Error(`Invalid indent unit: `+JSON.stringify(e[0]));return t}});function Xu(e){let t=e.facet(Yu);return t.charCodeAt(0)==9?e.tabSize*t.length:t.length}function Zu(e,t){let n=``,r=e.tabSize,i=e.facet(Yu)[0];if(i==`	`){for(;t>=r;)n+=`	`,t-=r;i=` `}for(let e=0;e<t;e++)n+=i;return n}function Qu(e,t){e instanceof P&&(e=new $u(e));for(let n of e.state.facet(Ju)){let r=n(e,t);if(r!==void 0)return r}let n=Ru(e.state);return n.length>=t?td(e,n,t):null}var $u=class{constructor(e,t={}){this.state=e,this.options=t,this.unit=Xu(e)}lineAt(e,t=1){let n=this.state.doc.lineAt(e),{simulateBreak:r,simulateDoubleBreak:i}=this.options;return r!=null&&r>=n.from&&r<=n.to?i&&r==e?{text:``,from:e}:(t<0?r<e:r<=e)?{text:n.text.slice(r-n.from),from:r}:{text:n.text.slice(0,r-n.from),from:n.from}:n}textAfterPos(e,t=1){if(this.options.simulateDoubleBreak&&e==this.options.simulateBreak)return``;let{text:n,from:r}=this.lineAt(e,t);return n.slice(e-r,Math.min(n.length,e+100-r))}column(e,t=1){let{text:n,from:r}=this.lineAt(e,t),i=this.countColumn(n,e-r),a=this.options.overrideIndentation?this.options.overrideIndentation(r):-1;return a>-1&&(i+=a-this.countColumn(n,n.search(/\S|$/))),i}countColumn(e,t=e.length){return Jt(e,this.state.tabSize,t)}lineIndent(e,t=1){let{text:n,from:r}=this.lineAt(e,t),i=this.options.overrideIndentation;if(i){let e=i(r);if(e>-1)return e}return this.countColumn(n,n.search(/\S|$/))}get simulatedBreak(){return this.options.simulateBreak||null}},ed=new K;function td(e,t,n){let r=t.resolveStack(n),i=t.resolveInner(n,-1).resolve(n,0).enterUnfinishedNodesBefore(n);if(i!=r.node){let e=[];for(let t=i;t&&!(t.from<r.node.from||t.to>r.node.to||t.from==r.node.from&&t.type==r.node.type);t=t.parent)e.push(t);for(let t=e.length-1;t>=0;t--)r={node:e[t],next:r}}return nd(r,e,n)}function nd(e,t,n){for(let r=e;r;r=r.next){let e=id(r.node);if(e)return e(od.create(t,n,r))}return 0}function rd(e){return e.pos==e.options.simulateBreak&&e.options.simulateDoubleBreak}function id(e){let t=e.type.prop(ed);if(t)return t;let n=e.firstChild,r;if(n&&(r=n.type.prop(K.closedBy))){let t=e.lastChild,n=t&&r.indexOf(t.name)>-1;return e=>ld(e,!0,1,void 0,n&&!rd(e)?t.from:void 0)}return e.parent==null?ad:null}function ad(){return 0}var od=class e extends $u{constructor(e,t,n){super(e.state,e.options),this.base=e,this.pos=t,this.context=n}get node(){return this.context.node}static create(t,n,r){return new e(t,n,r)}get textAfter(){return this.textAfterPos(this.pos)}get baseIndent(){return this.baseIndentFor(this.node)}baseIndentFor(e){let t=this.state.doc.lineAt(e.from);for(;;){let n=e.resolve(t.from);for(;n.parent&&n.parent.from==n.from;)n=n.parent;if(sd(n,e))break;t=this.state.doc.lineAt(n.from)}return this.lineIndent(t.from)}continue(){return nd(this.context.next,this.base,this.pos)}};function sd(e,t){for(let n=t;n;n=n.parent)if(e==n)return!0;return!1}function cd(e){let t=e.node,n=t.childAfter(t.from),r=t.lastChild;if(!n)return null;let i=e.options.simulateBreak,a=e.state.doc.lineAt(n.from),o=i==null||i<=a.from?a.to:Math.min(a.to,i);for(let e=n.to;;){let i=t.childAfter(e);if(!i||i==r)return null;if(!i.type.isSkipped){if(i.from>=o)return null;let e=/^ */.exec(a.text.slice(n.to-a.from))[0].length;return{from:n.from,to:n.to+e}}e=i.to}}function ld(e,t,n,r,i){let a=e.textAfter,o=a.match(/^\s*/)[0].length,s=r&&a.slice(o,o+r.length)==r||i==e.pos+o,c=t?cd(e):null;return c?s?e.column(c.from):e.column(c.to):e.baseIndent+(s?0:e.unit*n)}var ud=200;function dd(){return P.transactionFilter.of(e=>{if(!e.docChanged||!e.isUserEvent(`input.type`)&&!e.isUserEvent(`input.complete`))return e;let t=e.startState.languageDataAt(`indentOnInput`,e.startState.selection.main.head);if(!t.length)return e;let n=e.newDoc,{head:r}=e.newSelection.main,i=n.lineAt(r);if(r>i.from+ud)return e;let a=n.sliceString(i.from,r);if(!t.some(e=>e.test(a)))return e;let{state:o}=e,s=-1,c=[];for(let{head:e}of o.selection.ranges){let t=o.doc.lineAt(e);if(t.from==s)continue;s=t.from;let n=Qu(o,t.from);if(n==null)continue;let r=/^\s*/.exec(t.text)[0],i=Zu(o,n);r!=i&&c.push({from:t.from,to:t.from+r.length,insert:i})}return c.length?[e,{changes:c,sequential:!0}]:e})}var fd=j.define(),pd=new K;function md(e,t,n){let r=Ru(e);if(r.length<n)return null;let i=r.resolveStack(n,1),a=null;for(let o=i;o;o=o.next){let i=o.node;if(i.to<=n||i.from>n)continue;if(a&&i.from<t)break;let s=i.type.prop(pd);if(s&&(i.to<r.length-50||r.length==e.doc.length||!hd(i))){let r=s(i,e);r&&r.from<=n&&r.from>=t&&r.to>n&&(a=r)}}return a}function hd(e){let t=e.lastChild;return t&&t.to==e.to&&t.type.isError}function gd(e,t,n){for(let r of e.facet(fd)){let i=r(e,t,n);if(i)return i}return md(e,t,n)}function _d(e,t){let n=t.mapPos(e.from,1),r=t.mapPos(e.to,-1);return n>=r?void 0:{from:n,to:r}}var vd=M.define({map:_d}),yd=M.define({map:_d});function bd(e){let t=[];for(let{head:n}of e.state.selection.ranges)t.some(e=>e.from<=n&&e.to>=n)||t.push(e.lineBlockAt(n));return t}var xd=Ye.define({create(){return R.none},update(e,t){t.isUserEvent(`delete`)&&t.changes.iterChangedRanges((t,n)=>e=Sd(e,t,n)),e=e.map(t.changes);let n=[];for(let r of t.effects)r.is(vd)&&!wd(e,r.value.from,r.value.to)?n.push(r.value):r.is(yd)&&(e=e.update({filter:(e,t)=>r.value.from!=e||r.value.to!=t,filterFrom:r.value.from,filterTo:r.value.to}));if(n.length){let{preparePlaceholder:r}=t.state.facet(jd),i=n.map(e=>(r?R.replace({widget:new Fd(r(t.state,e))}):Pd).range(e.from,e.to));e=e.update({add:i})}return t.selection&&(e=Sd(e,t.selection.main.head)),e},provide:e=>G.decorations.from(e),toJSON(e,t){let n=[];return e.between(0,t.doc.length,(e,t)=>{n.push(e,t)}),n},fromJSON(e){if(!Array.isArray(e)||e.length%2)throw RangeError(`Invalid JSON for fold state`);let t=[];for(let n=0;n<e.length;){let r=e[n++],i=e[n++];if(typeof r!=`number`||typeof i!=`number`)throw RangeError(`Invalid JSON for fold state`);t.push(Pd.range(r,i))}return R.set(t,!0)}});function Sd(e,t,n=t){let r=!1;return e.between(t,n,(e,i)=>{e<n&&i>t&&(r=!0)}),r?e.update({filterFrom:t,filterTo:n,filter:(e,r)=>e>=n||r<=t}):e}function Cd(e,t,n){var r;let i=null;return(r=e.field(xd,!1))==null||r.between(t,n,(e,t)=>{(!i||i.from>e)&&(i={from:e,to:t})}),i}function wd(e,t,n){let r=!1;return e.between(t,t,(e,i)=>{e==t&&i==n&&(r=!0)}),r}function Td(e,t){return e.field(xd,!1)?t:t.concat(M.appendConfig.of(Md()))}var Ed=e=>{for(let t of bd(e)){let n=gd(e.state,t.from,t.to);if(n)return e.dispatch({effects:Td(e.state,[vd.of(n),Od(e,n)])}),!0}return!1},Dd=e=>{if(!e.state.field(xd,!1))return!1;let t=[];for(let n of bd(e)){let r=Cd(e.state,n.from,n.to);r&&t.push(yd.of(r),Od(e,r,!1))}return t.length&&e.dispatch({effects:t}),t.length>0};function Od(e,t,n=!0){let r=e.state.doc.lineAt(t.from).number,i=e.state.doc.lineAt(t.to).number;return G.announce.of(`${e.state.phrase(n?`Folded lines`:`Unfolded lines`)} ${r} ${e.state.phrase(`to`)} ${i}.`)}var kd=[{key:`Ctrl-Shift-[`,mac:`Cmd-Alt-[`,run:Ed},{key:`Ctrl-Shift-]`,mac:`Cmd-Alt-]`,run:Dd},{key:`Ctrl-Alt-[`,run:e=>{let{state:t}=e,n=[];for(let r=0;r<t.doc.length;){let i=e.lineBlockAt(r),a=gd(t,i.from,i.to);a&&n.push(vd.of(a)),r=(a?e.lineBlockAt(a.to):i).to+1}return n.length&&e.dispatch({effects:Td(e.state,n)}),!!n.length}},{key:`Ctrl-Alt-]`,run:e=>{let t=e.state.field(xd,!1);if(!t||!t.size)return!1;let n=[];return t.between(0,e.state.doc.length,(e,t)=>{n.push(yd.of({from:e,to:t}))}),e.dispatch({effects:n}),!0}}],Ad={placeholderDOM:null,preparePlaceholder:null,placeholderText:`…`},jd=j.define({combine(e){return kt(e,Ad)}});function Md(e){let t=[xd,zd];return e&&t.push(jd.of(e)),t}function Nd(e,t){let{state:n}=e,r=n.facet(jd),i=t=>{let n=e.lineBlockAt(e.posAtDOM(t.target)),r=Cd(e.state,n.from,n.to);r&&e.dispatch({effects:yd.of(r)}),t.preventDefault()};if(r.placeholderDOM)return r.placeholderDOM(e,i,t);let a=document.createElement(`span`);return a.textContent=r.placeholderText,a.setAttribute(`aria-label`,n.phrase(`folded code`)),a.title=n.phrase(`unfold`),a.className=`cm-foldPlaceholder`,a.onclick=i,a}var Pd=R.replace({widget:new class extends kn{toDOM(e){return Nd(e,null)}}}),Fd=class extends kn{constructor(e){super(),this.value=e}eq(e){return this.value==e.value}toDOM(e){return Nd(e,this.value)}},Id={openText:`⌄`,closedText:`›`,markerDOM:null,domEventHandlers:{},foldingChanged:()=>!1},Ld=class extends el{constructor(e,t){super(),this.config=e,this.open=t}eq(e){return this.config==e.config&&this.open==e.open}toDOM(e){if(this.config.markerDOM)return this.config.markerDOM(this.open);let t=document.createElement(`span`);return t.textContent=this.open?this.config.openText:this.config.closedText,t.title=e.state.phrase(this.open?`Fold line`:`Unfold line`),t}};function Rd(e={}){let t={...Id,...e},n=new Ld(t,!0),r=new Ld(t,!1),i=H.fromClass(class{constructor(e){this.from=e.viewport.from,this.markers=this.buildMarkers(e)}update(e){(e.docChanged||e.viewportChanged||e.startState.facet(qu)!=e.state.facet(qu)||e.startState.field(xd,!1)!=e.state.field(xd,!1)||Ru(e.startState)!=Ru(e.state)||t.foldingChanged(e))&&(this.markers=this.buildMarkers(e.view))}buildMarkers(e){let t=new Lt;for(let i of e.viewportLineBlocks){let a=Cd(e.state,i.from,i.to)?r:gd(e.state,i.from,i.to)?n:null;a&&t.add(i.from,i.from,a)}return t.finish()}}),{domEventHandlers:a}=t;return[i,al({class:`cm-foldGutter`,markers(e){return e.plugin(i)?.markers||F.empty},initialSpacer(){return new Ld(t,!1)},domEventHandlers:{...a,click:(e,t,n)=>{if(a.click&&a.click(e,t,n))return!0;let r=Cd(e.state,t.from,t.to);if(r)return e.dispatch({effects:yd.of(r)}),!0;let i=gd(e.state,t.from,t.to);return i?(e.dispatch({effects:vd.of(i)}),!0):!1}}}),Md()]}var zd=G.baseTheme({".cm-foldPlaceholder":{backgroundColor:`#eee`,border:`1px solid #ddd`,color:`#888`,borderRadius:`.2em`,margin:`0 1px`,padding:`0 1px`,cursor:`pointer`},".cm-foldGutter span":{padding:`0 1px`,cursor:`pointer`}}),Bd=class e{constructor(e,t){this.specs=e;let n;function r(e){let t=en.newName();return(n||=Object.create(null))[`.`+t]=e,t}let i=typeof t.all==`string`?t.all:t.all?r(t.all):void 0,a=t.scope;this.scope=a instanceof Iu?e=>e.prop(Nu)==a.data:a?e=>e==a:void 0,this.style=fu(e.map(e=>({tag:e.tag,class:e.class||r(Object.assign({},e,{tag:null}))})),{all:i}).style,this.module=n?new en(n):null,this.themeType=t.themeType}static define(t,n){return new e(t,n||{})}},Vd=j.define(),Hd=j.define({combine(e){return e.length?[e[0]]:null}});function Ud(e){let t=e.facet(Vd);return t.length?t:e.facet(Hd)}function Wd(e,t){let n=[Kd],r;return e instanceof Bd&&(e.module&&n.push(G.styleModule.of(e.module)),r=e.themeType),t?.fallback?n.push(Hd.of(e)):r?n.push(Vd.computeN([G.darkTheme],t=>t.facet(G.darkTheme)==(r==`dark`)?[e]:[])):n.push(Vd.of(e)),n}var Gd=class{constructor(e){this.markCache=Object.create(null),this.tree=Ru(e.state),this.decorations=this.buildDeco(e,Ud(e.state)),this.decoratedTo=e.viewport.to}update(e){let t=Ru(e.state),n=Ud(e.state),r=n!=Ud(e.startState),{viewport:i}=e.view,a=e.changes.mapPos(this.decoratedTo,1);t.length<i.to&&!r&&t.type==this.tree.type&&a>=i.to?(this.decorations=this.decorations.map(e.changes),this.decoratedTo=a):(t!=this.tree||e.viewportChanged||r)&&(this.tree=t,this.decorations=this.buildDeco(e.view,n),this.decoratedTo=i.to)}buildDeco(e,t){if(!t||!this.tree.length)return R.none;let n=new Lt;for(let{from:r,to:i}of e.visibleRanges)mu(this.tree,t,(e,t,r)=>{n.add(e,t,this.markCache[r]||(this.markCache[r]=R.mark({class:r})))},r,i);return n.finish()}},Kd=Qe.high(H.fromClass(Gd,{decorations:e=>e.decorations})),qd=Bd.define([{tag:X.meta,color:`#404740`},{tag:X.link,textDecoration:`underline`},{tag:X.heading,textDecoration:`underline`,fontWeight:`bold`},{tag:X.emphasis,fontStyle:`italic`},{tag:X.strong,fontWeight:`bold`},{tag:X.strikethrough,textDecoration:`line-through`},{tag:X.keyword,color:`#708`},{tag:[X.atom,X.bool,X.url,X.contentSeparator,X.labelName],color:`#219`},{tag:[X.literal,X.inserted],color:`#164`},{tag:[X.string,X.deleted],color:`#a11`},{tag:[X.regexp,X.escape,X.special(X.string)],color:`#e40`},{tag:X.definition(X.variableName),color:`#00f`},{tag:X.local(X.variableName),color:`#30a`},{tag:[X.typeName,X.namespace],color:`#085`},{tag:X.className,color:`#167`},{tag:[X.special(X.variableName),X.macroName],color:`#256`},{tag:X.definition(X.propertyName),color:`#00c`},{tag:X.comment,color:`#940`},{tag:X.invalid,color:`#f00`}]),Jd=G.baseTheme({"&.cm-focused .cm-matchingBracket":{backgroundColor:`#328c8252`},"&.cm-focused .cm-nonmatchingBracket":{backgroundColor:`#bb555544`}}),Yd=1e4,Xd=`()[]{}`,Zd=j.define({combine(e){return kt(e,{afterCursor:!0,brackets:Xd,maxScanDistance:Yd,renderMatch:ef})}}),Qd=R.mark({class:`cm-matchingBracket`}),$d=R.mark({class:`cm-nonmatchingBracket`});function ef(e){let t=[],n=e.matched?Qd:$d;return t.push(n.range(e.start.from,e.start.to)),e.end&&t.push(n.range(e.end.from,e.end.to)),t}function tf(e){let t=[],n=e.facet(Zd);for(let r of e.selection.ranges){if(!r.empty)continue;let i=cf(e,r.head,-1,n)||r.head>0&&cf(e,r.head-1,1,n)||n.afterCursor&&(cf(e,r.head,1,n)||r.head<e.doc.length&&cf(e,r.head+1,-1,n));i&&(t=t.concat(n.renderMatch(i,e)))}return R.set(t,!0)}var nf=[H.fromClass(class{constructor(e){this.paused=!1,this.decorations=tf(e.state)}update(e){(e.docChanged||e.selectionSet||this.paused)&&(e.view.composing?(this.decorations=this.decorations.map(e.changes),this.paused=!0):(this.decorations=tf(e.state),this.paused=!1))}},{decorations:e=>e.decorations}),Jd];function rf(e={}){return[Zd.of(e),nf]}var af=new K;function of(e,t,n){let r=e.prop(t<0?K.openedBy:K.closedBy);if(r)return r;if(e.name.length==1){let r=n.indexOf(e.name);if(r>-1&&r%2==+(t<0))return[n[r+t]]}return null}function sf(e){let t=e.type.prop(af);return t?t(e.node):e}function cf(e,t,n,r={}){let i=r.maxScanDistance||Yd,a=r.brackets||Xd,o=Ru(e),s=o.resolveInner(t,n);for(let r=s;r;r=r.parent){let i=of(r.type,n,a);if(i&&r.from<r.to){let o=sf(r);if(o&&(n>0?t>=o.from&&t<o.to:t>o.from&&t<=o.to))return lf(e,t,n,r,o,i,a)}}return uf(e,t,n,o,s.type,i,a)}function lf(e,t,n,r,i,a,o){let s=r.parent,c={from:i.from,to:i.to},l=0,u=s?.cursor();if(u&&(n<0?u.childBefore(r.from):u.childAfter(r.to)))do if(n<0?u.to<=r.from:u.from>=r.to){if(l==0&&a.indexOf(u.type.name)>-1&&u.from<u.to){let e=sf(u);return{start:c,end:e?{from:e.from,to:e.to}:void 0,matched:!0}}if(of(u.type,n,o))l++;else if(of(u.type,-n,o)){if(l==0){let e=sf(u);return{start:c,end:e&&e.from<e.to?{from:e.from,to:e.to}:void 0,matched:!1}}l--}}while(n<0?u.prevSibling():u.nextSibling());return{start:c,matched:!1}}function uf(e,t,n,r,i,a,o){if(n<0?!t:t==e.doc.length)return null;let s=n<0?e.sliceDoc(t-1,t):e.sliceDoc(t,t+1),c=o.indexOf(s);if(c<0||c%2==0!=n>0)return null;let l={from:n<0?t-1:t,to:n>0?t+1:t},u=e.doc.iterRange(t,n>0?e.doc.length:0),d=0;for(let e=0;!u.next().done&&e<=a;){let a=u.value;n<0&&(e+=a.length);let s=t+e*n;for(let e=n>0?0:a.length-1,t=n>0?a.length:-1;e!=t;e+=n){let t=o.indexOf(a[e]);if(!(t<0||r.resolveInner(s+e,1).type!=i)){if(t%2==0==n>0)d++;else if(d==1)return{start:l,end:{from:s+e,to:s+e+1},matched:t>>1==c>>1};else d--}}n>0&&(e+=a.length)}return u.done?{start:l,matched:!1}:null}function df(e){return{name:e.name||``,token:e.token,blankLine:e.blankLine||(()=>{}),startState:e.startState||(()=>!0),copyState:e.copyState||ff,indent:e.indent||(()=>null),languageData:e.languageData||{},tokenTable:e.tokenTable||bf,mergeTokens:e.mergeTokens!==!1}}function ff(e){if(typeof e!=`object`)return e;let t={};for(let n in e){let r=e[n];t[n]=r instanceof Array?r.slice():r}return t}var pf=new WeakMap,mf=class e extends Iu{constructor(e){let t=Pu(e.languageData),n=df(e),r,i=new class extends tu{createParse(e,t,n){return new vf(r,e,t,n)}};super(t,i,[],e.name),this.topNode=Af(t,this),r=this,this.streamParser=n,this.stateAfter=new K({perNode:!0}),this.tokenTable=e.tokenTable?new Ef(n.tokenTable):Df}static define(t){return new e(t)}getIndent(e){let t,{overrideIndentation:n}=e.options;n&&(t=pf.get(e.state),t!=null&&t<e.pos-1e4&&(t=void 0));let r=hf(this,e.node.tree,e.node.from,e.node.from,t??e.pos),i,a;if(r?(a=r.state,i=r.pos+1):(a=this.streamParser.startState(e.unit),i=e.node.from),e.pos-i>1e4)return null;for(;i<e.pos;){let t=e.state.doc.lineAt(i),r=Math.min(e.pos,t.to);if(t.length){let i=n?n(t.from):-1,o=new Mu(t.text,e.state.tabSize,e.unit,i<0?void 0:i);for(;o.pos<r-t.from;)yf(this.streamParser.token,o,a)}else this.streamParser.blankLine(a,e.unit);if(r==e.pos)break;i=t.to+1}let o=e.lineAt(e.pos);return n&&t==null&&pf.set(e.state,o.from),this.streamParser.indent(a,/^\s*(.*)/.exec(o.text)[1],e)}get allowsNesting(){return!1}};function hf(e,t,n,r,i){let a=n>=r&&n+t.length<=i&&t.prop(e.stateAfter);if(a)return{state:e.streamParser.copyState(a),pos:n+t.length};for(let a=t.children.length-1;a>=0;a--){let o=t.children[a],s=n+t.positions[a],c=o instanceof J&&s<i&&hf(e,o,s,r,i);if(c)return c}return null}function gf(e,t,n,r,i){if(i&&n<=0&&r>=t.length)return t;!i&&n==0&&t.type==e.topNode&&(i=!0);for(let a=t.children.length-1;a>=0;a--){let o=t.positions[a],s=t.children[a],c;if(o<r&&s instanceof J){if(!(c=gf(e,s,n-o,r-o,i)))break;return i?new J(t.type,t.children.slice(0,a).concat(c),t.positions.slice(0,a+1),o+c.length):c}}return null}function _f(e,t,n,r,i){for(let i of t){let t=i.from+(i.openStart?25:0),a=i.to-(i.openEnd?25:0),o=t<=n&&a>n&&hf(e,i.tree,0-i.offset,n,a),s;if(o&&o.pos<=r&&(s=gf(e,i.tree,n+i.offset,o.pos+i.offset,!1)))return{state:o.state,tree:s}}return{state:e.streamParser.startState(i?Xu(i):4),tree:J.empty}}var vf=class{constructor(e,t,n,r){this.lang=e,this.input=t,this.fragments=n,this.ranges=r,this.stoppedAt=null,this.chunks=[],this.chunkPos=[],this.chunk=[],this.chunkReused=void 0,this.rangeIndex=0,this.to=r[r.length-1].to;let i=Vu.get(),a=r[0].from,{state:o,tree:s}=_f(e,n,a,this.to,i?.state);this.state=o,this.parsedPos=this.chunkStart=a+s.length;for(let e=0;e<s.children.length;e++)this.chunks.push(s.children[e]),this.chunkPos.push(s.positions[e]);i&&this.parsedPos<i.viewport.from-1e5&&r.some(e=>e.from<=i.viewport.from&&e.to>=i.viewport.from)&&(this.state=this.lang.streamParser.startState(Xu(i.state)),i.skipUntilInView(this.parsedPos,i.viewport.from),this.parsedPos=i.viewport.from),this.moveRangeIndex()}advance(){let e=Vu.get(),t=this.stoppedAt==null?this.to:Math.min(this.to,this.stoppedAt),n=Math.min(t,this.chunkStart+512);for(e&&(n=Math.min(n,e.viewport.to));this.parsedPos<n;)this.parseLine(e);return this.chunkStart<this.parsedPos&&this.finishChunk(),this.parsedPos>=t?this.finish():e&&this.parsedPos>=e.viewport.to?(e.skipUntilInView(this.parsedPos,t),this.finish()):null}stopAt(e){this.stoppedAt=e}lineAfter(e){let t=this.input.chunk(e);if(this.input.lineChunks)t==`
`&&(t=``);else{let e=t.indexOf(`
`);e>-1&&(t=t.slice(0,e))}return e+t.length<=this.to?t:t.slice(0,this.to-e)}nextLine(){let e=this.parsedPos,t=this.lineAfter(e),n=e+t.length;for(let e=this.rangeIndex;;){let r=this.ranges[e].to;if(r>=n||(t=t.slice(0,r-(n-t.length)),e++,e==this.ranges.length))break;let i=this.ranges[e].from,a=this.lineAfter(i);t+=a,n=i+a.length}return{line:t,end:n}}skipGapsTo(e,t,n){for(;;){let r=this.ranges[this.rangeIndex].to,i=e+t;if(n>0?r>i:r>=i)break;let a=this.ranges[++this.rangeIndex].from;t+=a-r}return t}moveRangeIndex(){for(;this.ranges[this.rangeIndex].to<this.parsedPos;)this.rangeIndex++}emitToken(e,t,n,r){let i=4;if(this.ranges.length>1){r=this.skipGapsTo(t,r,1),t+=r;let e=this.chunk.length;r=this.skipGapsTo(n,r,-1),n+=r,i+=this.chunk.length-e}let a=this.chunk.length-4;return this.lang.streamParser.mergeTokens&&i==4&&a>=0&&this.chunk[a]==e&&this.chunk[a+2]==t?this.chunk[a+2]=n:this.chunk.push(e,t,n,i),r}parseLine(e){let{line:t,end:n}=this.nextLine(),r=0,{streamParser:i}=this.lang,a=new Mu(t,e?e.state.tabSize:4,e?Xu(e.state):2);if(a.eol())i.blankLine(this.state,a.indentUnit);else for(;!a.eol();){let e=yf(i.token,a,this.state);if(e&&(r=this.emitToken(this.lang.tokenTable.resolve(e),this.parsedPos+a.start,this.parsedPos+a.pos,r)),a.start>1e4)break}this.parsedPos=n,this.moveRangeIndex(),this.parsedPos<this.to&&this.parsedPos++}finishChunk(){let e=J.build({buffer:this.chunk,start:this.chunkStart,length:this.parsedPos-this.chunkStart,nodeSet:Sf,topID:0,maxBufferLength:512,reused:this.chunkReused});e=new J(e.type,e.children,e.positions,e.length,[[this.lang.stateAfter,this.lang.streamParser.copyState(this.state)]]),this.chunks.push(e),this.chunkPos.push(this.chunkStart-this.ranges[0].from),this.chunk=[],this.chunkReused=void 0,this.chunkStart=this.parsedPos}finish(){return new J(this.lang.topNode,this.chunks,this.chunkPos,this.parsedPos-this.ranges[0].from).balance()}};function yf(e,t,n){t.start=t.pos;for(let r=0;r<10;r++){let r=e(t,n);if(t.pos>t.start)return r}throw Error(`Stream parser failed to advance stream.`)}var bf=Object.create(null),xf=[jl.none],Sf=new Ml(xf),Cf=[],wf=Object.create(null),Tf=Object.create(null);for(let[e,t]of[[`variable`,`variableName`],[`variable-2`,`variableName.special`],[`string-2`,`string.special`],[`def`,`variableName.definition`],[`tag`,`tagName`],[`attribute`,`attributeName`],[`type`,`typeName`],[`builtin`,`variableName.standard`],[`qualifier`,`modifier`],[`error`,`invalid`],[`header`,`heading`],[`property`,`propertyName`]])Tf[e]=kf(bf,t);var Ef=class{constructor(e){this.extra=e,this.table=Object.assign(Object.create(null),Tf)}resolve(e){return e?this.table[e]||(this.table[e]=kf(this.extra,e)):0}},Df=new Ef(bf);function Of(e,t){Cf.indexOf(e)>-1||(Cf.push(e),console.warn(t))}function kf(e,t){let n=[];for(let r of t.split(` `)){let t=[];for(let n of r.split(`.`)){let r=e[n]||X[n];r?typeof r==`function`?t.length?t=t.map(r):Of(n,`Modifier ${n} used at start of tag`):t.length?Of(n,`Tag ${n} used as modifier`):t=Array.isArray(r)?r:[r]:Of(n,`Unknown highlighting tag ${n}`)}for(let e of t)n.push(e)}if(!n.length)return 0;let r=t.replace(/ /g,`_`),i=r+` `+n.map(e=>e.id),a=wf[i];if(a)return a.id;let o=wf[i]=jl.define({id:xf.length,name:r,props:[lu({[r]:n})]});return xf.push(o),o.id}function Af(e,t){let n=jl.define({id:xf.length,name:`Document`,props:[Nu.add(()=>e),ed.add(()=>e=>t.getIndent(e))],top:!0});return xf.push(n),n}B.RTL,B.LTR;var jf=e=>{let{state:t}=e,n=t.doc.lineAt(t.selection.main.from),r=If(e.state,n.from);return r.line?Nf(e):r.block?Ff(e):!1};function Mf(e,t){return({state:n,dispatch:r})=>{if(n.readOnly)return!1;let i=e(t,n);return i?(r(n.update(i)),!0):!1}}var Nf=Mf(Vf,0),Pf=Mf(Bf,0),Ff=Mf((e,t)=>Bf(e,t,zf(t)),0);function If(e,t){let n=e.languageDataAt(`commentTokens`,t,1);return n.length?n[0]:{}}var Lf=50;function Rf(e,{open:t,close:n},r,i){let a=e.sliceDoc(r-Lf,r),o=e.sliceDoc(i,i+Lf),s=/\s*$/.exec(a)[0].length,c=/^\s*/.exec(o)[0].length,l=a.length-s;if(a.slice(l-t.length,l)==t&&o.slice(c,c+n.length)==n)return{open:{pos:r-s,margin:s&&1},close:{pos:i+c,margin:c&&1}};let u,d;i-r<=100?u=d=e.sliceDoc(r,i):(u=e.sliceDoc(r,r+Lf),d=e.sliceDoc(i-Lf,i));let f=/^\s*/.exec(u)[0].length,p=/\s*$/.exec(d)[0].length,m=d.length-p-n.length;return u.slice(f,f+t.length)==t&&d.slice(m,m+n.length)==n?{open:{pos:r+f+t.length,margin:+!!/\s/.test(u.charAt(f+t.length))},close:{pos:i-p-n.length,margin:+!!/\s/.test(d.charAt(m-1))}}:null}function zf(e){let t=[];for(let n of e.selection.ranges){let r=e.doc.lineAt(n.from),i=n.to<=r.to?r:e.doc.lineAt(n.to);i.from>r.from&&i.from==n.to&&(i=n.to==r.to+1?r:e.doc.lineAt(n.to-1));let a=t.length-1;a>=0&&t[a].to>r.from?t[a].to=i.to:t.push({from:r.from+/^\s*/.exec(r.text)[0].length,to:i.to})}return t}function Bf(e,t,n=t.selection.ranges){let r=n.map(e=>If(t,e.from).block);if(!r.every(e=>e))return null;let i=n.map((e,n)=>Rf(t,r[n],e.from,e.to));if(e!=2&&!i.every(e=>e))return{changes:t.changes(n.map((e,t)=>i[t]?[]:[{from:e.from,insert:r[t].open+` `},{from:e.to,insert:` `+r[t].close}]))};if(e!=1&&i.some(e=>e)){let e=[];for(let t=0,n;t<i.length;t++)if(n=i[t]){let i=r[t],{open:a,close:o}=n;e.push({from:a.pos-i.open.length,to:a.pos+a.margin},{from:o.pos-o.margin,to:o.pos+i.close.length})}return{changes:e}}return null}function Vf(e,t,n=t.selection.ranges){let r=[],i=-1;ranges:for(let{from:e,to:a}of n){let n=r.length,o=1e9,s;for(let n=e;n<=a;){let c=t.doc.lineAt(n);if(s==null&&(s=If(t,c.from).line,!s))continue ranges;if(c.from>i&&(e==a||a>c.from)){i=c.from;let e=/^\s*/.exec(c.text)[0].length,t=e==c.length,n=c.text.slice(e,e+s.length)==s?e:-1;e<c.text.length&&e<o&&(o=e),r.push({line:c,comment:n,token:s,indent:e,empty:t,single:!1})}n=c.to+1}if(o<1e9)for(let e=n;e<r.length;e++)r[e].indent<r[e].line.text.length&&(r[e].indent=o);r.length==n+1&&(r[n].single=!0)}if(e!=2&&r.some(e=>e.comment<0&&(!e.empty||e.single))){let e=[];for(let{line:t,token:n,indent:i,empty:a,single:o}of r)(o||!a)&&e.push({from:t.from+i,insert:n+` `});let n=t.changes(e);return{changes:n,selection:t.selection.map(n,1)}}if(e!=1&&r.some(e=>e.comment>=0)){let e=[];for(let{line:t,comment:n,token:i}of r)if(n>=0){let r=t.from+n,a=r+i.length;t.text[a-t.from]==` `&&a++,e.push({from:r,to:a})}return{changes:e}}return null}var Hf=pt.define(),Uf=pt.define(),Wf=j.define(),Gf=j.define({combine(e){return kt(e,{minDepth:100,newGroupDelay:500,joinToEvent:(e,t)=>t},{minDepth:Math.max,newGroupDelay:Math.min,joinToEvent:(e,t)=>(n,r)=>e(n,r)||t(n,r)})}}),Kf=Ye.define({create(){return dp.empty},update(e,t){let n=t.state.facet(Gf),r=t.annotation(Hf);if(r){let i=$f.fromTransaction(t,r.selection),a=r.side,o=a==0?e.undone:e.done;return o=i?ep(o,o.length,n.minDepth,i):op(o,t.startState.selection),new dp(a==0?r.rest:o,a==0?o:r.rest)}let i=t.annotation(Uf);if((i==`full`||i==`before`)&&(e=e.isolate()),t.annotation(gt.addToHistory)===!1)return t.changes.empty?e:e.addMapping(t.changes.desc);let a=$f.fromTransaction(t),o=t.annotation(gt.time),s=t.annotation(gt.userEvent);return a?e=e.addChanges(a,o,s,n,t):t.selection&&(e=e.addSelection(t.startState.selection,o,s,n.newGroupDelay)),(i==`full`||i==`after`)&&(e=e.isolate()),e},toJSON(e){return{done:e.done.map(e=>e.toJSON()),undone:e.undone.map(e=>e.toJSON())}},fromJSON(e){return new dp(e.done.map($f.fromJSON),e.undone.map($f.fromJSON))}});function qf(e={}){return[Kf,Gf.of(e),G.domEventHandlers({beforeinput(e,t){let n=e.inputType==`historyUndo`?Yf:e.inputType==`historyRedo`?Xf:null;return n?(e.preventDefault(),n(t)):!1}})]}function Jf(e,t){return function({state:n,dispatch:r}){if(!t&&n.readOnly)return!1;let i=n.field(Kf,!1);if(!i)return!1;let a=i.pop(e,n,t);return a?(r(a),!0):!1}}var Yf=Jf(0,!1),Xf=Jf(1,!1),Zf=Jf(0,!0),Qf=Jf(1,!0),$f=class e{constructor(e,t,n,r,i){this.changes=e,this.effects=t,this.mapped=n,this.startSelection=r,this.selectionsAfter=i}setSelAfter(t){return new e(this.changes,this.effects,this.mapped,this.startSelection,t)}toJSON(){return{changes:this.changes?.toJSON(),mapped:this.mapped?.toJSON(),startSelection:this.startSelection?.toJSON(),selectionsAfter:this.selectionsAfter.map(e=>e.toJSON())}}static fromJSON(t){return new e(t.changes&&Pe.fromJSON(t.changes),[],t.mapped&&Ne.fromJSON(t.mapped),t.startSelection&&A.fromJSON(t.startSelection),t.selectionsAfter.map(A.fromJSON))}static fromTransaction(t,n){let r=ip;for(let e of t.startState.facet(Wf)){let n=e(t);n.length&&(r=r.concat(n))}return!r.length&&t.changes.empty?null:new e(t.changes.invert(t.startState.doc),r,void 0,n||t.startState.selection,ip)}static selection(t){return new e(void 0,ip,void 0,void 0,t)}};function ep(e,t,n,r){let i=t+1>n+20?t-n-1:0,a=e.slice(i,t);return a.push(r),a}function tp(e,t){let n=[],r=!1;return e.iterChangedRanges((e,t)=>n.push(e,t)),t.iterChangedRanges((e,t,i,a)=>{for(let e=0;e<n.length;){let t=n[e++],o=n[e++];a>=t&&i<=o&&(r=!0)}}),r}function np(e,t){return e.ranges.length==t.ranges.length&&e.ranges.filter((e,n)=>e.empty!=t.ranges[n].empty).length===0}function rp(e,t){return e.length?t.length?e.concat(t):e:t}var ip=[],ap=200;function op(e,t){if(e.length){let n=e[e.length-1],r=n.selectionsAfter.slice(Math.max(0,n.selectionsAfter.length-ap));return r.length&&r[r.length-1].eq(t)?e:(r.push(t),ep(e,e.length-1,1e9,n.setSelAfter(r)))}return[$f.selection([t])]}function sp(e){let t=e[e.length-1],n=e.slice();return n[e.length-1]=t.setSelAfter(t.selectionsAfter.slice(0,t.selectionsAfter.length-1)),n}function cp(e,t){if(!e.length)return e;let n=e.length,r=ip;for(;n;){let i=lp(e[n-1],t,r);if(i.changes&&!i.changes.empty||i.effects.length){let t=e.slice(0,n);return t[n-1]=i,t}t=i.mapped,n--,r=i.selectionsAfter}return r.length?[$f.selection(r)]:ip}function lp(e,t,n){let r=rp(e.selectionsAfter.length?e.selectionsAfter.map(e=>e.map(t)):ip,n);if(!e.changes)return $f.selection(r);let i=e.changes.map(t),a=t.mapDesc(e.changes,!0),o=e.mapped?e.mapped.composeDesc(a):a;return new $f(i,M.mapEffects(e.effects,t),o,e.startSelection.map(a),r)}var up=/^(input\.type|delete)($|\.)/,dp=class e{constructor(e,t,n=0,r=void 0){this.done=e,this.undone=t,this.prevTime=n,this.prevUserEvent=r}isolate(){return this.prevTime?new e(this.done,this.undone):this}addChanges(t,n,r,i,a){let o=this.done,s=o[o.length-1];return o=s&&s.changes&&!s.changes.empty&&t.changes&&(!r||up.test(r))&&(!s.selectionsAfter.length&&n-this.prevTime<i.newGroupDelay&&i.joinToEvent(a,tp(s.changes,t.changes))||r==`input.type.compose`)?ep(o,o.length-1,i.minDepth,new $f(t.changes.compose(s.changes),rp(M.mapEffects(t.effects,s.changes),s.effects),s.mapped,s.startSelection,ip)):ep(o,o.length,i.minDepth,t),new e(o,ip,n,r)}addSelection(t,n,r,i){let a=this.done.length?this.done[this.done.length-1].selectionsAfter:ip;return a.length>0&&n-this.prevTime<i&&r==this.prevUserEvent&&r&&/^select($|\.)/.test(r)&&np(a[a.length-1],t)?this:new e(op(this.done,t),this.undone,n,r)}addMapping(t){return new e(cp(this.done,t),cp(this.undone,t),this.prevTime,this.prevUserEvent)}pop(e,t,n){let r=e==0?this.done:this.undone;if(r.length==0)return null;let i=r[r.length-1],a=i.selectionsAfter[0]||(i.startSelection?i.startSelection.map(i.changes.invertedDesc,1):t.selection);if(n&&i.selectionsAfter.length)return t.update({selection:i.selectionsAfter[i.selectionsAfter.length-1],annotations:Hf.of({side:e,rest:sp(r),selection:a}),userEvent:e==0?`select.undo`:`select.redo`,scrollIntoView:!0});if(i.changes){let n=r.length==1?ip:r.slice(0,r.length-1);return i.mapped&&(n=cp(n,i.mapped)),t.update({changes:i.changes,selection:i.startSelection,effects:i.effects,annotations:Hf.of({side:e,rest:n,selection:a}),filter:!1,userEvent:e==0?`undo`:`redo`,scrollIntoView:!0})}return null}};dp.empty=new dp(ip,ip);var fp=[{key:`Mod-z`,run:Yf,preventDefault:!0},{key:`Mod-y`,mac:`Mod-Shift-z`,run:Xf,preventDefault:!0},{linux:`Ctrl-Shift-z`,run:Xf,preventDefault:!0},{key:`Mod-u`,run:Zf,preventDefault:!0},{key:`Alt-u`,mac:`Mod-Shift-u`,run:Qf,preventDefault:!0}];function pp(e,t){return A.create(e.ranges.map(t),e.mainIndex)}function mp(e,t){return e.update({selection:t,scrollIntoView:!0,userEvent:`select`})}function hp({state:e,dispatch:t},n){let r=pp(e.selection,n);return!r.eq(e.selection,!0)&&(t(mp(e,r)),!0)}function gp(e,t){return A.cursor(t?e.to:e.from)}function _p(e,t){return hp(e,n=>n.empty?e.moveByChar(n,t):gp(n,t))}function vp(e){return e.textDirectionAt(e.state.selection.main.head)==B.LTR}var yp=e=>_p(e,!vp(e)),bp=e=>_p(e,vp(e));function xp(e,t){return hp(e,n=>n.empty?e.moveByGroup(n,t):gp(n,t))}var Sp=e=>xp(e,!vp(e)),Cp=e=>xp(e,vp(e));typeof Intl<`u`&&Intl.Segmenter;function wp(e,t,n){if(t.type.prop(n))return!0;let r=t.to-t.from;return r&&(r>2||/[^\s,.;:]/.test(e.sliceDoc(t.from,t.to)))||t.firstChild}function Tp(e,t,n){let r=Ru(e).resolveInner(t.head),i=n?K.closedBy:K.openedBy;for(let a=t.head;;){let t=n?r.childAfter(a):r.childBefore(a);if(!t)break;wp(e,t,i)?r=t:a=n?t.to:t.from}let a=r.type.prop(i),o,s;return s=a&&(o=n?cf(e,r.from,1):cf(e,r.to,-1))&&o.matched?n?o.end.to:o.end.from:n?r.to:r.from,A.cursor(s,n?-1:1)}var Ep=e=>hp(e,t=>Tp(e.state,t,!vp(e))),Dp=e=>hp(e,t=>Tp(e.state,t,vp(e)));function Op(e,t){return hp(e,n=>{if(!n.empty)return gp(n,t);let r=e.moveVertically(n,t);return r.head==n.head?e.moveToLineBoundary(n,t):r})}var kp=e=>Op(e,!1),Ap=e=>Op(e,!0);function jp(e){let t=e.scrollDOM.clientHeight<e.scrollDOM.scrollHeight-2,n=0,r=0,i;if(t){for(let t of e.state.facet(G.scrollMargins)){let i=t(e);i?.top&&(n=Math.max(i?.top,n)),i?.bottom&&(r=Math.max(i?.bottom,r))}i=e.scrollDOM.clientHeight-n-r}else i=(e.dom.ownerDocument.defaultView||window).innerHeight;return{marginTop:n,marginBottom:r,selfScroll:t,height:Math.max(e.defaultLineHeight,i-5)}}function Mp(e,t){let n=jp(e),{state:r}=e,i=pp(r.selection,r=>r.empty?e.moveVertically(r,t,n.height):gp(r,t));if(i.eq(r.selection))return!1;let a;if(n.selfScroll){let t=e.coordsAtPos(r.selection.main.head),o=e.scrollDOM.getBoundingClientRect(),s=o.top+n.marginTop,c=o.bottom-n.marginBottom;t&&t.top>s&&t.bottom<c&&(a=G.scrollIntoView(i.main.head,{y:`start`,yMargin:t.top-s}))}return e.dispatch(mp(r,i),{effects:a}),!0}var Np=e=>Mp(e,!1),Pp=e=>Mp(e,!0);function Fp(e,t,n){let r=e.lineBlockAt(t.head),i=e.moveToLineBoundary(t,n);if(i.head==t.head&&i.head!=(n?r.to:r.from)&&(i=e.moveToLineBoundary(t,n,!1)),!n&&i.head==r.from&&r.length){let n=/^\s*/.exec(e.state.sliceDoc(r.from,Math.min(r.from+100,r.to)))[0].length;n&&t.head!=r.from+n&&(i=A.cursor(r.from+n))}return i}var Ip=e=>hp(e,t=>Fp(e,t,!0)),Lp=e=>hp(e,t=>Fp(e,t,!1)),Rp=e=>hp(e,t=>Fp(e,t,!vp(e))),zp=e=>hp(e,t=>Fp(e,t,vp(e))),Bp=e=>hp(e,t=>e.moveToLineBoundary(t,!1,!1)),Vp=e=>hp(e,t=>e.moveToLineBoundary(t,!0,!1));function Hp(e,t,n){let r=!1,i=pp(e.selection,t=>{let i=cf(e,t.head,-1)||cf(e,t.head,1)||t.head>0&&cf(e,t.head-1,1)||t.head<e.doc.length&&cf(e,t.head+1,-1);if(!i||!i.end)return t;r=!0;let a=i.start.from==t.head?i.end.to:i.end.from;return n?A.range(t.anchor,a):A.cursor(a)});return r?(t(mp(e,i)),!0):!1}var Up=({state:e,dispatch:t})=>Hp(e,t,!1);function Wp(e,t,n){let r=pp(e.state.selection,e=>{e.undirectional&&e.head>=e.anchor!=t&&(e=A.range(e.head,e.anchor));let r=n(e);return A.range(e.anchor,r.head,r.goalColumn,r.bidiLevel||void 0,r.assoc)});return!r.eq(e.state.selection)&&(e.dispatch(mp(e.state,r)),!0)}function Gp(e,t){return Wp(e,t,n=>e.moveByChar(n,t))}var Kp=e=>Gp(e,!vp(e)),qp=e=>Gp(e,vp(e));function Jp(e,t){return Wp(e,t,n=>e.moveByGroup(n,t))}var Yp=e=>Jp(e,!vp(e)),Xp=e=>Jp(e,vp(e)),Zp=e=>{let t=!vp(e);return Wp(e,t,n=>Tp(e.state,n,t))},Qp=e=>{let t=vp(e);return Wp(e,t,n=>Tp(e.state,n,t))};function $p(e,t){return Wp(e,t,n=>e.moveVertically(n,t))}var em=e=>$p(e,!1),tm=e=>$p(e,!0);function nm(e,t){return Wp(e,t,n=>e.moveVertically(n,t,jp(e).height))}var rm=e=>nm(e,!1),im=e=>nm(e,!0),am=e=>Wp(e,!0,t=>Fp(e,t,!0)),om=e=>Wp(e,!1,t=>Fp(e,t,!1)),sm=e=>{let t=!vp(e);return Wp(e,t,n=>Fp(e,n,t))},cm=e=>{let t=vp(e);return Wp(e,t,n=>Fp(e,n,t))},lm=e=>Wp(e,!1,t=>A.cursor(e.lineBlockAt(t.head).from)),um=e=>Wp(e,!0,t=>A.cursor(e.lineBlockAt(t.head).to)),dm=({state:e,dispatch:t})=>(t(mp(e,{anchor:0})),!0),fm=({state:e,dispatch:t})=>(t(mp(e,{anchor:e.doc.length})),!0),pm=({state:e,dispatch:t})=>(t(mp(e,{anchor:e.selection.main.anchor,head:0})),!0),mm=({state:e,dispatch:t})=>(t(mp(e,{anchor:e.selection.main.anchor,head:e.doc.length})),!0),hm=({state:e,dispatch:t})=>(t(e.update({selection:{anchor:0,head:e.doc.length},userEvent:`select`})),!0),gm=({state:e,dispatch:t})=>{let n=Fm(e).map(({from:t,to:n})=>A.undirectionalRange(t,Math.min(n+1,e.doc.length)));return t(e.update({selection:A.create(n),userEvent:`select`})),!0},_m=({state:e,dispatch:t})=>{let n=pp(e.selection,t=>{let n=Ru(e),r=n.resolveStack(t.from,1);if(t.empty){let e=n.resolveStack(t.from,-1);e.node.from>=r.node.from&&e.node.to<=r.node.to&&(r=e)}for(let e=r;e;e=e.next){let{node:n}=e;if((n.from<t.from&&n.to>=t.to||n.to>t.to&&n.from<=t.from)&&e.next)return A.undirectionalRange(n.from,n.to)}return t});return!n.eq(e.selection)&&(t(mp(e,n)),!0)};function vm(e,t){let{state:n}=e,r=n.selection,i=n.selection.ranges.slice();for(let r of n.selection.ranges){let a=n.doc.lineAt(r.head);if(t?a.to<e.state.doc.length:a.from>0)for(let n=r;;){let r=e.moveVertically(n,t);if(r.head<a.from||r.head>a.to){i.some(e=>e.head==r.head)||i.push(r);break}if(r.head==n.head)break;n=r}}return i.length!=r.ranges.length&&(e.dispatch(mp(n,A.create(i,i.length-1))),!0)}var ym=e=>vm(e,!1),bm=e=>vm(e,!0),xm=({state:e,dispatch:t})=>{let n=e.selection,r=null;return n.ranges.length>1?r=A.create([n.main]):n.main.empty||(r=A.create([A.cursor(n.main.head)])),r?(t(mp(e,r)),!0):!1};function Sm(e,t){if(e.state.readOnly)return!1;let n=`delete.selection`,{state:r}=e,i=r.changeByRange(r=>{let{from:i,to:a}=r;if(i==a){let o=t(r);o<i?(n=`delete.backward`,o=Cm(e,o,!1)):o>i&&(n=`delete.forward`,o=Cm(e,o,!0)),i=Math.min(i,o),a=Math.max(a,o)}else i=Cm(e,i,!1),a=Cm(e,a,!0);return i==a?{range:r}:{changes:{from:i,to:a},range:A.cursor(i,i<r.head?-1:1)}});return!i.changes.empty&&(e.dispatch(r.update(i,{scrollIntoView:!0,userEvent:n,effects:n==`delete.selection`?G.announce.of(r.phrase(`Selection deleted`)):void 0})),!0)}function Cm(e,t,n){if(e instanceof G)for(let r of e.state.facet(G.atomicRanges).map(t=>t(e)))r.between(t,t,(e,r)=>{e<t&&r>t&&(t=n?r:e)});return t}var wm=(e,t,n)=>Sm(e,r=>{let i=r.from,{state:a}=e,o=a.doc.lineAt(i),s,c;if(n&&!t&&i>o.from&&i<o.from+200&&!/[^ \t]/.test(s=o.text.slice(0,i-o.from))){if(s[s.length-1]==`	`)return i-1;let e=Jt(s,a.tabSize)%Xu(a)||Xu(a);for(let t=0;t<e&&s[s.length-1-t]==` `;t++)i--;c=i}else c=O(o.text,i-o.from,t,t)+o.from,c==i&&o.number!=(t?a.doc.lines:1)?c+=t?1:-1:!t&&/[\ufe00-\ufe0f]/.test(o.text.slice(c-o.from,i-o.from))&&(c=O(o.text,c-o.from,!1,!1)+o.from);return c}),Tm=e=>wm(e,!1,!0),Em=e=>wm(e,!0,!1),Dm=(e,t)=>Sm(e,n=>{let r=n.head,{state:i}=e,a=i.doc.lineAt(r),o=i.charCategorizer(r);for(let e=null;;){if(r==(t?a.to:a.from)){r==n.head&&a.number!=(t?i.doc.lines:1)&&(r+=t?1:-1);break}let s=O(a.text,r-a.from,t)+a.from,c=a.text.slice(Math.min(r,s)-a.from,Math.max(r,s)-a.from),l=o(c);if(e!=null&&l!=e)break;(c!=` `||r!=n.head)&&(e=l),r=s}return r}),Om=e=>Dm(e,!1),km=e=>Dm(e,!0),Am=e=>Sm(e,t=>{let n=e.lineBlockAt(t.head).to;return t.head<n?n:Math.min(e.state.doc.length,t.head+1)}),jm=e=>Sm(e,t=>{let n=e.moveToLineBoundary(t,!1).head;return t.head>n?n:Math.max(0,t.head-1)}),Mm=e=>Sm(e,t=>{let n=e.moveToLineBoundary(t,!0).head;return t.head<n?n:Math.min(e.state.doc.length,t.head+1)}),Nm=({state:e,dispatch:t})=>{if(e.readOnly)return!1;let n=e.changeByRange(e=>({changes:{from:e.from,to:e.to,insert:E.of([``,``])},range:A.cursor(e.from)}));return t(e.update(n,{scrollIntoView:!0,userEvent:`input`})),!0},Pm=({state:e,dispatch:t})=>{if(e.readOnly)return!1;let n=e.changeByRange(t=>{if(!t.empty||t.from==0||t.from==e.doc.length)return{range:t};let n=t.from,r=e.doc.lineAt(n),i=n==r.from?n-1:O(r.text,n-r.from,!1)+r.from,a=n==r.to?n+1:O(r.text,n-r.from,!0)+r.from;return{changes:{from:i,to:a,insert:e.doc.slice(n,a).append(e.doc.slice(i,n))},range:A.cursor(a)}});return!n.changes.empty&&(t(e.update(n,{scrollIntoView:!0,userEvent:`move.character`})),!0)};function Fm(e){let t=[],n=-1;for(let r of e.selection.ranges){let i=e.doc.lineAt(r.from),a=e.doc.lineAt(r.to);if(!r.empty&&r.to==a.from&&(a=e.doc.lineAt(r.to-1)),n>=i.number){let e=t[t.length-1];e.to=a.to,e.ranges.push(r)}else t.push({from:i.from,to:a.to,ranges:[r]});n=a.number+1}return t}function Im(e,t,n){if(e.readOnly)return!1;let r=[],i=[];for(let t of Fm(e)){if(n?t.to==e.doc.length:t.from==0)continue;let a=e.doc.lineAt(n?t.to+1:t.from-1),o=a.length+1;if(n){r.push({from:t.to,to:a.to},{from:t.from,insert:a.text+e.lineBreak});for(let n of t.ranges)i.push(A.range(Math.min(e.doc.length,n.anchor+o),Math.min(e.doc.length,n.head+o)))}else{r.push({from:a.from,to:t.from},{from:t.to,insert:e.lineBreak+a.text});for(let e of t.ranges)i.push(A.range(e.anchor-o,e.head-o))}}return r.length?(t(e.update({changes:r,scrollIntoView:!0,selection:A.create(i,e.selection.mainIndex),userEvent:`move.line`})),!0):!1}var Lm=({state:e,dispatch:t})=>Im(e,t,!1),Rm=({state:e,dispatch:t})=>Im(e,t,!0);function zm(e,t,n){if(e.readOnly)return!1;let r=[];for(let t of Fm(e))n?r.push({from:t.from,insert:e.doc.slice(t.from,t.to)+e.lineBreak}):r.push({from:t.to,insert:e.lineBreak+e.doc.slice(t.from,t.to)});let i=e.changes(r);return t(e.update({changes:i,selection:e.selection.map(i,n?1:-1),scrollIntoView:!0,userEvent:`input.copyline`})),!0}var Bm=({state:e,dispatch:t})=>zm(e,t,!1),Vm=({state:e,dispatch:t})=>zm(e,t,!0),Hm=e=>{if(e.state.readOnly)return!1;let{state:t}=e,n=t.changes(Fm(t).map(({from:e,to:n})=>(e>0?e--:n<t.doc.length&&n++,{from:e,to:n}))),r=pp(t.selection,t=>{let n;if(e.lineWrapping){let r=e.lineBlockAt(t.head),i=e.coordsAtPos(t.head,t.assoc||1);i&&(n=r.bottom+e.documentTop-i.bottom+e.defaultLineHeight/2)}return e.moveVertically(t,!0,n)}).map(n);return e.dispatch({changes:n,selection:r,scrollIntoView:!0,userEvent:`delete.line`}),!0};function Um(e,t){if(/\(\)|\[\]|\{\}/.test(e.sliceDoc(t-1,t+1)))return{from:t,to:t};let n=Ru(e).resolveInner(t),r=n.childBefore(t),i=n.childAfter(t),a;return r&&i&&r.to<=t&&i.from>=t&&(a=r.type.prop(K.closedBy))&&a.indexOf(i.name)>-1&&e.doc.lineAt(r.to).from==e.doc.lineAt(i.from).from&&!/\S/.test(e.sliceDoc(r.to,i.from))?{from:r.to,to:i.from}:null}var Wm=Km(!1),Gm=Km(!0);function Km(e){return({state:t,dispatch:n})=>{if(t.readOnly)return!1;let r=t.changeByRange(n=>{let{from:r,to:i}=n,a=t.doc.lineAt(r),o=!e&&r==i&&Um(t,r);e&&(r=i=(i<=a.to?a:t.doc.lineAt(i)).to);let s=new $u(t,{simulateBreak:r,simulateDoubleBreak:!!o}),c=Qu(s,r);for(c??=Jt(/^\s*/.exec(t.doc.lineAt(r).text)[0],t.tabSize);i<a.to&&/\s/.test(a.text[i-a.from]);)i++;o?{from:r,to:i}=o:r>a.from&&r<a.from+100&&!/\S/.test(a.text.slice(0,r))&&(r=a.from);let l=[``,Zu(t,c)];return o&&l.push(Zu(t,s.lineIndent(a.from,-1))),{changes:{from:r,to:i,insert:E.of(l)},range:A.cursor(r+1+l[1].length)}});return n(t.update(r,{scrollIntoView:!0,userEvent:`input`})),!0}}function qm(e,t){let n=-1;return e.changeByRange(r=>{let i=[];for(let a=r.from;a<=r.to;){let o=e.doc.lineAt(a);o.number>n&&(r.empty||r.to>o.from)&&(t(o,i,r),n=o.number),a=o.to+1}let a=e.changes(i);return{changes:i,range:A.range(a.mapPos(r.anchor,1),a.mapPos(r.head,1))}})}var Jm=({state:e,dispatch:t})=>{if(e.readOnly)return!1;let n=Object.create(null),r=new $u(e,{overrideIndentation:e=>n[e]??-1}),i=qm(e,(t,i,a)=>{let o=Qu(r,t.from);if(o==null)return;/\S/.test(t.text)||(o=0);let s=/^\s*/.exec(t.text)[0],c=Zu(e,o);(s!=c||a.from<t.from+s.length)&&(n[t.from]=o,i.push({from:t.from,to:t.from+s.length,insert:c}))});return i.changes.empty||t(e.update(i,{userEvent:`indent`})),!0},Ym=({state:e,dispatch:t})=>!e.readOnly&&(t(e.update(qm(e,(t,n)=>{n.push({from:t.from,insert:e.facet(Yu)})}),{userEvent:`input.indent`})),!0),Xm=({state:e,dispatch:t})=>!e.readOnly&&(t(e.update(qm(e,(t,n)=>{let r=/^\s*/.exec(t.text)[0];if(!r)return;let i=Jt(r,e.tabSize),a=0,o=Zu(e,Math.max(0,i-Xu(e)));for(;a<r.length&&a<o.length&&r.charCodeAt(a)==o.charCodeAt(a);)a++;n.push({from:t.from+a,to:t.from+r.length,insert:o.slice(a)})}),{userEvent:`delete.dedent`})),!0),Zm=e=>(e.setTabFocusMode(),!0),Qm=[{key:`Ctrl-b`,run:yp,shift:Kp,preventDefault:!0},{key:`Ctrl-f`,run:bp,shift:qp},{key:`Ctrl-p`,run:kp,shift:em},{key:`Ctrl-n`,run:Ap,shift:tm},{key:`Ctrl-a`,run:Bp,shift:lm},{key:`Ctrl-e`,run:Vp,shift:um},{key:`Ctrl-d`,run:Em},{key:`Ctrl-h`,run:Tm},{key:`Ctrl-k`,run:Am},{key:`Ctrl-Alt-h`,run:Om},{key:`Ctrl-o`,run:Nm},{key:`Ctrl-t`,run:Pm},{key:`Ctrl-v`,run:Pp}],$m=[{key:`ArrowLeft`,run:yp,shift:Kp,preventDefault:!0},{key:`Mod-ArrowLeft`,mac:`Alt-ArrowLeft`,run:Sp,shift:Yp,preventDefault:!0},{mac:`Cmd-ArrowLeft`,run:Rp,shift:sm,preventDefault:!0},{key:`ArrowRight`,run:bp,shift:qp,preventDefault:!0},{key:`Mod-ArrowRight`,mac:`Alt-ArrowRight`,run:Cp,shift:Xp,preventDefault:!0},{mac:`Cmd-ArrowRight`,run:zp,shift:cm,preventDefault:!0},{key:`ArrowUp`,run:kp,shift:em,preventDefault:!0},{mac:`Cmd-ArrowUp`,run:dm,shift:pm},{mac:`Ctrl-ArrowUp`,run:Np,shift:rm},{key:`ArrowDown`,run:Ap,shift:tm,preventDefault:!0},{mac:`Cmd-ArrowDown`,run:fm,shift:mm},{mac:`Ctrl-ArrowDown`,run:Pp,shift:im},{key:`PageUp`,run:Np,shift:rm},{key:`PageDown`,run:Pp,shift:im},{key:`Home`,run:Lp,shift:om,preventDefault:!0},{key:`Mod-Home`,run:dm,shift:pm},{key:`End`,run:Ip,shift:am,preventDefault:!0},{key:`Mod-End`,run:fm,shift:mm},{key:`Enter`,run:Wm,shift:Wm},{key:`Mod-a`,run:hm},{key:`Backspace`,run:Tm,shift:Tm,preventDefault:!0},{key:`Delete`,run:Em,preventDefault:!0},{key:`Mod-Backspace`,mac:`Alt-Backspace`,run:Om,preventDefault:!0},{key:`Mod-Delete`,mac:`Alt-Delete`,run:km,preventDefault:!0},{mac:`Mod-Backspace`,run:jm,preventDefault:!0},{mac:`Mod-Delete`,run:Mm,preventDefault:!0}].concat(Qm.map(e=>({mac:e.key,run:e.run,shift:e.shift}))),eh=[{key:`Alt-ArrowLeft`,mac:`Ctrl-ArrowLeft`,run:Ep,shift:Zp},{key:`Alt-ArrowRight`,mac:`Ctrl-ArrowRight`,run:Dp,shift:Qp},{key:`Alt-ArrowUp`,run:Lm},{key:`Shift-Alt-ArrowUp`,run:Bm},{key:`Alt-ArrowDown`,run:Rm},{key:`Shift-Alt-ArrowDown`,run:Vm},{key:`Mod-Alt-ArrowUp`,run:ym},{key:`Mod-Alt-ArrowDown`,run:bm},{key:`Escape`,run:xm},{key:`Mod-Enter`,run:Gm},{key:`Alt-l`,mac:`Ctrl-l`,run:gm},{key:`Mod-i`,run:_m,preventDefault:!0},{key:`Mod-[`,run:Xm},{key:`Mod-]`,run:Ym},{key:`Mod-Alt-\\`,run:Jm},{key:`Shift-Mod-k`,run:Hm},{key:`Shift-Mod-\\`,run:Up},{key:`Mod-/`,run:jf},{key:`Alt-A`,mac:`Ctrl-A`,run:Pf},{key:`Ctrl-m`,mac:`Shift-Alt-m`,run:Zm}].concat($m),th={key:`Tab`,run:Ym,shift:Xm},nh=typeof String.prototype.normalize==`function`?e=>e.normalize(`NFKD`):e=>e,rh=class{constructor(e,t,n=0,r=e.length,i,a){this.test=a,this.value={from:0,to:0,precise:!1},this.done=!1,this.matches=[],this.buffer=``,this.bufferPos=0,this.iter=e.iterRange(n,r),this.bufferStart=n,this.normalize=i?e=>i(nh(e)):nh,this.query=this.normalize(t)}peek(){if(this.bufferPos==this.buffer.length){if(this.bufferStart+=this.buffer.length,this.iter.next(),this.iter.done)return-1;this.bufferPos=0,this.buffer=this.iter.value}return Oe(this.buffer,this.bufferPos)}next(){for(;this.matches.length;)this.matches.pop();return this.nextOverlapping()}nextOverlapping(){for(;;){let e=this.peek();if(e<0)return this.done=!0,this;let t=ke(e),n=this.bufferStart+this.bufferPos;this.bufferPos+=Ae(e);let r=this.normalize(t);if(r.length)for(let e=0,i=n,a=!0;;e++){let n=r.charCodeAt(e),o=this.match(n,i,a,this.bufferPos+this.bufferStart,e==r.length-1);if(o)return this.value=o,this;if(e==r.length-1)break;a&&e<t.length&&t.charCodeAt(e)==n?i++:a=!1}}}match(e,t,n,r,i){let a=null;for(let t=0;t<this.matches.length;){let n=this.matches[t],o=!1;this.query.charCodeAt(n.index)==e&&(n.index==this.query.length-1?a={from:n.from,to:r,precise:i&&n.precise}:(n.index++,o=!0)),o?t++:this.matches.splice(t,1)}return this.query.charCodeAt(0)==e&&(this.query.length==1?a={from:t,to:r,precise:n&&i}:this.matches.push({from:t,index:1,precise:n})),a&&this.test&&!this.test(a.from,a.to,this.buffer,this.bufferStart)&&(a=null),a}};typeof Symbol<`u`&&(rh.prototype[Symbol.iterator]=function(){return this});var ih={from:-1,to:-1,match:/.*/.exec(``),precise:!0},ah=`gm`+(/x/.unicode==null?``:`u`),oh=class{constructor(e,t,n,r=0,i=e.length){if(this.text=e,this.to=i,this.curLine=``,this.done=!1,this.value=ih,/\\[sWDnr]|\n|\r|\[\^/.test(t))return new lh(e,t,n,r,i);this.re=new RegExp(t,ah+(n?.ignoreCase?`i`:``)),this.test=n?.test,this.iter=e.iter();let a=e.lineAt(r);this.curLineStart=a.from,this.matchPos=dh(e,r),this.getLine(this.curLineStart)}getLine(e){this.iter.next(e),this.iter.lineBreak?this.curLine=``:(this.curLine=this.iter.value,this.curLineStart+this.curLine.length>this.to&&(this.curLine=this.curLine.slice(0,this.to-this.curLineStart)),this.iter.next())}nextLine(){this.curLineStart=this.curLineStart+this.curLine.length+1,this.curLineStart>this.to?this.curLine=``:this.getLine(0)}next(){for(let e=this.matchPos-this.curLineStart;;){this.re.lastIndex=e;let t=this.matchPos<=this.to&&this.re.exec(this.curLine);if(t){let n=this.curLineStart+t.index,r=n+t[0].length;if(this.matchPos=dh(this.text,r+ +(n==r)),n==this.curLineStart+this.curLine.length&&this.nextLine(),(n<r||n>this.value.to)&&(!this.test||this.test(n,r,t)))return this.value={from:n,to:r,precise:!0,match:t},this;e=this.matchPos-this.curLineStart}else if(this.curLineStart+this.curLine.length<this.to)this.nextLine(),e=0;else return this.done=!0,this}}},sh=new WeakMap,ch=class e{constructor(e,t){this.from=e,this.text=t}get to(){return this.from+this.text.length}static get(t,n,r){let i=sh.get(t);if(!i||i.from>=r||i.to<=n){let i=new e(n,t.sliceString(n,r));return sh.set(t,i),i}if(i.from==n&&i.to==r)return i;let{text:a,from:o}=i;return o>n&&(a=t.sliceString(n,o)+a,o=n),i.to<r&&(a+=t.sliceString(i.to,r)),sh.set(t,new e(o,a)),new e(n,a.slice(n-o,r-o))}},lh=class{constructor(e,t,n,r,i){this.text=e,this.to=i,this.done=!1,this.value=ih,this.matchPos=dh(e,r),this.re=new RegExp(t,ah+(n?.ignoreCase?`i`:``)),this.test=n?.test,this.flat=ch.get(e,r,this.chunkEnd(r+5e3))}chunkEnd(e){return e>=this.to?this.to:this.text.lineAt(e).to}next(){for(;;){let e=this.re.lastIndex=this.matchPos-this.flat.from,t=this.re.exec(this.flat.text);if(t&&!t[0]&&t.index==e&&(this.re.lastIndex=e+1,t=this.re.exec(this.flat.text)),t){let e=this.flat.from+t.index,n=e+t[0].length;if((this.flat.to>=this.to||t.index+t[0].length<=this.flat.text.length-10)&&(!this.test||this.test(e,n,t)))return this.value={from:e,to:n,precise:!0,match:t},this.matchPos=dh(this.text,n+ +(e==n)),this}if(this.flat.to==this.to)return this.done=!0,this;this.flat=ch.get(this.text,this.flat.from,this.chunkEnd(this.flat.from+this.flat.text.length*2))}}};typeof Symbol<`u`&&(oh.prototype[Symbol.iterator]=lh.prototype[Symbol.iterator]=function(){return this});function uh(e){try{return new RegExp(e,ah),!0}catch{return!1}}function dh(e,t){if(t>=e.length)return t;let n=e.lineAt(t),r;for(;t<n.to&&(r=n.text.charCodeAt(t-n.from))>=56320&&r<57344;)t++;return t}var fh=e=>{let t=Yc(e,`cm-goto-line`);if(t){let e=t.dom.querySelector(`input[type=text]`);return e&&e.select(),!0}let{state:n}=e,r=String(n.doc.lineAt(e.state.selection.main.head).number),{close:i,result:a}=Jc(e,{class:`cm-goto-line`,label:n.phrase(`Go to line`),input:{type:`text`,name:`line`,value:r},focus:!0,submitLabel:n.phrase(`go`)});return a.then(t=>{let r=t&&/^([+-])?(\d+)?(:\d+)?(%)?$/.exec(t.elements.line.value);if(!r){e.dispatch({effects:i});return}let a=n.doc.lineAt(n.selection.main.head),[,o,s,c,l]=r,u=c?+c.slice(1):0,d=s?+s:a.number;if(s&&l){let e=d/100;o&&(e=e*(o==`-`?-1:1)+a.number/n.doc.lines),d=Math.round(n.doc.lines*e)}else s&&o&&(d=d*(o==`-`?-1:1)+a.number);let f=n.doc.line(Math.max(1,Math.min(n.doc.lines,d))),p=A.cursor(f.from+Math.max(0,Math.min(u,f.length)));e.dispatch({effects:[i,G.scrollIntoView(p.from,{y:`center`})],selection:p})}),!0},ph={highlightWordAroundCursor:!1,minSelectionLength:1,maxMatches:100,wholeWords:!1},mh=j.define({combine(e){return kt(e,ph,{highlightWordAroundCursor:(e,t)=>e||t,minSelectionLength:Math.min,maxMatches:Math.min})}});function hh(e){let t=[xh,bh];return e&&t.push(mh.of(e)),t}var gh=R.mark({class:`cm-selectionMatch`}),_h=R.mark({class:`cm-selectionMatch cm-selectionMatch-main`});function vh(e,t,n,r){return(n==0||e(t.sliceDoc(n-1,n))!=N.Word)&&(r==t.doc.length||e(t.sliceDoc(r,r+1))!=N.Word)}function yh(e,t,n,r){return e(t.sliceDoc(n,n+1))==N.Word&&e(t.sliceDoc(r-1,r))==N.Word}var bh=H.fromClass(class{constructor(e){this.decorations=this.getDeco(e)}update(e){(e.selectionSet||e.docChanged||e.viewportChanged)&&(this.decorations=this.getDeco(e.view))}getDeco(e){let t=e.state.facet(mh),{state:n}=e,r=n.selection;if(r.ranges.length>1)return R.none;let i=r.main,a,o=null;if(i.empty){if(!t.highlightWordAroundCursor)return R.none;let e=n.wordAt(i.head);if(!e)return R.none;o=n.charCategorizer(i.head),a=n.sliceDoc(e.from,e.to)}else{let e=i.to-i.from;if(e<t.minSelectionLength||e>200)return R.none;if(t.wholeWords){if(a=n.sliceDoc(i.from,i.to),o=n.charCategorizer(i.head),!(vh(o,n,i.from,i.to)&&yh(o,n,i.from,i.to)))return R.none}else if(a=n.sliceDoc(i.from,i.to),!a)return R.none}let s=[];for(let r of e.visibleRanges){let e=new rh(n.doc,a,r.from,r.to);for(;!e.next().done;){let{from:r,to:a}=e.value;if((!o||vh(o,n,r,a))&&(i.empty&&r<=i.from&&a>=i.to?s.push(_h.range(r,a)):(r>=i.to||a<=i.from)&&s.push(gh.range(r,a)),s.length>t.maxMatches))return R.none}}return R.set(s)}},{decorations:e=>e.decorations}),xh=G.baseTheme({".cm-selectionMatch":{backgroundColor:`#99ff7780`},".cm-searchMatch .cm-selectionMatch":{backgroundColor:`transparent`}}),Sh=({state:e,dispatch:t})=>{let{selection:n}=e,r=A.create(n.ranges.map(t=>e.wordAt(t.head)||A.cursor(t.head)),n.mainIndex);return!r.eq(n)&&(t(e.update({selection:r})),!0)};function Ch(e,t){let{main:n,ranges:r}=e.selection,i=e.wordAt(n.head),a=i&&i.from==n.from&&i.to==n.to;for(let n=!1,i=new rh(e.doc,t,r[r.length-1].to);;)if(i.next(),i.done){if(n)return null;i=new rh(e.doc,t,0,Math.max(0,r[r.length-1].from-1)),n=!0}else{if(n&&r.some(e=>e.from==i.value.from))continue;if(a){let t=e.wordAt(i.value.from);if(!t||t.from!=i.value.from||t.to!=i.value.to)continue}return i.value}}var wh=({state:e,dispatch:t})=>{let{ranges:n}=e.selection;if(n.some(e=>e.from===e.to))return Sh({state:e,dispatch:t});let r=e.sliceDoc(n[0].from,n[0].to);if(e.selection.ranges.some(t=>e.sliceDoc(t.from,t.to)!=r))return!1;let i=Ch(e,r);return i?(t(e.update({selection:e.selection.addRange(A.range(i.from,i.to),!1),effects:G.scrollIntoView(i.to)})),!0):!1},Th=j.define({combine(e){return kt(e,{top:!1,caseSensitive:!1,literal:!1,regexp:!1,wholeWord:!1,createPanel:e=>new ag(e),scrollToMatch:e=>G.scrollIntoView(e)})}}),Eh=class{constructor(e){this.search=e.search,this.caseSensitive=!!e.caseSensitive,this.literal=!!e.literal,this.regexp=!!e.regexp,this.replace=e.replace||``,this.valid=!!this.search&&(!this.regexp||uh(this.search)),this.unquoted=this.unquote(this.search),this.wholeWord=!!e.wholeWord,this.test=e.test}unquote(e){return this.literal?e:e.replace(/\\([nrt\\])/g,(e,t)=>t==`n`?`
`:t==`r`?`\r`:t==`t`?`	`:`\\`)}eq(e){return this.search==e.search&&this.replace==e.replace&&this.caseSensitive==e.caseSensitive&&this.regexp==e.regexp&&this.wholeWord==e.wholeWord&&this.test==e.test}create(){return this.regexp?new Lh(this):new jh(this)}getCursor(e,t=0,n){let r=e.doc?e:P.create({doc:e});return n??=r.doc.length,this.regexp?Nh(this,r,t,n):kh(this,r,t,n)}},Dh=class{constructor(e){this.spec=e}};function Oh(e,t,n){return(r,i,a,o)=>n&&!n(r,i,a,o)?!1:e(r>=o&&i<=o+a.length?a.slice(r-o,i-o):t.doc.sliceString(r,i),t,r,i)}function kh(e,t,n,r){let i;return e.wholeWord&&(i=Ah(t.doc,t.charCategorizer(t.selection.main.head))),e.test&&(i=Oh(e.test,t,i)),new rh(t.doc,e.unquoted,n,r,e.caseSensitive?void 0:e=>e.toLowerCase(),i)}function Ah(e,t){return(n,r,i,a)=>((a>n||a+i.length<r)&&(a=Math.max(0,n-2),i=e.sliceString(a,Math.min(e.length,r+2))),(t(Ph(i,n-a))!=N.Word||t(Fh(i,n-a))!=N.Word)&&(t(Fh(i,r-a))!=N.Word||t(Ph(i,r-a))!=N.Word))}var jh=class extends Dh{constructor(e){super(e)}nextMatch(e,t,n){let r=kh(this.spec,e,n,e.doc.length).nextOverlapping();if(r.done){let n=Math.min(e.doc.length,t+this.spec.unquoted.length);r=kh(this.spec,e,0,n).nextOverlapping()}return r.done||r.value.from==t&&r.value.to==n?null:r.value}prevMatchInRange(e,t,n){for(let r=n;;){let n=Math.max(t,r-1e4-this.spec.unquoted.length),i=kh(this.spec,e,n,r),a=null;for(;!i.nextOverlapping().done;)a=i.value;if(a)return a;if(n==t)return null;r-=1e4}}prevMatch(e,t,n){let r=this.prevMatchInRange(e,0,t);return r||=this.prevMatchInRange(e,Math.max(0,n-this.spec.unquoted.length),e.doc.length),r&&(r.from!=t||r.to!=n)?r:null}getReplacement(e){return this.spec.unquote(this.spec.replace)}matchAll(e,t){let n=kh(this.spec,e,0,e.doc.length),r=[];for(;!n.next().done;){if(r.length>=t)return null;r.push(n.value)}return r}highlight(e,t,n,r){let i=kh(this.spec,e,Math.max(0,t-this.spec.unquoted.length),Math.min(n+this.spec.unquoted.length,e.doc.length));for(;!i.next().done;)r(i.value.from,i.value.to)}};function Mh(e,t,n){return(r,i,a)=>(!n||n(r,i,a))&&e(a[0],t,r,i)}function Nh(e,t,n,r){let i;return e.wholeWord&&(i=Ih(t.charCategorizer(t.selection.main.head))),e.test&&(i=Mh(e.test,t,i)),new oh(t.doc,e.search,{ignoreCase:!e.caseSensitive,test:i},n,r)}function Ph(e,t){return e.slice(O(e,t,!1),t)}function Fh(e,t){return e.slice(t,O(e,t))}function Ih(e){return(t,n,r)=>!r[0].length||(e(Ph(r.input,r.index))!=N.Word||e(Fh(r.input,r.index))!=N.Word)&&(e(Fh(r.input,r.index+r[0].length))!=N.Word||e(Ph(r.input,r.index+r[0].length))!=N.Word)}var Lh=class extends Dh{nextMatch(e,t,n){let r=Nh(this.spec,e,n,e.doc.length).next();return r.done&&(r=Nh(this.spec,e,0,t).next()),r.done?null:r.value}prevMatchInRange(e,t,n){for(let r=1;;r++){let i=Math.max(t,n-r*1e4),a=Nh(this.spec,e,i,n),o=null;for(;!a.next().done;)o=a.value;if(o&&(i==t||o.from>i+10))return o;if(i==t)return null}}prevMatch(e,t,n){return this.prevMatchInRange(e,0,t)||this.prevMatchInRange(e,n,e.doc.length)}getReplacement(e){return this.spec.unquote(this.spec.replace).replace(/\$([$&]|\d+)/g,(t,n)=>{if(n==`&`)return e.match[0];if(n==`$`)return`$`;for(let t=n.length;t>0;t--){let r=+n.slice(0,t);if(r>0&&r<e.match.length)return e.match[r]+n.slice(t)}return t})}matchAll(e,t){let n=Nh(this.spec,e,0,e.doc.length),r=[];for(;!n.next().done;){if(r.length>=t)return null;r.push(n.value)}return r}highlight(e,t,n,r){let i=Nh(this.spec,e,Math.max(0,t-250),Math.min(n+250,e.doc.length));for(;!i.next().done;)r(i.value.from,i.value.to)}},Rh=M.define(),zh=M.define(),Bh=Ye.define({create(e){return new Vh($h(e).create(),null)},update(e,t){for(let n of t.effects)n.is(Rh)?e=new Vh(n.value.create(),e.panel):n.is(zh)&&(e=new Vh(e.query,n.value?Qh:null));return e},provide:e=>qc.from(e,e=>e.panel)}),Vh=class{constructor(e,t){this.query=e,this.panel=t}},Hh=R.mark({class:`cm-searchMatch`}),Uh=R.mark({class:`cm-searchMatch cm-searchMatch-selected`}),Wh=H.fromClass(class{constructor(e){this.view=e,this.decorations=this.highlight(e.state.field(Bh))}update(e){let t=e.state.field(Bh);(t!=e.startState.field(Bh)||e.docChanged||e.selectionSet||e.viewportChanged)&&(this.decorations=this.highlight(t))}highlight({query:e,panel:t}){if(!t||!e.spec.valid)return R.none;let{view:n}=this,r=new Lt;for(let t=0,i=n.visibleRanges,a=i.length;t<a;t++){let{from:o,to:s}=i[t];for(;t<a-1&&s>i[t+1].from-500;)s=i[++t].to;e.highlight(n.state,o,s,(e,t)=>{let i=n.state.selection.ranges.some(n=>n.from==e&&n.to==t);r.add(e,t,i?Uh:Hh)})}return r.finish()}},{decorations:e=>e.decorations});function Gh(e){return t=>{let n=t.state.field(Bh,!1);return n&&n.query.spec.valid?e(t,n):ng(t)}}var Kh=Gh((e,{query:t})=>{let{to:n}=e.state.selection.main,r=t.nextMatch(e.state,n,n);if(!r)return!1;let i=A.single(r.from,r.to),a=e.state.facet(Th);return e.dispatch({selection:i,effects:[lg(e,r),a.scrollToMatch(i.main,e)],userEvent:`select.search`}),tg(e),!0}),qh=Gh((e,{query:t})=>{let{state:n}=e,{from:r}=n.selection.main,i=t.prevMatch(n,r,r);if(!i)return!1;let a=A.single(i.from,i.to),o=e.state.facet(Th);return e.dispatch({selection:a,effects:[lg(e,i),o.scrollToMatch(a.main,e)],userEvent:`select.search`}),tg(e),!0}),Jh=Gh((e,{query:t})=>{let n=t.matchAll(e.state,1e3);return!n||!n.length?!1:(e.dispatch({selection:A.create(n.map(e=>A.range(e.from,e.to))),userEvent:`select.search.matches`}),!0)}),Yh=({state:e,dispatch:t})=>{let n=e.selection;if(n.ranges.length>1||n.main.empty)return!1;let{from:r,to:i}=n.main,a=[],o=0;for(let t=new rh(e.doc,e.sliceDoc(r,i));!t.next().done;){if(a.length>1e3)return!1;t.value.from==r&&(o=a.length),a.push(A.range(t.value.from,t.value.to))}return t(e.update({selection:A.create(a,o),userEvent:`select.search.matches`})),!0},Xh=Gh((e,{query:t})=>{let{state:n}=e,{from:r,to:i}=n.selection.main;if(n.readOnly)return!1;let a=t.nextMatch(n,r,r);if(!a)return!1;let o=a,s=[],c,l,u=[];o.precise?o.from==r&&o.to==i&&(l=n.toText(t.getReplacement(o)),s.push({from:o.from,to:o.to,insert:l}),o=t.nextMatch(n,o.from,o.to),u.push(G.announce.of(n.phrase(`replaced match on line $`,n.doc.lineAt(r).number)+`.`))):o=t.nextMatch(n,o.from,o.to);let d=e.state.changes(s);return o&&(c=A.single(o.from,o.to).map(d),u.push(lg(e,o)),u.push(n.facet(Th).scrollToMatch(c.main,e))),e.dispatch({changes:d,selection:c,effects:u,userEvent:`input.replace`}),!0}),Zh=Gh((e,{query:t})=>{if(e.state.readOnly)return!1;let n=[];for(let r of t.matchAll(e.state,1e9)){let{from:e,to:i,precise:a}=r;a&&n.push({from:e,to:i,insert:t.getReplacement(r)})}if(!n.length)return!1;let r=e.state.phrase(`replaced $ matches`,n.length)+`.`;return e.dispatch({changes:n,effects:G.announce.of(r),userEvent:`input.replace.all`}),!0});function Qh(e){return e.state.facet(Th).createPanel(e)}function $h(e,t){let n=e.selection.main,r=n.empty||n.to>n.from+100?``:e.sliceDoc(n.from,n.to);if(t&&!r)return t;let i=e.facet(Th);return new Eh({search:t?.literal??i.literal?r:r.replace(/\n/g,`\\n`),caseSensitive:t?.caseSensitive??i.caseSensitive,literal:t?.literal??i.literal,regexp:t?.regexp??i.regexp,wholeWord:t?.wholeWord??i.wholeWord})}function eg(e){let t=Uc(e,Qh);return t&&t.dom.querySelector(`[main-field]`)}function tg(e){let t=eg(e);t&&t==e.root.activeElement&&t.select()}var ng=e=>{let t=e.state.field(Bh,!1);if(t&&t.panel){let n=eg(e);if(n&&n!=e.root.activeElement){let r=$h(e.state,t.query.spec);r.valid&&e.dispatch({effects:Rh.of(r)}),n.focus(),n.select()}}else e.dispatch({effects:[zh.of(!0),t?Rh.of($h(e.state,t.query.spec)):M.appendConfig.of(dg)]});return!0},rg=e=>{let t=e.state.field(Bh,!1);if(!t||!t.panel)return!1;let n=Uc(e,Qh);return n&&n.dom.contains(e.root.activeElement)&&e.focus(),e.dispatch({effects:zh.of(!1)}),!0},ig=[{key:`Mod-f`,run:ng,scope:`editor search-panel`},{key:`F3`,run:Kh,shift:qh,scope:`editor search-panel`,preventDefault:!0},{key:`Mod-g`,run:Kh,shift:qh,scope:`editor search-panel`,preventDefault:!0},{key:`Escape`,run:rg,scope:`editor search-panel`},{key:`Mod-Shift-l`,run:Yh},{key:`Mod-Alt-g`,run:fh},{key:`Mod-d`,run:wh,preventDefault:!0}],ag=class{constructor(e){this.view=e;let t=this.query=e.state.field(Bh).query.spec;this.commit=this.commit.bind(this),this.searchField=I(`input`,{value:t.search,placeholder:og(e,`Find`),"aria-label":og(e,`Find`),class:`cm-textfield`,name:`search`,form:``,"main-field":`true`,onchange:this.commit,onkeyup:this.commit}),this.replaceField=I(`input`,{value:t.replace,placeholder:og(e,`Replace`),"aria-label":og(e,`Replace`),class:`cm-textfield`,name:`replace`,form:``,onchange:this.commit,onkeyup:this.commit}),this.caseField=I(`input`,{type:`checkbox`,name:`case`,form:``,checked:t.caseSensitive,onchange:this.commit}),this.reField=I(`input`,{type:`checkbox`,name:`re`,form:``,checked:t.regexp,onchange:this.commit}),this.wordField=I(`input`,{type:`checkbox`,name:`word`,form:``,checked:t.wholeWord,onchange:this.commit});function n(e,t,n){return I(`button`,{class:`cm-button`,name:e,onclick:t,type:`button`},n)}this.dom=I(`div`,{onkeydown:e=>this.keydown(e),class:`cm-search`},[this.searchField,n(`next`,()=>Kh(e),[og(e,`next`)]),n(`prev`,()=>qh(e),[og(e,`previous`)]),n(`select`,()=>Jh(e),[og(e,`all`)]),I(`label`,null,[this.caseField,og(e,`match case`)]),I(`label`,null,[this.reField,og(e,`regexp`)]),I(`label`,null,[this.wordField,og(e,`by word`)]),...e.state.readOnly?[]:[I(`br`),this.replaceField,n(`replace`,()=>Xh(e),[og(e,`replace`)]),n(`replaceAll`,()=>Zh(e),[og(e,`replace all`)])],I(`button`,{name:`close`,onclick:()=>rg(e),"aria-label":og(e,`close`),type:`button`},[`×`])])}commit(){let e=new Eh({search:this.searchField.value,caseSensitive:this.caseField.checked,regexp:this.reField.checked,wholeWord:this.wordField.checked,replace:this.replaceField.value});e.eq(this.query)||(this.query=e,this.view.dispatch({effects:Rh.of(e)}))}keydown(e){vs(this.view,e,`search-panel`)?e.preventDefault():e.keyCode==13&&e.target==this.searchField?(e.preventDefault(),(e.shiftKey?qh:Kh)(this.view)):e.keyCode==13&&e.target==this.replaceField&&(e.preventDefault(),Xh(this.view))}update(e){for(let t of e.transactions)for(let e of t.effects)e.is(Rh)&&!e.value.eq(this.query)&&this.setQuery(e.value)}setQuery(e){this.query=e,this.searchField.value=e.search,this.replaceField.value=e.replace,this.caseField.checked=e.caseSensitive,this.reField.checked=e.regexp,this.wordField.checked=e.wholeWord}mount(){this.searchField.select()}get pos(){return 80}get top(){return this.view.state.facet(Th).top}};function og(e,t){return e.state.phrase(t)}var sg=30,cg=/[\s\.,:;?!]/;function lg(e,{from:t,to:n}){let r=e.state.doc.lineAt(t),i=e.state.doc.lineAt(n).to,a=Math.max(r.from,t-sg),o=Math.min(i,n+sg),s=e.state.sliceDoc(a,o);if(a!=r.from){for(let e=0;e<sg;e++)if(!cg.test(s[e+1])&&cg.test(s[e])){s=s.slice(e);break}}if(o!=i){for(let e=s.length-1;e>s.length-sg;e--)if(!cg.test(s[e-1])&&cg.test(s[e])){s=s.slice(0,e);break}}return G.announce.of(`${e.state.phrase(`current match`)}. ${s} ${e.state.phrase(`on line`)} ${r.number}.`)}var ug=G.baseTheme({".cm-panel.cm-search":{padding:`2px 6px 4px`,position:`relative`,"& [name=close]":{position:`absolute`,top:`0`,right:`4px`,backgroundColor:`inherit`,border:`none`,font:`inherit`,padding:0,margin:0},"& input, & button, & label":{margin:`.2em .6em .2em 0`},"& input[type=checkbox]":{marginRight:`.2em`},"& label":{fontSize:`80%`,whiteSpace:`pre`}},"&light .cm-searchMatch":{backgroundColor:`#ffff0054`},"&dark .cm-searchMatch":{backgroundColor:`#00ffff8a`},"&light .cm-searchMatch-selected":{backgroundColor:`#ff6a0054`},"&dark .cm-searchMatch-selected":{backgroundColor:`#ff00ff8a`}}),dg=[Bh,Qe.low(Wh),ug],fg=class{constructor(e,t,n,r){this.state=e,this.pos=t,this.explicit=n,this.view=r,this.abortListeners=[],this.abortOnDocChange=!1}tokenBefore(e){let t=Ru(this.state).resolveInner(this.pos,-1);for(;t&&e.indexOf(t.name)<0;)t=t.parent;return t?{from:t.from,to:this.pos,text:this.state.sliceDoc(t.from,this.pos),type:t.type}:null}matchBefore(e){let t=this.state.doc.lineAt(this.pos),n=Math.max(t.from,this.pos-250),r=t.text.slice(n-t.from,this.pos-t.from),i=r.search(vg(e,!1));return i<0?null:{from:n+i,to:this.pos,text:r.slice(i)}}get aborted(){return this.abortListeners==null}addEventListener(e,t,n){e==`abort`&&this.abortListeners&&(this.abortListeners.push(t),n&&n.onDocChange&&(this.abortOnDocChange=!0))}};function pg(e){let t=Object.keys(e).join(``),n=/\w/.test(t);return n&&(t=t.replace(/\w/g,``)),`[${n?`\\w`:``}${t.replace(/[^\w\s]/g,`\\$&`)}]`}function mg(e){let t=Object.create(null),n=Object.create(null);for(let{label:r}of e){t[r[0]]=!0;for(let e=1;e<r.length;e++)n[r[e]]=!0}let r=pg(t)+pg(n)+`*$`;return[RegExp(`^`+r),new RegExp(r)]}function hg(e){let t=e.map(e=>typeof e==`string`?{label:e}:e),[n,r]=t.every(e=>/^\w+$/.test(e.label))?[/\w*$/,/\w+$/]:mg(t);return e=>{let i=e.matchBefore(r);return i||e.explicit?{from:i?i.from:e.pos,options:t,validFor:n}:null}}var gg=class{constructor(e,t,n,r){this.completion=e,this.source=t,this.match=n,this.score=r}};function _g(e){return e.selection.main.from}function vg(e,t){let{source:n}=e,r=t&&n[0]!=`^`,i=n[n.length-1]!=`$`;return!r&&!i?e:RegExp(`${r?`^`:``}(?:${n})${i?`$`:``}`,e.flags??(e.ignoreCase?`i`:``))}var yg=pt.define();function bg(e,t,n,r){let{main:i}=e.selection,a=n-i.from,o=r-i.from;return{...e.changeByRange(s=>{if(s!=i&&n!=r&&e.sliceDoc(s.from+a,s.from+o)!=e.sliceDoc(n,r))return{range:s};let c=e.toText(t);return{changes:{from:s.from+a,to:r==i.from?s.to:s.from+o,insert:c},range:A.cursor(s.from+a+c.length)}}),scrollIntoView:!0,userEvent:`input.complete`}}var xg=new WeakMap;function Sg(e){if(!Array.isArray(e))return e;let t=xg.get(e);return t||xg.set(e,t=hg(e)),t}var Cg=M.define(),wg=M.define(),Tg=class{constructor(e){this.pattern=e,this.chars=[],this.folded=[],this.any=[],this.precise=[],this.byWord=[],this.score=0,this.matched=[];for(let t=0;t<e.length;){let n=Oe(e,t),r=Ae(n);this.chars.push(n);let i=e.slice(t,t+r),a=i.toUpperCase();this.folded.push(Oe(a==i?i.toLowerCase():a,0)),t+=r}this.astral=e.length!=this.chars.length}ret(e,t){return this.score=e,this.matched=t,this}match(e){if(this.pattern.length==0)return this.ret(-100,[]);if(e.length<this.pattern.length)return null;let{chars:t,folded:n,any:r,precise:i,byWord:a}=this;if(t.length==1){let r=Oe(e,0),i=Ae(r),a=i==e.length?0:-100;if(r!=t[0]){if(r==n[0])a+=-200;else return null}return this.ret(a,[0,i])}let o=e.indexOf(this.pattern);if(o==0)return this.ret(e.length==this.pattern.length?0:-100,[0,this.pattern.length]);let s=t.length,c=0;if(o<0){for(let i=0,a=Math.min(e.length,200);i<a&&c<s;){let a=Oe(e,i);(a==t[c]||a==n[c])&&(r[c++]=i),i+=Ae(a)}if(c<s)return null}let l=0,u=0,d=!1,f=0,p=-1,m=-1,h=/[a-z]/.test(e),g=!0;for(let r=0,c=Math.min(e.length,200),_=0;r<c&&u<s;){let c=Oe(e,r);o<0&&(l<s&&c==t[l]&&(i[l++]=r),f<s&&(c==t[f]||c==n[f]?(f==0&&(p=r),m=r+1,f++):f=0));let v,y=c<255?c>=48&&c<=57||c>=97&&c<=122?2:+(c>=65&&c<=90):(v=ke(c))==v.toLowerCase()?v==v.toUpperCase()?0:2:1;(!r||y==1&&h||_==0&&y!=0)&&(t[u]==c||n[u]==c&&(d=!0)?a[u++]=r:a.length&&(g=!1)),_=y,r+=Ae(c)}return u==s&&a[0]==0&&g?this.result(-100+(d?-200:0),a,e):f==s&&p==0?this.ret(-200-e.length+(m==e.length?0:-100),[0,m]):o>-1?this.ret(-700-e.length,[o,o+this.pattern.length]):f==s?this.ret(-900-e.length,[p,m]):u==s?this.result(-100+(d?-200:0)+-700+(g?0:-1100),a,e):t.length==2?null:this.result((r[0]?-700:0)+-200+-1100,r,e)}result(e,t,n){let r=[],i=0;for(let e of t){let t=e+(this.astral?Ae(Oe(n,e)):1);i&&r[i-1]==e?r[i-1]=t:(r[i++]=e,r[i++]=t)}return this.ret(e-n.length,r)}},Eg=class{constructor(e){this.pattern=e,this.matched=[],this.score=0,this.folded=e.toLowerCase()}match(e){if(e.length<this.pattern.length)return null;let t=e.slice(0,this.pattern.length),n=t==this.pattern?0:t.toLowerCase()==this.folded?-200:null;return n==null?null:(this.matched=[0,t.length],this.score=n+(e.length==this.pattern.length?0:-100),this)}},Z=j.define({combine(e){return kt(e,{activateOnTyping:!0,activateOnCompletion:()=>!1,activateOnTypingDelay:100,selectOnOpen:!0,override:null,closeOnBlur:!0,maxRenderedOptions:100,defaultKeymap:!0,tooltipClass:()=>``,optionClass:()=>``,aboveCursor:!1,icons:!0,addToOptions:[],positionInfo:Og,filterStrict:!1,compareCompletions:(e,t)=>(e.sortText||e.label).localeCompare(t.sortText||t.label),interactionDelay:75,updateSyncTime:100},{defaultKeymap:(e,t)=>e&&t,closeOnBlur:(e,t)=>e&&t,icons:(e,t)=>e&&t,tooltipClass:(e,t)=>n=>Dg(e(n),t(n)),optionClass:(e,t)=>n=>Dg(e(n),t(n)),addToOptions:(e,t)=>e.concat(t),filterStrict:(e,t)=>e||t})}});function Dg(e,t){return e?t?e+` `+t:e:t}function Og(e,t,n,r,i,a){let o=e.textDirection==B.RTL,s=o,c=!1,l=`top`,u,d,f=t.left-i.left,p=i.right-t.right,m=r.right-r.left,h=r.bottom-r.top;if(s&&f<Math.min(m,p)?s=!1:!s&&p<Math.min(m,f)&&(s=!0),m<=(s?f:p))u=Math.max(i.top,Math.min(n.top,i.bottom-h))-t.top,d=Math.min(400,s?f:p);else{c=!0,d=Math.min(400,(o?t.right:i.right-t.left)-30);let e=i.bottom-t.bottom;e>=h||e>t.top?u=n.bottom-t.top:(l=`bottom`,u=t.bottom-n.top)}let g=(t.bottom-t.top)/a.offsetHeight,_=(t.right-t.left)/a.offsetWidth;return{style:`${l}: ${u/g}px; max-width: ${d/_}px`,class:`cm-completionInfo-`+(c?o?`left-narrow`:`right-narrow`:s?`left`:`right`)}}var kg=M.define();function Ag(e){let t=e.addToOptions.slice();return e.icons&&t.push({render(e){let t=document.createElement(`div`);return t.classList.add(`cm-completionIcon`),e.type&&t.classList.add(...e.type.split(/\s+/g).map(e=>`cm-completionIcon-`+e)),t.setAttribute(`aria-hidden`,`true`),t},position:20}),t.push({render(e,t,n,r){let i=document.createElement(`span`);i.className=`cm-completionLabel`;let a=e.displayLabel||e.label,o=0;for(let e=0;e<r.length;){let t=r[e++],n=r[e++];t>o&&i.appendChild(document.createTextNode(a.slice(o,t)));let s=i.appendChild(document.createElement(`span`));s.appendChild(document.createTextNode(a.slice(t,n))),s.className=`cm-completionMatchedText`,o=n}return o<a.length&&i.appendChild(document.createTextNode(a.slice(o))),i},position:50},{render(e){if(!e.detail)return null;let t=document.createElement(`span`);return t.className=`cm-completionDetail`,t.textContent=e.detail,t},position:80}),t.sort((e,t)=>e.position-t.position).map(e=>e.render)}function jg(e,t,n){if(e<=n)return{from:0,to:e};if(t<0&&(t=0),t<=e>>1){let e=Math.floor(t/n);return{from:e*n,to:(e+1)*n}}let r=Math.ceil((e-t)/n);return{from:e-r*n,to:e-(r-1)*n}}var Mg=class{constructor(e,t,n){this.view=e,this.stateField=t,this.applyCompletion=n,this.info=null,this.infoDestroy=null,this.placeInfoReq={read:()=>this.measureInfo(),write:e=>this.placeInfo(e),key:this},this.space=null,this.currentClass=``;let r=e.state.field(t),{options:i,selected:a}=r.open,o=e.state.facet(Z);this.optionContent=Ag(o),this.optionClass=o.optionClass,this.tooltipClass=o.tooltipClass,this.range=jg(i.length,a,o.maxRenderedOptions),this.dom=document.createElement(`div`),this.dom.className=`cm-tooltip-autocomplete`,this.updateTooltipClass(e.state),this.dom.addEventListener(`mousedown`,n=>{let{options:r}=e.state.field(t).open;for(let t=n.target,i;t&&t!=this.dom;t=t.parentNode)if(t.nodeName==`LI`&&(i=/-(\d+)$/.exec(t.id))&&+i[1]<r.length){this.applyCompletion(e,r[+i[1]]),n.preventDefault();return}if(n.target==this.list){let t=this.list.classList.contains(`cm-completionListIncompleteTop`)&&n.clientY<this.list.firstChild.getBoundingClientRect().top?this.range.from-1:this.list.classList.contains(`cm-completionListIncompleteBottom`)&&n.clientY>this.list.lastChild.getBoundingClientRect().bottom?this.range.to:null;t!=null&&(e.dispatch({effects:kg.of(t)}),n.preventDefault())}}),this.dom.addEventListener(`focusout`,t=>{let n=e.state.field(this.stateField,!1);n&&n.tooltip&&e.state.facet(Z).closeOnBlur&&t.relatedTarget!=e.contentDOM&&e.dispatch({effects:wg.of(null)})}),this.showOptions(i,r.id)}mount(){this.updateSel()}showOptions(e,t){this.list&&this.list.remove(),this.list=this.dom.appendChild(this.createListBox(e,t,this.range)),this.list.addEventListener(`scroll`,()=>{this.info&&this.view.requestMeasure(this.placeInfoReq)})}update(e){let t=e.state.field(this.stateField),n=e.startState.field(this.stateField);if(this.updateTooltipClass(e.state),t!=n){let{options:r,selected:i,disabled:a}=t.open;n.open&&n.open.options==r||(this.range=jg(r.length,i,e.state.facet(Z).maxRenderedOptions),this.showOptions(r,t.id)),this.updateSel(),a!=n.open?.disabled&&this.dom.classList.toggle(`cm-tooltip-autocomplete-disabled`,!!a)}}updateTooltipClass(e){let t=this.tooltipClass(e);if(t!=this.currentClass){for(let e of this.currentClass.split(` `))e&&this.dom.classList.remove(e);for(let e of t.split(` `))e&&this.dom.classList.add(e);this.currentClass=t}}positioned(e){this.space=e,this.info&&this.view.requestMeasure(this.placeInfoReq)}updateSel(){let e=this.view.state.field(this.stateField),t=e.open;(t.selected>-1&&t.selected<this.range.from||t.selected>=this.range.to)&&(this.range=jg(t.options.length,t.selected,this.view.state.facet(Z).maxRenderedOptions),this.showOptions(t.options,e.id));let n=this.updateSelectedOption(t.selected);if(n){this.destroyInfo();let{completion:r}=t.options[t.selected],{info:i}=r;if(!i)return;let a=typeof i==`string`?document.createTextNode(i):i(r);if(!a)return;`then`in a?a.then(t=>{t&&this.view.state.field(this.stateField,!1)==e&&this.addInfoPane(t,r)}).catch(e=>Yr(this.view.state,e,`completion info`)):(this.addInfoPane(a,r),n.setAttribute(`aria-describedby`,this.info.id))}}addInfoPane(e,t){this.destroyInfo();let n=this.info=document.createElement(`div`);if(n.className=`cm-tooltip cm-completionInfo`,n.id=`cm-completionInfo-`+Math.floor(Math.random()*65535).toString(16),e.nodeType!=null)n.appendChild(e),this.infoDestroy=null;else{let{dom:t,destroy:r}=e;n.appendChild(t),this.infoDestroy=r||null}this.dom.appendChild(n),this.view.requestMeasure(this.placeInfoReq)}updateSelectedOption(e){let t=null;for(let n=this.list.firstChild,r=this.range.from;n;n=n.nextSibling,r++)n.nodeName!=`LI`||!n.id?r--:r==e?n.hasAttribute(`aria-selected`)||(n.setAttribute(`aria-selected`,`true`),t=n):n.hasAttribute(`aria-selected`)&&(n.removeAttribute(`aria-selected`),n.removeAttribute(`aria-describedby`));return t&&Pg(this.list,t),t}measureInfo(){let e=this.dom.querySelector(`[aria-selected]`);if(!e||!this.info)return null;let t=this.dom.getBoundingClientRect(),n=this.info.getBoundingClientRect(),r=e.getBoundingClientRect(),i=this.space;if(!i){let e=this.dom.ownerDocument.documentElement;i={left:0,top:0,right:e.clientWidth,bottom:e.clientHeight}}return r.top>Math.min(i.bottom,t.bottom)-10||r.bottom<Math.max(i.top,t.top)+10?null:this.view.state.facet(Z).positionInfo(this.view,t,r,n,i,this.dom)}placeInfo(e){this.info&&(e?(e.style&&(this.info.style.cssText=e.style),this.info.className=`cm-tooltip cm-completionInfo `+(e.class||``)):this.info.style.cssText=`top: -1e6px`)}createListBox(e,t,n){let r=document.createElement(`ul`);r.id=t,r.setAttribute(`role`,`listbox`),r.setAttribute(`aria-expanded`,`true`),r.setAttribute(`aria-label`,this.view.state.phrase(`Completions`)),r.addEventListener(`mousedown`,e=>{e.target==r&&e.preventDefault()});let i=null;for(let a=n.from;a<n.to;a++){let{completion:o,match:s}=e[a],{section:c}=o;if(c){let e=typeof c==`string`?c:c.name;if(e!=i&&(a>n.from||n.from==0)){if(i=e,typeof c!=`string`&&c.header)r.appendChild(c.header(c));else{let t=r.appendChild(document.createElement(`completion-section`));t.textContent=e}}}let l=r.appendChild(document.createElement(`li`));l.id=t+`-`+a,l.setAttribute(`role`,`option`);let u=this.optionClass(o);u&&(l.className=u);for(let e of this.optionContent){let t=e(o,this.view.state,this.view,s);t&&l.appendChild(t)}}return n.from&&r.classList.add(`cm-completionListIncompleteTop`),n.to<e.length&&r.classList.add(`cm-completionListIncompleteBottom`),r}destroyInfo(){this.info&&=(this.infoDestroy&&this.infoDestroy(),this.info.remove(),null)}destroy(){this.destroyInfo()}};function Ng(e,t){return n=>new Mg(n,e,t)}function Pg(e,t){let n=e.getBoundingClientRect(),r=t.getBoundingClientRect(),i=n.height/e.offsetHeight;r.top<n.top?e.scrollTop-=(n.top-r.top)/i:r.bottom>n.bottom&&(e.scrollTop+=(r.bottom-n.bottom)/i)}function Fg(e){return(e.boost||0)*100+(e.apply?10:0)+(e.info?5:0)+ +!!e.type}function Ig(e,t){let n=[],r=null,i=null,a=e=>{n.push(e);let{section:t}=e.completion;if(t){r||=[];let e=typeof t==`string`?t:t.name;r.some(t=>t.name==e)||r.push(typeof t==`string`?{name:e}:t)}},o=t.facet(Z);for(let r of e)if(r.hasResult()){let e=r.result.getMatch;if(r.result.filter===!1)for(let t of r.result.options)a(new gg(t,r.source,e?e(t):[],1e9-n.length));else{let n=t.sliceDoc(r.from,r.to),s,c=o.filterStrict?new Eg(n):new Tg(n);for(let t of r.result.options)if(s=c.match(t.label)){let n=t.displayLabel?e?e(t,s.matched):[]:s.matched,o=s.score+(t.boost||0);if(a(new gg(t,r.source,n,o)),typeof t.section==`object`&&t.section.rank===`dynamic`){let{name:e}=t.section;i||=Object.create(null),i[e]=Math.max(o,i[e]||-1e9)}}}}if(r){let e=Object.create(null),t=0,a=(e,t)=>(e.rank===`dynamic`&&t.rank===`dynamic`?i[t.name]-i[e.name]:0)||(typeof e.rank==`number`?e.rank:1e9)-(typeof t.rank==`number`?t.rank:1e9)||(e.name<t.name?-1:1);for(let n of r.sort(a))t-=1e5,e[n.name]=t;for(let t of n){let{section:n}=t.completion;n&&(t.score+=e[typeof n==`string`?n:n.name])}}let s=[],c=null,l=o.compareCompletions;for(let e of n.sort((e,t)=>t.score-e.score||l(e.completion,t.completion))){let t=e.completion;!c||c.label!=t.label||c.detail!=t.detail||c.type!=null&&t.type!=null&&c.type!=t.type||c.apply!=t.apply||c.boost!=t.boost?s.push(e):Fg(e.completion)>Fg(c)&&(s[s.length-1]=e),c=e.completion}return s}var Lg=class e{constructor(e,t,n,r,i,a){this.options=e,this.attrs=t,this.tooltip=n,this.timestamp=r,this.selected=i,this.disabled=a}setSelected(t,n){return t==this.selected||t>=this.options.length?this:new e(this.options,Hg(n,t),this.tooltip,this.timestamp,t,this.disabled)}static build(t,n,r,i,a,o){if(i&&!o&&t.some(e=>e.isPending))return i.setDisabled();let s=Ig(t,n);if(!s.length)return i&&t.some(e=>e.isPending)?i.setDisabled():null;let c=n.facet(Z).selectOnOpen?0:-1;if(i&&i.selected!=c&&i.selected!=-1){let e=i.options[i.selected].completion;for(let t=0;t<s.length;t++)if(s[t].completion==e){c=t;break}}return new e(s,Hg(r,c),{pos:t.reduce((e,t)=>t.hasResult()?Math.min(e,t.from):e,1e8),create:Zg,above:a.aboveCursor},i?i.timestamp:Date.now(),c,!1)}map(t){return new e(this.options,this.attrs,{...this.tooltip,pos:t.mapPos(this.tooltip.pos)},this.timestamp,this.selected,this.disabled)}setDisabled(){return new e(this.options,this.attrs,this.tooltip,this.timestamp,this.selected,!0)}},Rg=class e{constructor(e,t,n){this.active=e,this.id=t,this.open=n}static start(){return new e(Ug,`cm-ac-`+Math.floor(Math.random()*2e6).toString(36),null)}update(t){let{state:n}=t,r=n.facet(Z),i=(r.override||n.languageDataAt(`autocomplete`,_g(n)).map(Sg)).map(e=>(this.active.find(t=>t.source==e)||new Gg(e,+!!this.active.some(e=>e.state!=0))).update(t,r));i.length==this.active.length&&i.every((e,t)=>e==this.active[t])&&(i=this.active);let a=this.open,o=t.effects.some(e=>e.is(Jg));a&&t.docChanged&&(a=a.map(t.changes)),t.selection||i.some(e=>e.hasResult()&&t.changes.touchesRange(e.from,e.to))||!zg(i,this.active)||o?a=Lg.build(i,n,this.id,a,r,o):a&&a.disabled&&!i.some(e=>e.isPending)&&(a=null),!a&&i.every(e=>!e.isPending)&&i.some(e=>e.hasResult())&&(i=i.map(e=>e.hasResult()?new Gg(e.source,0):e));for(let e of t.effects)e.is(kg)&&(a&&=a.setSelected(e.value,this.id));return i==this.active&&a==this.open?this:new e(i,this.id,a)}get tooltip(){return this.open?this.open.tooltip:null}get attrs(){return this.open?this.open.attrs:this.active.length?Bg:Vg}};function zg(e,t){if(e==t)return!0;for(let n=0,r=0;;){for(;n<e.length&&!e[n].hasResult();)n++;for(;r<t.length&&!t[r].hasResult();)r++;let i=n==e.length,a=r==t.length;if(i||a)return i==a;if(e[n++].result!=t[r++].result)return!1}}var Bg={"aria-autocomplete":`list`},Vg={};function Hg(e,t){let n={"aria-autocomplete":`list`,"aria-haspopup":`listbox`,"aria-controls":e};return t>-1&&(n[`aria-activedescendant`]=e+`-`+t),n}var Ug=[];function Wg(e,t){if(e.isUserEvent(`input.complete`)){let n=e.annotation(yg);if(n&&t.activateOnCompletion(n))return 12}let n=e.isUserEvent(`input.type`);return n&&t.activateOnTyping?5:n?1:e.isUserEvent(`delete.backward`)?2:e.selection?8:e.docChanged?16:0}var Gg=class e{constructor(e,t,n=!1){this.source=e,this.state=t,this.explicit=n}hasResult(){return!1}get isPending(){return this.state==1}update(t,n){let r=Wg(t,n),i=this;(r&8||r&16&&this.touches(t))&&(i=new e(i.source,0)),r&4&&i.state==0&&(i=new e(this.source,1)),i=i.updateFor(t,r);for(let n of t.effects)if(n.is(Cg))i=new e(i.source,1,n.value);else if(n.is(wg))i=new e(i.source,0);else if(n.is(Jg))for(let e of n.value)e.source==i.source&&(i=e);return i}updateFor(e,t){return this.map(e.changes)}map(e){return this}touches(e){return e.changes.touchesRange(_g(e.state))}},Kg=class e extends Gg{constructor(e,t,n,r,i,a){super(e,3,t),this.limit=n,this.result=r,this.from=i,this.to=a}hasResult(){return!0}updateFor(t,n){if(!(n&3))return this.map(t.changes);let r=this.result;r.map&&!t.changes.empty&&(r=r.map(r,t.changes));let i=t.changes.mapPos(this.from),a=t.changes.mapPos(this.to,1),o=_g(t.state);if(o>a||!r||n&2&&(_g(t.startState)==this.from||o<this.limit))return new Gg(this.source,n&4?1:0);let s=t.changes.mapPos(this.limit);return qg(r.validFor,t.state,i,a)?new e(this.source,this.explicit,s,r,i,a):r.update&&(r=r.update(r,i,a,new fg(t.state,o,!1)))?new e(this.source,this.explicit,s,r,r.from,r.to??_g(t.state)):new Gg(this.source,1,this.explicit)}map(t){if(t.empty)return this;let n=this.result.map?this.result.map(this.result,t):this.result;return n?new e(this.source,this.explicit,t.mapPos(this.limit),n,t.mapPos(this.from),t.mapPos(this.to,1)):new Gg(this.source,0)}touches(e){return e.changes.touchesRange(this.from,this.to)}};function qg(e,t,n,r){if(!e)return!1;let i=t.sliceDoc(n,r);return typeof e==`function`?e(i,n,r,t):vg(e,!0).test(i)}var Jg=M.define({map(e,t){return e.map(e=>e.map(t))}}),Yg=Ye.define({create(){return Rg.start()},update(e,t){return e.update(t)},provide:e=>[kc.from(e,e=>e.tooltip),G.contentAttributes.from(e,e=>e.attrs)]});function Xg(e,t){let n=t.completion.apply||t.completion.label,r=e.state.field(Yg).active.find(e=>e.source==t.source);return r instanceof Kg&&(typeof n==`string`?e.dispatch({...bg(e.state,n,r.from,r.to),annotations:yg.of(t.completion)}):n(e,t.completion,r.from,r.to),!0)}var Zg=Ng(Yg,Xg);function Qg(e,t=`option`){return n=>{let r=n.state.field(Yg,!1);if(!r||!r.open||r.open.disabled||Date.now()-r.open.timestamp<n.state.facet(Z).interactionDelay)return!1;let i=1,a;t==`page`&&(a=Bc(n,r.open.tooltip))&&(i=Math.max(2,Math.floor(a.dom.offsetHeight/a.dom.querySelector(`li`).offsetHeight)-1));let{length:o}=r.open.options,s=r.open.selected>-1?r.open.selected+i*(e?1:-1):e?0:o-1;return s<0?s=t==`page`?0:o-1:s>=o&&(s=t==`page`?o-1:0),n.dispatch({effects:kg.of(s)}),!0}}var $g=e=>{let t=e.state.field(Yg,!1);return e.state.readOnly||!t||!t.open||t.open.selected<0||t.open.disabled||Date.now()-t.open.timestamp<e.state.facet(Z).interactionDelay?!1:Xg(e,t.open.options[t.open.selected])},e_=e=>e.state.field(Yg,!1)?(e.dispatch({effects:Cg.of(!0)}),!0):!1,t_=e=>{let t=e.state.field(Yg,!1);return!t||!t.active.some(e=>e.state!=0)?!1:(e.dispatch({effects:wg.of(null)}),!0)},n_=class{constructor(e,t){this.active=e,this.context=t,this.time=Date.now(),this.updates=[],this.done=void 0}},r_=50,i_=1e3,a_=H.fromClass(class{constructor(e){this.view=e,this.debounceUpdate=-1,this.running=[],this.debounceAccept=-1,this.pendingStart=!1,this.composing=0;for(let t of e.state.field(Yg).active)t.isPending&&this.startQuery(t)}update(e){let t=e.state.field(Yg),n=e.state.facet(Z);if(!e.selectionSet&&!e.docChanged&&e.startState.field(Yg)==t)return;let r=e.transactions.some(e=>{let t=Wg(e,n);return t&8||(e.selection||e.docChanged)&&!(t&3)});for(let t=0;t<this.running.length;t++){let n=this.running[t];if(r||n.context.abortOnDocChange&&e.docChanged||n.updates.length+e.transactions.length>r_&&Date.now()-n.time>i_){for(let e of n.context.abortListeners)try{e()}catch(e){Yr(this.view.state,e)}n.context.abortListeners=null,this.running.splice(t--,1)}else n.updates.push(...e.transactions)}this.debounceUpdate>-1&&clearTimeout(this.debounceUpdate),e.transactions.some(e=>e.effects.some(e=>e.is(Cg)))&&(this.pendingStart=!0);let i=this.pendingStart?50:n.activateOnTypingDelay;if(this.debounceUpdate=t.active.some(e=>e.isPending&&!this.running.some(t=>t.active.source==e.source))?setTimeout(()=>this.startUpdate(),i):-1,this.composing!=0)for(let t of e.transactions)t.isUserEvent(`input.type`)?this.composing=2:this.composing==2&&t.selection&&(this.composing=3)}startUpdate(){this.debounceUpdate=-1,this.pendingStart=!1;let{state:e}=this.view,t=e.field(Yg);for(let e of t.active)e.isPending&&!this.running.some(t=>t.active.source==e.source)&&this.startQuery(e);this.running.length&&t.open&&t.open.disabled&&(this.debounceAccept=setTimeout(()=>this.accept(),this.view.state.facet(Z).updateSyncTime))}startQuery(e){let{state:t}=this.view,n=new fg(t,_g(t),e.explicit,this.view),r=new n_(e,n);this.running.push(r),Promise.resolve(e.source(n)).then(e=>{r.context.aborted||(r.done=e||null,this.scheduleAccept())},e=>{this.view.dispatch({effects:wg.of(null)}),Yr(this.view.state,e)})}scheduleAccept(){this.running.every(e=>e.done!==void 0)?this.accept():this.debounceAccept<0&&(this.debounceAccept=setTimeout(()=>this.accept(),this.view.state.facet(Z).updateSyncTime))}accept(){this.debounceAccept>-1&&clearTimeout(this.debounceAccept),this.debounceAccept=-1;let e=[],t=this.view.state.facet(Z),n=this.view.state.field(Yg);for(let r=0;r<this.running.length;r++){let i=this.running[r];if(i.done===void 0)continue;if(this.running.splice(r--,1),i.done){let n=_g(i.updates.length?i.updates[0].startState:this.view.state),r=Math.min(n,i.done.from+ +!i.active.explicit),a=new Kg(i.active.source,i.active.explicit,r,i.done,i.done.from,i.done.to??n);for(let e of i.updates)a=a.update(e,t);if(a.hasResult()){e.push(a);continue}}let a=n.active.find(e=>e.source==i.active.source);if(a&&a.isPending){if(i.done==null){let n=new Gg(i.active.source,0);for(let e of i.updates)n=n.update(e,t);n.isPending||e.push(n)}else this.startQuery(a)}}(e.length||n.open&&n.open.disabled)&&this.view.dispatch({effects:Jg.of(e)})}},{eventHandlers:{blur(e){let t=this.view.state.field(Yg,!1);if(t&&t.tooltip&&this.view.state.facet(Z).closeOnBlur){let n=t.open&&Bc(this.view,t.open.tooltip);n&&n.dom.contains(e.relatedTarget)||setTimeout(()=>this.view.dispatch({effects:wg.of(null)}),10)}},compositionstart(){this.composing=1},compositionend(){this.composing==3&&setTimeout(()=>this.view.dispatch({effects:Cg.of(!1)}),20),this.composing=0}}}),o_=typeof navigator==`object`&&/Win/.test(navigator.platform),s_=Qe.highest(G.domEventHandlers({keydown(e,t){let n=t.state.field(Yg,!1);if(!n||!n.open||n.open.disabled||n.open.selected<0||e.key.length>1||e.ctrlKey&&!(o_&&e.altKey)||e.metaKey)return!1;let r=n.open.options[n.open.selected],i=n.active.find(e=>e.source==r.source),a=r.completion.commitCharacters||i.result.commitCharacters;return a&&a.indexOf(e.key)>-1&&Xg(t,r),!1}})),c_=G.baseTheme({".cm-tooltip.cm-tooltip-autocomplete":{"& > ul":{fontFamily:`monospace`,whiteSpace:`nowrap`,overflow:`hidden auto`,maxWidth_fallback:`700px`,maxWidth:`min(700px, 95vw)`,minWidth:`250px`,maxHeight:`10em`,height:`100%`,listStyle:`none`,margin:0,padding:0,"& > li, & > completion-section":{padding:`1px 3px`,lineHeight:1.2},"& > li":{overflowX:`hidden`,textOverflow:`ellipsis`,cursor:`pointer`},"& > completion-section":{display:`list-item`,borderBottom:`1px solid silver`,paddingLeft:`0.5em`,opacity:.7}}},"&light .cm-tooltip-autocomplete ul li[aria-selected]":{background:`#17c`,color:`white`},"&light .cm-tooltip-autocomplete-disabled ul li[aria-selected]":{background:`#777`},"&dark .cm-tooltip-autocomplete ul li[aria-selected]":{background:`#347`,color:`white`},"&dark .cm-tooltip-autocomplete-disabled ul li[aria-selected]":{background:`#444`},".cm-completionListIncompleteTop:before, .cm-completionListIncompleteBottom:after":{content:`"···"`,opacity:.5,display:`block`,textAlign:`center`,cursor:`pointer`},".cm-tooltip.cm-completionInfo":{position:`absolute`,padding:`3px 9px`,width:`max-content`,maxWidth:`400px`,boxSizing:`border-box`,whiteSpace:`pre-line`},".cm-completionInfo.cm-completionInfo-left":{right:`100%`},".cm-completionInfo.cm-completionInfo-right":{left:`100%`},".cm-completionInfo.cm-completionInfo-left-narrow":{right:`30px`},".cm-completionInfo.cm-completionInfo-right-narrow":{left:`30px`},"&light .cm-snippetField":{backgroundColor:`#00000022`},"&dark .cm-snippetField":{backgroundColor:`#ffffff22`},".cm-snippetFieldPosition":{verticalAlign:`text-top`,width:0,height:`1.15em`,display:`inline-block`,margin:`0 -0.7px -.7em`,borderLeft:`1.4px dotted #888`},".cm-completionMatchedText":{textDecoration:`underline`},".cm-completionDetail":{marginLeft:`0.5em`,fontStyle:`italic`},".cm-completionIcon":{fontSize:`90%`,width:`.8em`,display:`inline-block`,textAlign:`center`,paddingRight:`.6em`,opacity:`0.6`,boxSizing:`content-box`},".cm-completionIcon-function, .cm-completionIcon-method":{"&:after":{content:`'ƒ'`}},".cm-completionIcon-class":{"&:after":{content:`'○'`}},".cm-completionIcon-interface":{"&:after":{content:`'◌'`}},".cm-completionIcon-variable":{"&:after":{content:`'𝑥'`}},".cm-completionIcon-constant":{"&:after":{content:`'𝐶'`}},".cm-completionIcon-type":{"&:after":{content:`'𝑡'`}},".cm-completionIcon-enum":{"&:after":{content:`'∪'`}},".cm-completionIcon-property":{"&:after":{content:`'□'`}},".cm-completionIcon-keyword":{"&:after":{content:`'🔑︎'`}},".cm-completionIcon-namespace":{"&:after":{content:`'▢'`}},".cm-completionIcon-text":{"&:after":{content:`'abc'`,fontSize:`50%`,verticalAlign:`middle`}}}),l_={brackets:[`(`,`[`,`{`,`'`,`"`],before:`)]}:;>`,stringPrefixes:[]},u_=M.define({map(e,t){return t.mapPos(e,-1,Me.TrackAfter)??void 0}}),d_=new class extends At{};d_.startSide=1,d_.endSide=-1;var f_=Ye.define({create(){return F.empty},update(e,t){if(e=e.map(t.changes),t.selection){let n=t.state.doc.lineAt(t.selection.main.head);e=e.update({filter:e=>e>=n.from&&e<=n.to})}for(let n of t.effects)n.is(u_)&&(e=e.update({add:[d_.range(n.value,n.value+1)]}));return e}});function p_(){return[v_,f_]}var m_=`()[]{}<>«»»«［］｛｝`;function h_(e){for(let t=0;t<16;t+=2)if(m_.charCodeAt(t)==e)return m_.charAt(t+1);return ke(e<128?e:e+1)}function g_(e,t){return e.languageDataAt(`closeBrackets`,t)[0]||l_}var __=typeof navigator==`object`&&/Android\b/.test(navigator.userAgent),v_=G.inputHandler.of((e,t,n,r)=>{if((__?e.composing:e.compositionStarted)||e.state.readOnly)return!1;let i=e.state.selection.main;if(r.length>2||r.length==2&&Ae(Oe(r,0))==1||t!=i.from||n!=i.to)return!1;let a=b_(e.state,r);return a?(e.dispatch(a),!0):!1}),y_=[{key:`Backspace`,run:({state:e,dispatch:t})=>{if(e.readOnly)return!1;let n=g_(e,e.selection.main.head).brackets||l_.brackets,r=null,i=e.changeByRange(t=>{if(t.empty){let r=C_(e.doc,t.head);for(let i of n)if(i==r&&S_(e.doc,t.head)==h_(Oe(i,0)))return{changes:{from:t.head-i.length,to:t.head+i.length},range:A.cursor(t.head-i.length)}}return{range:r=t}});return r||t(e.update(i,{scrollIntoView:!0,userEvent:`delete.backward`})),!r}}];function b_(e,t){let n=g_(e,e.selection.main.head),r=n.brackets||l_.brackets;for(let i of r){let a=h_(Oe(i,0));if(t==i)return a==i?E_(e,i,r.indexOf(i+i+i)>-1,n):w_(e,i,a,n.before||l_.before);if(t==a&&x_(e,e.selection.main.from))return T_(e,i,a)}return null}function x_(e,t){let n=!1;return e.field(f_).between(0,e.doc.length,e=>{e==t&&(n=!0)}),n}function S_(e,t){let n=e.sliceString(t,t+2);return n.slice(0,Ae(Oe(n,0)))}function C_(e,t){let n=e.sliceString(t-2,t);return Ae(Oe(n,0))==n.length?n:n.slice(1)}function w_(e,t,n,r){let i=null,a=e.changeByRange(a=>{if(!a.empty)return{changes:[{insert:t,from:a.from},{insert:n,from:a.to}],effects:u_.of(a.to+t.length),range:A.range(a.anchor+t.length,a.head+t.length)};let o=S_(e.doc,a.head);return!o||/\s/.test(o)||r.indexOf(o)>-1?{changes:{insert:t+n,from:a.head},effects:u_.of(a.head+t.length),range:A.cursor(a.head+t.length)}:{range:i=a}});return i?null:e.update(a,{scrollIntoView:!0,userEvent:`input.type`})}function T_(e,t,n){let r=null,i=e.changeByRange(t=>t.empty&&S_(e.doc,t.head)==n?{changes:{from:t.head,to:t.head+n.length,insert:n},range:A.cursor(t.head+n.length)}:r={range:t});return r?null:e.update(i,{scrollIntoView:!0,userEvent:`input.type`})}function E_(e,t,n,r){let i=r.stringPrefixes||l_.stringPrefixes,a=null,o=e.changeByRange(r=>{if(!r.empty)return{changes:[{insert:t,from:r.from},{insert:t,from:r.to}],effects:u_.of(r.to+t.length),range:A.range(r.anchor+t.length,r.head+t.length)};let o=r.head,s=S_(e.doc,o),c;if(s==t){if(D_(e,o))return{changes:{insert:t+t,from:o},effects:u_.of(o+t.length),range:A.cursor(o+t.length)};if(x_(e,o)){let r=n&&e.sliceDoc(o,o+t.length*3)==t+t+t?t+t+t:t;return{changes:{from:o,to:o+r.length,insert:r},range:A.cursor(o+r.length)}}}else if(n&&e.sliceDoc(o-2*t.length,o)==t+t&&(c=k_(e,o-2*t.length,i))>-1&&D_(e,c))return{changes:{insert:t+t+t+t,from:o},effects:u_.of(o+t.length),range:A.cursor(o+t.length)};else if(e.charCategorizer(o)(s)!=N.Word&&k_(e,o,i)>-1&&!O_(e,o,t,i))return{changes:{insert:t+t,from:o},effects:u_.of(o+t.length),range:A.cursor(o+t.length)};return{range:a=r}});return a?null:e.update(o,{scrollIntoView:!0,userEvent:`input.type`})}function D_(e,t){let n=Ru(e).resolveInner(t+1);return n.parent&&n.from==t}function O_(e,t,n,r){let i=Ru(e).resolveInner(t,-1),a=r.reduce((e,t)=>Math.max(e,t.length),0);for(let o=0;o<5;o++){let o=e.sliceDoc(i.from,Math.min(i.to,i.from+n.length+a)),s=o.indexOf(n);if(!s||s>-1&&r.indexOf(o.slice(0,s))>-1){let t=i.firstChild;for(;t&&t.from==i.from&&t.to-t.from>n.length+s;){if(e.sliceDoc(t.to-n.length,t.to)==n)return!1;t=t.firstChild}return!0}let c=i.to==t&&i.parent;if(!c)break;i=c}return!1}function k_(e,t,n){let r=e.charCategorizer(t);if(r(e.sliceDoc(t-1,t))!=N.Word)return t;for(let i of n){let n=t-i.length;if(e.sliceDoc(n,t)==i&&r(e.sliceDoc(n-1,n))!=N.Word)return n}return-1}function A_(e={}){return[s_,Yg,Z.of(e),a_,M_,c_]}var j_=[{key:`Ctrl-Space`,run:e_},{mac:"Alt-`",run:e_},{mac:`Alt-i`,run:e_},{key:`Escape`,run:t_},{key:`ArrowDown`,run:Qg(!0)},{key:`ArrowUp`,run:Qg(!1)},{key:`PageDown`,run:Qg(!0,`page`)},{key:`PageUp`,run:Qg(!1,`page`)},{key:`Enter`,run:$g}],M_=Qe.highest(hs.computeN([Z],e=>e.facet(Z).defaultKeymap?[j_]:[])),N_=class{constructor(e,t,n){this.from=e,this.to=t,this.diagnostic=n}},P_=class e{constructor(e,t,n){this.diagnostics=e,this.panel=t,this.selected=n}static init(t,n,r){let i=r.facet(Z_).markerFilter;i&&(t=i(t,r));let a=t.slice().sort((e,t)=>e.from-t.from||e.to-t.to),o=new Lt,s=[],c=0,l=r.doc.iter(),u=0,d=r.doc.length;for(let e=0;;){let t=e==a.length?null:a[e];if(!t&&!s.length)break;let n,r;if(s.length)n=c,r=s.reduce((e,t)=>Math.min(e,t.to),t&&t.from>n?t.from:1e8);else{if(n=t.from,n>d)break;r=t.to,s.push(t),e++}for(;e<a.length;){let t=a[e];if(t.from==n&&(t.to>t.from||t.to==n))s.push(t),e++,r=Math.min(t.to,r);else{r=Math.min(t.from,r);break}}r=Math.min(r,d);let i=!1;if(s.some(e=>e.from==n&&(e.to==r||r==d))&&(i=n==r,!i&&r-n<10)){let e=n-(u+l.value.length);e>0&&(l.next(e),u=n);for(let e=n;;){if(e>=r){i=!0;break}if(!l.lineBreak&&u+l.value.length>e)break;e=u+l.value.length,u+=l.value.length,l.next()}}let f=lv(s);if(i)o.add(n,n,R.widget({widget:new nv(f),diagnostics:s.slice()}));else{let e=s.reduce((e,t)=>t.markClass?e+` `+t.markClass:e,``);o.add(n,r,R.mark({class:`cm-lintRange cm-lintRange-`+f+e,diagnostics:s.slice(),inclusiveEnd:s.some(e=>e.to>r)}))}if(c=r,c==d)break;for(let e=0;e<s.length;e++)s[e].to<=c&&s.splice(e--,1)}let f=o.finish();return new e(f,n,F_(f))}};function F_(e,t=null,n=0){let r=null;return e.between(n,1e9,(e,n,{spec:i})=>{if(!(t&&i.diagnostics.indexOf(t)<0)){if(!r)r=new N_(e,n,t||i.diagnostics[0]);else if(i.diagnostics.indexOf(r.diagnostic)<0)return!1;else r=new N_(r.from,n,r.diagnostic)}}),r}function I_(e,t){let n=t.pos,r=t.end||n,i=e.state.facet(Z_).hideOn(e,n,r);if(i!=null)return i;let a=e.startState.doc.lineAt(t.pos);return!!(e.effects.some(e=>e.is(z_))||e.changes.touchesRange(a.from,Math.max(a.to,r)))}function L_(e,t){return e.field(H_,!1)?t:t.concat(M.appendConfig.of(bv))}function R_(e,t){return{effects:L_(e,[z_.of(t)])}}var z_=M.define(),B_=M.define(),V_=M.define(),H_=Ye.define({create(){return new P_(R.none,null,null)},update(e,t){if(t.docChanged&&e.diagnostics.size){let n=e.diagnostics.map(t.changes),r=null,i=e.panel;if(e.selected){let i=t.changes.mapPos(e.selected.from,1);r=F_(n,e.selected.diagnostic,i)||F_(n,null,i)}!n.size&&i&&t.state.facet(Z_).autoPanel&&(i=null),e=new P_(n,i,r)}for(let n of t.effects)if(n.is(z_)){let r=t.state.facet(Z_).autoPanel?n.value.length?iv.open:null:e.panel;e=P_.init(n.value,r,t.state)}else n.is(B_)?e=new P_(e.diagnostics,n.value?iv.open:null,e.selected):n.is(V_)&&(e=new P_(e.diagnostics,e.panel,n.value));return e},provide:e=>[qc.from(e,e=>e.panel),G.decorations.from(e,e=>e.diagnostics)]}),U_=R.mark({class:`cm-lintRange cm-lintRange-active`});function W_(e,t,n){let{diagnostics:r}=e.state.field(H_),i,a=-1,o=-1;r.between(t-+(n<0),t+ +(n>0),(e,r,{spec:s})=>{if(t>=e&&t<=r&&(e==r||(t>e||n>0)&&(t<r||n<0)))return i=s.diagnostics,a=e,o=r,!1});let s=e.state.facet(Z_).tooltipFilter;return i&&s&&(i=s(i,e.state)),i?{pos:a,end:o,above:!0,create(){return{dom:G_(e,i)}}}:null}function G_(e,t){return I(`ul`,{class:`cm-tooltip-lint`},t.map(t=>tv(e,t,!1)))}var K_=e=>{let t=e.state.field(H_,!1);t&&t.panel||e.dispatch({effects:L_(e.state,[B_.of(!0)])});let n=Uc(e,iv.open);return n&&n.dom.querySelector(`.cm-panel-lint ul`).focus(),!0},q_=e=>{let t=e.state.field(H_,!1);return!t||!t.panel?!1:(e.dispatch({effects:B_.of(!1)}),!0)},J_=[{key:`Mod-Shift-m`,run:K_,preventDefault:!0},{key:`F8`,run:e=>{let t=e.state.field(H_,!1);if(!t)return!1;let n=e.state.selection.main,r=F_(t.diagnostics,null,n.to+1);return!r&&(r=F_(t.diagnostics,null,0),!r||r.from==n.from&&r.to==n.to)?!1:(e.dispatch({selection:{anchor:r.from,head:r.to},scrollIntoView:!0}),zc(e,r.from,1,{tooltip:yv,until:e=>e.docChanged||e.newSelection.main.head<r.from||e.newSelection.main.head>r.to}),!0)}}],Y_=H.fromClass(class{constructor(e){this.view=e,this.timeout=-1,this.set=!0;let{delay:t}=e.state.facet(Z_);this.lintTime=Date.now()+t,this.run=this.run.bind(this),this.timeout=setTimeout(this.run,t)}run(){clearTimeout(this.timeout);let e=Date.now();if(e<this.lintTime-10)this.timeout=setTimeout(this.run,this.lintTime-e);else{this.set=!1;let{state:e}=this.view,{sources:t}=e.facet(Z_);t.length&&X_(t.map(e=>Promise.resolve(e(this.view))),t=>{this.view.state.doc==e.doc&&this.view.dispatch(R_(this.view.state,t.reduce((e,t)=>e.concat(t))))},e=>{Yr(this.view.state,e)})}}update(e){let t=e.state.facet(Z_);(e.docChanged||t!=e.startState.facet(Z_)||t.needsRefresh&&t.needsRefresh(e))&&(this.lintTime=Date.now()+t.delay,this.set||(this.set=!0,this.timeout=setTimeout(this.run,t.delay)))}force(){this.set&&(this.lintTime=Date.now(),this.run())}destroy(){clearTimeout(this.timeout)}});function X_(e,t,n){let r=[],i=-1;for(let a of e)a.then(n=>{r.push(n),clearTimeout(i),r.length==e.length?t(r):i=setTimeout(()=>t(r),200)},n)}var Z_=j.define({combine(e){return{sources:e.map(e=>e.source).filter(e=>e!=null),...kt(e.map(e=>e.config),{delay:750,markerFilter:null,tooltipFilter:null,needsRefresh:null,hideOn:()=>null},{delay:Math.max,markerFilter:Q_,tooltipFilter:Q_,needsRefresh:(e,t)=>e?t?n=>e(n)||t(n):e:t,hideOn:(e,t)=>e?t?(n,r,i)=>e(n,r,i)||t(n,r,i):e:t,autoPanel:(e,t)=>e||t})}}});function Q_(e,t){return e?t?(n,r)=>t(e(n,r),r):e:t}function $_(e,t={}){return[Z_.of({source:e,config:t}),Y_,bv]}function ev(e){let t=[];if(e)actions:for(let{name:n}of e){for(let e=0;e<n.length;e++){let r=n[e];if(/[a-zA-Z]/.test(r)&&!t.some(e=>e.toLowerCase()==r.toLowerCase())){t.push(r);continue actions}}t.push(``)}return t}function tv(e,t,n){let r=n?ev(t.actions):[];return I(`li`,{class:`cm-diagnostic cm-diagnostic-`+t.severity},I(`span`,{class:`cm-diagnosticText`},t.renderMessage?t.renderMessage(e):t.message),t.actions?.map((n,i)=>{let a=!1,o=r=>{if(r.preventDefault(),a)return;a=!0;let i=F_(e.state.field(H_).diagnostics,t);i&&n.apply(e,i.from,i.to)},{name:s}=n,c=r[i]?s.indexOf(r[i]):-1,l=c<0?s:[s.slice(0,c),I(`u`,s.slice(c,c+1)),s.slice(c+1)];return I(`button`,{type:`button`,class:`cm-diagnosticAction`+(n.markClass?` `+n.markClass:``),onclick:o,onmousedown:o,"aria-label":` Action: ${s}${c<0?``:` (access key "${r[i]})"`}.`},l)}),t.source&&I(`div`,{class:`cm-diagnosticSource`},t.source))}var nv=class extends kn{constructor(e){super(),this.sev=e}eq(e){return e.sev==this.sev}toDOM(){return I(`span`,{class:`cm-lintPoint cm-lintPoint-`+this.sev})}},rv=class{constructor(e,t){this.diagnostic=t,this.id=`item_`+Math.floor(Math.random()*4294967295).toString(16),this.dom=tv(e,t,!0),this.dom.id=this.id,this.dom.setAttribute(`role`,`option`)}},iv=class e{constructor(e){this.view=e,this.items=[];let t=t=>{if(!(t.ctrlKey||t.altKey||t.metaKey)){if(t.keyCode==27)q_(this.view),this.view.focus();else if(t.keyCode==38||t.keyCode==33)this.moveSelection((this.selectedIndex-1+this.items.length)%this.items.length);else if(t.keyCode==40||t.keyCode==34)this.moveSelection((this.selectedIndex+1)%this.items.length);else if(t.keyCode==36)this.moveSelection(0);else if(t.keyCode==35)this.moveSelection(this.items.length-1);else if(t.keyCode==13)this.view.focus();else if(t.keyCode>=65&&t.keyCode<=90&&this.selectedIndex>=0){let{diagnostic:n}=this.items[this.selectedIndex],r=ev(n.actions);for(let i=0;i<r.length;i++)if(r[i].toUpperCase().charCodeAt(0)==t.keyCode){let t=F_(this.view.state.field(H_).diagnostics,n);t&&n.actions[i].apply(e,t.from,t.to)}}else return;t.preventDefault()}},n=e=>{for(let t=0;t<this.items.length;t++)this.items[t].dom.contains(e.target)&&this.moveSelection(t)};this.list=I(`ul`,{tabIndex:0,role:`listbox`,"aria-label":this.view.state.phrase(`Diagnostics`),onkeydown:t,onclick:n}),this.dom=I(`div`,{class:`cm-panel-lint`},this.list,I(`button`,{type:`button`,name:`close`,"aria-label":this.view.state.phrase(`close`),onclick:()=>q_(this.view)},`×`)),this.update()}get selectedIndex(){let e=this.view.state.field(H_).selected;if(!e)return-1;for(let t=0;t<this.items.length;t++)if(this.items[t].diagnostic==e.diagnostic)return t;return-1}update(){let{diagnostics:e,selected:t}=this.view.state.field(H_),n=0,r=!1,i=null,a=new Set;for(e.between(0,this.view.state.doc.length,(e,o,{spec:s})=>{for(let e of s.diagnostics){if(a.has(e))continue;a.add(e);let o=-1,s;for(let t=n;t<this.items.length;t++)if(this.items[t].diagnostic==e){o=t;break}o<0?(s=new rv(this.view,e),this.items.splice(n,0,s),r=!0):(s=this.items[o],o>n&&(this.items.splice(n,o-n),r=!0)),t&&s.diagnostic==t.diagnostic?s.dom.hasAttribute(`aria-selected`)||(s.dom.setAttribute(`aria-selected`,`true`),i=s):s.dom.hasAttribute(`aria-selected`)&&s.dom.removeAttribute(`aria-selected`),n++}});n<this.items.length&&!(this.items.length==1&&this.items[0].diagnostic.from<0);)r=!0,this.items.pop();this.items.length==0&&(this.items.push(new rv(this.view,{from:-1,to:-1,severity:`info`,message:this.view.state.phrase(`No diagnostics`)})),r=!0),i?(this.list.setAttribute(`aria-activedescendant`,i.id),this.view.requestMeasure({key:this,read:()=>({sel:i.dom.getBoundingClientRect(),panel:this.list.getBoundingClientRect()}),write:({sel:e,panel:t})=>{let n=t.height/this.list.offsetHeight;e.top<t.top?this.list.scrollTop-=(t.top-e.top)/n:e.bottom>t.bottom&&(this.list.scrollTop+=(e.bottom-t.bottom)/n)}})):this.selectedIndex<0&&this.list.removeAttribute(`aria-activedescendant`),r&&this.sync()}sync(){let e=this.list.firstChild;function t(){let t=e;e=t.nextSibling,t.remove()}for(let n of this.items)if(n.dom.parentNode==this.list){for(;e!=n.dom;)t();e=n.dom.nextSibling}else this.list.insertBefore(n.dom,e);for(;e;)t()}moveSelection(e){if(this.selectedIndex<0)return;let t=F_(this.view.state.field(H_).diagnostics,this.items[e].diagnostic);t&&this.view.dispatch({selection:{anchor:t.from,head:t.to},scrollIntoView:!0,effects:V_.of(t)})}static open(t){return new e(t)}};function av(e,t=`viewBox="0 0 40 40"`){return`url('data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" ${t}>${encodeURIComponent(e)}</svg>')`}function ov(e){return av(`<path d="m0 2.5 l2 -1.5 l1 0 l2 1.5 l1 0" stroke="${e}" fill="none" stroke-width=".7"/>`,`width="6" height="3"`)}var sv=G.baseTheme({".cm-diagnostic":{padding:`3px 6px 3px 8px`,marginLeft:`-1px`,display:`block`,whiteSpace:`pre-wrap`},".cm-diagnostic-error":{borderLeft:`5px solid #d11`},".cm-diagnostic-warning":{borderLeft:`5px solid orange`},".cm-diagnostic-info":{borderLeft:`5px solid #999`},".cm-diagnostic-hint":{borderLeft:`5px solid #66d`},".cm-diagnosticAction":{font:`inherit`,border:`none`,padding:`2px 4px`,backgroundColor:`#444`,color:`white`,borderRadius:`3px`,marginLeft:`8px`,cursor:`pointer`},".cm-diagnosticSource":{fontSize:`70%`,opacity:.7},".cm-lintRange":{backgroundPosition:`left bottom`,backgroundRepeat:`repeat-x`,paddingBottom:`0.7px`},".cm-lintRange-error":{backgroundImage:ov(`#f11`)},".cm-lintRange-warning":{backgroundImage:ov(`orange`)},".cm-lintRange-info":{backgroundImage:ov(`#999`)},".cm-lintRange-hint":{backgroundImage:ov(`#66d`)},".cm-lintRange-active":{backgroundColor:`#ffdd9980`},".cm-tooltip-lint":{padding:0,margin:0},".cm-lintPoint":{position:`relative`,"&:after":{content:`""`,position:`absolute`,bottom:0,left:`-2px`,borderLeft:`3px solid transparent`,borderRight:`3px solid transparent`,borderBottom:`4px solid #d11`}},".cm-lintPoint-warning":{"&:after":{borderBottomColor:`orange`}},".cm-lintPoint-info":{"&:after":{borderBottomColor:`#999`}},".cm-lintPoint-hint":{"&:after":{borderBottomColor:`#66d`}},".cm-panel.cm-panel-lint":{position:`relative`,"& ul":{maxHeight:`100px`,overflowY:`auto`,"& [aria-selected]":{backgroundColor:`#ddd`,"& u":{textDecoration:`underline`}},"&:focus [aria-selected]":{background_fallback:`#bdf`,backgroundColor:`Highlight`,color_fallback:`white`,color:`HighlightText`},"& u":{textDecoration:`none`},padding:0,margin:0},"& [name=close]":{position:`absolute`,top:`0`,right:`2px`,background:`inherit`,border:`none`,font:`inherit`,padding:0,margin:0}},"&dark .cm-lintRange-active":{backgroundColor:`#86714a80`},"&dark .cm-panel.cm-panel-lint ul":{"& [aria-selected]":{backgroundColor:`#2e343e`}}});function cv(e){return e==`error`?4:e==`warning`?3:e==`info`?2:1}function lv(e){let t=`hint`,n=1;for(let r of e){let e=cv(r.severity);e>n&&(n=e,t=r.severity)}return t}var uv=class extends el{constructor(e){super(),this.diagnostics=e,this.severity=lv(e)}toDOM(e){let t=document.createElement(`div`);t.className=`cm-lint-marker cm-lint-marker-`+this.severity;let n=this.diagnostics,r=e.state.facet(xv).tooltipFilter;return r&&(n=r(n,e.state)),n.length&&(t.onmouseover=()=>fv(e,t,n)),t}};function dv(e,t){let n=r=>{let i=t.getBoundingClientRect();if(!(r.clientX>i.left-10&&r.clientX<i.right+10&&r.clientY>i.top-10&&r.clientY<i.bottom+10)){for(let e=r.target;e;e=e.parentNode)if(e.nodeType==1&&e.classList.contains(`cm-tooltip-lint`))return;window.removeEventListener(`mousemove`,n),e.state.field(_v)&&e.dispatch({effects:gv.of(null)})}};window.addEventListener(`mousemove`,n)}function fv(e,t,n){function r(){let r=e.elementAtHeight(t.getBoundingClientRect().top+5-e.documentTop);e.coordsAtPos(r.from)&&e.dispatch({effects:gv.of({pos:r.from,above:!1,clip:!1,create(){return{dom:G_(e,n),getCoords:()=>t.getBoundingClientRect()}}})}),t.onmouseout=t.onmousemove=null,dv(e,t)}let{hoverTime:i}=e.state.facet(xv),a=setTimeout(r,i);t.onmouseout=()=>{clearTimeout(a),t.onmouseout=t.onmousemove=null},t.onmousemove=()=>{clearTimeout(a),a=setTimeout(r,i)}}function pv(e,t){let n=Object.create(null);for(let r of t){let t=e.lineAt(r.from);(n[t.from]||(n[t.from]=[])).push(r)}let r=[];for(let e in n)r.push(new uv(n[e]).range(+e));return F.of(r,!0)}var mv=al({class:`cm-gutter-lint`,markers:e=>e.state.field(hv),widgetMarker:(e,t,n)=>{let r=[];return e.state.field(hv).between(n.from,n.to,(e,t,i)=>{e>n.from&&e<n.to&&r.push(...i.diagnostics)}),r.length?new uv(r):null}}),hv=Ye.define({create(){return F.empty},update(e,t){e=e.map(t.changes);let n=t.state.facet(xv).markerFilter;for(let r of t.effects)if(r.is(z_)){let i=r.value;n&&(i=n(i||[],t.state)),e=pv(t.state.doc,i.slice(0))}return e}}),gv=M.define(),_v=Ye.define({create(){return null},update(e,t){return e&&t.docChanged&&(e=I_(t,e)?null:{...e,pos:t.changes.mapPos(e.pos)}),t.effects.reduce((e,t)=>t.is(gv)?t.value:e,e)},provide:e=>kc.from(e)}),vv=G.baseTheme({".cm-gutter-lint":{width:`1.4em`,"& .cm-gutterElement":{padding:`.2em`}},".cm-lint-marker":{width:`1em`,height:`1em`},".cm-lint-marker-info":{content:av(`<path fill="#aaf" stroke="#77e" stroke-width="6" stroke-linejoin="round" d="M5 5L35 5L35 35L5 35Z"/>`)},".cm-lint-marker-warning":{content:av(`<path fill="#fe8" stroke="#fd7" stroke-width="6" stroke-linejoin="round" d="M20 6L37 35L3 35Z"/>`)},".cm-lint-marker-error":{content:av(`<circle cx="20" cy="20" r="15" fill="#f87" stroke="#f43" stroke-width="6"/>`)}}),yv=Rc(W_,{hideOn:I_}),bv=[H_,G.decorations.compute([H_],e=>{let{selected:t,panel:n}=e.field(H_);return!t||!n||t.from==t.to?R.none:R.set([U_.range(t.from,t.to)])}),yv,sv],xv=j.define({combine(e){return kt(e,{hoverTime:300,markerFilter:null,tooltipFilter:null})}});function Sv(e={}){return[xv.of(e),hv,mv,vv,_v]}var Cv=[xl(),Tl(),ec(),qf(),Rd(),Ps(),Ws(),P.allowMultipleSelections.of(!0),dd(),Wd(qd,{fallback:!0}),rf(),p_(),A_(),hc(),vc(),sc(),hh(),hs.of([...y_,...eh,...ig,...fp,...kd,...j_,...J_])],wv=mf.define(s),Q=c(),Tv=Bd.define([{tag:X.keyword,color:`var(--syn-keyword)`,fontWeight:`600`},{tag:X.typeName,color:`var(--syn-type)`},{tag:X.className,color:`var(--syn-event)`},{tag:X.special(X.variableName),color:`var(--syn-name)`},{tag:X.string,color:`var(--syn-string)`},{tag:X.number,color:`var(--syn-number)`},{tag:X.atom,color:`var(--syn-number)`},{tag:X.comment,color:`var(--syn-comment)`,fontStyle:`italic`},{tag:X.operator,color:`var(--syn-op)`}]),Ev=G.theme({"&":{height:`100%`,fontSize:`var(--editor-font, 13px)`,backgroundColor:`var(--editor-bg)`,color:`var(--fg)`},".cm-scroller":{fontFamily:`var(--mono)`,lineHeight:`1.5`},".cm-gutters":{backgroundColor:`var(--editor-bg)`,color:`var(--muted)`,border:`none`},".cm-activeLine, .cm-activeLineGutter":{backgroundColor:`var(--active-line)`},"&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection":{backgroundColor:`var(--selection) !important`},".cm-cursor":{borderLeftColor:`var(--fg)`},".cm-tooltip":{backgroundColor:`var(--panel)`,color:`var(--fg)`,border:`1px solid var(--border)`},".cm-flashLine":{backgroundColor:`var(--flash)`},"&.cm-lineWrapping .cm-line":{paddingLeft:`calc(2ch + 6px)`,textIndent:`-2ch`}});function Dv({path:e,value:t,diagnostics:n,jump:r,onChange:i,onRun:a}){let o=(0,l.useRef)(null),s=(0,l.useRef)(void 0),c=(0,l.useRef)(new Map),u=(0,l.useRef)(e),d=(0,l.useRef)({onChange:i,onRun:a});d.current={onChange:i,onRun:a};let f=(0,l.useRef)([Cv,hs.of([{key:`Mod-Enter`,run:()=>(d.current.onRun(),!0)},th]),wv,G.lineWrapping,Wd(Tv),Ev,Sv(),$_(null),G.updateListener.of(e=>{e.docChanged&&d.current.onChange(u.current,e.state.doc.toString())})]),p=(e,t)=>{let n=c.current.get(e);return n&&n.doc.toString()===t?n:P.create({doc:t,extensions:f.current})};return(0,l.useEffect)(()=>(s.current=new G({state:p(e,t),parent:o.current}),()=>s.current?.destroy()),[]),(0,l.useEffect)(()=>{let n=s.current;u.current===e?n.state.doc.toString()!==t&&n.setState(p(e,t)):(c.current.set(u.current,n.state),u.current=e,n.setState(p(e,t)))},[e,t]),(0,l.useEffect)(()=>{let e=s.current,t=e.state.doc,r=n.filter(e=>e.line>=1&&e.line<=t.lines).map(e=>{let n=t.line(e.line),r=Math.min(n.from+Math.max(e.col-1,0),n.to),i=/^\w+/.exec(t.sliceString(r,n.to))?.[0].length??0;return{from:r,to:Math.max(r+i,r===n.to?r:r+1),severity:`error`,message:e.message}});e.dispatch(R_(e.state,r))},[n,e,t]),(0,l.useEffect)(()=>{if(!r)return;let e=s.current,t=e.state.doc,n=t.line(Math.min(Math.max(r.line,1),t.lines));e.dispatch({selection:{anchor:n.from},effects:G.scrollIntoView(n.from,{y:`center`})}),r.focus!==!1&&e.focus()},[r]),(0,Q.jsx)(`div`,{className:`editor`,ref:o})}var Ov=class{worker;nextId=1;pending=new Map;constructor(){this.start()}start(){this.worker=new Worker(new URL(new URL(`p.worker-2-u7uikt.js`,import.meta.url).href,``+import.meta.url),{type:`module`}),this.worker.onmessage=e=>{let t=this.pending.get(e.data.id);if(t){if(e.data.progress)return t.onProgress?.(e.data.progress);this.pending.delete(e.data.id),e.data.error?t.reject(Error(e.data.error)):t.resolve(e.data.result)}}}send(e,t){let n=this.nextId++;return new Promise((r,i)=>{this.pending.set(n,{resolve:r,reject:i,onProgress:t}),this.worker.postMessage({id:n,req:e})})}compile(e){return this.send({kind:`compile`,files:e})}check(e,t){return this.send({kind:`check`,...e},t)}cancel(){this.worker.terminate();for(let e of this.pending.values())e.reject(Error(`cancelled`));this.pending.clear(),this.start()}};function kv(e,t){let n=t.lane===void 0?`The model`:e.lanes[t.lane].label,r=t.to===void 0?`another actor`:e.lanes[t.to].label,i=t.payload===void 0?``:` with ${JSON.stringify(t.payload)}`;if(t.inspection?.write){let e=t.inspection.write;return`${n} writes ${e.name}: ${e.before} → ${e.after}.${e.unchanged?` The value stays the same.`:``}`}if(t.inspection)return`${n}: ${t.inspection.phase===`after`?`assignment completed`:`about to execute this statement`}.`;switch(t.kind){case`create`:return`${t.lane===t.to?`${r} is created`:`${n} creates ${r}`}. Each actor has its own state and data.`;case`send`:return`${n} sends ${t.label}${i} to ${r}. Sending queues the message; it does not execute the receiver's handler.`;case`dequeue`:return`${n} takes ${t.label}${i} from its inbox and begins or resumes handling it.`;case`state`:return`${n} enters ${t.label}.${t.hot?` This is a hot state: the model requires progress out of it.`:``}`;case`monitor`:return`${n} observes ${t.label}${i} to check a rule. An observer is separate from the actors implementing the system.`;case`error`:return`This execution violates a rule or encounters a runtime failure. Inspect the preceding messages and assignments to understand how it reached this point.`;case`raise`:return`${n} raises an event for itself, interrupting its current action.`;case`halt`:return`${n} stops.`;case`print`:return`The model prints a message: ${t.log}`;default:return t.log}}function Av(e,t){let n=new Map;for(let r=0;r<=t;r++){let t=e.steps[r],i=t.kind===`create`?t.to:t.lane;if(i!==void 0&&!n.has(i)&&n.set(i,{inbox:[]}),t.to!==void 0&&t.kind===`send`){let e=n.get(t.to)??{inbox:[]};e.inbox.push(r),n.set(t.to,e)}if(t.lane===void 0)continue;let a=n.get(t.lane);t.kind===`state`&&(a.state=t.label),t.inspection&&(a.snapshot=t.inspection,a.at=r,a.state=t.inspection.state),t.kind===`dequeue`&&t.peer!==void 0&&(a.inbox=a.inbox.filter(e=>e!==t.peer))}return n}function jv(e,t){let n=e.steps[t]?.lane;for(let r=t-1;r>=0;r--)if(e.steps[r].lane===n&&e.steps[r].inspection)return e.steps[r].inspection}function Mv({trace:e,cursor:t,onSelect:n,onOpen:r,sourceAvailable:i=!0}){let a=e.steps[t],o=Av(e,t),s=jv(e,t),c=a?.inspection,l=c?.write;if(!a)return(0,Q.jsx)(`p`,{children:`No execution steps were recorded.`});let u=[{label:`Start`,i:0},{label:`First message`,i:e.steps.findIndex(e=>e.kind===`send`)},{label:`First assignment`,i:e.steps.findIndex(e=>e.inspection?.phase===`after`)},{label:`Failure`,i:e.steps.findIndex(e=>e.kind===`error`)}].filter(e=>e.i>=0).sort((e,t)=>e.i-t.i);return(0,Q.jsxs)(`div`,{className:`replay-story`,children:[(0,Q.jsx)(`div`,{className:`replay-milestones`,"aria-label":`Execution landmarks`,children:u.map(e=>(0,Q.jsxs)(`button`,{onClick:()=>n(e.i),children:[e.label,(0,Q.jsxs)(`span`,{children:[`step `,e.i+1]})]},e.label))}),(0,Q.jsxs)(`section`,{className:`replay-caption ${a.kind===`error`?`failure`:``}`,"aria-live":`polite`,children:[(0,Q.jsxs)(`span`,{className:`eyebrow`,children:[`STEP `,t+1,` · `,c?c.phase===`after`?`AFTER ASSIGNMENT`:`BEFORE STATEMENT`:a.kind.toUpperCase()]}),(0,Q.jsx)(`h3`,{children:kv(e,a)}),c?(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsxs)(`button`,{className:`link source-link`,disabled:!i,onClick:()=>r(c.source),children:[c.source.file,`:`,c.source.line,` ↗`]}),(0,Q.jsx)(`pre`,{className:`replay-code`,children:c.source.text}),c.phase===`after`&&(0,Q.jsxs)(`div`,{className:`assignment-deltas`,children:[l?(0,Q.jsxs)(`div`,{children:[(0,Q.jsx)(`code`,{children:l.name}),(0,Q.jsxs)(`span`,{children:[l.before,` → `,(0,Q.jsx)(`strong`,{children:l.after}),l.unchanged&&(0,Q.jsx)(`em`,{children:` · same value written again`})]})]}):[`fields`,`locals`].flatMap(e=>Object.entries(c[e]).filter(([t,n])=>s?.[e][t]!==n).map(([t,n])=>(0,Q.jsxs)(`div`,{children:[(0,Q.jsx)(`code`,{children:t}),(0,Q.jsxs)(`span`,{children:[s?.[e][t]??`not captured`,` → `,(0,Q.jsx)(`strong`,{children:n})]})]},`${e}-${t}`))),(0,Q.jsx)(`p`,{className:`model-hint`,children:`The assignment ran even if the stored value stayed the same. Values below were captured by the checker.`})]})]}):(0,Q.jsx)(`pre`,{className:`replay-code`,children:a.kind===`error`?a.label:a.log}),a.peer!==void 0&&(0,Q.jsxs)(`button`,{className:`link`,onClick:()=>n(a.peer),children:[a.kind===`send`?`Jump to when this message is handled`:`Jump to when this message was sent`,` · step `,a.peer+1]})]}),(0,Q.jsx)(Nv,{trace:e,onSelect:n}),(0,Q.jsxs)(`h3`,{className:`actor-heading`,children:[`Actors at this point `,(0,Q.jsx)(`span`,{children:`Snapshots and recorded messages`})]}),(0,Q.jsx)(`div`,{className:`replay-actors`,children:[...o].map(([r,i])=>(0,Q.jsxs)(`section`,{className:`replay-actor ${a.lane===r?`active`:``}`,children:[(0,Q.jsxs)(`div`,{className:`actor-title`,children:[(0,Q.jsx)(`strong`,{children:e.lanes[r].label}),(0,Q.jsx)(`span`,{className:`model-tag`,children:e.lanes[r].monitor?`observer`:`actor`})]}),(0,Q.jsx)(`p`,{className:`actor-state`,children:i.state??`Created · waiting to enter its start state`}),i.snapshot?(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`dl`,{className:`actor-values`,children:Object.entries(i.snapshot.fields).map(([e,t])=>(0,Q.jsxs)(`div`,{children:[(0,Q.jsx)(`dt`,{children:e}),(0,Q.jsx)(`dd`,{children:t})]},e))}),Object.keys(i.snapshot.locals).length>0&&(0,Q.jsxs)(`details`,{open:a.lane===r,children:[(0,Q.jsx)(`summary`,{children:`Local variables · last captured frame`}),(0,Q.jsx)(`dl`,{className:`actor-values`,children:Object.entries(i.snapshot.locals).map(([e,t])=>(0,Q.jsxs)(`div`,{children:[(0,Q.jsx)(`dt`,{children:e}),(0,Q.jsx)(`dd`,{children:t})]},e))})]}),(0,Q.jsxs)(`p`,{className:`capture-note`,children:[`Captured `,i.snapshot.phase,` step `,i.at+1,i.at===t?``:` · last known values`]})]}):(0,Q.jsx)(`p`,{className:`capture-note`,children:`Variables have not been captured yet.`}),!e.lanes[r].monitor&&(0,Q.jsxs)(`div`,{className:`actor-inbox`,children:[(0,Q.jsxs)(`span`,{className:`eyebrow`,children:[`PENDING MESSAGES · `,i.inbox.length]}),i.inbox.slice(0,5).map(t=>(0,Q.jsxs)(`button`,{onClick:()=>n(t),children:[e.steps[t].label,` `,(0,Q.jsx)(`code`,{children:e.steps[t].payload===void 0?``:JSON.stringify(e.steps[t].payload)})]},t)),i.inbox.length>5&&(0,Q.jsxs)(`span`,{children:[`+`,i.inbox.length-5,` more`]})]})]},r))}),(0,Q.jsx)(`p`,{className:`model-hint`,children:`Replay follows one execution found by the checker. It does not explore new choices as you step. Actor values show their latest capture; inboxes are reconstructed from recorded sends and receives.`})]})}function Nv({trace:e,onSelect:t}){let[n,r]=(0,l.useState)(``),i=new Map;e.steps.forEach((e,t)=>{let n=e.inspection?.write;if(!n||n.scope!==`fields`||e.lane===void 0)return;let r=`${e.lane}:${n.name}`,a=i.get(r)??{lane:e.lane,name:n.name,rows:[],repeated:0};a.rows.push(t),n.unchanged&&a.repeated++,i.set(r,a)});let a=[...i].sort((e,t)=>t[1].repeated-e[1].repeated||t[1].rows.length-e[1].rows.length),o=i.get(n)??a[0]?.[1];if(!o)return null;let s=[...i].find(([,e])=>e===o)[0],c=e.steps.map((e,t)=>({s:e,i:t})).filter(({s:e})=>(e.kind===`send`||e.kind===`dequeue`)&&(e.lane===o.lane||e.to===o.lane));return(0,Q.jsxs)(`details`,{className:`variable-history`,open:!0,children:[(0,Q.jsx)(`summary`,{children:`Investigate this execution · recorded writes`}),(0,Q.jsx)(`p`,{className:`model-hint`,children:`Inspect a variable across the entire execution, then jump to a write or a related message. Repeated values are evidence to inspect, not automatically a bug.`}),(0,Q.jsxs)(`label`,{children:[`Variable `,(0,Q.jsx)(`select`,{"aria-label":`Variable history`,value:s,onChange:e=>r(e.target.value),children:a.map(([t,n])=>(0,Q.jsxs)(`option`,{value:t,children:[e.lanes[n.lane].label,`.`,n.name,` · `,n.rows.length,` writes`]},t))})]}),(0,Q.jsx)(`ol`,{className:`write-history`,children:o.rows.map(n=>{let r=e.steps[n],i=r.inspection.write,a;for(let t=n-1;t>=0;t--)if(e.steps[t].lane===r.lane&&e.steps[t].kind===`dequeue`){a=t;break}let s=a===void 0?void 0:e.steps[a].peer,c=s===void 0?void 0:e.steps[s].lane,l;if(c!==void 0&&a!==void 0)for(let t=a-1;t>=0;t--){let n=e.steps[t];if(n.kind===`send`&&n.lane===o.lane&&n.to===c){l=t;break}}return(0,Q.jsxs)(`li`,{children:[(0,Q.jsxs)(`button`,{onClick:()=>t(n),children:[(0,Q.jsxs)(`span`,{children:[`Step `,n+1,` · `,i.name]}),(0,Q.jsxs)(`strong`,{children:[i.before,` → `,i.after]}),i.unchanged&&(0,Q.jsx)(`span`,{className:`same-write`,children:`same value written again`}),a!==void 0&&(0,Q.jsxs)(`small`,{children:[`Most recent handled message: `,e.steps[a].label,c===void 0?``:` from ${e.lanes[c].label}`]})]}),l!==void 0&&(0,Q.jsxs)(`button`,{className:`prior-message`,onClick:()=>t(l),children:[(0,Q.jsxs)(`span`,{children:[`Earlier message to that sender: `,(0,Q.jsx)(`strong`,{children:e.steps[l].label}),e.steps[l].payload===void 0?``:` · ${JSON.stringify(e.steps[l].payload)}`]}),(0,Q.jsxs)(`span`,{children:[`step `,l+1,` ↗`]})]})]},n)})}),(0,Q.jsxs)(`details`,{className:`history-messages`,children:[(0,Q.jsxs)(`summary`,{children:[`Related messages · `,c.length]}),(0,Q.jsx)(`ol`,{children:c.map(({s:n,i:r})=>(0,Q.jsx)(`li`,{children:(0,Q.jsxs)(`button`,{className:`link`,onClick:()=>t(r),children:[`Step `,r+1,` · `,kv(e,n)]})},r))})]})]})}function Pv({model:e,activeMachine:t,activeState:n,onOpen:r,readOnly:i=!1}){let[a,o]=(0,l.useState)(``),s=e.machines.find(e=>e.name===(a||t))??e.machines[0];return s?(0,Q.jsxs)(`div`,{className:`state-explorer`,children:[(0,Q.jsxs)(`div`,{className:`model-toolbar`,children:[(0,Q.jsxs)(`label`,{children:[`State machine `,(0,Q.jsx)(`select`,{"aria-label":`State machine`,value:s.name,onChange:e=>o(e.target.value),children:e.machines.map(e=>(0,Q.jsxs)(`option`,{value:e.name,children:[e.name,e.kind===`machine`?``:` · ${e.kind}`]},e.name))})]}),t&&(0,Q.jsx)(`button`,{className:`link`,onClick:()=>o(``),children:`Follow active actor`})]}),(0,Q.jsxs)(`p`,{className:`model-hint`,children:[`From your code. Arrows show declared or possible `,(0,Q.jsx)(`code`,{children:`goto`}),` transitions; conditions still apply. Select a state or action to open its source.`]}),(0,Q.jsx)(Fv,{readOnly:i,machine:s,current:s.name===t?n:void 0,onOpen:r}),(0,Q.jsx)(`div`,{className:`state-actions`,children:s.states.map(e=>(0,Q.jsxs)(`section`,{className:s.name===t&&e.name===n?`state-action active`:`state-action`,children:[(0,Q.jsx)(`button`,{className:`link`,disabled:i,onClick:()=>r(e.source),children:(0,Q.jsx)(`strong`,{children:e.name})}),e.start&&(0,Q.jsx)(`span`,{className:`model-tag`,children:`start`}),e.hot&&(0,Q.jsx)(`span`,{className:`model-tag`,children:`hot · progress required`}),e.actions.length===0&&(0,Q.jsx)(`p`,{className:`muted`,children:`No actions declared.`}),e.actions.map((e,t)=>(0,Q.jsxs)(`button`,{className:`state-action-row`,disabled:i,onClick:()=>r(e.source),children:[(0,Q.jsx)(`span`,{children:e.label}),(0,Q.jsx)(`span`,{children:e.targets.length?e.targets.map(e=>`${e.conditional?`may go to`:`→`} ${e.state}`).join(` · `):`action / stays unless it transitions`})]},t))]},e.name))})]}):(0,Q.jsx)(`p`,{className:`model-empty`,children:`No state machines in this project yet. Add a machine in the editor.`})}function Fv({machine:e,current:t,onOpen:n,readOnly:r}){let i=(0,l.useId)().replace(/:/g,``),a=e.states,o=a.flatMap((e,t)=>e.actions.flatMap(e=>e.targets.map(n=>({from:t,to:a.findIndex(e=>e.name===n.state),label:e.label,...n})))).filter(e=>e.to>=0),s=Math.max(150,a.length*95+45);return(0,Q.jsxs)(`div`,{className:`state-graph`,children:[(0,Q.jsxs)(`svg`,{viewBox:`0 0 540 ${s}`,role:`img`,"aria-label":`States and transitions of ${e.name}`,children:[(0,Q.jsx)(`defs`,{children:(0,Q.jsx)(`marker`,{id:i,markerWidth:`8`,markerHeight:`8`,refX:`7`,refY:`4`,orient:`auto`,children:(0,Q.jsx)(`path`,{d:`M0,0 L8,4 L0,8 z`,fill:`currentColor`})})}),o.map((e,t)=>{let n=52+e.from*95,r=52+e.to*95,a=355+t%7*20,o=e.from===e.to?`M310 ${n-9} C${a+90} ${n-65},${a+90} ${n+65},310 ${n+9}`:`M310 ${n} C${a} ${n},${a} ${r},310 ${r}`;return(0,Q.jsxs)(`g`,{children:[(0,Q.jsx)(`path`,{d:o,fill:`none`,stroke:`currentColor`,strokeWidth:`1.5`,strokeDasharray:e.conditional?`5 4`:void 0,markerEnd:`url(#${i})`}),(0,Q.jsx)(`text`,{x:a-12,y:(n+r)/2-5,className:`edge-number`,children:t+1}),(0,Q.jsxs)(`title`,{children:[e.label,` → `,e.state,e.conditional?` (conditional)`:``]})]},t)}),a.map((e,a)=>(0,Q.jsxs)(`g`,{className:`state-node ${e.name===t?`active`:``}`,role:`button`,"aria-disabled":r,tabIndex:r?-1:0,"aria-label":`Open state ${e.name}`,onClick:()=>n(e.source),onKeyDown:t=>{(t.key===`Enter`||t.key===` `)&&(t.preventDefault(),n(e.source))},children:[e.start&&(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`path`,{d:`M30 ${52+a*95} H105`,stroke:`currentColor`,markerEnd:`url(#${i})`}),(0,Q.jsx)(`text`,{x:`30`,y:42+a*95,children:`start`})]}),(0,Q.jsx)(`rect`,{x:`110`,y:30+a*95,width:`200`,height:`44`,rx:`22`}),(0,Q.jsx)(`text`,{x:`210`,y:57+a*95,textAnchor:`middle`,children:e.name.length>23?e.name.slice(0,21)+`…`:e.name}),(0,Q.jsxs)(`title`,{children:[e.name,e.name===t?` · active in replay`:``]})]},e.name))]}),o.length>0&&(0,Q.jsx)(`ol`,{className:`edge-key`,children:o.map((e,t)=>(0,Q.jsx)(`li`,{children:(0,Q.jsxs)(`button`,{className:`link`,disabled:r,onClick:()=>n(e.source),children:[a[e.from].name,` · `,e.label,` → `,e.state,e.conditional?` (conditional)`:``]})},t))}),o.length===0&&(0,Q.jsx)(`p`,{className:`model-hint`,children:`This machine has no explicit state transitions. Its actions can still send messages and change variables.`})]})}function Iv(e,t){let n=e[t],r=n.peer===void 0?void 0:e[n.peer]?.lane;return[n.lane,n.to,r].filter(e=>e!==void 0)}function Lv(e,t){let n=e.steps.map((e,t)=>t);if(!t.size)return{rows:n,cols:e.lanes.map((e,t)=>t)};let r=n.filter(n=>{let r=e.steps[n];return r.kind===`error`||r.kind===`print`||Iv(e.steps,n).some(e=>t.has(e))}),i=new Set(t);for(let t of r)for(let n of Iv(e.steps,t))i.add(n);return{rows:r,cols:e.lanes.map((e,t)=>t).filter(e=>i.has(e))}}var $=e=>typeof e==`string`?e:void 0;function Rv(e){let t=JSON.parse(e),n=[],r=new Map,i=[],a=new Map,o,s,c=(e,t=!1)=>{if(!e)return;let i=r.get(e);return i===void 0&&(i=n.length,n.push({id:e,label:e,monitor:t}),r.set(e,i)),i},l=e=>JSON.stringify(e??null);for(let{type:e,details:n}of t){let t=n.log??e,u=n.payload===`null`?void 0:n.payload;switch(e){case`Inspection`:{let e=n.inspection;i.push({kind:`note`,lane:c($(n.id)),label:e.source.text.split(`
`)[0],log:t,inspection:e});break}case`CreateMonitor`:c($(n.monitor),!0);break;case`CreateStateMachine`:{let e=c($(n.id)),a=r.get($(n.creatorName)??``);i.push({kind:`create`,lane:a??e,to:e,label:`new`,log:t,payload:u});break}case`StateTransition`:case`MonitorStateTransition`:n.isEntry&&i.push({kind:`state`,lane:c($(n.id)??$(n.monitor),e===`MonitorStateTransition`),label:$(n.state)??`?`,log:t,hot:n.isInHotState===!0||/enters hot state/.test(t)});break;case`SendEvent`:{let e=c($(n.sender)),r=c($(n.target)),o=i.length;if(i.push({kind:`send`,lane:e,to:r,label:$(n.event)??`?`,log:t,payload:u}),r!==void 0){let e=a.get(r)??[];e.push({event:$(n.event)??``,key:l(u),step:o}),a.set(r,e)}break}case`DequeueEvent`:case`ReceiveEvent`:{let e=c($(n.id)),r=$(n.event)??`?`,o=i.length,s={kind:`dequeue`,lane:e,label:r,log:t,payload:u};if(e!==void 0){let t=a.get(e)??[],n=t.findIndex(e=>e.event===r&&e.key===l(u));n<0&&(n=t.findIndex(e=>e.event===r)),n>=0&&(s.peer=t[n].step,i[t[n].step].peer=o,t.splice(n,1))}i.push(s);break}case`MonitorProcessEvent`:i.push({kind:`monitor`,lane:c($(n.monitor),!0),to:r.get($(n.sender)??``),label:$(n.event)??`?`,log:t,payload:u});break;case`Announce`:i.push({kind:`announce`,lane:c($(n.id)),label:`announce ${$(n.event)}`,log:t,payload:u});break;case`RaiseEvent`:case`MonitorRaiseEvent`:i.push({kind:`raise`,lane:c($(n.id)??$(n.monitor)),label:`raise ${$(n.event)}`,log:t});break;case`Print`:i.push({kind:`print`,label:t,log:t});break;case`Halt`:i.push({kind:`halt`,lane:c($(n.id)),label:`halt`,log:t});break;case`AssertionFailure`:{o=$(n.error)??t;let e=[...r.keys()].filter(e=>o.includes(e)).sort((e,t)=>t.length-e.length)[0],a=[...i].reverse().find(e=>e.lane!==void 0)?.lane;i.push({kind:`error`,lane:e===void 0?a:r.get(e),label:o,log:t});break}case`StrategyDescription`:s=$(n.strategyDescription);break;case`GotoState`:case`WaitMultipleEvents`:break;default:i.push({kind:`note`,lane:c($(n.id)??$(n.monitor)),label:e,log:t,payload:u})}}let u=n.map((e,t)=>t).sort((e,t)=>Number(n[e].monitor)-Number(n[t].monitor)||e-t),d=new Map(u.map((e,t)=>[e,t])),f=e=>e===void 0?void 0:d.get(e);return{lanes:u.map(e=>n[e]),steps:i.map(e=>({...e,lane:f(e.lane),to:f(e.to)})),error:o,strategy:s}}function zv(e){if(e==null)return``;if(typeof e!=`object`)return String(e);let t=JSON.stringify(e);return t.length>60?t.slice(0,57)+`…`:t}var Bv=150,Vv=96,Hv=30,Uv=10,Wv=44,Gv=.35,Kv=56,qv=2.5,Jv=6,Yv=e=>t=>t*e+e/2,Xv=e=>Uv+e*Hv+Hv/2,Zv=e=>Math.min(qv,Math.max(Gv,e)),Qv=7.2;function $v(e,t){let n=Math.max(4,Math.floor(t/Qv));return e.length>n?e.slice(0,n-1)+`…`:e}function ey(e,t,n,r,i=7){let a=Math.atan2(r-t,n-e),o=e=>`${n-i*Math.cos(a+e)},${r-i*Math.sin(a+e)}`;return`M${n},${r} L${o(.42)} L${o(-.42)} z`}function ty(e){let t=0;return e.map(e=>e.monitor?`var(--spec)`:`var(--lane-${t++%Jv})`)}function ny({trace:e,rows:t,cols:n,color:r,focus:i,cursor:a,hover:o,onSelect:s,onHover:c,onToggleLane:u}){let{lanes:d,steps:f}=e,[p]=(0,l.useState)(()=>matchMedia(`(max-width: 600px)`).matches?Vv:Bv),m=Yv(p),h=Math.max(n.length,1)*p,g=20+t.length*Hv+Kv+16,_=(0,l.useMemo)(()=>{let e=new Map(n.map((e,t)=>[e,t])),r=new Map(t.map((e,t)=>[e,t]));return{col:t=>t===void 0?void 0:e.get(t),row:e=>e===void 0?void 0:r.get(e)}},[t,n]),v=(0,l.useRef)(null),[y,ee]=(0,l.useState)(1),[b,x]=(0,l.useState)(!1),te=(0,l.useRef)(null),ne=(0,l.useMemo)(()=>{let e=new Map;for(let t of d)e.set(oy(t.id),(e.get(oy(t.id))??0)+1);return new Set([...e].filter(([,e])=>e>1).map(([e])=>e))},[d]),re=(0,l.useMemo)(()=>{let e=d.map(e=>e.monitor?0:-1);return t.forEach((t,n)=>{let r=f[t];r.kind===`create`&&r.to!==void 0&&e[r.to]<0&&(e[r.to]=n),r.lane!==void 0&&e[r.lane]<0&&(e[r.lane]=n)}),e.map(e=>Math.max(e,0))},[d,f,t]),ie=(0,l.useCallback)(()=>{let e=v.current;return e?Zv((e.clientWidth-4)/h):1},[h]),S=(0,l.useRef)(y),C=(0,l.useCallback)((e,t,n)=>{let r=v.current;if(!r)return;let i=S.current,a=Zv(e),o=t??r.clientWidth/2,s=n??r.clientHeight/2,c=te.current,l=c?c.cx*i-c.sx:r.scrollLeft,u=c?c.cy*i+Wv*i-c.sy:r.scrollTop;te.current={cx:(l+o)/i,cy:(u+s-Wv*i)/i,sx:o,sy:s},S.current=a,ee(a)},[]);(0,l.useLayoutEffect)(()=>{let e=v.current,t=te.current;e&&t&&(te.current=null,e.scrollLeft=t.cx*y-t.sx,e.scrollTop=t.cy*y+Wv*y-t.sy)},[y]),(0,l.useLayoutEffect)(()=>{S.current=1,ee(1)},[e,ie]),(0,l.useEffect)(()=>{let e=v.current;if(!e)return;let t=t=>{if(!t.ctrlKey&&!t.metaKey)return;t.preventDefault();let n=e.getBoundingClientRect(),r=Math.exp(-Math.max(-50,Math.min(50,t.deltaY))*.006);C(S.current*r,t.clientX-n.left,t.clientY-n.top)};return e.addEventListener(`wheel`,t,{passive:!1}),()=>e.removeEventListener(`wheel`,t)},[C]);let w=(0,l.useRef)(new Map),ae=(0,l.useRef)(null),T=()=>{let[e,t]=[...w.current.values()];return Math.hypot(e.x-t.x,e.y-t.y)},oe=e=>{e.pointerType===`touch`&&(w.current.set(e.pointerId,{x:e.clientX,y:e.clientY}),w.current.size===2&&(ae.current={d:T(),z:S.current}))},se=e=>{if(!w.current.has(e.pointerId))return;w.current.set(e.pointerId,{x:e.clientX,y:e.clientY});let t=ae.current,n=v.current;if(!t||w.current.size!==2||!n)return;let[r,i]=[...w.current.values()],a=n.getBoundingClientRect();C(t.z*(T()/t.d),(r.x+i.x)/2-a.left,(r.y+i.y)/2-a.top)},ce=e=>{w.current.delete(e.pointerId),w.current.size<2&&(ae.current=null)};return(0,l.useEffect)(()=>{let e=v.current;if(!e)return;let t=_.row(a);if(t===void 0)return;let n=Wv*S.current,r=n+(Xv(t)-Hv)*S.current,i=n+(Xv(t)+Hv)*S.current;r<e.scrollTop+n?e.scrollTop=r-n:i>e.scrollTop+e.clientHeight-Kv&&(e.scrollTop=i-e.clientHeight+Kv);let o=f[a];if(!o)return;let s=[o.lane,o.to,o.peer===void 0?void 0:f[o.peer]?.lane].map(_.col).filter(e=>e!==void 0);if(!s.length)return;let c=S.current,l=Math.min(...s)*p*c,u=(Math.max(...s)+1)*p*c;if(o.kind===`error`){let e=iy(o.label,Yv(p)(s[0]),h);l=(e.left-8)*c,u=(e.left+e.w+8)*c}u-l>e.clientWidth?e.scrollLeft=(l+u)/2-e.clientWidth/2:l<e.scrollLeft?e.scrollLeft=l:u>e.scrollLeft+e.clientWidth&&(e.scrollLeft=u-e.clientWidth)},[a,f,p,h,_]),(0,Q.jsxs)(`div`,{className:`seq-wrap`,children:[(0,Q.jsxs)(`div`,{className:`seq`,ref:v,onPointerDown:oe,onPointerMove:se,onPointerUp:ce,onPointerCancel:ce,children:[(0,Q.jsx)(`svg`,{className:`seq-head`,width:h*y,height:Wv*y,viewBox:`0 0 ${h} ${Wv}`,role:`presentation`,children:n.map((e,t)=>{let n=d[e];return(0,Q.jsxs)(`g`,{className:i.has(e)?`lane focused`:`lane`,transform:`translate(${m(t)},${Wv/2})`,style:{"--c":r[e]},onClick:()=>u(e),children:[(0,Q.jsx)(`rect`,{x:-p/2+4,y:-15,width:p-8,height:30,rx:15,className:n.monitor?`lane-head monitor`:`lane-head`}),(0,Q.jsx)(sy,{id:n.id,monitor:n.monitor,w:p,showId:ne.has(oy(n.id)),focused:i.has(e)})]},n.id)})}),(0,Q.jsxs)(`svg`,{width:h*y,height:g*y,viewBox:`0 0 ${h} ${g}`,className:`seq-body`,children:[n.map((e,t)=>(0,Q.jsx)(`line`,{x1:m(t),x2:m(t),y1:Xv(re[e])-Hv/2,y2:g,className:d[e].monitor?`lifeline monitor`:`lifeline`,style:{"--c":r[e]}},d[e].id)),t.map(e=>(0,Q.jsx)(ry,{i:e,s:f[e],W:p,width:h,cursor:a,hover:o===e,color:r,layout:_,onSelect:s,onHover:c},e))]})]}),b&&(0,Q.jsx)(ly,{onClose:()=>x(!1)}),(0,Q.jsxs)(`div`,{className:`zoom-controls`,role:`group`,"aria-label":`Zoom`,children:[(0,Q.jsx)(`button`,{className:`key-toggle`,onClick:()=>x(!b),"aria-expanded":b,children:`Key`}),(0,Q.jsx)(`button`,{onClick:()=>C(y/1.25),"aria-label":`Zoom out`,disabled:y<=Gv,children:`−`}),(0,Q.jsxs)(`button`,{className:`zoom-level`,onClick:()=>C(y===1?ie():1),title:`Toggle 100% / fit width`,children:[Math.round(y*100),`%`]}),(0,Q.jsx)(`button`,{onClick:()=>C(y*1.25),"aria-label":`Zoom in`,disabled:y>=qv,children:`+`})]})]})}function ry({i:e,s:t,W:n,width:r,cursor:i,hover:a,color:o,layout:s,onSelect:c,onHover:l}){let u=Yv(n),d=e=>u(s.col(e)??0),f=Xv(s.row(e)??0),p=e>i,m=e===i,h=t.peer!==void 0&&t.peer===i,g=[`row`,p&&`future`,(m||h)&&`selected`,a&&`hover`,`k-${t.kind}`].filter(Boolean).join(` `),_=t.lane===void 0?0:d(t.lane),v=t.lane===void 0?`var(--fg)`:o[t.lane],y=null;switch(t.kind){case`create`:if(t.lane!==void 0&&t.to!==void 0&&t.lane!==t.to){let e=d(t.to),n=e+(e>_?-6:6);y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:_,y1:f,x2:n,y2:f,className:`msg create`}),(0,Q.jsx)(`path`,{d:ey(_,f,n,f),className:`arrowhead create`}),(0,Q.jsx)(`text`,{x:(_+e)/2,y:f-5,textAnchor:`middle`,className:`msg-label muted`,children:`new`})]})}else t.to!==void 0&&(y=(0,Q.jsx)(`circle`,{cx:d(t.to),cy:f,r:4,className:`dot`,style:{fill:o[t.to]}}));break;case`send`:{if(t.lane===void 0||t.to===void 0)break;let e=d(t.to),n=s.row(t.peer),r=n===void 0?f:Xv(n),i=e>=_?1:-1;y=t.to===t.lane?(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`path`,{d:`M${_},${f} h18 a8,8 0 0 1 8,8 V${r-8} a8,8 0 0 1 -8,8 h-12`,className:`msg`,fill:`none`}),(0,Q.jsx)(`path`,{d:ey(_+18,r,_+6,r),className:`arrowhead`}),(0,Q.jsx)(cy,{x:_+32,y:f-4,anchor:`start`,event:t.label,payload:t.payload,max:130})]}):(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:_,y1:f,x2:e-i*5,y2:r,className:t.peer===void 0?`msg pending`:`msg`}),(0,Q.jsx)(`path`,{d:ey(_,f,e-i*5,r),className:`arrowhead`}),(0,Q.jsx)(cy,{x:_+i*10,y:f-6,anchor:i>0?`start`:`end`,event:t.label,payload:t.payload,max:Math.max(Math.abs(e-_)-20,90)})]});break}case`dequeue`:y=(0,Q.jsx)(`circle`,{cx:_,cy:f,r:4,className:`dot`});break;case`state`:{let e=Math.min(n-8,18+t.label.length*Qv),i=Math.min(Math.max(_,e/2+2),r-e/2-2);y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:i-e/2,y:f-10,width:e,height:20,rx:10,className:t.hot?`state hot`:`state`}),(0,Q.jsx)(`text`,{x:i,y:f,dy:`0.35em`,textAnchor:`middle`,className:`state-label`,children:$v(t.label,n-16)})]});break}case`monitor`:{let e=t.to===void 0?void 0:d(t.to);y=(0,Q.jsxs)(Q.Fragment,{children:[e!==void 0&&(0,Q.jsx)(`line`,{x1:e,y1:f,x2:_,y2:f,className:`observe`}),(0,Q.jsx)(`rect`,{x:_-5,y:f-5,width:10,height:10,rx:2,transform:`rotate(45 ${_} ${f})`,className:`diamond`}),(0,Q.jsx)(`text`,{x:_+11,y:f-6,className:`msg-label`,children:$v(t.label,n-20)})]});break}case`announce`:case`raise`:case`halt`:case`note`:y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:_-4,y:f-4,width:8,height:8,rx:2,className:`dot`}),(0,Q.jsx)(`text`,{x:_+9,y:f,dy:`0.35em`,className:`note-label`,children:$v(t.label,n-20)})]});break;case`print`:y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:6,y:f-9,width:Math.min(r-12,8+$v(`print: ${t.label}`,r-20).length*Qv),height:18,rx:4,className:`label-bg`}),(0,Q.jsx)(`text`,{x:10,y:f,dy:`0.35em`,className:`print-label`,children:$v(`print: ${t.label}`,r-20)})]});break;case`error`:{let{left:e,w:n}=iy(t.label,t.lane===void 0?void 0:_,r);y=(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:e,y:f-12,width:n,height:24,rx:12,className:`error-bar`}),(0,Q.jsx)(`text`,{x:e+12,y:f,dy:`0.35em`,className:`error-label`,children:ay(t.label)})]});break}}return(0,Q.jsxs)(`g`,{className:g,"data-row":e,onClick:()=>c(e),onMouseEnter:()=>l(e),onMouseLeave:()=>l(void 0),style:{"--c":v},children:[(0,Q.jsx)(`rect`,{x:0,y:f-Hv/2,width:r,height:Hv,className:`row-hit`}),m&&(0,Q.jsx)(`rect`,{x:0,y:f-Hv/2+3,width:3,height:24,rx:1.5,className:`row-cursor`}),(0,Q.jsx)(`title`,{children:t.log}),y]})}function iy(e,t,n){let r=Math.min(n-8,24+ay(e).length*7.8);return{left:Math.max(4,Math.min(t===void 0?4:t-16,n-r-4)),w:r}}function ay(e){return`✕ ${(/^([^:.]+)/.exec(e)?.[1]??e).trim()}`}var oy=e=>e.replace(/\(\d+\)$/,``);function sy({id:e,monitor:t,w:n,showId:r,focused:i}){let a=/^(.*?)(\(\d+\))?$/.exec(e),o=r?a[2]??``:``,s=n-14,c=(a[1].length+o.length)*Qv,l=Math.max(.82,Math.min(1,s/c));return(0,Q.jsxs)(`text`,{textAnchor:`middle`,dy:`0.35em`,className:`lane-label`,style:l<1?{fontSize:`${(12*l).toFixed(1)}px`}:void 0,children:[(0,Q.jsx)(`title`,{children:`${t?`spec ${e}`:e} (${i?`click to stop focusing`:`click to show only its steps`})`}),$v(a[1],s/l-o.length*Qv),o&&(0,Q.jsx)(`tspan`,{className:`lane-id`,children:o})]})}function cy({x:e,y:t,anchor:n,event:r,payload:i,max:a}){let o=$v(r,a),s=a-o.length*Qv-6,c=i!==void 0&&s>30?$v(zv(i),s):``,l=(o.length+(c?c.length+1:0))*Qv+8,u=n===`middle`?e-l/2:n===`end`?e-l+4:e-4;return(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`rect`,{x:u,y:t-11,width:l,height:15,rx:4,className:`label-bg`}),(0,Q.jsxs)(`text`,{x:e,y:t,textAnchor:n,className:`msg-label`,children:[(0,Q.jsx)(`tspan`,{className:`ev`,children:o}),c&&(0,Q.jsxs)(`tspan`,{className:`payload`,children:[` `,c]})]})]})}function ly({onClose:e}){let t=(e,t)=>(0,Q.jsxs)(`li`,{children:[(0,Q.jsx)(`svg`,{width:`40`,height:`18`,viewBox:`0 0 40 18`,"aria-hidden":`true`,children:e}),(0,Q.jsx)(`span`,{children:t})]});return(0,Q.jsxs)(`div`,{className:`legend`,role:`dialog`,"aria-label":`Diagram key`,children:[(0,Q.jsxs)(`div`,{className:`legend-head`,children:[(0,Q.jsx)(`strong`,{children:`Key`}),(0,Q.jsx)(`button`,{className:`ghost`,onClick:e,"aria-label":`Close key`,children:`✕`})]}),(0,Q.jsxs)(`ul`,{children:[t((0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:`4`,y1:`9`,x2:`30`,y2:`9`,className:`msg`,style:{"--c":`var(--lane-0)`}}),(0,Q.jsx)(`path`,{d:ey(4,9,34,9),className:`arrowhead`,style:{"--c":`var(--lane-0)`}})]}),`Message, in the sender's color; it ends where the receiver handles it`),t((0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:`4`,y1:`9`,x2:`30`,y2:`9`,className:`msg pending`,style:{"--c":`var(--lane-0)`}}),(0,Q.jsx)(`path`,{d:ey(4,9,34,9),className:`arrowhead`,style:{"--c":`var(--lane-0)`}})]}),`Sent but never handled`),t((0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:`4`,y1:`9`,x2:`30`,y2:`9`,className:`msg create`}),(0,Q.jsx)(`path`,{d:ey(4,9,34,9),className:`arrowhead create`})]}),`Created a machine`),t((0,Q.jsx)(`rect`,{x:`4`,y:`2`,width:`32`,height:`14`,rx:`7`,className:`state`,style:{"--c":`var(--lane-1)`}}),`Entered a state`),t((0,Q.jsx)(`rect`,{x:`4`,y:`2`,width:`32`,height:`14`,rx:`7`,className:`state hot`}),`Spec in a hot state (must leave it eventually)`),t((0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`line`,{x1:`4`,y1:`9`,x2:`26`,y2:`9`,className:`observe`}),(0,Q.jsx)(`rect`,{x:`26`,y:`4`,width:`10`,height:`10`,rx:`2`,transform:`rotate(45 31 9)`,className:`diamond`})]}),`Spec observed an event`),t((0,Q.jsx)(`rect`,{x:`4`,y:`3`,width:`32`,height:`12`,rx:`6`,className:`error-bar`}),`Where the bug was detected`),t((0,Q.jsx)(`rect`,{x:`4`,y:`2`,width:`32`,height:`14`,rx:`7`,className:`lane-head`,style:{"--c":`var(--lane-0)`}}),`Tap a machine's name to show only its steps`)]})]})}var uy=/([\w./-]+\.p):(\d+):(\d+)/,dy=820,fy=560;function py({json:e,view:t,model:n,stale:r,onOpenLocation:i,onShowDiagram:a}){let o=(0,l.useMemo)(()=>Rv(e),[e]),s=(0,l.useMemo)(()=>ty(o.lanes),[o]),[c,u]=(0,l.useState)(o.steps.length-1),[d,f]=(0,l.useState)(`story`),[p,m]=(0,l.useState)(!1),[h,g]=(0,l.useState)(!1),[_,v]=(0,l.useState)(!1),[y,ee]=(0,l.useState)(new Set),[b,x]=(0,l.useState)(),te=(0,l.useRef)(null),[ne,re]=(0,l.useState)(`none`),{rows:ie,cols:S}=(0,l.useMemo)(()=>Lv(o,y),[o,y]),C=(0,l.useMemo)(()=>d===`sequence`?ie.filter(e=>!o.steps[e].inspection):ie,[ie,d,o]),w=C.indexOf(c),ae=C.length-1;(0,l.useEffect)(()=>{u(o.steps.length-1),g(!1),ee(new Set)},[o]),(0,l.useEffect)(()=>{if(w>=0||!C.length)return;let e=C.filter(e=>e<c);u(e.length?e[e.length-1]:C[0])},[w,C,c]),(0,l.useEffect)(()=>{if(!h)return;if(w>=ae){g(!1);return}let e=setTimeout(()=>u(C[Math.min(w+1,ae)]),1100);return()=>clearTimeout(e)},[h,w,ae,C]),(0,l.useLayoutEffect)(()=>{let e=te.current;if(!e)return;let t=new ResizeObserver(()=>re(e.clientWidth>=dy?`side`:e.clientHeight>=fy&&!matchMedia(`(max-width: 900px)`).matches?`bottom`:`none`));return t.observe(e),()=>t.disconnect()},[]);let T=e=>{e!==void 0&&(g(!1),u(e))},oe=e=>T(C[Math.max(0,Math.min(e,ae))]),se=e=>ee(t=>{let n=new Set(t);return n.has(e)?n.delete(e):n.add(e),n}),ce=o.steps[c],le=ce?.inspection?.source;(0,l.useEffect)(()=>{p&&le&&!r&&i(le.file,le.line,!1)},[p,le,r,i]);let ue=(0,l.useMemo)(()=>Av(o,c),[o,c]),de=ce?.lane===void 0?void 0:ue.get(ce.lane),fe=de?.snapshot?.machine??(ce?.lane===void 0?void 0:o.lanes[ce.lane].id.replace(/\(.*$/,``).replace(/^.*\./,``)),pe=o.error?uy.exec(o.error):null,me=(0,Q.jsx)(my,{trace:o,rows:C,color:s,cursor:c,hover:b,onSelect:e=>{T(e),t===`log`&&a()},onHover:x});return(0,Q.jsxs)(`div`,{className:`trace`,ref:te,children:[r&&(0,Q.jsx)(`div`,{className:`stale-trace`,role:`status`,children:`Source or test changed. This is the previous execution. Check again to replay your edits; source links are paused.`}),t===`diagram`&&(0,Q.jsxs)(`div`,{className:`replay-toolbar`,children:[(0,Q.jsxs)(`div`,{className:`view-switch`,"aria-label":`Replay view`,children:[(0,Q.jsx)(`button`,{"aria-pressed":d===`story`,onClick:()=>f(`story`),children:`Walkthrough`}),(0,Q.jsx)(`button`,{"aria-pressed":d===`sequence`,onClick:()=>f(`sequence`),children:`Sequence`}),(0,Q.jsx)(`button`,{"aria-pressed":d===`states`,onClick:()=>f(`states`),disabled:!n,children:`States`})]}),(0,Q.jsxs)(`label`,{className:`follow-source`,children:[(0,Q.jsx)(`input`,{type:`checkbox`,checked:p,disabled:r,onChange:e=>m(e.target.checked)}),`Follow source`]})]}),o.error&&(0,Q.jsxs)(`div`,{className:_?`trace-error open`:`trace-error`,onClick:()=>v(!_),children:[(0,Q.jsxs)(`span`,{className:`trace-error-text`,children:[(0,Q.jsx)(`strong`,{children:`Bug:`}),` `,o.error]}),pe&&(0,Q.jsxs)(`button`,{className:`link`,disabled:r,onClick:e=>{e.stopPropagation(),r||i(pe[1],Number(pe[2]))},children:[(0,Q.jsxs)(`span`,{className:`goto-long`,children:[`Go to `,pe[1],`:`,pe[2]]}),(0,Q.jsxs)(`span`,{className:`goto-short`,children:[`line `,pe[2],` →`]})]})]}),t===`diagram`&&(0,Q.jsxs)(`div`,{className:`trace-controls`,children:[(0,Q.jsx)(`button`,{onClick:()=>oe(0),title:`First step`,"aria-label":`First step`,className:`jump`,children:`⏮`}),(0,Q.jsx)(`button`,{onClick:()=>oe(w-1),title:`Previous step`,"aria-label":`Previous step`,children:`◀`}),(0,Q.jsx)(`button`,{className:`primary`,onClick:()=>{!h&&w>=ae&&u(C[0]),g(!h)},title:h?`Pause`:`Play`,children:h?`Pause`:`Play`}),(0,Q.jsx)(`button`,{onClick:()=>oe(w+1),title:`Next step`,"aria-label":`Next step`,children:`▶`}),(0,Q.jsx)(`button`,{onClick:()=>oe(ae),title:`Last step`,"aria-label":`Last step`,className:`jump`,children:`⏭`}),(0,Q.jsx)(`input`,{type:`range`,min:0,max:Math.max(ae,0),value:Math.max(w,0),onChange:e=>oe(Number(e.target.value)),"aria-label":`Step`}),(0,Q.jsxs)(`span`,{className:`muted`,children:[w+1,`/`,C.length]})]}),y.size>0&&(0,Q.jsxs)(`div`,{className:`trace-focus`,children:[(0,Q.jsx)(`span`,{className:`muted`,children:`Only steps of`}),[...y].map(e=>(0,Q.jsxs)(`button`,{className:`chip`,style:{"--c":s[e]},onClick:()=>se(e),"aria-label":`Stop focusing ${o.lanes[e].id}`,children:[(0,Q.jsx)(`span`,{className:`chip-name`,children:o.lanes[e].id}),` `,(0,Q.jsx)(`span`,{"aria-hidden":`true`,children:`✕`})]},e)),(0,Q.jsx)(`button`,{className:`link`,onClick:()=>ee(new Set),children:`Show all`})]}),t===`log`?me:d===`story`?(0,Q.jsx)(Mv,{sourceAvailable:!r,trace:o,cursor:c,onSelect:T,onOpen:e=>{r||i(e.file,e.line)}}):d===`states`&&n?(0,Q.jsx)(`div`,{className:`replay-state-panel`,children:(0,Q.jsx)(Pv,{readOnly:r,model:n,activeMachine:fe,activeState:de?.state,onOpen:e=>{r||i(e.file,e.line)}},e)}):(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsxs)(`div`,{className:`trace-main list-${ne}`,children:[(0,Q.jsx)(ny,{trace:o,rows:C,cols:S,color:s,focus:y,cursor:c,hover:b,onSelect:T,onHover:x,onToggleLane:se}),ne!==`none`&&me]}),ce&&ne===`none`&&(0,Q.jsxs)(`div`,{className:`step-detail`,children:[(0,Q.jsx)(`div`,{children:ce.log}),ce.payload!==void 0&&(0,Q.jsx)(`pre`,{children:JSON.stringify(ce.payload,null,2)})]})]})]})}function my({trace:e,rows:t,color:n,cursor:r,hover:i,onSelect:a,onHover:o}){let s=(0,l.useRef)(null);return(0,l.useEffect)(()=>{let e=s.current,t=e?.querySelector(`[data-step="${r}"]`);if(!e||!t)return;let n=t.offsetTop-e.offsetTop;n<e.scrollTop?e.scrollTop=n-8:n+t.offsetHeight>e.scrollTop+e.clientHeight&&(e.scrollTop=n+t.offsetHeight-e.clientHeight+8)},[r,t]),(0,Q.jsx)(`ol`,{className:`step-list`,ref:s,"aria-label":`Steps`,children:t.map(t=>{let s=e.steps[t],c=[t===r&&`current`,t===i&&`hover`,s.kind===`error`&&`error`].filter(Boolean).join(` `);return(0,Q.jsx)(`li`,{"data-step":t,className:c,children:(0,Q.jsxs)(`button`,{onClick:()=>a(t),onMouseEnter:()=>o(t),onMouseLeave:()=>o(void 0),"aria-current":t===r?`step`:void 0,children:[(0,Q.jsx)(`span`,{className:`n`,children:t+1}),(0,Q.jsx)(`span`,{className:`swatch`,style:{background:s.lane===void 0?`var(--muted)`:n[s.lane]}}),(0,Q.jsx)(`span`,{className:`text`,children:s.log})]})},t)})})}function hy({model:e,testCase:t,pending:n,onOpen:r}){let[i,a]=(0,l.useState)(`overview`);if(!e)return(0,Q.jsxs)(`div`,{className:`model-overview`,children:[(0,Q.jsx)(`h2`,{children:`Explore the model you edit`}),(0,Q.jsx)(`p`,{children:n?`Reading your model…`:`Fix the source errors to update the model overview and state diagrams.`}),(0,Q.jsx)(`p`,{className:`muted`,children:`Edit any example or open your own files. Check explores different executions of the selected test.`})]});let o=e.tests.find(e=>e.name===t);return(0,Q.jsxs)(`div`,{className:`model-overview`,children:[(0,Q.jsxs)(`div`,{className:`model-heading`,children:[(0,Q.jsxs)(`div`,{children:[(0,Q.jsx)(`span`,{className:`eyebrow`,children:`YOUR EDITABLE MODEL`}),(0,Q.jsx)(`h2`,{children:`What are we checking?`})]}),(0,Q.jsxs)(`div`,{className:`view-switch`,"aria-label":`Model view`,children:[(0,Q.jsx)(`button`,{"aria-pressed":i===`overview`,onClick:()=>a(`overview`),children:`Overview`}),(0,Q.jsx)(`button`,{"aria-pressed":i===`states`,onClick:()=>a(`states`),children:`State diagrams`})]})]}),n&&(0,Q.jsx)(`p`,{role:`status`,className:`model-hint`,children:`Updating from your edits…`}),i===`states`?(0,Q.jsx)(Pv,{model:e,onOpen:r}):(0,Q.jsxs)(Q.Fragment,{children:[(0,Q.jsx)(`p`,{className:`model-intro`,children:`State machines keep their own data and communicate through events. The checker explores different choices and execution orders to find a rule violation.`}),o&&(0,Q.jsxs)(`section`,{className:`test-brief`,children:[(0,Q.jsx)(`span`,{className:`eyebrow`,children:`SELECTED TEST`}),(0,Q.jsx)(`button`,{className:`link`,onClick:()=>r(o.source),children:(0,Q.jsx)(`strong`,{children:o.name})}),(0,Q.jsxs)(`p`,{children:[`Starts with `,(0,Q.jsx)(`strong`,{children:o.main}),`. The test defines which machines and rules participate.`]}),(0,Q.jsxs)(`details`,{children:[(0,Q.jsx)(`summary`,{children:`Test composition`}),(0,Q.jsx)(`pre`,{children:o.composition})]})]}),(0,Q.jsx)(`h3`,{children:`Actors and rules`}),(0,Q.jsx)(`div`,{className:`model-actors`,children:e.machines.map(e=>(0,Q.jsxs)(`button`,{className:`model-actor`,onClick:()=>r(e.source),children:[(0,Q.jsx)(`span`,{className:`model-tag`,children:e.kind===`machine`?`actor`:`rule observer`}),(0,Q.jsx)(`strong`,{children:e.name}),(0,Q.jsxs)(`span`,{children:[e.states.length,` state`,e.states.length===1?``:`s`,e.variables.length?` · ${e.variables.map(e=>e.name).join(`, `)}`:``]})]},e.name))}),(0,Q.jsx)(`p`,{className:`model-hint`,children:`Declarations in this project; the selected test may use only some of them. Select any actor to read its code.`}),(0,Q.jsx)(`h3`,{children:`Assertions in the source`}),(0,Q.jsx)(`div`,{className:`model-rules`,children:e.machines.flatMap(e=>e.rules.map((t,n)=>(0,Q.jsxs)(`button`,{onClick:()=>r(t.source),children:[(0,Q.jsx)(`span`,{children:e.name}),(0,Q.jsx)(`code`,{children:t.condition})]},`${e.name}-${n}`)))}),!e.machines.some(e=>e.rules.length)&&(0,Q.jsx)(`p`,{className:`muted`,children:`No assertions found in state actions. Rules may also live in functions; hot states express progress obligations.`}),(0,Q.jsxs)(`div`,{className:`model-next`,children:[(0,Q.jsx)(`strong`,{children:`Edit → Check → Replay`}),(0,Q.jsx)(`p`,{children:`Change the source, then press Check. If a bug is found, walk through the recorded execution with variables, messages, and source side by side.`}),(0,Q.jsx)(`p`,{className:`muted`,children:`This checks your model under its assumptions and search limits. It does not execute your application.`})]})]})]})}function gy({size:e=34}){return(0,Q.jsx)(`svg`,{className:`pea-mark`,width:e,height:e*96/132,viewBox:`0 -12 132 96`,"aria-hidden":`true`,children:(0,Q.jsxs)(`g`,{transform:`rotate(-18 64 36)`,children:[(0,Q.jsx)(`path`,{d:`M119 34 Q129 27 126 16`,fill:`none`,stroke:`var(--pod)`,strokeWidth:`5`,strokeLinecap:`round`}),(0,Q.jsx)(`path`,{d:`M8 36 A72 72 0 0 1 120 36 A72 72 0 0 1 8 36 Z`,fill:`var(--pod)`}),(0,Q.jsx)(`circle`,{cx:`41`,cy:`36`,r:`9`,fill:`var(--pea)`}),(0,Q.jsx)(`circle`,{cx:`64`,cy:`36`,r:`11`,fill:`var(--pea)`}),(0,Q.jsx)(`circle`,{cx:`87`,cy:`36`,r:`9.5`,fill:`var(--pea)`}),(0,Q.jsxs)(`g`,{fill:`#fff`,fillOpacity:`0.85`,children:[(0,Q.jsx)(`circle`,{cx:`38.8`,cy:`31.8`,r:`2.7`}),(0,Q.jsx)(`circle`,{cx:`61.4`,cy:`30.8`,r:`3.3`}),(0,Q.jsx)(`circle`,{cx:`84.7`,cy:`31.5`,r:`2.85`})]})]})})}function _y(){return(0,Q.jsxs)(`span`,{className:`logo`,children:[(0,Q.jsx)(gy,{}),(0,Q.jsxs)(`span`,{className:`wordmark`,children:[`snap`,(0,Q.jsx)(`span`,{children:`pea`})]})]})}var vy=()=>Math.floor(Math.random()*1e6),yy=e=>e.split(/\s+/).filter(Boolean),by=`https://github.com/crowecawcaw/snappea`,xy=`(max-width: 900px)`,Sy=500,Cy={files:`Files`,model:`Model`,console:`Console`,trace:`Trace`,log:`Log`};function wy(){let e=(0,l.useMemo)(()=>new Ov,[]),[t,r]=(0,l.useState)(p[0]),[o,s]=(0,l.useState)({}),[c,u]=(0,l.useState)(``),[d,f]=(0,l.useState)([]),[h,v]=(0,l.useState)(p[0].testCase),[y,ee]=(0,l.useState)(10),[b,x]=(0,l.useState)(vy),[te,ne]=(0,l.useState)(!1),[re,ie]=(0,l.useState)(``),[S,C]=(0,l.useState)(!1),[T,oe]=(0,l.useState)([]),[se,ce]=(0,l.useState)(``),[le,ue]=(0,l.useState)(),[de,fe]=(0,l.useState)(``),[pe,me]=(0,l.useState)(``),[he,ge]=(0,l.useState)(),E=JSON.stringify(o),_e=(0,l.useMemo)(()=>t===m?void 0:_(t),[t]),ve=_e&&(Object.keys(_e).length!==Object.keys(o).length||Object.entries(_e).some(([e,t])=>o[e]!==t)),[ye,be]=(0,l.useState)(),[xe,Se]=(0,l.useState)(!1),[D,Ce]=(0,l.useState)(),[we,Te]=(0,l.useState)(),[O,Ee]=(0,l.useState)(),[De,Oe]=(0,l.useState)(()=>matchMedia(xy).matches),[ke,Ae]=(0,l.useState)(()=>matchMedia(xy).matches?`files`:`model`),[je,Me]=(0,l.useState)(),[Ne,Pe]=(0,l.useState)(!1),k=(0,l.useRef)(!1),[Fe,Ie]=(0,l.useState)(!1),[Le,Re]=(0,l.useState)(!1),ze=(0,l.useRef)(null),[Be,A]=(0,l.useState)(!1),[Ve,He]=(0,l.useState)(!1),[j,Ue]=(0,l.useState)(!1),We=(0,l.useRef)(0),Ge=(0,l.useRef)(null),Ke=(0,l.useCallback)((e,t,n)=>{r(e),s(t.files),u(t.files[t.open]===void 0?g(Object.keys(t.files))[0]??``:t.open),v(t.testCase??e.testCase),ee(t.timeout??10),ie(t.extraArgs??e.extraArgs??``),C(t.strict??!1),Ce(void 0),Te(void 0),oe([]),ue(void 0),fe(``),Ae(matchMedia(xy).matches?`files`:`model`),k.current=n},[]),qe=(0,l.useCallback)((e,t=e.open)=>Ke(e,{demo:e.id,files:_(e),open:t},!1),[Ke]),Je=(0,l.useCallback)(()=>{let e=n();if(e)try{let t=a(e);Ke(p.find(e=>e.id===t.demo)??m,t,!0);return}catch{alert(`This link's project couldn't be read; it may have been cut off. Opening a demo instead.`)}let t=new URLSearchParams(location.search),r=p.find(e=>e.id===t.get(`demo`))??p[0];qe(r,t.get(`file`)??r.open)},[qe,Ke]);(0,l.useEffect)(()=>{let e=ze.current;if(!e)return;let t=()=>Re(e.scrollHeight>e.clientHeight+1);t();let n=new ResizeObserver(t);return n.observe(e),()=>n.disconnect()},[t]),(0,l.useEffect)(()=>{let e=matchMedia(xy),t=()=>{Oe(e.matches),e.matches||Ae(e=>e===`files`?`model`:e)};return e.addEventListener(`change`,t),()=>e.removeEventListener(`change`,t)},[]),(0,l.useEffect)(()=>{Je(),Pe(!0);let e=()=>{n()&&Je()};return addEventListener(`hashchange`,e),()=>removeEventListener(`hashchange`,e)},[Je]),(0,l.useEffect)(()=>{if(!Ne)return;let e=setTimeout(()=>{let e={demo:t===m?void 0:t.id,files:o,open:c,testCase:h,timeout:y,extraArgs:re||void 0,strict:S||void 0},{url:n,tooBig:r}=i(e,k.current);He(r),n!==location.search+location.hash&&history.replaceState(null,``,n)},Sy);return()=>clearTimeout(e)},[Ne,t,o,c,h,y,re,S]),(0,l.useEffect)(()=>{if(!Ne||Object.keys(o).length===0)return;let t=!1,n=setTimeout(async()=>{try{let n=await e.compile(o);if(t)return;oe(n.diagnostics),ce(n.console),be(n.ok),ue(n.model),fe(JSON.stringify(o)),n.ok&&(f(n.tests),v(e=>n.tests.includes(e)?e:n.tests[0]??``))}catch{}},400);return()=>{t=!0,clearTimeout(n)}},[o,Ne,e]);let Ye=(0,l.useCallback)(async()=>{if(xe||!h)return;let t=te?b:vy();x(t),Se(!0),Te(void 0),Ee(void 0);try{let n=await e.compile(o),r=await e.check({files:o,testCase:h,timeout:y,seed:t,strict:S,extraArgs:yy(re)},Ee);Ce(r),me(JSON.stringify(o)),ge(n.model),r.diagnostics.length&&oe(r.diagnostics),Ae(r.bug&&r.traceJson?`trace`:`console`)}catch(e){let t=String(e.message??e);Te(t===`cancelled`?`Stopped.`:`The checker crashed: ${t}`),Ce(void 0),Ae(`console`)}finally{Se(!1)}},[xe,h,te,b,e,o,y,S,re]),Xe=(0,l.useCallback)((e,t,n=!0)=>{let r=e.replace(/^\/work\//,``),i=o[r]===void 0?Object.keys(o).find(e=>e.endsWith(`/`+r)||e===r):r;i&&(u(i),Me({line:t,nonce:Date.now(),focus:n}),n&&matchMedia(xy).matches&&Ae(`files`))},[o]),Ze=e=>{(!k.current||confirm(`Discard your edits and load another demo?`))&&qe(p.find(t=>t.id===e))},Qe=async e=>{let t;try{t=await e}catch(e){alert(`Couldn't open those files: ${e.message??e}`);return}if(k.current&&!confirm(`Discard your edits and open these files?`))return;let n=g(Object.keys(t));Ke(m,{files:t,open:n.find(e=>e.endsWith(`.p`))??n[0]},!0)},$e={onDragEnter:e=>{e.dataTransfer.types.includes(`Files`)&&(e.preventDefault(),We.current++,Ue(!0))},onDragOver:e=>{e.dataTransfer.types.includes(`Files`)&&(e.preventDefault(),e.dataTransfer.dropEffect=`copy`)},onDragLeave:()=>{--We.current<=0&&(We.current=0,Ue(!1))},onDrop:e=>{e.dataTransfer.types.includes(`Files`)&&(e.preventDefault(),We.current=0,Ue(!1),Qe(w(e.dataTransfer)))}},et=()=>{let e=prompt(`New file name`,`PSrc/NewMachine.p`)?.trim();if(!e)return;let t=e.endsWith(`.p`)||e.includes(`.`)?e:`${e}.p`;o[t]===void 0&&s({...o,[t]:``}),u(t),k.current=!0},tt=(0,l.useMemo)(()=>g(Object.keys(o)),[o]),nt=(0,l.useMemo)(()=>{let e=new Map;for(let t of T)e.set(t.file,(e.get(t.file)??0)+1);return e},[T]),rt=!!D&&(pe!==E||D.testCase!==h),it=xe?O?ky(O):`Checking…`:we||(ye===!1||D&&!D.ok?`Compile error`:rt?`Previous run · source or test changed`:D?D.bug?`Bug found`:D.exitCode===0?`No bugs found (see console for completion)`:`Error (see console)`:ye?`Ready`:`Loading…`),at=xe||rt?``:ye===!1||D?.bug||D&&(!D.ok||D.exitCode!==0)?`bad`:D?.ok?`good`:``,ot=(0,Q.jsxs)(`div`,{className:Fe?`blurb open`:`blurb`,children:[(0,Q.jsx)(`p`,{ref:ze,children:ve?`You are editing “${t.title}”. Explore the current source in Model, or Check again to find and replay its executions.`:t.blurb}),(Le||Fe)&&(0,Q.jsx)(`button`,{className:`more`,onClick:()=>Ie(!Fe),children:Fe?`less`:`more`})]}),st=!xe&&(ye===!1||D!==void 0&&!D.ok)&&T.length>0,ct=De?xe||!!we||st:xe||!!D||!!we||ye===!1,lt=D?.schedule&&!xe&&!rt&&(0,Q.jsxs)(`span`,{className:`downloads`,children:[(0,Q.jsx)(Oy,{name:`${D.testCase}.schedule`,text:D.schedule}),D.traceText&&(0,Q.jsx)(Oy,{name:`${D.testCase}.txt`,text:D.traceText}),D.traceJson&&(0,Q.jsx)(Oy,{name:`${D.testCase}.trace.json`,text:D.traceJson})]});return(0,Q.jsxs)(`div`,{className:`app`,...$e,children:[j&&(0,Q.jsx)(`div`,{className:`drop-overlay`,"aria-hidden":`true`,children:(0,Q.jsx)(`div`,{children:`Drop .p files, a folder or a .zip to open them`})}),(0,Q.jsxs)(`header`,{className:`top`,children:[(0,Q.jsxs)(`a`,{className:`brand`,href:`./`,"aria-label":`Snap Pea home`,children:[(0,Q.jsx)(_y,{}),(0,Q.jsx)(`span`,{className:`brand-name`,children:`Playground`})]}),(0,Q.jsxs)(`select`,{className:`demo-select`,value:t.id,onChange:e=>Ze(e.target.value),"aria-label":`Demo`,children:[t===m&&(0,Q.jsx)(`option`,{value:m.id,children:m.title}),p.map(e=>(0,Q.jsx)(`option`,{value:e.id,children:e.title},e.id))]}),(0,Q.jsx)(`button`,{className:`ghost open-files`,onClick:()=>Ge.current?.click(),title:`Open .p files or a .zip (or drop them anywhere)`,children:`Open files`}),(0,Q.jsx)(`input`,{ref:Ge,type:`file`,multiple:!0,accept:`.p,.zip,.toml,.json,.txt,.md`,hidden:!0,onChange:e=>{let t=e.target.files;t?.length&&Qe(ae(t)),e.target.value=``}}),Ve&&(0,Q.jsx)(`span`,{className:`share-note`,title:`The project is over 32k characters compressed, so the page URL no longer holds your edits.`,children:`Too big to link`}),(0,Q.jsx)(`div`,{className:`spacer`}),(0,Q.jsx)(`a`,{className:`button ghost docs-link`,href:`./docs/overview.html`,children:`Docs`}),(0,Q.jsxs)(`a`,{className:`button ghost gh`,href:by,target:`_blank`,rel:`noreferrer`,"aria-label":`GitHub`,children:[(0,Q.jsx)(`svg`,{width:`20`,height:`20`,viewBox:`0 0 16 16`,"aria-hidden":`true`,fill:`currentColor`,children:(0,Q.jsx)(`path`,{d:`M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z`})}),(0,Q.jsx)(`span`,{className:`gh-text`,children:`GitHub`})]})]}),!De&&ot,(0,Q.jsxs)(`main`,{className:`panes tab-${ke}`,children:[(0,Q.jsxs)(`section`,{className:`pane editor-pane`,children:[De&&ot,(0,Q.jsxs)(`div`,{className:`file-tabs`,role:`tablist`,children:[tt.map(e=>{let t=e.lastIndexOf(`/`),n=nt.get(e);return(0,Q.jsxs)(`button`,{role:`tab`,"aria-selected":e===c,className:e===c?`file-tab active`:`file-tab`,onClick:()=>u(e),title:e,children:[t>=0&&(0,Q.jsx)(`span`,{className:`dir`,children:e.slice(0,t+1)}),e.slice(t+1),n?(0,Q.jsx)(`span`,{className:`badge`,children:n}):null]},e)}),(0,Q.jsx)(`button`,{className:`file-tab add`,onClick:et,title:`New file`,"aria-label":`New file`,children:`+`})]}),c&&o[c]!==void 0&&(0,Q.jsx)(Dv,{path:c,value:o[c],diagnostics:T.filter(e=>e.file===c),jump:je,onChange:(e,t)=>{k.current=!0,s(n=>({...n,[e]:t}))},onRun:Ye})]}),(0,Q.jsxs)(`section`,{className:`pane run-pane`,children:[(0,Q.jsxs)(`div`,{className:Be?`controls options-open`:`controls`,children:[(0,Q.jsxs)(`div`,{className:`run-bar`,children:[(0,Q.jsxs)(`label`,{className:`tc`,children:[(0,Q.jsx)(`span`,{className:`lbl`,children:`Test case`}),(0,Q.jsxs)(`select`,{value:h,onChange:e=>v(e.target.value),disabled:!d.length,"aria-label":`Test case`,children:[d.length===0&&(0,Q.jsx)(`option`,{value:h,children:h||`—`}),d.map(e=>(0,Q.jsx)(`option`,{value:e,children:e},e))]})]}),xe?(0,Q.jsx)(`button`,{className:`danger run`,onClick:()=>e.cancel(),children:`Stop`}):(0,Q.jsx)(`button`,{className:`primary run`,onClick:Ye,disabled:!h||ye===!1,title:`Ctrl/⌘+Enter`,children:ye===!1?T.length===1?`Fix 1 error`:T.length?`Fix ${T.length} errors`:`Fix errors`:`Check`})]}),(0,Q.jsxs)(`details`,{className:`options`,open:Be,onToggle:e=>A(e.target.open),children:[(0,Q.jsxs)(`summary`,{children:[(0,Q.jsx)(`svg`,{className:`chevron`,width:`10`,height:`10`,viewBox:`0 0 10 10`,"aria-hidden":`true`,children:(0,Q.jsx)(`path`,{d:`M3 1.5 L6.5 5 L3 8.5`,fill:`none`,stroke:`currentColor`,strokeWidth:`1.6`,strokeLinecap:`round`,strokeLinejoin:`round`})}),`Options`,(0,Q.jsxs)(`span`,{className:`muted`,children:[` `,`· up to `,y,`s`,te?` · seed ${b}`:``,S?` · strict`:``,re.trim()?` · ${re.trim()}`:``]})]}),(0,Q.jsxs)(`div`,{className:`options-grid`,children:[(0,Q.jsxs)(`label`,{children:[`Timeout (seconds)`,(0,Q.jsx)(`input`,{type:`number`,inputMode:`numeric`,min:1,max:3600,value:y,onChange:e=>ee(Math.min(3600,Math.max(1,Math.trunc(Number(e.target.value))||1)))})]}),(0,Q.jsxs)(`label`,{children:[`Seed`,(0,Q.jsx)(`input`,{type:`number`,inputMode:`numeric`,value:b,onChange:e=>{x(Number(e.target.value)||0),ne(!0)}})]}),(0,Q.jsxs)(`label`,{className:`check`,children:[(0,Q.jsx)(`input`,{type:`checkbox`,checked:te,onChange:e=>ne(e.target.checked)}),`Keep seed`]}),(0,Q.jsxs)(`label`,{className:`check`,title:`Fail on lint findings and missing coverage, as the snappea CLI does. Off: they are shown as warnings and the check goes on.`,children:[(0,Q.jsx)(`input`,{type:`checkbox`,checked:S,onChange:e=>C(e.target.checked)}),`Strict checks`]}),(0,Q.jsxs)(`label`,{className:`span-all`,children:[`Extra flags`,(0,Q.jsx)(`input`,{value:re,placeholder:`e.g. --timed`,onChange:e=>ie(e.target.value),spellCheck:!1,autoCapitalize:`off`,autoCorrect:`off`})]})]})]})]}),(0,Q.jsxs)(`div`,{className:`out-head`,children:[(0,Q.jsxs)(`div`,{className:`out-tabs`,role:`tablist`,children:[(De?[`files`,`model`,`console`,`trace`,`log`]:[`model`,`console`,`trace`,`log`]).map(e=>(0,Q.jsxs)(`button`,{role:`tab`,"aria-selected":ke===e,className:ke===e?`out-tab active`:`out-tab`,onClick:()=>Ae(e),disabled:(e===`trace`||e===`log`)&&!D?.traceJson,children:[Cy[e],e===`files`&&T.length>0&&(0,Q.jsx)(`span`,{className:`badge`,children:T.length}),e===`trace`&&D?.bug&&(0,Q.jsx)(`span`,{className:`bug-dot`,"aria-label":`bug found`})]},e)),De&&(0,Q.jsx)(`button`,{className:Be?`out-tab gear active`:`out-tab gear`,onClick:()=>A(!Be),"aria-expanded":Be,"aria-label":`Checker options`,title:`Checker options`,children:(0,Q.jsxs)(`svg`,{width:`20`,height:`20`,viewBox:`0 0 24 24`,"aria-hidden":`true`,fill:`none`,stroke:`currentColor`,strokeWidth:`1.8`,strokeLinecap:`round`,strokeLinejoin:`round`,children:[(0,Q.jsx)(`path`,{d:`M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0`}),(0,Q.jsx)(`circle`,{cx:`16`,cy:`6`,r:`2`}),(0,Q.jsx)(`circle`,{cx:`10`,cy:`12`,r:`2`}),(0,Q.jsx)(`circle`,{cx:`18`,cy:`18`,r:`2`})]})})]}),ct&&(0,Q.jsxs)(`div`,{className:`status-line`,children:[(0,Q.jsx)(`span`,{className:`status ${at}`,children:it}),xe&&!!O?.time_limit&&(0,Q.jsx)(`span`,{className:`progress`,role:`progressbar`,"aria-label":`Time used`,"aria-valuemin":0,"aria-valuemax":100,"aria-valuenow":Math.round(Math.min(1,O.elapsed/O.time_limit)*100),children:(0,Q.jsx)(`span`,{style:{width:`${Math.min(1,O.elapsed/O.time_limit)*100}%`}})}),!xe&&(ye===!1||D&&!D.ok)&&T[0]&&(0,Q.jsxs)(`button`,{className:`diag`,onClick:()=>Xe(T[0].file,T[0].line),children:[T[0].file,`:`,T[0].line,` `,T[0].message]}),D&&!xe&&!st&&(0,Q.jsxs)(`span`,{className:`muted`,children:[(D.millis/1e3).toFixed(2),` s`]}),!De&&lt]})]}),(0,Q.jsxs)(`div`,{className:`output`,children:[ke===`model`&&(0,Q.jsx)(hy,{model:de===E?le:void 0,testCase:h,pending:de!==E,onOpen:e=>Xe(e.file,e.line)}),ke===`console`&&(0,Q.jsx)(Dy,{summary:!we&&D?.ok&&(D.bug||D.exitCode===0)?D.bug?`✕ ${Ey(D)}`:`✓ No bugs found in ${D.testCase}`:void 0,bad:D?.bug,text:we??(D?D.console:ye===!1?se:``),onOpenLocation:Xe,empty:ye===!1?``:`Press Check (Ctrl/⌘+Enter) to explore schedules of the selected test case.`}),(ke===`trace`||ke===`log`)&&D?.traceJson&&(0,Q.jsx)(py,{json:D.traceJson,model:he,stale:rt,view:ke===`log`?`log`:`diagram`,onOpenLocation:Xe,onShowDiagram:()=>Ae(`trace`)})]})]})]})]})}var Ty=/([\w./-]+\.p):(\d+)(?::(\d+))?/g;function Ey(e){let t=`${e.traceText??``}\n${e.console}`.split(`
`).find(e=>/<ErrorLog>|Assertion Failed|Deadlock|liveness|Exception/i.test(e));return t?t.replace(/^.*?<ErrorLog>\s*/,``).trim():`Bug found`}function Dy({text:e,empty:t,summary:n,bad:r,onOpenLocation:i}){return e?(0,Q.jsxs)(`pre`,{className:`console`,children:[n&&(0,Q.jsx)(`div`,{className:r?`summary bad`:`summary good`,children:n}),e.split(`
`).map((e,t)=>{let n=/^FAIL\s|\bfailed in\b|Error|Assertion Failed|\bbug found\b|found a bug|Found [1-9]/i.test(e)?`line bad`:/^ok\s|\bpassed in\b|Found 0 bug/.test(e)?`line good`:e.startsWith(`$ `)?`line cmd`:`line`,r=[],a=0;for(let t of e.matchAll(Ty)){r.push(e.slice(a,t.index));let[n,o,s]=t;r.push((0,Q.jsx)(`button`,{className:`loc`,onClick:()=>i(o,Number(s)),children:n},t.index)),a=(t.index??0)+n.length}return r.push(e.slice(a)),(0,Q.jsx)(`div`,{className:n,children:r},t)})]}):(0,Q.jsx)(`div`,{className:`console empty`,children:t})}function Oy({name:e,text:t}){let n=(0,l.useMemo)(()=>URL.createObjectURL(new Blob([t],{type:`text/plain`})),[t]);return(0,l.useEffect)(()=>()=>URL.revokeObjectURL(n),[n]),(0,Q.jsxs)(`a`,{className:`download`,href:n,download:e,title:`Download ${e}`,children:[`⤓ `,e.slice(e.indexOf(`.`)+1)]})}function ky(e){let t=e.time_limit?`${e.elapsed.toFixed(1)} of ${e.time_limit} s`:`${e.elapsed.toFixed(1)} s`;if(e.phase===`mutation`)return`Mutation testing: ${e.detected} of ${e.mutants} mutants detected · ${t}`;let n=e.schedules.toLocaleString(`en-US`),r=e.bugs?` · ${e.bugs} bug${e.bugs===1?``:`s`}`:``;return`Checking ${e.test_case}: ${n} schedule${e.schedules===1?``:`s`}${r} · ${t}`}(0,u.createRoot)(document.getElementById(`root`)).render((0,Q.jsx)(l.StrictMode,{children:(0,Q.jsx)(wy,{})}));