// Shared bounded queue: one versioned record {items, memos}, load + whole-record CAS.
// kind: 0 = put, 1 = take. key = id*2+kind (put and take ids are separate requests).
// result: put -> 1 accepted / 0 full; take -> item (>=1) or -1 empty.
type Memo = (item: int, result: int);
type Snap = (version: int, items: seq[int], memos: map[int, Memo]);
type Op = (id: int, kind: int, item: int);
event eLoad: machine;
event eLoaded: Snap;
event eTry: (client: machine, expected: int, items: seq[int], memos: map[int, Memo], op: Op, result: int);
event eReplay: (client: machine, op: Op);
event eResult: int;
event eConflict;
event eAgain;
event eFirstDone;
event eClientDone;
event eObserved: (op: Op, result: int, items: seq[int]);
event eReturned: (op: Op, result: int);
event eDone: int;

spec QueueContract observes eObserved, eReturned, eDone {
  var items: seq[int];
  var saved: map[int, Memo];
  var observations: int;
  var returns: int;
  start hot state Working {
    on eObserved do (p: (op: Op, result: int, items: seq[int])) {
      var expected: int;
      var key: int;
      var i: int;
      key = p.op.id * 2 + p.op.kind;
      if (key in saved) {
        assert saved[key].item == p.op.item, "retry changed its arguments";
        expected = saved[key].result;
      } else {
        if (p.op.kind == 0) {
          if (sizeof(items) < 2) { items += (sizeof(items), p.op.item); expected = 1; }
          else { expected = 0; }
        } else {
          if (sizeof(items) == 0) { expected = -1; }
          else { expected = items[0]; items -= (0); }
        }
        saved[key] = (item = p.op.item, result = expected);
      }
      assert p.result == expected, "result violates queue contract";
      assert sizeof(p.items) == sizeof(items), "queue contents differ in size";
      while (i < sizeof(items)) {
        assert p.items[i] == items[i], "queue contents differ";
        i = i + 1;
      }
      observations = observations + 1;
    }
    on eReturned do (p: (op: Op, result: int)) {
      assert (p.op.id * 2 + p.op.kind) in saved, "reply before linearized";
      assert p.result == saved[p.op.id * 2 + p.op.kind].result, "client received wrong result";
      returns = returns + 1;
    }
    on eDone do (n: int) {
      assert returns == n, "scenario omitted replies";
      assert observations == n, "scenario omitted operations";
      goto Complete;
    }
  }
  state Complete { }
}

machine Store {
  var version: int;
  var items: seq[int];
  var memos: map[int, Memo];
  start state Ready {
    on eLoad do (c: machine) {
      send c, eLoaded, (version = version, items = items, memos = memos);
    }
    on eTry do (p: (client: machine, expected: int, items: seq[int], memos: map[int, Memo], op: Op, result: int)) {
      if (p.expected == version) {
        items = p.items; memos = p.memos;
        // snappea-lint: allow mutation -- any strictly fresh version number is equivalent
        version = version + 1;
        announce eObserved, (op = p.op, result = p.result, items = items);
        send p.client, eResult, p.result;
      } else {
        send p.client, eConflict;
      }
    }
    on eReplay do (p: (client: machine, op: Op)) {
      var key: int;
      key = p.op.id * 2 + p.op.kind;
      assert key in memos;
      assert memos[key].item == p.op.item;
      announce eObserved, (op = p.op, result = memos[key].result, items = items);
      send p.client, eResult, memos[key].result;
    }
  }
}

