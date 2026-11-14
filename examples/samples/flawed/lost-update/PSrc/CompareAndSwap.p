// THE FIX. The store keeps a version with the balance and offers compare-and-swap:
// a write succeeds only if the row still has the version the client read. A client
// whose read went stale re-reads and recomputes, so no deposit is computed from a
// balance that someone else has already replaced.
//
// Real primitives: read (balance, version); cas(expected version, new balance), one
// conditional UPDATE ... WHERE version = expected.

event Read: (client: machine);
event ReadResp: (balance: int, version: int);
event Cas: (client: machine, id: int, expected: int, balance: int);
event CasResp: (ok: bool);

machine VersionedStore {
  var balance: int;
  var version: int;
  start state Serving {
    on Read do (r: (client: machine)) {
      send r.client, ReadResp, (balance = balance, version = version);
    }
    on Cas do (w: (client: machine, id: int, expected: int, balance: int)) {
      if (w.expected != version) {
        send w.client, CasResp, (ok = false,);
        return;
      }
      balance = w.balance;
      // snappea-lint: allow mutation:swap-operator -- versions are only compared for equality; counting down keeps them unique
      version = version + 1;
      announce BalanceWritten, (id = w.id, balance = balance);
      send w.client, CasResp, (ok = true,);
    }
  }
}

machine CasClient {
  var store: VersionedStore;
  var id: int;
  var amount: int;
  var newBalance: int;

  start state Init {
    entry (d: (store: VersionedStore, id: int, amount: int)) {
      store = d.store;
      id = d.id;
      amount = d.amount;
      goto Reading;
    }
  }

  state Reading {
    entry {
      send store, Read, (client = this,);
    }
    on ReadResp do (r: (balance: int, version: int)) {
      newBalance = r.balance + amount;
      send store, Cas, (client = this, id = id, expected = r.version, balance = newBalance);
      goto Writing;
    }
  }

  state Writing {
    on CasResp do (r: (ok: bool)) {
      if (!r.ok) {
        // Someone committed since our read: start over from a fresh read.
        goto Reading;
      }
      announce DepositReturned, (id = id, balance = newBalance);
    }
  }
}
