// The client written RPC-style: each call sends a request and blocks in `receive` until
// its reply arrives. The algorithm reads top to bottom, like the real client code.
machine ReceiveClient {
  var store: Store;

  start state Idle {
    entry (s: Store) {
      store = s;
    }
    // A second eIncrement that arrives while Increment() is blocked waits in the queue:
    // this state handles it, so it is kept until the receive returns.
    on eIncrement do {
      Increment();
    }
  }

  fun Increment() {
    var current: (version: int, value: int);
    var done: bool;
    while (!done) {
      current = Get();
      done = Cas(current.version, current.value + 1);
    }
  }

  fun Get(): (version: int, value: int) {
    var result: (version: int, value: int);
    send store, eGet, (client = this,);
    receive {
      case eGetResp: (r: (version: int, value: int)) {
        result = r;
      }
    }
    return result;
  }

  fun Cas(version: int, value: int): bool {
    var ok: bool;
    send store, eCas, (client = this, version = version, value = value);
    receive {
      case eCasResp: (r: (ok: bool)) {
        ok = r.ok;
      }
    }
    return ok;
  }
}
