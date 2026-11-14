// The same client as a state machine: one state per outstanding request, and the
// reply handler decides the next step. Each waiting state must say what happens to
// an Increment that arrives meanwhile: here, defer it until the client is Idle.
machine StateClient {
  var store: Store;
  var current: (version: int, value: int);

  start state Init {
    entry (s: Store) {
      store = s;
      goto Idle;
    }
  }

  state Idle {
    on Increment goto Reading;
  }

  state Reading {
    defer Increment;
    entry {
      send store, Get, (client = this,);
    }
    on GetResponse do (r: (version: int, value: int)) {
      current = r;
      goto Writing;
    }
  }

  state Writing {
    defer Increment;
    entry {
      send store, Cas, (client = this, version = current.version, value = current.value + 1);
    }
    on CasResponse do (r: (ok: bool)) {
      if (r.ok) {
        goto Idle;
      } else {
        goto Reading;
      }
    }
  }
}
