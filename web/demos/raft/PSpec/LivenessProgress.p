// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

type RequestMetadata = (client: machine, transactionId: int);

spec LivenessProgress observes ClientWaitingResponse, ClientGotResponse {
  var clientRequests: set[RequestMetadata];

  start state Init {
    entry {
      clientRequests = default(set[RequestMetadata]);
      goto Done;
    }
  }

  hot state PendingRequestsExist {
    on ClientWaitingResponse do (payload: RequestMetadata) {
      clientRequests += ((client=payload.client, transactionId=payload.transactionId));
    }

    on ClientGotResponse do (payload: RequestMetadata) {
      clientRequests -= ((client=payload.client, transactionId=payload.transactionId));
      if (sizeof(clientRequests) == 0) {
        goto Done;
      }
    }
  }

  cold state Done {
    on ClientWaitingResponse do (payload: RequestMetadata) {
      clientRequests += ((client=payload.client, transactionId=payload.transactionId));
      goto PendingRequestsExist;
    }
  }
}
