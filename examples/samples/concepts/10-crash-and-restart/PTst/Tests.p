// The process manager and crash injector. It starts the server and two clients, crashes
// the server at most once, at a moment the scheduler picks, restarts it on the same
// storage, and tells the clients the new address.
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
      if ($) { send server, Crash; }
    }
    on Exited do {
      Reconnect(clients, new Server((storage = storage, owner = this)));
    }
  }
}

fun StartClients(server: machine): seq[Client] {
  var clients: seq[Client];
  var c: Client;
  var i: int;
  announce ClientsPlanned, 2;
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
    send c, Restarted, server;
  }
}

// The broken servers replace Server wherever the driver creates one (see
// 06-tests-and-modules for bindings).
module MemoryOnly = { MemoryOnlyServer -> Server };
module ReplyFirst = { ReplyFirstServer -> Server };

test tcCrashAnywhere [main=CrashOnce]:
  assert TicketsUnique in { CrashOnce, Storage, Server, Client };

// snappea: expect-bug "ticket 1 issued twice"
test tcBugMemoryOnly [main=CrashOnce]:
  assert TicketsUnique in (union { CrashOnce, Storage, Client }, MemoryOnly);

// snappea: expect-bug "ticket 1 issued twice"
test tcBugReplyFirst [main=CrashOnce]:
  assert TicketsUnique in (union { CrashOnce, Storage, Client }, ReplyFirst);
