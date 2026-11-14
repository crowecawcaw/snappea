// Drivers: clients that each make one request and wait for the answer.

machine Client {
  start state Asking {
    entry (input: (server: machine, id: int)) {
      announce Requested, (id = input.id,);
      send input.server, Request, (client = this, id = input.id);
    }
    on Response goto Answered;
  }
  state Answered {}
}

// Two clients at once, so one request arrives while the server is busy with the other.
machine TwoClients {
  start state Init {
    entry {
      var server: Server;
      server = new Server();
      new Client((server = server, id = 1));
      new Client((server = server, id = 2));
    }
  }
}

machine TwoClientsDroppingServer {
  start state Init {
    entry {
      var server: DroppingServer;
      server = new DroppingServer();
      new Client((server = server, id = 1));
      new Client((server = server, id = 2));
    }
  }
}

test tcTwoClients [main=TwoClients]:
  assert RequestsAnswered in { TwoClients, Client, Server, Worker };

// snappea: expect-bug "liveness bug in hot state 'Waiting'"
test tcBugDropsWhileBusy [main=TwoClientsDroppingServer]:
  assert RequestsAnswered in { TwoClientsDroppingServer, Client, DroppingServer, Worker };
