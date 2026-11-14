// The client written RPC-style: each call sends a request and blocks in `receive` until
// its reply arrives. The algorithm reads top to bottom, like the real client code.
machine ReceiveClient {
  var store: Store;

  start state Idle {
    entry (s: Store) {
      store = s;
    }
    // While ApplyIncrement() blocks in a receive, a second Increment stays queued.
    // When the receive returns, the machine is still in Idle, which handles it next.
    on Increment do {
      ApplyIncrement();
    }
  }

  fun ApplyIncrement() {
    var current: (version: int, value: int);
    var done: bool;
    while (!done) {
      current = FetchCurrent();
      done = TryCas(current.version, current.value + 1);
    }
  }

  fun FetchCurrent(): (version: int, value: int) {
    var result: (version: int, value: int);
    send store, Get, (client = this,);
    receive {
      case Got: (r: (version: int, value: int)) {
        result = r;
      }
    }
    return result;
  }

  fun TryCas(version: int, value: int): bool {
    var ok: bool;
    send store, Cas, (client = this, version = version, value = value);
    receive {
      case CasResult: (r: (ok: bool)) {
        ok = r.ok;
      }
    }
    return ok;
  }
}
