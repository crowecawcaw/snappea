// The design: a server must load its configuration from storage before it can serve.
// Clients and a health probe do not wait for that: their events can arrive while the
// server is still initializing.
//   - Requests must not be lost: the server defers them until it is serving.
//   - Health checks are only meaningful now: while initializing, the server ignores
//     them (no answer means "not healthy yet"), rather than answering late.

event Load: (server: machine);
event Loaded: (config: int);
event Request: (client: machine, id: int);
event Response: (id: int, config: int);
event HealthCheck: (probe: machine);
event Healthy;

machine Storage {
  start state Serving {
    on Load do (l: (server: machine)) {
      send l.server, Loaded, (config = 7,);
    }
  }
}

machine Server {
  var config: int;

  start state Initializing {
    // A client is waiting for this answer: keep the request until Serving.
    defer Request;
    // "Not healthy yet" is the answer: the probe times out and asks again.
    ignore HealthCheck;
    entry (storage: Storage) {
      send storage, Load, (server = this,);
    }
    on Loaded do (l: (config: int)) {
      config = l.config;
      announce Ready;
      goto Serving;
    }
  }

  state Serving {
    on Request do (r: (client: machine, id: int)) {
      send r.client, Response, (id = r.id, config = config);
    }
    on HealthCheck do (h: (probe: machine)) {
      send h.probe, Healthy;
    }
  }
}

// A plausible broken variant, kept as a negative control: it forgot that requests can
// arrive before the configuration does. tcBugNoDefer shows the run failing.
machine NoDeferServer {
  var config: int;

  start state Initializing {
    // BUG: no `defer Request;`, so a request that arrives now cannot be handled.
    ignore HealthCheck;
    entry (storage: Storage) {
      send storage, Load, (server = this,);
    }
    on Loaded do (l: (config: int)) {
      config = l.config;
      announce Ready;
      goto Serving;
    }
  }

  state Serving {
    on Request do (r: (client: machine, id: int)) {
      send r.client, Response, (id = r.id, config = config);
    }
    on HealthCheck do (h: (probe: machine)) {
      send h.probe, Healthy;
    }
  }
}
