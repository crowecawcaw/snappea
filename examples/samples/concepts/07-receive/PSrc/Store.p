// The design: a versioned store with two atomic primitives, get and compare-and-swap.
// Clients increment the value with get, then CAS at the version they read, and retry
// when another client wrote in between.

event Get: (client: machine);
event GetResponse: (version: int, value: int);
event Cas: (client: machine, version: int, value: int);
event CasResponse: (ok: bool);
// From the application to a client: "add one to the value".
event Increment;

machine Store {
  var version: int;
  var value: int;

  start state Serving {
    on Get do (g: (client: machine)) {
      send g.client, GetResponse, (version = version, value = value);
    }
    on Cas do (c: (client: machine, version: int, value: int)) {
      if (c.version == version) {
        value = c.value;
        // snappea-lint: allow mutation:swap-operator -- versions only need to differ; counting down works as well
        version = version + 1;
        announce Committed, (value = value,);
        send c.client, CasResponse, (ok = true,);
      } else {
        send c.client, CasResponse, (ok = false,);
      }
    }
  }
}
