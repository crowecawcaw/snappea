type Command = (take: bool, id: int, item: int);
type Snapshot = (version: int, items: seq[int], puts: map[int, int], takes: map[int, int]);

event eLoad: machine;
event eLoaded: Snapshot;
event eTry: (client: machine, command: Command, expected: int, items: seq[int], puts: map[int, int], takes: map[int, int], result: int);
event eReplay: (client: machine, command: Command);
event eResult: int;
event eConflict;
event eFirstDone;
event eAgain;
event eClientDone;
// Monitor events. result: put 1 = stored, 0 = full; take -1 = empty, else the item.
event eInit: (items: seq[int], capacity: int);
event eObserved: (command: Command, result: int, items: seq[int], version: int);
event eReturned: (take: bool, id: int, result: int);
event eDone;
event eConflicted;
event eRequested: Command;

// Independent oracle: derives expected results from the contract and requests seen.
spec QueueContract observes eConflicted, eInit, eRequested, eObserved, eReturned, eDone {
  var q: seq[int];
  var cap: int;
  var putSaved: map[int, int];
  var takeSaved: map[int, int];
  var requested: set[Command];
  var lastVersion: int;
  var conflicts: int;
  var observations: int;
  var replies: int;
  start hot state Waiting {
    on eRequested do (c: Command) { requested += (c); }
    on eConflicted do { conflicts = conflicts + 1; assert conflicts <= 2, "CAS lost more often than competing commits allow"; }
    on eInit do (p: (items: seq[int], capacity: int)) { q = p.items; cap = p.capacity; }
    on eObserved do (p: (command: Command, result: int, items: seq[int], version: int)) {
      var fresh: bool;
      var expected: int;
      var i: int;
      assert p.command in requested, "store executed a command nobody requested";
      fresh = false;
      if (p.command.take) {
        if (p.command.id in takeSaved) { expected = takeSaved[p.command.id]; }
        else {
          if (sizeof(q) == 0) { expected = -1; }
          else { expected = q[0]; q -= (0); }
          takeSaved[p.command.id] = expected; fresh = true;
        }
      } else {
        if (p.command.id in putSaved) { expected = putSaved[p.command.id]; }
        else {
          if (sizeof(q) < cap) { expected = 1; q += (sizeof(q), p.command.item); }
          else { expected = 0; }
          putSaved[p.command.id] = expected; fresh = true;
        }
      }
      if (fresh) { assert p.version == lastVersion + 1, "version must advance by one per commit"; lastVersion = p.version; }
      else { assert p.version == lastVersion, "replay must not change the version"; }
      assert p.result == expected, "result violates queue contract";
      assert sizeof(p.items) == sizeof(q), "queue length differs from contract";
      i = 0;
      while (i < sizeof(q)) {
        assert p.items[i] == q[i], "queue contents differ from contract";
        i = i + 1;
      }
      observations = observations + 1;
    }
    on eReturned do (p: (take: bool, id: int, result: int)) {
      if (p.take) { assert p.id in takeSaved, "reply before take linearized";
        assert takeSaved[p.id] == p.result, "client got wrong take result"; }
      else { assert p.id in putSaved, "reply before put linearized";
        assert putSaved[p.id] == p.result, "client got wrong put result"; }
      replies = replies + 1;
    }
    on eDone do {
      assert observations == 4, "scenario omitted an invocation or retry";
      assert replies == observations, "every call must return";
      goto Complete;
    }
  }
  state Complete { }
}

// Store: the only atomic primitives are load and whole-record CAS.
machine Store {
  var version: int;
  var items: seq[int];
  var puts: map[int, int];
  var takes: map[int, int];
  start state Ready {
    entry (init: (items: seq[int], capacity: int)) {
      items = init.items;
      announce eInit, init;
    }
    on eLoad do (client: machine) {
      send client, eLoaded, (version = version, items = items, puts = puts, takes = takes);
    }
    on eTry do (p: (client: machine, command: Command, expected: int, items: seq[int], puts: map[int, int], takes: map[int, int], result: int)) {
      if (p.expected == version) {
        items = p.items; puts = p.puts; takes = p.takes;
        version = version + 1;
        announce eObserved, (command = p.command, result = p.result, items = items, version = version);
        send p.client, eResult, p.result;
      } else {
        // snappea-lint: allow mutation -- conflict count is only a livelock guard for broken CAS
        announce eConflicted;
        send p.client, eConflict;
      }
    }
    on eReplay do (p: (client: machine, command: Command)) {
      var r: int;
      if (p.command.take) { assert p.command.id in takes; r = takes[p.command.id]; }
      else { assert p.command.id in puts; r = puts[p.command.id]; }
      announce eObserved, (command = p.command, result = r, items = items, version = version);
      send p.client, eResult, r;
    }
  }
}

