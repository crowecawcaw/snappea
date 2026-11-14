// The deposit contract, stated over events alone. It knows nothing about versions,
// reads or retries, so it judges the flawed and the fixed design by the same rule.

// The driver announces how many deposits it will make, then each one it starts.
event DepositsPlanned: int;
event DepositRequested: (id: int, amount: int);
// The store announces each write at the moment it commits, with the balance it stored.
event BalanceWritten: (id: int, balance: int);
// The client announces that its deposit call returned.
event DepositReturned: (id: int, balance: int);

// Safety: every committed deposit is added to the balance exactly once, on top of
// every deposit committed before it. A write that overwrites an earlier deposit
// leaves the balance short of the sum, which is a lost update.
spec NoLostUpdate observes DepositRequested, BalanceWritten {
  var requested: map[int, int];
  var committed: set[int];
  var expected: int;

  start state Watching {
    on DepositRequested do (p: (id: int, amount: int)) {
      assert !(p.id in requested), format("deposit {0} requested twice", p.id);
      requested[p.id] = p.amount;
    }
    on BalanceWritten do (p: (id: int, balance: int)) {
      assert p.id in requested, format("deposit {0} written but never requested", p.id);
      assert !(p.id in committed), format("deposit {0} applied twice", p.id);
      committed += (p.id);
      expected = expected + requested[p.id];
      assert p.balance == expected,
        format("lost update: deposit {0} stored balance {1}, but the deposits committed so far sum to {2}",
          p.id, p.balance, expected);
    }
  }
}

// Progress: every deposit the driver starts returns, after it committed, and reports
// the balance its own write stored. Hot until every planned deposit has returned.
spec DepositsReturn observes DepositsPlanned, DepositRequested, BalanceWritten, DepositReturned {
  var pending: set[int];
  // id -> balance its committed write stored
  var written: map[int, int];
  var unfinished: int;

  start state Idle {
    on DepositsPlanned do (n: int) {
      assert n > 0, "planned no deposits";
      unfinished = n;
      goto Busy;
    }
    on DepositRequested do { assert false, "deposit started without a plan"; }
    on BalanceWritten do { assert false, "write with no deposit in progress"; }
    on DepositReturned do { assert false, "deposit returned with none in progress"; }
  }

  hot state Busy {
    on DepositsPlanned do { assert false, "deposits planned twice"; }

    // snappea-lint: allow unused-payload-field -- amounts are checked by NoLostUpdate
    on DepositRequested do (p: (id: int, amount: int)) { pending += (p.id); }
    on BalanceWritten do (p: (id: int, balance: int)) {
      assert p.id in pending, format("deposit {0} written but not in progress", p.id);
      written[p.id] = p.balance;
    }
    on DepositReturned do (p: (id: int, balance: int)) {
      assert p.id in pending, format("deposit {0} returned twice or never started", p.id);
      assert p.id in written, format("deposit {0} returned before it committed", p.id);
      assert p.balance == written[p.id],
        format("deposit {0} returned balance {1}, but its write stored {2}", p.id, p.balance, written[p.id]);
      pending -= (p.id);
      unfinished = unfinished - 1;
      if (unfinished == 0) { goto Idle; }
    }
  }
}
