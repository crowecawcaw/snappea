/*
 * End-to-end CAS queue. Read README.md, then Contract.p (oracle), this file
 * (load/CAS clients), queue_store.py (implementation), test_queue.py (regressions).
 * Run: snappea check Queue.pproj --timeout 30s --deny mutation --json
 * Require zero incomplete searches, coverage gaps and undetected mutants.
 * Then: python -m pip install pysnappea pytest; python -m pytest -q
 * Bounds and ordering assumptions are in README.md.
 */
type Snapshot = (version: int, items: seq[int], memos: map[int, Memo]);
event eLoad: machine;
event eLoaded: Snapshot;
event eTry: (client: machine, command: Command, version: int, items: seq[int], memos: map[int, Memo], result: Result);
event eReplay: (client: machine, command: Command);
event eResult: Result;
event eConflict;
event eAgain;
event eFirstDone;
event eClientDone;

machine QueueStore {
  var version: int;
  var items: seq[int];
  var memos: map[int, Memo];
  start state Ready {
    entry (initial: seq[int]) { items = initial; }
    on eLoad do (client: machine) {
      send client, eLoaded, (version = version, items = items, memos = memos);
    }
    on eTry do (p: (client: machine, command: Command, version: int, items: seq[int], memos: map[int, Memo], result: Result)) {
      if (p.version == version) {
        items = p.items;
        memos = p.memos;
        version = version + 1;
        announce eObserved, (command = p.command, result = p.result, items = items);
        send p.client, eResult, p.result;
      } else { send p.client, eConflict; }
    }
    on eReplay do (p: (client: machine, command: Command)) {
      var key: int;
      key = p.command.id * 2;
      if (p.command.put) { key = key + 1; }
      assert key in memos;
      announce eObserved, (command = p.command, result = memos[key].result, items = items);
      send p.client, eResult, memos[key].result;
    }
  }
}

machine Client {
  var store: QueueStore;
  var coordinator: machine;
  var command: Command;
  var clientId: int;
  var capacity: int;
  start state Run {
    entry (p: (store: QueueStore, coordinator: machine, command: Command, id: int, capacity: int)) {
      store = p.store;
      coordinator = p.coordinator;
      command = p.command;
      clientId = p.id;
      capacity = p.capacity;
      invoke();
      send coordinator, eFirstDone;
      goto Waiting;
    }
  }
  state Waiting {
    on eAgain do {
      invoke();
      send coordinator, eClientDone;
    }
  }
  fun invoke() {
    var snapshot: Snapshot;
    var result: Result;
    var key: int;
    var done: bool;
    key = command.id * 2;
    if (command.put) { key = key + 1; }
    while (!done) {
      send store, eLoad, this;
      receive { case eLoaded: (p: Snapshot) { snapshot = p; } }
      if (key in snapshot.memos) {
        send store, eReplay, (client = this, command = command);
        receive { case eResult: (r: Result) { result = r; done = true; } }
      } else {
        result = (ok = false, present = false, value = 0);
        if (command.put) {
          if (sizeof(snapshot.items) < capacity) {
            result.ok = true;
            snapshot.items += (sizeof(snapshot.items), command.item);
          }
        } else {
          if (sizeof(snapshot.items) > 0) {
            result.present = true;
            result.value = snapshot.items[0];
            snapshot.items -= 0;
          }
        }
        snapshot.memos[key] = (item = command.item, result = result);
        send store, eTry, (client = this, command = command, version = snapshot.version,
          items = snapshot.items, memos = snapshot.memos, result = result);
        receive {
          case eResult: (r: Result) {
            // snappea-lint: allow mutation:remove-statement -- Store echoes this exact candidate result; retain the real reply for protocol fidelity.
            result = r;
            done = true;
          }
          case eConflict: { }
        }
      }
    }
    announce eReturned, (client = clientId, command = command, result = result);
  }
}

machine Scenario {
  start state Run {
    entry {
      var store: QueueStore;
      var left: Client;
      var right: Client;
      var a: Command;
      var b: Command;
      var capacity: int;
      var initial: seq[int];
      var count: int;
      var i: int;
      capacity = 1 + choose(2);
      count = choose(capacity + 1);
      i = 0;
      while (i < count) { initial += (i, 10 + i); i = i + 1; }
      a = (id = 0, put = $, item = choose(2) - 1);
      b = (id = choose(2), put = $, item = choose(2) - 1);
      if (b.id == a.id && b.put == a.put) { b.item = a.item; }
      announce ePlan, (capacity = capacity, initial = initial, left = a, right = b);
      store = new QueueStore(initial);
      left = new Client((store = store, coordinator = this, command = a, id = 0, capacity = capacity));
      right = new Client((store = store, coordinator = this, command = b, id = 1, capacity = capacity));
      receive { case eFirstDone: { } }
      receive { case eFirstDone: { } }
      send left, eAgain;
      receive { case eClientDone: { } }
      send right, eAgain;
      receive { case eClientDone: { } }
      announce eDone;
    }
  }
}

test tcQueue [main=Scenario]: assert QueueContract in { Scenario, Client, QueueStore };

// A fixed last-slot race makes stale candidate results easy to distinguish.
machine CompetingPuts {
  start state Run {
    entry {
      var store: QueueStore;
      var left: Client;
      var right: Client;
      var a: Command;
      var b: Command;
      var capacity: int;
      var initial: seq[int];
      capacity = 1;
      a = (id = 0, put = true, item = 0);
      b = (id = 1, put = true, item = 1);
      announce ePlan, (capacity = capacity, initial = initial, left = a, right = b);
      store = new QueueStore(initial);
      left = new Client((store = store, coordinator = this, command = a, id = 0, capacity = capacity));
      right = new Client((store = store, coordinator = this, command = b, id = 1, capacity = capacity));
      receive { case eFirstDone: { } }
      receive { case eFirstDone: { } }
      send left, eAgain;
      receive { case eClientDone: { } }
      send right, eAgain;
      receive { case eClientDone: { } }
      announce eDone;
    }
  }
}


test tcCompetingPuts [main=CompetingPuts]: assert QueueContract in { CompetingPuts, Client, QueueStore };
