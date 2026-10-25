type ReadRequest = (client: Client);
type WriteRequest = (client: Client, order: string, units: int, snapshot: int);
event eRead: ReadRequest;
event eReadReply: (available: int);
event eWrite: WriteRequest;

machine Store {
  var available: int;

  start state Serving {
    entry (initial: int) {
      available = initial;
      announce eOpened, (available = initial,);
    }
    on eRead do (req: ReadRequest) {
      send req.client, eReadReply, (available = available,);
    }
    on eWrite do (req: WriteRequest) {
      // Deliberately omits version/stock validation at the write boundary.
      available = req.snapshot - req.units;
      announce eReserved, (order = req.order, units = req.units);
    }
  }
}

machine Client {
  var store: Store;
  var order: string;
  var units: int;

  start state Start {
    entry (input: (store: Store, order: string, units: int)) {
      store = input.store;
      order = input.order;
      units = input.units;
      send store, eRead, (client = this,);
      goto WaitingForRead;
    }
  }

  state WaitingForRead {
    on eReadReply do (reply: (available: int)) {
      if (units <= reply.available) {
        send store, eWrite, (client = this, order = order, units = units, snapshot = reply.available);
      }
      goto Done;
    }
  }

  state Done {}
}

machine StaleWriteCase {
  start state Init {
    entry {
      var store: Store;
      store = new Store(1);
      new Client((store = store, order = "a", units = 1));
      new Client((store = store, order = "b", units = 1));
      new Client((store = store, order = "too-large", units = 2));
    }
  }
}
