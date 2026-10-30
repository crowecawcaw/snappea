/*
 * Model of the shared bounded queue: load / CAS of one record {items, memo}.
 * Contract: put appends iff size < capacity (else false, no effect); take removes
 * the oldest or returns none; retries of (kind,id) return the original result with
 * no further effect. put:id and take:id are distinct requests.
 * Bounds: capacities 1..3, preloaded 0..cap items, two clients, one retry each,
 * ids 0/1, item = id+1 per id (so same-key requests repeat their arguments).
 * Omitted: crashes, memo expiry, conflicting reuse of an id, store failures.
 */
type Key = int; // id*2 + (0 put, 1 take)
type Memo = (ok: bool, val: int); // put: ok=result; take: ok=nonempty, val=item
type Snap = (version: int, items: seq[int], memos: map[Key, Memo]);
type Cmd = (id: int, isTake: bool, item: int);
event eLoad: machine;
event eLoaded: Snap;
event eCas: (client: machine, cmd: Cmd, expected: int, items: seq[int], memos: map[Key, Memo], r: Memo);
event eReplay: (client: machine, cmd: Cmd);
event eResult: Memo;
event eConflict;
event eAgain;
event eFirstDone;
event eClientDone;
event eInit: (cap: int, items: seq[int]);
event eObserved: (cmd: Cmd, r: Memo, items: seq[int]);
event eReturned: (cmd: Cmd, r: Memo);
event eDone: (obs: int, replies: int);

spec QueueContract observes eInit, eObserved, eReturned, eDone {
  var cap: int;
  var q: seq[int];
  var saved: map[Key, Memo];
  var n: int;
  var replies: int;
  start hot state Working {
    on eInit do (p: (cap: int, items: seq[int])) { cap = p.cap; q = p.items; }
    on eObserved do (p: (cmd: Cmd, r: Memo, items: seq[int])) {
      var k: Key;
      var e: Memo;
      var i: int;
      k = p.cmd.id * 2;
      if (p.cmd.isTake) { k = k + 1; }
      if (k in saved) {
        e = saved[k];
      } else {
        if (p.cmd.isTake) {
          if (sizeof(q) > 0) { e = (ok = true, val = q[0]); q -= (0); }
          else { e = (ok = false, val = 0); }
        } else {
          if (sizeof(q) < cap) { e = (ok = true, val = 0); q += (sizeof(q), p.cmd.item); }
          else { e = (ok = false, val = 0); }
        }
        saved[k] = e;
      }
      assert p.r.ok == e.ok, "wrong result flag";
      assert p.r.val == e.val, "wrong taken item";
      assert sizeof(p.items) == sizeof(q), "queue size differs from contract";
      i = 0;
      while (i < sizeof(q)) { assert p.items[i] == q[i], "queue contents differ"; i = i + 1; }
      n = n + 1;
    }
    on eReturned do (p: (cmd: Cmd, r: Memo)) {
      var k: Key;
      k = p.cmd.id * 2;
      if (p.cmd.isTake) { k = k + 1; }
      assert k in saved, "reply before linearization";
      assert p.r.ok == saved[k].ok && p.r.val == saved[k].val, "client got wrong result";
      replies = replies + 1;
    }
    on eDone do (p: (obs: int, replies: int)) {
      assert n == p.obs, "scenario omitted an invocation";
      assert replies == p.replies, "missing replies";
      goto Complete;
    }
  }
  state Complete { }
}

