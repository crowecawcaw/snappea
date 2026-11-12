// Drivers: two clients share one store. The first gets two increments back to back,
// so the second arrives while the client is still busy with the first.

machine TwoReceiveClients {
  start state Init {
    entry {
      var store: Store;
      var a: ReceiveClient;
      var b: ReceiveClient;
      store = new Store();
      announce eIncrementsPlanned, 3;
      a = new ReceiveClient(store);
      b = new ReceiveClient(store);
      send a, eIncrement;
      send a, eIncrement;
      send b, eIncrement;
    }
  }
}

machine TwoStateClients {
  start state Init {
    entry {
      var store: Store;
      var a: StateClient;
      var b: StateClient;
      store = new Store();
      announce eIncrementsPlanned, 3;
      a = new StateClient(store);
      b = new StateClient(store);
      send a, eIncrement;
      send a, eIncrement;
      send b, eIncrement;
    }
  }
}

test tcReceiveClients [main=TwoReceiveClients]:
  assert IncrementsApplied in { TwoReceiveClients, Store, ReceiveClient };

test tcStateClients [main=TwoStateClients]:
  assert IncrementsApplied in { TwoStateClients, Store, StateClient };
