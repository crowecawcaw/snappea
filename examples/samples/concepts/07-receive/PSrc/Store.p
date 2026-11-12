// The design: a versioned store with two atomic primitives, get and compare-and-swap.
// Clients increment the value with get, then CAS at the version they read, and retry
// when another client wrote in between.

event eGet: (client: machine);
event eGetResp: (version: int, value: int);
event eCas: (client: machine, version: int, value: int);
event eCasResp: (ok: bool);
// From the application to a client: "add one to the value".
event eIncrement;

machine Store {
  var version: int;
  var value: int;

  start state Serving {
    on eGet do (g: (client: machine)) {
      send g.client, eGetResp, (version = version, value = value);
    }
    on eCas do (c: (client: machine, version: int, value: int)) {
      if (c.version == version) {
        value = c.value;
        // snappea-lint: allow mutation:swap-operator -- versions only need to differ; counting down works as well
        version = version + 1;
        announce eCommitted, (value = value,);
        send c.client, eCasResp, (ok = true,);
      } else {
        send c.client, eCasResp, (ok = false,);
      }
    }
  }
}
