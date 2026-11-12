// Two-phase commit: clients request writes; the coordinator asks every participant to
// prepare before committing. Timeouts and crashes can interrupt this agreement.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// Each participant maintains a local key-value store which is updated based on the
// transactions committed by the coordinator. On receiving a prepare request from the
// coordinator, the participant chooses to either accept or reject the transaction.

machine Participant {
  // local key value store
  var kvStore: map[string, Transaction];
  // pending write transactions that have not been committed or aborted yet.
  var pendingWriteTrans: map[int, Transaction];
  // coordinator machine
  var coordinator: Coordinator;

  start state Init {
    on InformCoordinator goto WaitForRequests with (coor: Coordinator) {
      coordinator = coor;
    }

    defer ShutDown;
  }

  state WaitForRequests {
    on AbortTrans do (transactionId: int) {
      // check that abort transaction request is received for a pending transaction only.
      assert transactionId in pendingWriteTrans,
      format ("Abort request for a non-pending transaction, transactionId: {0}, pendingTrans set: {1}",
      transactionId, pendingWriteTrans);
      // remove the transaction from the pending transactions set
      pendingWriteTrans -= (transactionId);
    }

    on CommitTrans do (transactionId:int) {
      // check that  commit transaction request is received for a pending transactions only.
      assert transactionId in pendingWriteTrans,
      format ("Commit request for a non-pending transaction, transactionId: {0}, pendingTrans set: {1}",
      transactionId, pendingWriteTrans);
      // commit the transaction locally
      kvStore[pendingWriteTrans[transactionId].key] = pendingWriteTrans[transactionId];
      // remove the transaction from the pending transactions set
      pendingWriteTrans -= (transactionId);
    }

    on PrepareRequest do (prepareReq :PrepareRequestData) {
      // cannot receive prepare for an already pending transaction
      assert !(prepareReq.transactionId in pendingWriteTrans),
      format ("Duplicate transaction ids not allowed!, received transactionId: {0}, pending transactions: {1}",
      prepareReq.transactionId, pendingWriteTrans);
      // add the transaction to the pending transactions set
      pendingWriteTrans[prepareReq.transactionId] = prepareReq;
      // accept the transaction unless it is older than the value already stored for the key
      if (!(prepareReq.key in kvStore) || prepareReq.transactionId > kvStore[prepareReq.key].transactionId) {
        send coordinator, PrepareResponse, (participant = this, transactionId = prepareReq.transactionId, status = SUCCESS);
      }
      else {
        send coordinator, PrepareResponse, (participant = this, transactionId = prepareReq.transactionId, status = ERROR);
      }
    }

    on ReadTransRequest do (request: ReadTransRequestData) {
      // clients read only keys they wrote, after the write committed on every participant
      assert request.key in kvStore, format ("read of key {0}, which was never committed", request.key);
      send request.client, ReadTransResponse, (key = request.key, value = kvStore[request.key].value, status = SUCCESS);
    }

    on ShutDown goto Crashed;
  }

  // A crashed participant takes no part in transactions. A read sent to it fails fast,
  // as a refused connection would, so the client is not left waiting forever.
  state Crashed {
    on ReadTransRequest do (request: ReadTransRequestData) {
      send request.client, ReadTransResponse, (key = request.key, value = -1, status = TIMEOUT);
    }

    ignore PrepareRequest, CommitTrans, AbortTrans, ShutDown;
  }
}
