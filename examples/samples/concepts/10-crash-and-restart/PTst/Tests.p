// The process manager and crash injector. It starts the server and two clients, crashes
// the server at most once, at a moment the scheduler picks, restarts it on the same storage, and
// tells the clients the new address.
machine CrashOnce {
  var storage: Storage;
  var clients: seq[Client];

  start state Running {
    entry {
      var server: Server;
      storage = new Storage();
      server = new Server((storage = storage, owner = this));
      clients = StartClients(server);
      // No crash, or one that lands before, between or during the requests, or after both.
      if ($) { send server, eCrash; }
    }
    on eExited do {
      Reconnect(clients, new Server((storage = storage, owner = this)));
    }
  }
}

// The same process manager, running the memory-only server.
machine CrashOnceMemoryOnly {
  var storage: Storage;
  var clients: seq[Client];

  start state Running {
    entry {
      var server: MemoryOnlyServer;
      storage = new Storage();
      server = new MemoryOnlyServer((storage = storage, owner = this));
      clients = StartClients(server);
      send server, eCrash;
    }
    on eExited do {
      Reconnect(clients, new MemoryOnlyServer((storage = storage, owner = this)));
    }
  }
}

fun StartClients(server: machine): seq[Client] {
  var clients: seq[Client];
  var c: Client;
  var i: int;
  announce eClientsPlanned, 2;
  while (i < 2) {
    c = new Client(server);
    clients += (i, c);
    i = i + 1;
  }
  return clients;
}

fun Reconnect(clients: seq[Client], server: machine) {
  var c: Client;
  foreach (c in clients) {
    send c, eRestarted, server;
  }
}

test tcCrashAnywhere [main=CrashOnce]:
  assert TicketsUnique in { CrashOnce, Storage, Server, Client };

// expect-bug: issued twice
test tcBugMemoryOnly [main=CrashOnceMemoryOnly]:
  assert TicketsUnique in { CrashOnceMemoryOnly, Storage, MemoryOnlyServer, Client };
