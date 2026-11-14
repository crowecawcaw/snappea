// The compare-and-swap counter from 07-receive, with the same names. Each client gets
// (version, value), then CASes value + 1 at that version; on a conflict it gets again
// and retries. The store's two handlers are the real atomic primitives.

event Get: (client: machine);
event Got: (version: int, value: int);
event Cas: (client: machine, version: int, value: int);
event CasResult: (ok: bool);

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

// Increments the counter once.
machine Client {
  var store: Store;

  start state Init {
    entry (s: Store) {
      store = s;
      goto Reading;
    }
  }

  state Reading {
    entry {
      send store, Get, (client = this,);
    }
    on Got do (r: (version: int, value: int)) {
      send store, Cas, (client = this, version = r.version, value = r.value + 1);
    }
    on CasResult do (r: (ok: bool)) {
      if (r.ok) {
        goto Done;
      }
      // Someone else wrote first: get the new value and try again.
      goto Reading;
    }
  }

  state Done {
    entry {
      announce ClientDone, (client = this,);
    }
  }
}
