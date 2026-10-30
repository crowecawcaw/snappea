// Lock service with fencing tokens: load/CAS model of the algorithm.
// kinds: 0 acquire, 1 release, 2 write. Client ids are 1,2; holder 0 = none.
// Store primitives: atomic load and CAS on key "lock" and on key "res".
type Op = (id: int, kind: int, client: int, now: int, token: int);
type LockRec = (version: int, last: int, holder: int, token: int, exp: int);

event eLoadL: (c: machine, op: Op);
event eLoadedL: LockRec;
event eCasL: (c: machine, op: Op, rec: LockRec);
event eLoadR: (c: machine, op: Op);
event eLoadedR: (version: int, high: int);
event eCasR: (c: machine, op: Op, expected: int, high: int);
event eCasOk;
event eConflict;
event eLoadObs: (op: Op);
event eLockCommit: (op: Op, result: int);
event eResLoadObs: (op: Op);
event eResCommit: (op: Op);
event eLoadLObs: (op: Op);
event eReturned: (op: Op, result: int);
event eClientDone;
event eDone: int;

// Independent oracle: computed from the contract and commit history only.
spec FenceContract observes eLoadObs, eLoadLObs, eResLoadObs, eLockCommit, eResCommit, eReturned, eDone {
  var last: int;
  var holder: int;
  var token: int;
  var exp: int;
  var high: int;
  var expected: map[int, int];
  var returned: int;
  start state Watching {
    on eLoadObs do (p: (op: Op)) { observeLock(p.op); }
    on eLoadLObs do (p: (op: Op)) {
      // write: unissued token is rejected at the moment last was read
      if (p.op.token < 1 || p.op.token > last) { expected[p.op.id] = 0; }
    }
    on eResLoadObs do (p: (op: Op)) {
      assert p.op.token <= last, "write reached resource with unissued token";
      if (p.op.token < high) { expected[p.op.id] = 0; }
    }
    on eLockCommit do (p: (op: Op, result: int)) {
      var e: int;
      e = lockExpect(p.op);
      assert p.result == e, "lock commit result disagrees with contract";
      if (p.op.kind == 0) {
        assert e == last + 1, "acquire committed without a fresh increasing token";
        last = e; holder = p.op.client; token = e; exp = p.op.now + 2;
      } else {
        assert e == 1, "release committed but contract says it must not";
        holder = 0;
      }
      expected[p.op.id] = e;
    }
    on eResCommit do (p: (op: Op)) {
      assert p.op.token <= last, "accepted a never-issued token";
      assert p.op.token >= high, "accepted a token lower than an accepted one";
      high = p.op.token;
      expected[p.op.id] = 1;
    }
    on eReturned do (p: (op: Op, result: int)) {
      assert p.op.id in expected, "result returned without a justifying read or commit";
      assert expected[p.op.id] == p.result, "client received wrong result";
      returned = returned + 1;
    }
    on eDone do (n: int) { assert returned == n, "scenario omitted an operation"; }
  }
  // acquire: token (existing/new, new = last+1) or 0; release: 1/0
  fun lockExpect(op: Op): int {
    var live: bool;
    live = holder != 0 && op.now < exp;
    if (op.kind == 0) {
      if (live) { if (holder == op.client) { return token; } return 0; }
      return last + 1;
    }
    if (live && holder == op.client && token == op.token) { return 1; }
    return 0;
  }
  fun observeLock(op: Op) {
    var e: int;
    e = lockExpect(op);
    if ((op.kind == 0 && e <= last) || (op.kind == 1 && e == 0)) { expected[op.id] = e; }
  }
}

