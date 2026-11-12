// The design: a server must load its configuration from storage before it can serve.
// Clients and a health probe do not wait for that: their events can arrive while the
// server is still initializing.
//   - Requests must not be lost: the server defers them until it is serving.
//   - Health checks are only meaningful now: while initializing, the server ignores
//     them (no answer means "not healthy yet"), rather than answering late.

event eLoad: (server: machine);
event eLoaded: (config: int);
event eRequest: (client: machine, id: int);
event eResponse: (id: int, config: int);
event eHealthCheck: (probe: machine);
event eHealthy;

machine Storage {
  start state Serving {
    on eLoad do (l: (server: machine)) {
      send l.server, eLoaded, (config = 7,);
    }
  }
}

machine Server {
  var config: int;

  start state Initializing {
    defer eRequest;
    ignore eHealthCheck;
    entry (storage: Storage) {
      send storage, eLoad, (server = this,);
    }
    on eLoaded do (l: (config: int)) {
      config = l.config;
      announce eReady;
      goto Serving;
    }
  }

  state Serving {
    on eRequest do (r: (client: machine, id: int)) {
      send r.client, eResponse, (id = r.id, config = config);
    }
    on eHealthCheck do (h: (probe: machine)) {
      send h.probe, eHealthy;
    }
  }
}

// A plausible broken variant, kept as a negative control: it forgot that requests can
// arrive before the configuration does. tcBugNoDefer shows the run failing.
machine NoDeferServer {
  var config: int;

  start state Initializing {
    ignore eHealthCheck;
    entry (storage: Storage) {
      send storage, eLoad, (server = this,);
    }
    on eLoaded do (l: (config: int)) {
      config = l.config;
      announce eReady;
      goto Serving;
    }
  }

  state Serving {
    on eRequest do (r: (client: machine, id: int)) {
      send r.client, eResponse, (id = r.id, config = config);
    }
    on eHealthCheck do (h: (probe: machine)) {
      send h.probe, eHealthy;
    }
  }
}
