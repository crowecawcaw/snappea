// The design: a versioned store with two atomic primitives, get and compare-and-swap.
// Clients increment the value with get, then CAS at the version they read, and retry
// when another client wrote in between. 12-scenarios uses the same store.

event Get: (client: machine);
event Got: (version: int, value: int);
event Cas: (client: machine, version: int, value: int);
event CasResult: (ok: bool);
// From the application to a client: "add one to the value".
event Increment;

machine Store {
  var version: int;
  var value: int;

  start state Serving {
    // Atomic primitive: read the value and its version together.
    on Get do (g: (client: machine)) {
      send g.client, Got, (version = version, value = value);
    }
    // Atomic primitive: write only if nobody wrote since the caller's get.
    on Cas do (c: (client: machine, version: int, value: int)) {
      if (c.version == version) {
        // commit point: the value changes and the reply says so
        value = c.value;
        // snappea-lint: allow mutation:swap-operator -- equivalent: versions are only compared for equality, so counting down works as well
        version = version + 1;
        announce Committed, (value = value,);
        send c.client, CasResult, (ok = true,);
      } else {
        send c.client, CasResult, (ok = false,);
      }
    }
  }
}
