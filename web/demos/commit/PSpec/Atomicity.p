// Two-phase commit: clients request writes; the coordinator asks every participant to
// prepare before committing. Timeouts and crashes can interrupt this agreement.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

// event: initialize the AtomicityInvariant spec monitor
event Monitor_AtomicityInitialize: int;

// We would like to assert the atomicity property that: if a transaction is committed by
// the coordinator then it was agreed on by all participants
spec AtomicityInvariant observes WriteTransResponse, PrepareResponse, Monitor_AtomicityInitialize
{
  // a map from transaction id to a map from responses status to number of participants with that response
  var participantsResponse: map[int, map[TransactionStatus, int]];
  var numParticipants: int;
  start state Init {
    on Monitor_AtomicityInitialize goto WaitForEvents with (n: int) {
      numParticipants = n;
    }
  }

  state WaitForEvents {
    // snappea-lint: allow unused-payload-field -- counts responses per transaction; each participant responds once
    on PrepareResponse do (response: PrepareResponseData){
      var transactionId: int;
      transactionId = response.transactionId;

      if(!(transactionId in participantsResponse)) {
        participantsResponse[transactionId] = default(map[TransactionStatus, int]);
        participantsResponse[transactionId][SUCCESS] = 0;
        participantsResponse[transactionId][ERROR] = 0;
      }

      participantsResponse[transactionId][response.status] = participantsResponse[transactionId][response.status] + 1;
    }

    on WriteTransResponse do (response: WriteTransResponseData) {
      assert (response.transactionId in participantsResponse || response.status == TIMEOUT),
      format ("Write transaction was responded to the client without receiving any responses from the participants!");

      if(response.status == SUCCESS) {
        assert participantsResponse[response.transactionId][SUCCESS] == numParticipants,
        format ("Write transaction was responded as committed before receiving success from all participants. ") +
        format ("participants sent success: {0}, participants sent error: {1}", participantsResponse[response.transactionId][SUCCESS],
        participantsResponse[response.transactionId][ERROR]);
      }

      else if(response.status == ERROR)
      {
        assert participantsResponse[response.transactionId][ERROR] > 0,
        format ("Write transaction {0} was responded as failed before receiving error from atleast one participant.", response.transactionId) +
        format ("participants sent success: {0}, participants sent error: {1}", participantsResponse[response.transactionId][SUCCESS],
        participantsResponse[response.transactionId][ERROR]);
      }

      // remove the transaction information
      participantsResponse -= (response.transactionId);
    }
  }
}

// Every received transaction from a client must be eventually responded back. Note, the
// usage of hot and cold states.
spec Progress observes WriteTransRequest, WriteTransResponse {
  var pendingTransactions: int;
  start state Init {
    on WriteTransRequest goto WaitForResponses with {
      pendingTransactions = pendingTransactions + 1;
    }
  }

  hot state WaitForResponses
  {
    on WriteTransResponse do {
      pendingTransactions = pendingTransactions - 1;
      if(pendingTransactions == 0) {
        goto AllTransactionsFinished;
      }
    }

    on WriteTransRequest do {
      pendingTransactions = pendingTransactions + 1;
    }
  }

  cold state AllTransactionsFinished {
    on WriteTransRequest goto WaitForResponses with {
      pendingTransactions = pendingTransactions + 1;
    }
  }
}
