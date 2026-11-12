// The design: a permit server hands out at most `capacity` permits at a time. Each
// client asks for a permit, and if it gets one, uses it and gives it back. Clients run
// concurrently, so requests and releases from different clients interleave.

event eAcquire: (client: Client, id: int);
event eAcquireResp: (ok: bool);
event eRelease: (id: int);

machine PermitServer {
  var capacity: int;
  var holders: set[int];

  start state Serving {
    entry (n: int) {
      capacity = n;
    }
    on eAcquire do (a: (client: Client, id: int)) {
      if (sizeof(holders) < capacity) {
        holders += (a.id);
        announce eGranted, (id = a.id,);
        send a.client, eAcquireResp, (ok = true,);
      } else {
        announce eDenied, (id = a.id,);
        send a.client, eAcquireResp, (ok = false,);
      }
    }
    on eRelease do (r: (id: int)) {
      holders -= (r.id);
      announce eReleased, (id = r.id,);
    }
  }
}

machine Client {
  var id: int;
  var server: PermitServer;

  start state Acquiring {
    entry (input: (server: PermitServer, id: int)) {
      id = input.id;
      server = input.server;
      send server, eAcquire, (client = this, id = id);
    }
    on eAcquireResp do (r: (ok: bool)) {
      if (r.ok) {
        // Use the permit, then give it back.
        send server, eRelease, (id = id,);
      }
      announce eClientFinished, (id = id, gotPermit = r.ok);
    }
  }
}
