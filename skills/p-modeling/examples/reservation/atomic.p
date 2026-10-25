type Request = (order: string, units: int);
event eTry: Request;

machine Store {
  var available: int;

  start state Serving {
    entry (initial: int) {
      available = initial;
      announce eOpened, (available = initial,);
    }
    on eTry do (req: Request) {
      if (req.units <= available) {
        available = available - req.units;
        announce eReserved, (order = req.order, units = req.units);
      } else {
        // A rejected conditional write commits no reservation.
      }
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
      send store, eTry, (order = order, units = units);
    }
  }
}

machine AtomicCase {
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


machine TwoReservationsCase {
  start state Init {
    entry {
      var store: Store;
      store = new Store(2);
      new Client((store = store, order = "c", units = 1));
      new Client((store = store, order = "d", units = 1));
    }
  }
}
