// An application that opens its connection and, unless it holds it, closes it as soon
// as it is connected. It tells the driver when it is connected.
machine App {
  var owner: machine;
  start state Running {
    entry (input: (owner: machine, conn: Connection, holds: bool)) {
      owner = input.owner;
      send input.conn, eOpen, this;
      if (input.holds) { goto Holding; }
    }
    on eConnected do (conn: machine) {
      send owner, eConnected, conn;
      send conn, eClose, ByClient;
    }
  }
  // Keeps the connection open until the server takes it away.
  state Holding {
    on eConnected do (conn: machine) { send owner, eConnected, conn; }
  }
}

fun StartApp(owner: machine, server: Server, holds: bool) {
  new App((owner = owner, conn = new Connection(server), holds = holds));
}

// Two connections compete for one slot. The second evicts the first or is refused; the
// first's close races the server's reset.
machine TwoConnectionsOneSlot {
  start state Init {
    entry {
      var server: Server;
      announce ePlanned, (slots = 1, connections = 2);
      server = new Server(1);
      StartApp(this, server, false);
      StartApp(this, server, false);
    }
    ignore eConnected;
  }
}

// Exactly at capacity: two connections, two slots, nobody is refused. Each connection
// must free its own session.
machine TwoConnectionsTwoSlots {
  start state Init {
    entry {
      var server: Server;
      announce ePlanned, (slots = 2, connections = 2);
      server = new Server(2);
      StartApp(this, server, false);
      StartApp(this, server, false);
    }
    ignore eConnected;
  }
}

// An idle holder: the first connection stays open, and only once it is connected does
// the second arrive. Without eviction, the holder would never close.
machine EvictIdleHolder {
  var server: Server;
  start state First {
    entry {
      announce ePlanned, (slots = 1, connections = 2);
      server = new Server(1);
      StartApp(this, server, true);
    }
    on eConnected goto Second;
  }
  state Second {
    entry { StartApp(this, server, false); }
    ignore eConnected;
  }
}

test tcTwoConnectionsOneSlot [main=TwoConnectionsOneSlot]:
  assert Lifecycle in { TwoConnectionsOneSlot, App, Server, Connection };

test tcTwoConnectionsTwoSlots [main=TwoConnectionsTwoSlots]:
  assert Lifecycle in { TwoConnectionsTwoSlots, App, Server, Connection };

test tcEvictIdleHolder [main=EvictIdleHolder]:
  assert Lifecycle in { EvictIdleHolder, App, Server, Connection };
