// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

spec LivenessClientsDone observes ClientPutRequest, ClientGetRequest, ClientFinishedMonitor {
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
    on ClientFinishedMonitor do (c: Client) {
      finishedClients += (c);
      if (finishedClients == activeClients) {
        goto AllClientsDone;
      }
    }

    // on ClientRequest do (payload: ClientRequestData) {
    //     if (payload.client == payload.sender) {
    //         activeClients += (payload.client);
    //     }
    // }

    on ClientPutRequest do (payload: ClientPutRequestData) {
      if (payload.client == payload.sender) {
        activeClients += (payload.client);
      }
    }

    on ClientGetRequest do (payload: ClientGetRequestData) {
      if (payload.client == payload.sender) {
        activeClients += (payload.client);
      }
    }
  }

  cold state AllClientsDone {
    on ClientPutRequest do (payload: ClientPutRequestData) {
      if (payload.client == payload.sender) {
        activeClients += (payload.client);
        goto ClientsActive;
      }
    }

    on ClientGetRequest do (payload: ClientGetRequestData) {
      if (payload.client == payload.sender) {
        activeClients += (payload.client);
        goto ClientsActive;
      }
    }

    on ClientFinishedMonitor do (c: Client) {
      print format("Problematic: {0}", c);
      assert false, "Received a client finished event after all clients are done.";
    }
  }
}
