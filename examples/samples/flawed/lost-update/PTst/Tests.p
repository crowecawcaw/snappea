// Drivers. They announce the plan and each deposit before starting it, so the specs
// know what should happen even if the client code is wrong.

// Two deposits of different amounts race on one balance (initially 0).
machine TwoDepositsBlind {
  start state Init {
    entry {
      var store: BlindStore;
      store = new BlindStore();
      announce DepositsPlanned, 2;
      announce DepositRequested, (id = 1, amount = 5);
      new ReadModifyWriteClient((store = store, id = 1, amount = 5));
      announce DepositRequested, (id = 2, amount = 3);
      new ReadModifyWriteClient((store = store, id = 2, amount = 3));
    }
  }
}

machine TwoDepositsCas {
  start state Init {
    entry {
      var store: VersionedStore;
      store = new VersionedStore();
      announce DepositsPlanned, 2;
      announce DepositRequested, (id = 1, amount = 5);
      new CasClient((store = store, id = 1, amount = 5));
      announce DepositRequested, (id = 2, amount = 3);
      new CasClient((store = store, id = 2, amount = 3));
    }
  }
}

// Three deposits: a client can lose the compare-and-swap twice in a row, so the
// retry must work more than once.
machine ThreeDepositsCas {
  start state Init {
    entry {
      var store: VersionedStore;
      var id: int;
      store = new VersionedStore();
      announce DepositsPlanned, 3;
      id = 1;
      while (id <= 3) {
        announce DepositRequested, (id = id, amount = id);
        new CasClient((store = store, id = id, amount = id));
        id = id + 1;
      }
    }
  }
}

// snappea: expect-bug "lost update: deposit"
test tcBugLostUpdate [main=TwoDepositsBlind]:
  assert NoLostUpdate, DepositsReturn in
  { TwoDepositsBlind, BlindStore, ReadModifyWriteClient };

test tcFixedTwoDeposits [main=TwoDepositsCas]:
  assert NoLostUpdate, DepositsReturn in
  { TwoDepositsCas, VersionedStore, CasClient };

test tcFixedThreeDeposits [main=ThreeDepositsCas]:
  assert NoLostUpdate, DepositsReturn in
  { ThreeDepositsCas, VersionedStore, CasClient };
