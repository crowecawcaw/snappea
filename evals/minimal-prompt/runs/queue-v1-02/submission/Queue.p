/* Shared bounded queue: whole-record CAS (queue + result memo), idempotent retries.
 * Bounds: capacity 1 or 2, two concurrent clients, one delayed retry each, no crashes.
 * Memo key: (isPut, id) -- a put and a take with equal id are different requests. */
type Op = (id: int, isPut: bool, item: int);
type Memo = (item: int, ok: bool, val: int);   // val: taken item, -1 for none
type Rec = (items: seq[int], memos: map[int, Memo]);
type Snap = (version: int, rec: Rec);
event eLoad: machine;
event eLoaded: Snap;
event eTry: (client: machine, op: Op, expected: int, rec: Rec, ok: bool, val: int);
event eReplay: (client: machine, op: Op);
event eResult: (ok: bool, val: int);
event eConflict;
event eFirstDone;
event eAgain;
event eClientDone;
event eObserved: (op: Op, ok: bool, val: int, items: seq[int]);
event eReturned: (op: Op, ok: bool, val: int);
event eDone;

fun key(op: Op): int { if (op.isPut) { return op.id * 2 + 1; } return op.id * 2; }

spec QueueContract observes eInit, eObserved, eReturned, eDone {
  var cap: int;
  var q: seq[int];
  var saved: map[int, Memo];
  var observations: int;
  var replies: map[int, int];
  start hot state Working {
    entry { }
    on eInit do (c: (cap: int, items: seq[int])) { cap = c.cap; q = c.items; }
    on eObserved do (p: (op: Op, ok: bool, val: int, items: seq[int])) {
      var ok: bool; var val: int; var k: int; var i: int;
      k = key(p.op);
      if (k in saved) {
        assert saved[k].item == p.op.item, "retry changed arguments";
        ok = saved[k].ok; val = saved[k].val;
      } else {
        val = -1;
        if (p.op.isPut) {
          ok = sizeof(q) < cap;
          if (ok) { q += (sizeof(q), p.op.item); }
        } else {
          ok = sizeof(q) > 0;
          if (ok) { val = q[0]; q -= (0); }
        }
        saved[k] = (item = p.op.item, ok = ok, val = val);
      }
      assert p.ok == ok, "wrong success/full/empty result";
      assert p.val == val, "wrong taken value";
      assert sizeof(p.items) == sizeof(q), "queue length wrong after operation";
      while (i < sizeof(q)) { assert p.items[i] == q[i], "queue contents wrong"; i = i + 1; }
      observations = observations + 1;
    }
    on eReturned do (p: (op: Op, ok: bool, val: int)) {
      var k: int;
      k = key(p.op);
      assert k in saved, "reply before linearization";
      assert p.ok == saved[k].ok && p.val == saved[k].val, "client got wrong result";
      if (!(k in replies)) { replies[k] = 0; }
      replies[k] = replies[k] + 1;
    }
    on eDone do {
      var k: int;
      assert observations == 4, "scenario omitted an invocation";
      foreach (k in keys(saved)) { assert replies[k] == 2, "request must return twice"; }
      goto Complete;
    }
  }
  state Complete { }
}

machine Store {
  var version: int;
  var rec: Rec;
  start state Ready {
    entry (init: seq[int]) { rec.items = init; }
    on eLoad do (c: machine) { send c, eLoaded, (version = version, rec = rec); }
    on eTry do (p: (client: machine, op: Op, expected: int, rec: Rec, ok: bool, val: int)) {
      if (p.expected == version) {
        rec = p.rec; version = version + 1;
        announce eObserved, (op = p.op, ok = p.ok, val = p.val, items = rec.items);
        send p.client, eResult, (ok = p.ok, val = p.val);
      } else { send p.client, eConflict; }
    }
    on eReplay do (p: (client: machine, op: Op)) {
      var m: Memo;
      m = rec.memos[key(p.op)];
      announce eObserved, (op = p.op, ok = m.ok, val = m.val, items = rec.items);
      send p.client, eResult, (ok = m.ok, val = m.val);
    }
  }
}

