/*
 * File map (search these names):
 * Capacity = oracle; Store/Client = correct conditional write;
 * StaleStore/StaleClient = defective split read/write; test tc* = selections;
 * storage.py/test_storage.py = real implementation and saved capture tests.
 *
 * Reservation: distinguish an atomic conditional write from stale read/check/write.
 *
 * Contract: committed reservations never exceed initial stock; Python tests also
 * compare actual remaining stock and reservation records. Capacity subtracts
 * positive committed effects independently, rejects duplicate order IDs, and
 * stays hot after opening until the first reservation, requiring useful work.
 * It does not trust an implementation-valid flag.
 *
 * Correct Store check/update is one atomic storage primitive. Broken StaleStore
 * separates read from unconditional write: both clients may read the last unit,
 * then each reserve it. Locking only final writes still oversells/loses a decrement.
 * The real implementation needs a transaction, conditional update, or one lock
 * covering validation + stock + reservation. This is NOT a client-side CAS loop;
 * see Balance.p in `snappea guide example cas-retry` for candidates, failed CAS, reload and recomputation.
 *
 * Run from this directory:
 *   snappea check Reservation.p tcAtomicConditionalReserve tcTwoSequentialSuccesses --timeout 5s
 *   snappea check Reservation.p tcReadCheckWrite --timeout 5s
 * Positive suite must complete with required coverage; negative must fail
 * "reservations exceed available capacity". Unfiltered check includes the defect.
 *   python -m pip install pysnappea
 *   python -m unittest -v
 * Saved Python tests call real storage.py methods under check_spec, require actual
 * source locations, and independently verify actual rows/state. A barrier forces
 * both reads before either write. Isolated stale-version/capacity/duplicate-order
 * rejections check unchanged state immediately, followed by useful success.
 * Broken implementation must fail both P capture and independent inventory checks.
 *
 * Capture is at committed effects after lock release: positive reservation sums
 * are order-independent, but this order is NOT a general linearization witness.
 * strict=True requires relevant events; check_hot=True checks modeled progress;
 * neither proves complete instrumentation. Runtime emission must not break code.
 *
 * Limits: fixed competing clients, stock 1/2, process-local Python Lock (not other
 * processes); no retries, crashes, persistence, timeouts or general DB semantics.
 * P abstracts the conditional capacity primitive; Python additionally rejects stale
 * versions, covered by ordinary tests. Models only help where code matches them.
 */

// --- Shared independent capacity monitor ---

event eOpened: (available: int);
event eReserved: (order: string, units: int);

// The monitor is an independent contract: each committed reservation consumes
// capacity. It does not trust an application-supplied `valid` result.
// This is a committed-effect safety oracle, not a complete acceptance contract:
// Python tests separately require useful success and check rejected calls leave
// actual stock/records unchanged. Post-lock events may reorder across threads;
// subtracting positive reservations is order-independent for this invariant.
spec Capacity observes eOpened, eReserved {
  var available: int;
  var orders: set[string];

  start state BeforeOpen {
    on eOpened do (p: (available: int)) {
      assert p.available >= 0, "opened with negative stock";
      available = p.available;
      goto Open;
    }
    on eReserved do { assert false, "reservation before store opened"; }
  }

  hot state Open {
    on eOpened do { assert false, "store opened twice"; }
    on eReserved do (p: (order: string, units: int)) {
      assert p.units > 0, "reservation must be positive";
      assert !(p.order in orders), "order reserved twice";
      available = available - p.units;
      assert available >= 0, "reservations exceed available capacity";
      orders += (p.order);
      goto HasReservation;
    }
  }

  state HasReservation {
    on eOpened do { assert false, "store opened twice"; }
    on eReserved do (p: (order: string, units: int)) {
      assert p.units > 0, "reservation must be positive";
      assert !(p.order in orders), "order reserved twice";
      available = available - p.units;
      assert available >= 0, "reservations exceed available capacity";
      orders += (p.order);
    }
  }
}

// --- Correct atomic storage primitive and fixed drivers ---

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

// --- Selected positive tests ---

test tcAtomicConditionalReserve [main=AtomicCase]:
  assert Capacity in { Store, Client, AtomicCase };

test tcTwoSequentialSuccesses [main=TwoReservationsCase]:
  assert Capacity in { Store, Client, TwoReservationsCase };

// --- Broken read/check/write primitive and driver ---

type ReadRequest = (client: StaleClient);
type WriteRequest = (client: StaleClient, order: string, units: int, snapshot: int);
event eRead: ReadRequest;
event eReadReply: (available: int);
event eWrite: WriteRequest;

machine StaleStore {
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

machine StaleClient {
  var store: StaleStore;
  var order: string;
  var units: int;

  start state Start {
    entry (input: (store: StaleStore, order: string, units: int)) {
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
      var store: StaleStore;
      store = new StaleStore(1);
      new StaleClient((store = store, order = "a", units = 1));
      new StaleClient((store = store, order = "b", units = 1));
      new StaleClient((store = store, order = "too-large", units = 2));
    }
  }
}

// --- Selected negative test ---

test tcReadCheckWrite [main=StaleWriteCase]:
  assert Capacity in { StaleStore, StaleClient, StaleWriteCase };
