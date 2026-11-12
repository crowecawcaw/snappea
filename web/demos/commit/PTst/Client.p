// Two-phase commit: clients request writes; the coordinator asks every participant to
// prepare before committing. Timeouts and crashes can interrupt this agreement.
// This test harness chooses the number of actors and starts the workload. The checker
// varies their scheduling and model choices.

// The client machine below implements the client of the two-phase-commit transaction
// service. Each client issues N non-deterministic write-transactions, if the transaction
// succeeds then it performs a read-transaction on the same key and asserts the value.
machine Client {
  // the coordinator machine
  var coordinator: Coordinator;
  // current transaction issued by the client
  var currTransaction : Transaction;
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
      currTransaction = ChooseRandomTransaction(id);
      send coordinator, WriteTransRequest, (client = this, transaction = currTransaction);
    }

    on WriteTransResponse goto ConfirmTransaction;
  }

  state ConfirmTransaction {
    entry (writeResp: WriteTransResponseData) {
      // assert that if write transaction was successful then value read is the value written.
      if(writeResp.status == SUCCESS) {
        send coordinator, ReadTransRequest, (client= this, key = currTransaction.key);
        // await response from the participant
        receive {
          case ReadTransResponse: (readResp: ReadTransResponseData) {
            // a crashed participant answers TIMEOUT; a live one must have the committed value
            assert readResp.status == TIMEOUT || readResp.key == currTransaction.key && readResp.value == currTransaction.value,
            format ("Record read is not same as what was written by the client:: read - {0}, written - {1}",
            readResp.value, currTransaction.value);
          }
        }
      }

      // has more work to do?
      if(N > 0) {
        N = N - 1;
        goto SendWriteTransaction;
      }
    }
  }
}

// Each client writes its own keys, so no other client's write can land between a write
// and the read that checks it.
// Transaction ids come in any order and can repeat, as retried or delayed requests would:
// the coordinator must reject repeats and participants must reject stale writes.
fun ChooseRandomTransaction(clientId: int): Transaction {
  return (key = format("{0}-{1}", clientId, choose(10)), value = choose(10), transactionId = clientId * 100 + choose(10));
}

// two phase commit client module
module TwoPCClient = {
  Client
}
;