machine Client {
  var store: Store;
  var coordinator: machine;
  var command: Command;
  var capacity: int;
  start state Run {
    entry (input: (store: Store, coordinator: machine, command: Command, capacity: int)) {
      store = input.store; coordinator = input.coordinator;
      command = input.command; capacity = input.capacity;
      invoke();
      send coordinator, eFirstDone;
      goto Waiting;
    }
  }
  state Waiting {
    on eAgain do { invoke(); send coordinator, eClientDone; }
  }

  fun invoke() {
    var s: Snapshot;
    var items: seq[int];
    var puts: map[int, int];
    var takes: map[int, int];
    var result: int;
    var completed: bool;
    var memoHit: bool;
    while (!completed) {
      send store, eLoad, this;
      receive { case eLoaded: (x: Snapshot) { s = x; } }
      if (command.take) { memoHit = command.id in s.takes; } else { memoHit = command.id in s.puts; }
      if (memoHit) {
        send store, eReplay, (client = this, command = command);
        receive { case eResult: (r: int) {
          announce eReturned, (take = command.take, id = command.id, result = r);
          completed = true;
        } }
      } else {
        items = s.items; puts = s.puts; takes = s.takes;
        if (command.take) {
          if (sizeof(items) == 0) { result = -1; }
          else { result = items[0]; items -= (0); }
          takes[command.id] = result;
        } else {
          if (sizeof(items) < capacity) { result = 1; items += (sizeof(items), command.item); }
          else { result = 0; }
          puts[command.id] = result;
        }
        send store, eTry, (client = this, command = command, expected = s.version,
          items = items, puts = puts, takes = takes, result = result);
        receive {
          case eResult: (r: int) {
            announce eReturned, (take = command.take, id = command.id, result = r);
            completed = true;
          }
          case eConflict: { }
        }
      }
    }
  }
}

// snappea: driver
machine Scenario {
  var left: Client;
  var right: Client;
  var firstDone: int;
  var clientsDone: int;
  start state Running {
    entry (input: (left: Command, right: Command, prefill: seq[int], capacity: int)) {
      var store: Store;
      store = new Store((items = input.prefill, capacity = input.capacity));
      announce eRequested, input.left;
      announce eRequested, input.right;
      left = new Client((store = store, coordinator = this, command = input.left, capacity = input.capacity));
      right = new Client((store = store, coordinator = this, command = input.right, capacity = input.capacity));
    }
    on eFirstDone do {
      firstDone = firstDone + 1;
      if (firstDone == 2) { send left, eAgain; send right, eAgain; }
    }
    on eClientDone do {
      clientsDone = clientsDone + 1;
      if (clientsDone == 2) { announce eDone; }
    }
  }
}

machine Generated {
  start state Init {
    entry {
      var cap: int;
      var fill: int;
      var prefill: seq[int];
      var lt: bool;
      var rt: bool;
      var rid: int;
      var litem: int;
      var ritem: int;
      cap = 1 + choose(2);
      fill = choose(cap + 1);
      while (sizeof(prefill) < fill) { prefill += (sizeof(prefill), 10 + sizeof(prefill)); }
      lt = $; rt = $;
      rid = choose(2);
      litem = 1;
      ritem = 2;
      // Same id and kind is a duplicate request: it must carry identical arguments.
      if (rid == 0 && lt == rt) { ritem = litem; }
      new Scenario((left = (take = lt, id = 0, item = litem),
        right = (take = rt, id = rid, item = ritem), prefill = prefill, capacity = cap));
    }
  }
}

// Fixed scenarios.
machine TwoPutsOneSlot {   // both read the same version; exactly one fits
  start state Init { entry {
    new Scenario((left = (take = false, id = 0, item = 1), right = (take = false, id = 1, item = 2),
      prefill = default(seq[int]), capacity = 1)); } }
}
machine TwoTakesOneItem {  // exactly one gets the item, other gets empty; retries stay stable
  start state Init { entry {
    var p: seq[int]; p += (0, 7);
    new Scenario((left = (take = true, id = 0, item = 0), right = (take = true, id = 1, item = 0),
      prefill = p, capacity = 2)); } }
}
machine PutTakeSameId {    // same id, different kinds are different requests
  start state Init { entry {
    new Scenario((left = (take = false, id = 0, item = 5), right = (take = true, id = 0, item = 0),
      prefill = default(seq[int]), capacity = 2)); } }
}
machine FullRejectedThenTake {  // rejected put retried after a take freed space stays rejected
  start state Init { entry {
    var p: seq[int]; p += (0, 7);
    new Scenario((left = (take = false, id = 0, item = 9), right = (take = true, id = 1, item = 0),
      prefill = p, capacity = 1)); } }
}
machine EmptyTakeThenPut {  // empty take retried after a put stays empty
  start state Init { entry {
    new Scenario((left = (take = true, id = 0, item = 0), right = (take = false, id = 1, item = 4),
      prefill = default(seq[int]), capacity = 1)); } }
}
machine DuplicateSamePut {  // two clients race the same request id
  start state Init { entry {
    new Scenario((left = (take = false, id = 0, item = 3), right = (take = false, id = 0, item = 3),
      prefill = default(seq[int]), capacity = 2)); } }
}

test tcGenerated [main=Generated]: assert QueueContract in { Generated, Scenario, Client, Store };
test tcTwoPuts [main=TwoPutsOneSlot]: assert QueueContract in { TwoPutsOneSlot, Scenario, Client, Store };
test tcTwoTakes [main=TwoTakesOneItem]: assert QueueContract in { TwoTakesOneItem, Scenario, Client, Store };
test tcSameId [main=PutTakeSameId]: assert QueueContract in { PutTakeSameId, Scenario, Client, Store };
test tcFullRejected [main=FullRejectedThenTake]: assert QueueContract in { FullRejectedThenTake, Scenario, Client, Store };
test tcEmptyTake [main=EmptyTakeThenPut]: assert QueueContract in { EmptyTakeThenPut, Scenario, Client, Store };
test tcDuplicate [main=DuplicateSamePut]: assert QueueContract in { DuplicateSamePut, Scenario, Client, Store };
