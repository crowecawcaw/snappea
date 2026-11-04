// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

/*
* A client that uses the Raft cluster.
* The client sends a sequence of commands to the cluster.
* It does not move on the next command until it receives a response from the cluster.
* The client retries sending the command if a certain amount of heartbeats timeouts occur.
*/

// Client requests
type ClientRequestData = (transactionId: TransactionId, client: Client, cmd: Command, sender: Client);
type ClientPutRequestData = (transactionId: TransactionId, client: Client, key: KeyT, value: ValueT, sender: Client);
type ClientGetRequestData = (transactionId: TransactionId, client: Client, key: KeyT, sender: Client);
event ClientPutRequest: ClientPutRequestData;
event ClientGetRequest: ClientGetRequestData;
// event ClientRequest: ClientRequestData;
// The event of notifying the monitor that the client is waiting for a response
event ClientWaitingResponse: (client: Client, transactionId: int);
// The event of notifying the monitor that the client got a response for a transaction
event ClientGotResponse: (client: Client, transactionId: int);
// The event of notifying the monitor that the client finished
event ClientFinishedMonitor: Client;
// The event of notifying the view service that the client finished
event ClientFinished: Client;

machine Client {
  var worklist: seq[Command];
  var servers: set[machine];
  var ptr: int;
  var Id: int;
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
      Id = 0;
      timer = new Timer((user=this, timeoutEvent=HeartbeatTimeout));
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
      }
      else {
        // get the current command and increase transaction id
        currentCmd = worklist[ptr];
        ptr = ptr + 1;
        Id = Id + 1;
        broadcastToCluster();
        goto WaitForResponse;
      }
    }
  }

  state WaitForResponse {
    entry {
      announce ClientWaitingResponse, (client=this, transactionId=Id);
      startTimer(timer);
      retries = 0;
    }

    on HeartbeatTimeout do {
      // print format("Client {0} timed out waiting for response {1}; current retries: {2}", this, Id, retries / 50);
      if (retries % 200 == 0) {
        // retries every 200 heartbeats
        broadcastToCluster();
      }

      retries = retries + 1;
      startTimer(timer);
    }

    on RaftGetResponse do (response: RaftGetResponseData) {
      handleResponse(response.transactionId);
    }

    on RaftPutResponse do (response: RaftPutResponseData) {
      handleResponse(response.transactionId);
    }
  }

  fun handleResponse(responseTransId: TransactionId) {
    if (responseTransId == Id) {
      announce ClientGotResponse, (client=this, transactionId=Id);
      retries = 0;
      goto SendOne;
    }
  }

  fun broadcastToCluster() {
    var s: machine;
    foreach (s in servers) {
      if (currentCmd.op == GET) {
        send s, ClientGetRequest, (transactionId=Id, client=this, key=currentCmd.key, sender=this);
      }
      else {
        send s, ClientPutRequest, (transactionId=Id, client=this, key=currentCmd.key, value=currentCmd.value, sender=this);
      }
    }
  }

  state Done {
    entry {
      announce ClientFinishedMonitor, this;
      send view, ClientFinished, this;
    }

    ignore RaftGetResponse, RaftPutResponse, HeartbeatTimeout;
  }
}
