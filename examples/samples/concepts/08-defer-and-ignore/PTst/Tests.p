// Drivers: two clients and a health probe start at the same time as the server, so
// their events can arrive before or after the configuration.

machine Client {
  var server: machine;
  var checkHealth: bool;

  start state Asking {
    entry (input: (server: machine, id: int, checkHealth: bool)) {
      server = input.server;
      checkHealth = input.checkHealth;
      announce eRequested, (id = input.id,);
      send server, eRequest, (client = this, id = input.id);
    }
    on eResponse do {
      // A response proves the server is serving, so a health check now must be answered.
      if (checkHealth) {
        announce eHealthCheckDue;
        send server, eHealthCheck, (probe = this,);
      }
    }
    ignore eHealthy;
  }
}

// Fire and forget: an ignored check gets no answer, like a probe that times out.
machine Probe {
  start state Checking {
    entry (server: machine) {
      send server, eHealthCheck, (probe = this,);
    }
    ignore eHealthy;
  }
}

fun StartAll(server: machine) {
  new Client((server = server, id = 1, checkHealth = true));
  new Client((server = server, id = 2, checkHealth = false));
  new Probe(server);
}

machine StartupRace {
  start state Init {
    entry { StartAll(new Server(new Storage())); }
  }
}

machine StartupRaceNoDefer {
  start state Init {
    entry { StartAll(new NoDeferServer(new Storage())); }
  }
}

test tcStartupRace [main=StartupRace]:
  assert ServedAfterInit in { StartupRace, Client, Probe, Server, Storage };

// expect-bug: eRequest' that cannot be handled
test tcBugNoDefer [main=StartupRaceNoDefer]:
  assert ServedAfterInit in { StartupRaceNoDefer, Client, Probe, NoDeferServer, Storage };
