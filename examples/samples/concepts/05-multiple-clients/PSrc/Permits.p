// The design: a permit server hands out at most `capacity` permits at a time. Each
// client asks for a permit, and if it gets one, uses it and gives it back. Clients run
// concurrently, so requests and releases from different clients interleave.

event Acquire: (client: Client, id: int);
event AcquireResponse: (ok: bool);
event Release: (id: int);

machine PermitServer {
  var capacity: int;
  var holders: set[int];

  start state Serving {
    entry (n: int) {
      capacity = n;
    }
    on Acquire do (a: (client: Client, id: int)) {
      if (sizeof(holders) < capacity) {
        holders += (a.id);
        announce Granted, (id = a.id,);
        send a.client, AcquireResponse, (ok = true,);
      } else {
        announce Denied, (id = a.id,);
        send a.client, AcquireResponse, (ok = false,);
      }
    }
    on Release do (r: (id: int)) {
      holders -= (r.id);
      announce Released, (id = r.id,);
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
      send server, Acquire, (client = this, id = id);
    }
    on AcquireResponse do (r: (ok: bool)) {
      if (r.ok) {
        // Use the permit, then give it back.
        send server, Release, (id = id,);
      }
      announce ClientFinished, (id = id, gotPermit = r.ok);
    }
  }
}
