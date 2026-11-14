// Drivers. Clients, senders and the server are the design; the network is environment.

fun Msgs(a: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  return s;
}

// Two senders, `clients` clients, and `drops` connections that may drop. Everyone
// starts at once, so messages are posted while clients connect and reconnect.
machine Room {
  start state Running {
    entry (r: (server: machine, clients: int, drops: int)) {
      var server: machine;
      var network: machine;
      var i: int;
      announce Planned, (msgs = 2, clients = r.clients);
      server = r.server;
      network = new Network(r.drops);
      while (i < r.clients) {
        new Client((id = i + 1, server = server, network = network));
        i = i + 1;
      }
      new Sender((server = server, msgs = Msgs(1)));
      new Sender((server = server, msgs = Msgs(2)));
    }
  }
}

// The core race: two senders, one client whose first connection may drop while a
// message is in flight; the client resumes, possibly while a message is being posted
// (during the handshake), and may get a replayed duplicate.
machine DropAndResume {
  start state Init {
    entry { new Room((server = new Server(), clients = 1, drops = 1)); }
  }
}

// Two clients, one of which may lose its connection: the other keeps receiving live
// messages meanwhile. Sampled.
machine TwoClients {
  start state Init {
    entry { new Room((server = new Server(), clients = 2, drops = 1)); }
  }
}

test tcDropAndResume [main=DropAndResume]:
  assert InOrderNoGaps, ResumesAfterLastShown, EveryoneCatchesUp in { DropAndResume, Room, Server, Network, Connection, Client, Sender };

test tcTwoClients [main=TwoClients]:
  assert InOrderNoGaps, ResumesAfterLastShown, EveryoneCatchesUp in { TwoClients, Room, Server, Network, Connection, Client, Sender };

machine BuggyDropAndResume {
  start state Init {
    entry { new Room((server = new NoReplayServer(), clients = 1, drops = 1)); }
  }
}

// snappea: expect-bug "EveryoneCatchesUp detected liveness bug in hot state 'CatchingUp'"
test tcBugNoReplay [main=BuggyDropAndResume]:
  assert InOrderNoGaps, EveryoneCatchesUp in { BuggyDropAndResume, Room, NoReplayServer, Network, Connection, Client, Sender };
