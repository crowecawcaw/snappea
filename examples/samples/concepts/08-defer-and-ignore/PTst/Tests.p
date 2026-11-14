// Drivers: two clients and a health probe start at the same time as the server, so
// their events can arrive before or after the configuration.

machine Client {
  var server: machine;
  var checkHealth: bool;

  start state Asking {
    entry (input: (server: machine, id: int, checkHealth: bool)) {
      server = input.server;
      checkHealth = input.checkHealth;
      announce Requested, (id = input.id,);
      send server, Request, (client = this, id = input.id);
    }
    on Response do {
      // A response proves the server is serving, so a health check now must be answered.
      if (checkHealth) {
        announce HealthCheckDue;
        send server, HealthCheck, (probe = this,);
      }
    }
    ignore Healthy;
  }
}

// Fire and forget: an ignored check gets no answer, like a probe that times out.
machine Probe {
  start state Checking {
    entry (server: machine) {
      send server, HealthCheck, (probe = this,);
    }
    ignore Healthy;
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

// snappea: expect-bug "Request' that it cannot handle"
test tcBugNoDefer [main=StartupRaceNoDefer]:
  assert ServedAfterInit in { StartupRaceNoDefer, Client, Probe, NoDeferServer, Storage };
