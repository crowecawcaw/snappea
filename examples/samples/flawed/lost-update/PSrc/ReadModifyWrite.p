// THE FLAW. A balance service whose clients deposit by reading the balance, adding
// the amount, and writing the result back. Each call is atomic on its own; the
// sequence is not. Two deposits that read the same balance both write "old + mine",
// and the second write erases the first deposit.
//
// Real primitives: get (read the row), put (overwrite the row). Nothing else.

event eGet: (client: machine);
event eGot: (balance: int);
event ePut: (client: machine, id: int, balance: int);
event ePutDone;

machine BlindStore {
  var balance: int;
  start state Serving {
    on eGet do (r: (client: machine)) {
      send r.client, eGot, (balance = balance,);
    }
    on ePut do (w: (client: machine, id: int, balance: int)) {
      balance = w.balance;
      announce eBalanceWritten, (id = w.id, balance = balance);
      send w.client, ePutDone;
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
      send store, eGet, (client = this,);
    }
    on eGot do (r: (balance: int)) {
      // BUG: another deposit can commit between this read and the write below.
      newBalance = r.balance + amount;
      send store, ePut, (client = this, id = id, balance = newBalance);
      goto Writing;
    }
  }

  state Writing {
    on ePutDone do {
      announce eDepositReturned, (id = id, balance = newBalance);
    }
  }
}