machine Store {
  var cap: int;
  var version: int;
  var items: seq[int];
  var memos: map[Key, Memo];
  start state Ready {
    entry (input: (cap: int, items: seq[int])) {
      cap = input.cap; items = input.items;
      announce eInit, (cap = input.cap, items = input.items);
    }
    on eLoad do (c: machine) {
      send c, eLoaded, (version = version, items = items, memos = memos);
    }
    on eCas do (p: (client: machine, cmd: Cmd, expected: int, items: seq[int], memos: map[Key, Memo], r: Memo)) {
      if (p.expected == version) {
        items = p.items; memos = p.memos; version = version + 1;
        announce eObserved, (cmd = p.cmd, r = p.r, items = items);
        send p.client, eResult, p.r;
      } else { send p.client, eConflict; }
    }
    on eReplay do (p: (client: machine, cmd: Cmd)) {
      var k: Key;
      k = p.cmd.id * 2;
      if (p.cmd.isTake) { k = k + 1; }
      assert k in memos;
      announce eObserved, (cmd = p.cmd, r = memos[k], items = items);
      send p.client, eResult, memos[k];
    }
  }
}

machine Client {
  var store: Store;
  var coord: machine;
  var cap: int;
  var cmd: Cmd;
  start state Run {
    entry (input: (store: Store, coord: machine, cap: int, cmd: Cmd)) {
      store = input.store; coord = input.coord; cap = input.cap; cmd = input.cmd;
      invoke();
      send coord, eFirstDone;
      goto Waiting;
    }
  }
  state Waiting {
    on eAgain do { invoke(); send coord, eClientDone; goto Finished; }
  }
  state Finished { }

  fun invoke() {
    var s: Snap;
    var k: Key;
    var its: seq[int];
    var ms: map[Key, Memo];
    var r: Memo;
    var done: bool;
    k = cmd.id * 2;
    if (cmd.isTake) { k = k + 1; }
    while (!done) {
      send store, eLoad, this;
      receive { case eLoaded: (x: Snap) { s = x; } }
      if (k in s.memos) {
        send store, eReplay, (client = this, cmd = cmd);
        receive { case eResult: (x: Memo) { r = x; } }
        done = true;
      } else {
        its = s.items; ms = s.memos;
        if (cmd.isTake) {
          if (sizeof(its) > 0) { r = (ok = true, val = its[0]); its -= (0); }
          else { r = (ok = false, val = 0); }
        } else {
          if (sizeof(its) < cap) { r = (ok = true, val = 0); its += (sizeof(its), cmd.item); }
          else { r = (ok = false, val = 0); }
        }
        ms[k] = r;
        send store, eCas, (client = this, cmd = cmd, expected = s.version, items = its, memos = ms, r = r);
        receive {
          case eResult: (x: Memo) { done = true; }
          case eConflict: { }
        }
      }
    }
    announce eReturned, (cmd = cmd, r = r);
  }
}

// snappea: driver
machine Scenario {
  var left: Client;
  var right: Client;
  var firstDone: int;
  var clientsDone: int;
  start state Running {
    entry (input: (cap: int, pre: seq[int], l: Cmd, r: Cmd)) {
      var store: Store;
      store = new Store((cap = input.cap, items = input.pre));
      left = new Client((store = store, coord = this, cap = input.cap, cmd = input.l));
      right = new Client((store = store, coord = this, cap = input.cap, cmd = input.r));
    }
    on eFirstDone do {
      firstDone = firstDone + 1;
      if (firstDone == 2) { send left, eAgain; send right, eAgain; }
    }
    on eClientDone do {
      clientsDone = clientsDone + 1;
      if (clientsDone == 2) { announce eDone, (obs = 4, replies = 4); }
    }
  }
}

fun mk(): Cmd {
  var id: int;
  id = choose(2);
  return (id = id, isTake = $, item = id + 1);
}

machine Generated {
  start state Init {
    entry {
      var cap: int;
      var fill: int;
      var pre: seq[int];
      cap = 1 + choose(3);
      fill = choose(cap + 1);
      while (sizeof(pre) < fill) { pre += (sizeof(pre), 7 + sizeof(pre)); }
      new Scenario((cap = cap, pre = pre, l = mk(), r = mk()));
    }
  }
}

test tcGenerated [main=Generated]:
  assert QueueContract in { Generated, Scenario, Client, Store };
