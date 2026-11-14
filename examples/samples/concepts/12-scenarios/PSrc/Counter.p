// A shared counter updated with compare-and-swap. Each client reads (version, value),
// computes value + 1 and writes it only if the version is unchanged; on a conflict it
// reads again and retries. The store's handlers are the real atomic primitives.

event Read: machine;
event ReadResponse: (version: int, value: int);
event Cas: (client: machine, version: int, value: int);
event CasResponse: bool;

machine Store {
  var version: int;
  var value: int;

  start state Serving {
    on Read do (client: machine) {
      send client, ReadResponse, (version = version, value = value);
    }
    on Cas do (c: (client: machine, version: int, value: int)) {
      if (c.version != version) {
        send c.client, CasResponse, false;
        return;
      }
      version = version + 1;
      value = c.value;
      announce Written, (client = c.client, version = version, value = value);
      send c.client, CasResponse, true;
    }
  }
}

machine Client {
  var store: Store;

  start state Reading {
    entry (s: Store) {
      store = s;
      send store, Read, this;
    }
    on ReadResponse do (r: (version: int, value: int)) {
      send store, Cas, (client = this, version = r.version, value = r.value + 1);
    }
    on CasResponse do (ok: bool) {
      if (ok) {
        goto Done;
      }
      // Someone else wrote first: read the new value and try again.
      send store, Read, this;
    }
  }

  state Done {
    entry { announce ClientDone, this; }
  }
}
