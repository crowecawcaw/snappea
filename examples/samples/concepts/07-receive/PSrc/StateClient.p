// The same client as a state machine: one state per outstanding request, and the
// reply handler decides the next step. Each waiting state must say what happens to
// an eIncrement that arrives meanwhile: here, defer it until the client is Idle.
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
    on eIncrement goto Reading;
  }

  state Reading {
    defer eIncrement;
    entry {
      send store, eGet, (client = this,);
    }
    on eGetResp do (r: (version: int, value: int)) {
      current = r;
      goto Writing;
    }
  }

  state Writing {
    defer eIncrement;
    entry {
      send store, eCas, (client = this, version = current.version, value = current.value + 1);
    }
    on eCasResp do (r: (ok: bool)) {
      if (r.ok) {
        goto Idle;
      } else {
        goto Reading;
      }
    }
  }
}