machine Client {
  var store: Store; var coord: machine; var op: Op; var cap: int;
  start state Run {
    entry (i: (store: Store, coord: machine, op: Op, cap: int)) {
      store = i.store; coord = i.coord; op = i.op; cap = i.cap;
      invoke(); send coord, eFirstDone; goto Waiting;
    }
  }
  state Waiting { on eAgain do { invoke(); send coord, eClientDone; goto Finished; } }
  state Finished { }
  fun invoke() {
    var s: Snap; var rec: Rec; var ok: bool; var val: int; var done: bool; var n: int;
    while (!done) {
      n = n + 1;
      assert n <= 2, "no progress within bounded contention";
      send store, eLoad, this;
      receive { case eLoaded: (x: Snap) { s = x; } }
      if (key(op) in s.rec.memos) {
        send store, eReplay, (client = this, op = op);
        receive { case eResult: (r: (ok: bool, val: int)) {
          announce eReturned, (op = op, ok = r.ok, val = r.val); done = true; } }
      } else {
        rec = s.rec; val = -1;
        if (op.isPut) {
          ok = sizeof(rec.items) < cap;
          if (ok) { rec.items += (sizeof(rec.items), op.item); }
        } else {
          ok = sizeof(rec.items) > 0;
          if (ok) { val = rec.items[0]; rec.items -= (0); }
        }
        rec.memos[key(op)] = (item = op.item, ok = ok, val = val);
        send store, eTry, (client = this, op = op, expected = s.version, rec = rec, ok = ok, val = val);
        receive {
          case eResult: (r: (ok: bool, val: int)) {
            announce eReturned, (op = op, ok = r.ok, val = r.val); done = true; }
          case eConflict: { }
        }
      }
    }
  }
}

// snappea: driver
machine Scenario {
  var left: Client; var right: Client; var firstDone: int; var done: int;
  start state Running {
    entry (i: (cap: int, init: seq[int], l: Op, r: Op)) {
      var store: Store;
      announce eInit, (cap = i.cap, items = i.init);
      store = new Store(i.init);
      left = new Client((store = store, coord = this, op = i.l, cap = i.cap));
      right = new Client((store = store, coord = this, op = i.r, cap = i.cap));
    }
    on eFirstDone do { firstDone = firstDone + 1; if (firstDone == 2) { send left, eAgain; send right, eAgain; } }
    on eClientDone do { done = done + 1; if (done == 2) { announce eDone; } }
  }
}
event eInit: (cap: int, items: seq[int]);

machine Generated {
  start state Init {
    entry {
      var init: seq[int]; var c: int; var n: int;
      c = 1 + choose(2);
      // initial contents: empty, one, or full (cap 2 => two items)
      n = choose(3);   // 0, 1 or 2 initial items (2 only when capacity allows)
      if (n > 0) { init += (0, 7); }
      if (n == 2 && c == 2) { init += (1, 8); }
      new Scenario((cap = c, init = init,
        l = (id = 0, isPut = $, item = 1 + choose(2)),
        r = (id = 1, isPut = $, item = 3 + choose(2))));
    }
  }
}
machine SameIdPutTake {   // same id on put and take must not alias
  start state Init { entry {
    var init: seq[int];
    init += (0, 7);
    new Scenario((cap = 2, init = init, l = (id = 5, isPut = true, item = 1), r = (id = 5, isPut = false, item = 1))); } }
}
machine FullThenRetry {   // rejected put stays rejected after a take frees room
  start state Init { entry {
    var init: seq[int];
    init += (0, 7);
    new Scenario((cap = 1, init = init, l = (id = 1, isPut = true, item = 2), r = (id = 2, isPut = false, item = 0))); } }
}
test tcGenerated [main=Generated]: assert QueueContract in { Generated, Scenario, Client, Store };
test tcSameId [main=SameIdPutTake]: assert QueueContract in { SameIdPutTake, Scenario, Client, Store };
test tcFullRetry [main=FullThenRetry]: assert QueueContract in { FullThenRetry, Scenario, Client, Store };
