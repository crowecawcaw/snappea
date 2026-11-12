// Drivers: start N clients in a loop, each with its own id, against one server.

fun StartClients(capacity: int, clients: int) {
  var server: PermitServer;
  var id: int;
  server = new PermitServer(capacity);
  announce eClientsPlanned, (capacity = capacity, clients = clients);
  id = 1;
  while (id <= clients) {
    announce eClientStarted, (id = id,);
    new Client((server = server, id = id));
    id = id + 1;
  }
}

// One more client than permits: someone must be refused, unless a holder released
// first. Three clients is the smallest number that exceeds two permits.
machine ThreeClientsTwoPermits {
  start state Init {
    entry { StartClients(2, 3); }
  }
}

// The boundary case of a lock: one permit, two clients.
machine TwoClientsOnePermit {
  start state Init {
    entry { StartClients(1, 2); }
  }
}

test tcThreeClientsTwoPermits [main=ThreeClientsTwoPermits]:
  assert PermitsRespected in { ThreeClientsTwoPermits, PermitServer, Client };

test tcTwoClientsOnePermit [main=TwoClientsOnePermit]:
  assert PermitsRespected in { TwoClientsOnePermit, PermitServer, Client };