machine Client {
  var store: Store;
  var coord: machine;
  var cap: int;
  var ops: seq[Op];
  var snap: Snap;

  fun Invoke(op: Op) {
    var key: int;
    var result: int;
    var done: bool;
    var conflicts: int;
    var newItems: seq[int];
    var newMemos: map[int, Memo];
    key = op.id * 2 + op.kind;
    while (!done) {
      send store, eLoad, this;
      receive { case eLoaded: (s: Snap) { snap = s; } }
      if (key in snap.memos) {
        send store, eReplay, (client = this, op = op);
        receive { case eResult: (r: int) { result = r; } }
        done = true;
      } else {
        newItems = snap.items;
        newMemos = snap.memos;
        if (op.kind == 0) {
          if (sizeof(newItems) < cap) { newItems += (sizeof(newItems), op.item); result = 1; }
          else { result = 0; }
        } else {
          if (sizeof(newItems) == 0) { result = -1; }
          else { result = newItems[0]; newItems -= (0); }
        }
        newMemos[key] = (item = op.item, result = result);
        send store, eTry, (client = this, expected = snap.version, items = newItems, memos = newMemos, op = op, result = result);
        receive {
          case eResult: (r: int) { done = true; }
          case eConflict: {
            // Each conflict needs another client's fresh commit; the other client has <= 2.
            // snappea-lint: allow mutation -- the bound is a progress check on the model, not the contract
            conflicts = conflicts + 1;
            assert conflicts <= 2, "retry loop not making progress";
          }
        }
      }
    }
    announce eReturned, (op = op, result = result);
  }

  start state Run {
    entry (c: (store: Store, coord: machine, cap: int, ops: seq[Op])) {
      var i: int;
      store = c.store; coord = c.coord; cap = c.cap; ops = c.ops;
      while (i < sizeof(ops)) { Invoke(ops[i]); i = i + 1; }
      send coord, eFirstDone;
      receive { case eAgain: { } }
      Invoke(ops[0]);
      send coord, eClientDone;
    }
  }
}

// Two clients, two ops each, then each retries its first op after both finished.
machine Scenario {
  var firstDone: int;
  var clientsDone: int;
  var left: Client;
  var right: Client;
  var total: int;
  start state Running {
    entry (ops: (l: seq[Op], r: seq[Op])) {
      var store: Store;
      store = new Store();
      total = sizeof(ops.l) + sizeof(ops.r) + 2;
      left = new Client((store = store, coord = this, cap = 2, ops = ops.l));
      right = new Client((store = store, coord = this, cap = 2, ops = ops.r));
    }
    // snappea-lint: allow mutation -- starting retries earlier is also a valid schedule
    on eFirstDone do {
      firstDone = firstDone + 1;
      if (firstDone == 2) { send left, eAgain; send right, eAgain; }
    }
    on eClientDone do {
      clientsDone = clientsDone + 1;
      if (clientsDone == 2) { announce eDone, total; }
    }
  }
}

// Generated: kind and id per op sampled; same (kind,id) may race across clients.
machine Generated {
  start state Init {
    entry {
      var l: seq[Op]; var r: seq[Op];
      var id: int;
      id = choose(2);
      l += (0, (id = id, kind = choose(2), item = id + 1));
      id = choose(2);
      r += (0, (id = id, kind = choose(2), item = id + 1));
      new Scenario((l = l, r = r));
    }
  }
}

// Full queue: third put rejected; retry stays rejected even after a take freed space.
machine FullRetry {
  start state Init {
    entry {
      var l: seq[Op]; var r: seq[Op];
      l += (0, (id = 0, kind = 0, item = 1));
      l += (1, (id = 1, kind = 0, item = 2));
      r += (0, (id = 2, kind = 0, item = 3));
      r += (1, (id = 0, kind = 1, item = 0));
      new Scenario((l = l, r = r));
    }
  }
}

// Empty take retried after a put: stays empty; same id for put and take is distinct.
machine EmptyRetry {
  start state Init {
    entry {
      var l: seq[Op]; var r: seq[Op];
      l += (0, (id = 0, kind = 1, item = 0));
      l += (1, (id = 1, kind = 1, item = 0));
      r += (0, (id = 0, kind = 0, item = 1));
      r += (1, (id = 1, kind = 0, item = 2));
      new Scenario((l = l, r = r));
    }
  }
}

test tcGenerated [main=Generated]: assert QueueContract in { Generated, Scenario, Client, Store };
test tcFullRetry [main=FullRetry]: assert QueueContract in { FullRetry, Scenario, Client, Store };
test tcEmptyRetry [main=EmptyRetry]: assert QueueContract in { EmptyRetry, Scenario, Client, Store };
