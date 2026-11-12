// A shared counter updated with compare-and-swap. Each client reads (version, value),
// computes value + 1 and writes it only if the version is unchanged; on a conflict it
// reads again and retries. The store's handlers are the real atomic primitives.

event eRead: machine;
event eReadResp: (version: int, value: int);
event eCas: (client: machine, version: int, value: int);
event eCasResp: bool;

machine Store {
  var version: int;
  var value: int;

  start state Serving {
    on eRead do (client: machine) {
      send client, eReadResp, (version = version, value = value);
    }
    on eCas do (c: (client: machine, version: int, value: int)) {
      if (c.version != version) {
        send c.client, eCasResp, false;
        return;
      }
      version = version + 1;
      value = c.value;
      announce eWritten, (client = c.client, version = version, value = value);
      send c.client, eCasResp, true;
    }
  }
}

machine Client {
  var store: Store;

  start state Reading {
    entry (s: Store) {
      store = s;
      send store, eRead, this;
    }
    on eReadResp do (r: (version: int, value: int)) {
      send store, eCas, (client = this, version = r.version, value = r.value + 1);
    }
    on eCasResp do (ok: bool) {
      if (ok) {
        goto Done;
      }
      // Someone else wrote first: read the new value and try again.
      send store, eRead, this;
    }
  }

  state Done {
    entry { announce eClientDone, this; }
  }
}