machine Store {
  var lk: LockRec;
  var rver: int;
  var rhigh: int;
  start state Ready {
    on eLoadL do (p: (c: machine, op: Op)) {
      if (p.op.kind == 2) { announce eLoadLObs, (op = p.op,); }
      else { announce eLoadObs, (op = p.op,); }
      send p.c, eLoadedL, lk;
    }
    on eCasL do (p: (c: machine, op: Op, rec: LockRec)) {
      var r: int;
      if (p.rec.version == lk.version) {
        lk = (version = lk.version + 1, last = p.rec.last, holder = p.rec.holder,
              token = p.rec.token, exp = p.rec.exp);
        if (p.op.kind == 0) { r = lk.token; } else { r = 1; }
        announce eLockCommit, (op = p.op, result = r);
        send p.c, eCasOk;
      } else { send p.c, eConflict; }
    }
    on eLoadR do (p: (c: machine, op: Op)) {
      announce eResLoadObs, (op = p.op,);
      send p.c, eLoadedR, (version = rver, high = rhigh);
    }
    on eCasR do (p: (c: machine, op: Op, expected: int, high: int)) {
      if (p.expected == rver) {
        rver = rver + 1; rhigh = p.high;
        announce eResCommit, (op = p.op,);
        send p.c, eCasOk;
      } else { send p.c, eConflict; }
    }
  }
}

machine Client {
  var store: Store;
  var coord: machine;
  var cid: int;
  start state Run {
    entry (input: (store: Store, coord: machine, cid: int, ops: seq[(kind: int, now: int, token: int)])) {
      var i: int;
      var op: Op;
      var res: int;
      store = input.store; coord = input.coord; cid = input.cid;
      while (i < sizeof(input.ops)) {
        op = (id = cid * 10 + i, kind = input.ops[i].kind, client = cid,
              now = input.ops[i].now, token = input.ops[i].token);
        res = run(op);
        announce eReturned, (op = op, result = res);
        i = i + 1;
      }
      send coord, eClientDone;
    }
  }
  fun run(op: Op): int {
    var rec: LockRec;
    var r: (version: int, high: int);
    var done: bool;
    var out: int;
    var ok: bool;
    if (op.kind == 2) {
      send store, eLoadL, (c = this, op = op);
      receive { case eLoadedL: (x: LockRec) { rec = x; } }
      if (op.token < 1 || op.token > rec.last) { done = true; }
      while (!done) {
        send store, eLoadR, (c = this, op = op);
        receive { case eLoadedR: (x: (version: int, high: int)) { r = x; } }
        if (op.token < r.high) { done = true; }
        else {
          send store, eCasR, (c = this, op = op, expected = r.version, high = op.token);
          ok = waitCas();
          if (ok) { out = 1; done = true; }
        }
      }
    }
    while (!done) {
      send store, eLoadL, (c = this, op = op);
      receive { case eLoadedL: (x: LockRec) { rec = x; } }
      if (op.kind == 0) {
        if (rec.holder != 0 && op.now < rec.exp) {
          if (rec.holder == op.client) { out = rec.token; }
          done = true;
        } else {
          send store, eCasL, (c = this, op = op, rec = (version = rec.version,
            last = rec.last + 1, holder = op.client, token = rec.last + 1, exp = op.now + 2));
          ok = waitCas();
          if (ok) { out = rec.last + 1; done = true; }
        }
      } else {
        if (!(rec.holder == op.client && rec.token == op.token && op.now < rec.exp)) { done = true; }
        else {
          send store, eCasL, (c = this, op = op, rec = (version = rec.version,
            last = rec.last, holder = 0, token = rec.token, exp = rec.exp));
          ok = waitCas();
          if (ok) { out = 1; done = true; }
        }
      }
    }
    return out;
  }
  fun waitCas(): bool {
    var ok: bool;
    receive {
      case eCasOk: { ok = true; }
      case eConflict: { ok = false; }
    }
    return ok;
  }
}

// snappea: driver
machine Scenario {
  var n: int;
  var total: int;
  start state Running {
    entry (input: (a: seq[(kind: int, now: int, token: int)], b: seq[(kind: int, now: int, token: int)])) {
      var s: Store;
      s = new Store();
      total = sizeof(input.a) + sizeof(input.b);
      new Client((store = s, coord = this, cid = 1, ops = input.a));
      new Client((store = s, coord = this, cid = 2, ops = input.b));
    }
    on eClientDone do { n = n + 1; if (n == 2) { announce eDone, total; } }
  }
}

