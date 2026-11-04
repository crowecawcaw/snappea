// Two-phase commit: clients request writes; the coordinator asks every participant to
// prepare before committing. Timeouts and crashes can interrupt this agreement.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

/* User Defined Types */

// a transaction consisting of the key, value, and the unique transaction id.
type Transaction = (key: string, value: int, transactionId: int);
// payload type associated with the `WriteTransRequest` event where `client`: client sending the
// transaction, `transaction`: transaction to be committed.
type WriteTransRequestData = (client: Client, transaction: Transaction);
// payload type associated with the `WriteTransResponse` event where `transactionId` is the transaction Id
// and `status` is the return status of the transaction request.
type WriteTransResponseData = (transactionId: int, status: TransactionStatus);
// payload type associated with the `ReadTransRequest` event where `client` is the Client
// machine sending
// the read request and `key` is the key whose value the client wants to read.
type ReadTransRequestData = (client: Client, key: string);
// payload type associated with the `ReadTransResponse` event where `value` is the value
// corresponding to
// the `key` in the read request and `status` is the read status (e.g., success or failure)
type ReadTransResponseData = (key: string, value: int, status: TransactionStatus);

// transaction status
enum TransactionStatus {
  SUCCESS,
  ERROR,
  TIMEOUT
}

/* Events used by the 2PC clients to communicate with the 2PC coordinator */
// event: write transaction request (client to coordinator)
event WriteTransRequest : WriteTransRequestData;
// event: write transaction response (coordinator to client)
event WriteTransResponse : WriteTransResponseData;
// event: read transaction request (client to coordinator)
event ReadTransRequest : ReadTransRequestData;
// event: read transaction response (participant to client)
event ReadTransResponse: ReadTransResponseData;

/* Events used for communication between the coordinator and the participants */
// event: prepare request for a transaction (coordinator to participant)
event PrepareRequest: PrepareRequestData;
// event: prepare response for a transaction (participant to coodinator)
event PrepareResponse: PrepareResponseData;
// event: commit transaction (coordinator to participant)
event CommitTrans: int;
// event: abort transaction (coordinator to participant)
event AbortTrans: int;

/* User Defined Types */
// payload type associated with the `PrepareRequest` event
type PrepareRequestData = Transaction;
// payload type assocated with the `PrepareResponse` event where `participant` is the
// participant machine
// sending the response, `transactionId` is the transaction id, and `status` is the status of the prepare
// request for that transaction.
type PrepareResponseData = (participant: Participant, transactionId: int, status: TransactionStatus);

// event: inform participant about the coordinator
event InformCoordinator: Coordinator;

// The Coordinator machine receives write and read transactions from the clients. The
// coordinator machine services these transactions one by one in the order in which they
// were received. On receiving a write transaction the coordinator sends prepare request to
// all the participants and waits for prepare responses from all the participants. Based on
// the responses, the coordinator either commits or aborts the transaction. If the
// coordinator fails to receive agreement from participants in time, then it timesout and
// aborts the transaction. On receiving a read transaction, the coordinator randomly
// selects a participant and forwards the read request to that participant.
machine Coordinator {
  // set of participants
  var participants: set[Participant];
  // current write transaction being handled
  var currentWriteTransReq: WriteTransRequestData;
  // previously seen transaction ids
  var seenTransIds: set[int];
  var timer: Timer;

  start state Init {
    entry (payload: set[Participant]){
      participants = payload;
      timer = CreateTimer(this);
      // inform all participants that I am the coordinator
      BroadcastToAllParticipants(InformCoordinator, this);
      goto WaitForTransactions;
    }
  }

  state WaitForTransactions {
    on WriteTransRequest do (wTrans : WriteTransRequestData) {
      if(wTrans.transaction.transactionId in seenTransIds) // transactionId have to be unique
      {
        send wTrans.client, WriteTransResponse, (transactionId = wTrans.transaction.transactionId, status = TIMEOUT);
        return;
      }

      currentWriteTransReq = wTrans;
      BroadcastToAllParticipants(PrepareRequest, wTrans.transaction);
      //start timer while waiting for responses from all participants
      StartTimer(timer);
      goto WaitForPrepareResponses;
    }

    on ReadTransRequest do (rTrans : ReadTransRequestData) {
      // non-deterministically pick a participant to read from.
      send choose(participants), ReadTransRequest, rTrans;
    }

    // when in this state it is fine to drop these messages as they are from the previous transaction
    ignore PrepareResponse, TimeOut;
  }

  var countPrepareResponses: int;

  state WaitForPrepareResponses {
    // defer requests, we are going to process transactions sequentially
    defer WriteTransRequest;

    on PrepareResponse do (response : PrepareResponseData) {
      // check if the response is for the current transaction else ignore it
      if (currentWriteTransReq.transaction.transactionId == response.transactionId) {
        if(response.status == SUCCESS) {
          countPrepareResponses = countPrepareResponses + 1;
          // check if we have received all responses
          if(countPrepareResponses == sizeof(participants)) {
            // lets commit the transaction
            DoGlobalCommit();
            // safe to go back and service the next transaction
            goto WaitForTransactions;
          }
        }

        else {
          DoGlobalAbort(ERROR);
          // safe to go back and service the next transaction
          goto WaitForTransactions;
        }
      }
    }

    // on timeout abort the transaction
    on TimeOut goto WaitForTransactions with {
      DoGlobalAbort(TIMEOUT);
    }

    on ReadTransRequest do (rTrans : ReadTransRequestData) {
      // non-deterministically pick a participant to read from.
      send choose(participants), ReadTransRequest, rTrans;
    }

    exit {
      countPrepareResponses = 0;
    }
  }

  fun DoGlobalAbort(respStatus: TransactionStatus) {
    // ask all participants to abort and fail the transaction
    BroadcastToAllParticipants(AbortTrans, currentWriteTransReq.transaction.transactionId);
    send currentWriteTransReq.client, WriteTransResponse, (transactionId = currentWriteTransReq.transaction.transactionId, status = respStatus);
    if(respStatus != TIMEOUT)
    CancelTimer(timer);
  }

  fun DoGlobalCommit() {
    // ask all participants to commit and respond to client
    BroadcastToAllParticipants(CommitTrans, currentWriteTransReq.transaction.transactionId);
    send currentWriteTransReq.client, WriteTransResponse,
    (transactionId = currentWriteTransReq.transaction.transactionId, status = SUCCESS);
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
