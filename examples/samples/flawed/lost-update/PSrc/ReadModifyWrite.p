// THE FLAW. A balance service whose clients deposit by reading the balance, adding
// the amount, and writing the result back. Each call is atomic on its own; the
// sequence is not. Two deposits that read the same balance both write "old + mine",
// and the second write erases the first deposit.
//
// Real primitives: get (read the row), put (overwrite the row). Nothing else.

event Get: (client: machine);
event Got: (balance: int);
event Put: (client: machine, id: int, balance: int);
event PutDone;

machine BlindStore {
  var balance: int;
  start state Serving {
    on Get do (r: (client: machine)) {
      send r.client, Got, (balance = balance,);
    }
    on Put do (w: (client: machine, id: int, balance: int)) {
      balance = w.balance;
      announce BalanceWritten, (id = w.id, balance = balance);
      send w.client, PutDone;
    }
  }
}

machine ReadModifyWriteClient {
  var store: BlindStore;
  var id: int;
  var amount: int;
  var newBalance: int;

  start state Reading {
    entry (d: (store: BlindStore, id: int, amount: int)) {
      store = d.store;
      id = d.id;
      amount = d.amount;
      send store, Get, (client = this,);
    }
    on Got do (r: (balance: int)) {
      // BUG: another deposit can commit between this read and the write below.
      newBalance = r.balance + amount;
      send store, Put, (client = this, id = id, balance = newBalance);
      goto Writing;
    }
  }

  state Writing {
    on PutDone do {
      announce DepositReturned, (id = id, balance = newBalance);
    }
  }
}