fun mk(k: int, now: int, t: int): (kind: int, now: int, token: int) {
  return (kind = k, now = now, token = t);
}

// Generated acquire/release histories: client 1 x 2 ops, client 2 x 1 op, times 0..3, tokens 1..2.
machine GenLease {
  start state Init {
    entry {
      var a: seq[(kind: int, now: int, token: int)];
      var b: seq[(kind: int, now: int, token: int)];
      a += (0, mk(0, choose(3), 0));
      a += (1, mk(choose(2), choose(4), 1));
      b += (0, mk(choose(2), choose(4), 1 + choose(2)));
      new Scenario((a = a, b = b));
    }
  }
}

// Generated fencing: each client acquires at a chosen time then writes a chosen token
// (0 = never issued, 1..2 issued or stale, 3 = maybe never issued).
machine GenFence {
  start state Init {
    entry {
      var a: seq[(kind: int, now: int, token: int)];
      var b: seq[(kind: int, now: int, token: int)];
      a += (0, mk(0, choose(4), 0));
      a += (1, mk(2, 0, choose(4)));
      b += (0, mk(0, choose(4), 0));
      b += (1, mk(2, 0, choose(4)));
      new Scenario((a = a, b = b));
    }
  }
}

// Fixed: expiry boundary (granted at 0, other client at 1 refused, at 2 granted),
// then old token write after new token accepted must be fenced.
machine Expiry {
  start state Init {
    entry {
      var a: seq[(kind: int, now: int, token: int)];
      var b: seq[(kind: int, now: int, token: int)];
      a += (0, mk(0, 0, 0)); a += (1, mk(2, 0, 1)); a += (2, mk(2, 5, 1));
      b += (0, mk(0, 1, 0)); b += (1, mk(0, 2, 0)); b += (2, mk(2, 2, 2));
      new Scenario((a = a, b = b));
    }
  }
}

// Fixed: release by wrong client/token, release after expiry, own re-acquire, retry reacquire.
machine ReleaseCases {
  start state Init {
    entry {
      var a: seq[(kind: int, now: int, token: int)];
      var b: seq[(kind: int, now: int, token: int)];
      a += (0, mk(0, 0, 0)); a += (1, mk(0, 1, 0)); a += (2, mk(1, 2, 1));
      b += (0, mk(1, 0, 1)); b += (1, mk(1, 1, 1)); b += (2, mk(2, 0, 3));
      new Scenario((a = a, b = b));
    }
  }
}

test tcGenLease [main=GenLease]: assert FenceContract in { GenLease, Scenario, Client, Store };
test tcGenFence [main=GenFence]: assert FenceContract in { GenFence, Scenario, Client, Store };
test tcExpiry [main=Expiry]: assert FenceContract in { Expiry, Scenario, Client, Store };
test tcRelease [main=ReleaseCases]: assert FenceContract in { ReleaseCases, Scenario, Client, Store };

// Fixed: both clients race for the same expired/empty lock (CAS conflict), then
// boundary calls exactly at expiry (now == exp) for acquire and release,
// and both race to write tokens 1 and 2 to one resource.
machine Race {
  start state Init {
    entry {
      var a: seq[(kind: int, now: int, token: int)];
      var b: seq[(kind: int, now: int, token: int)];
      a += (0, mk(0, 0, 0)); a += (1, mk(1, 2, 1)); a += (2, mk(2, 0, 1));
      b += (0, mk(0, 0, 0)); b += (1, mk(0, 2, 0)); b += (2, mk(2, 0, 2));
      new Scenario((a = a, b = b));
    }
  }
}
test tcRace [main=Race]: assert FenceContract in { Race, Scenario, Client, Store };
