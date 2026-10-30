// Lock service with fencing tokens. See DESIGN.md.
// Atomic primitives: Store load / compare_swap per key ("lock" and "res:<name>").
type Rec = (ver: int, holder: int, tok: int, last: int, exp: int);
type Op = (kind: int, now: int, arg: int, name: int);  // kind 0 acquire 1 release 2 write
event eLoadLock: machine;
event eLockIs: Rec;
event eCasLock: (client: machine, expected: int, after: Rec, op: int, kind: int, who: int, now: int, arg: int, result: int);
event eLoadRes: (client: machine, name: int);
event eResIs: (ver: int, max: int);
event eCasRes: (client: machine, name: int, expected: int, token: int, op: int, lver: int);
event eCasDone: bool;
// Linearization points (announced by Store at commit, or by client at the read a no-op was decided from).
event eLockOp: (op: int, kind: int, who: int, now: int, arg: int, result: int, ver: int, committed: bool, after: Rec);
event eWriteOp: (op: int, name: int, token: int, result: bool, lver: int, rver: int, committed: bool);
event eReturned: (op: int, result: int);
event eStart: (clients: int, ops: int);
event eClientDone;

spec Contract observes eLockOp, eWriteOp, eReturned, eStart, eClientDone {
  var hist: map[int, Rec];
  var curver: int;
  var resHist: map[int, int];
  var resCur: map[int, int];
  var results: map[int, int];
  var clients: int;
  var total: int;
  var done: int;
  var returned: int;
  start hot state Working {
    entry {
      var n: int;
      hist[0] = (ver = 0, holder = -1, tok = 0, last = 0, exp = 0);
      while (n < 2) { resHist[n * 100] = 0; resCur[n] = 0; n = n + 1; }
    }
    on eStart do (p: (clients: int, ops: int)) { clients = p.clients; total = p.ops; }
    on eLockOp do (p: (op: int, kind: int, who: int, now: int, arg: int, result: int, ver: int, committed: bool, after: Rec)) {
      var s: Rec;
      var expected: int;
      var changes: bool;
      var nw: Rec;
      assert !(p.op in results), "operation linearized twice";
      assert p.ver in hist, "unknown lock version";
      s = hist[p.ver];
      nw = s;
      changes = false;
      if (p.kind == 0) {
        if (s.holder != -1 && p.now < s.exp) {
          if (s.holder == p.who) { expected = s.tok; } else { expected = 0; }
        } else {
          expected = s.last + 1;
          changes = true;
          nw = (ver = s.ver + 1, holder = p.who, tok = s.last + 1, last = s.last + 1, exp = p.now + 2);
        }
      } else {
        if (s.holder == p.who && s.tok == p.arg && p.now < s.exp) {
          expected = 1;
          changes = true;
          nw = (ver = s.ver + 1, holder = -1, tok = s.tok, last = s.last, exp = s.exp);
        } else { expected = 0; }
      }
      assert p.result == expected, "wrong lock result";
      assert p.committed == changes, "state change disagrees with contract";
      if (changes) {
        assert p.ver == curver, "commit on a stale lock version";
        assert p.after.holder == nw.holder && p.after.tok == nw.tok && p.after.last == nw.last && p.after.exp == nw.exp, "wrong lock record committed";
        curver = curver + 1;
        hist[curver] = nw;
      }
      results[p.op] = p.result;
    }
    on eWriteOp do (p: (op: int, name: int, token: int, result: bool, lver: int, rver: int, committed: bool)) {
      var issued: bool;
      var low: bool;
      var ok: bool;
      var r: int;
      assert !(p.op in results), "operation linearized twice";
      assert p.lver in hist, "unknown lock version";
      issued = p.token >= 1 && p.token <= hist[p.lver].last;
      low = p.token < resHist[p.name * 100 + p.rver];
      ok = issued && !low;
      // an unissued token is decided at the lock read; a commit also needs the resource version
      assert p.result == ok, "wrong write result";
      assert p.committed == (ok), "write effect disagrees with contract";
      if (ok) {
        assert p.rver == resCur[p.name], "commit on stale resource version";
        resCur[p.name] = p.rver + 1;
        resHist[p.name * 100 + p.rver + 1] = p.token;
      }
      if (p.result) { r = 1; } else { r = 0; }
      results[p.op] = r;
    }
    on eReturned do (p: (op: int, result: int)) {
      assert p.op in results, "returned before linearized";
      assert results[p.op] == p.result, "client returned a different result than the contract";
      returned = returned + 1;
    }
    on eClientDone do {
      done = done + 1;
      if (done == clients) {
        assert sizeof(results) == total, "not every operation was linearized";
        assert returned == total, "not every operation returned";
        goto Complete;
      }
    }
  }
  state Complete { }
}

machine Store {
  var lock: Rec;
  var res: map[int, (ver: int, max: int)];
  start state Ready {
    entry {
      lock = (ver = 0, holder = -1, tok = 0, last = 0, exp = 0);
      res[0] = (ver = 0, max = 0);
      res[1] = (ver = 0, max = 0);
    }
    on eLoadLock do (c: machine) { send c, eLockIs, lock; }
    on eCasLock do (p: (client: machine, expected: int, after: Rec, op: int, kind: int, who: int, now: int, arg: int, result: int)) {
      if (p.expected == lock.ver) {
        lock = (ver = lock.ver + 1, holder = p.after.holder, tok = p.after.tok, last = p.after.last, exp = p.after.exp);
        announce eLockOp, (op = p.op, kind = p.kind, who = p.who, now = p.now, arg = p.arg, result = p.result, ver = p.expected, committed = true, after = lock);
        send p.client, eCasDone, true;
      } else { send p.client, eCasDone, false; }
    }
    on eLoadRes do (p: (client: machine, name: int)) { send p.client, eResIs, res[p.name]; }
    on eCasRes do (p: (client: machine, name: int, expected: int, token: int, op: int, lver: int)) {
      if (p.expected == res[p.name].ver) {
        res[p.name] = (ver = p.expected + 1, max = p.token);
        announce eWriteOp, (op = p.op, name = p.name, token = p.token, result = true, lver = p.lver, rver = p.expected, committed = true);
        send p.client, eCasDone, true;
      } else { send p.client, eCasDone, false; }
    }
  }
}

// Client runs the algorithm; the linearization announce is made by the client right where it
// decides, tagged with the version it decided from. The commit announce happens on CAS success.
machine Client {
  var store: Store;
  var me: int;
  var ops: seq[Op];
  start state Run {
    entry (input: (store: Store, me: int, ops: seq[Op])) {
      var i: int;
      store = input.store; me = input.me; ops = input.ops;
      while (i < sizeof(ops)) { doOp(me * 10 + i, ops[i]); i = i + 1; }
      announce eClientDone;
    }
  }
  fun doOp(id: int, o: Op) {
    if (o.kind == 2) { doWrite(id, o); } else { doLock(id, o); }
  }
  fun doLock(id: int, o: Op) {
    var r: Rec;
    var after: Rec;
    var result: int;
    var change: bool;
    var ok: bool;
    var fin: bool;
    while (!fin) {
      send store, eLoadLock, this;
      receive { case eLockIs: (x: Rec) { r = x; } }
      change = false; result = 0; after = r;
      if (o.kind == 0) {
        if (r.holder != -1 && o.now < r.exp) {
          if (r.holder == me) { result = r.tok; }
        } else {
          change = true; result = r.last + 1;
          after = (ver = 0, holder = me, tok = r.last + 1, last = r.last + 1, exp = o.now + 2);
        }
      } else {
        if (r.holder == me && r.tok == o.arg && o.now < r.exp) {
          change = true; result = 1;
          after = (ver = 0, holder = -1, tok = r.tok, last = r.last, exp = r.exp);
        }
      }
      if (!change) {
        announce eLockOp, (op = id, kind = o.kind, who = me, now = o.now, arg = o.arg, result = result, ver = r.ver, committed = false, after = r);
        fin = true;
      } else {
        send store, eCasLock, (client = this, expected = r.ver, after = after, op = id, kind = o.kind, who = me, now = o.now, arg = o.arg, result = result);
        receive { case eCasDone: (b: bool) { ok = b; } }
        if (ok) { fin = true; }
      }
    }
    announce eReturned, (op = id, result = result);
  }
  fun doWrite(id: int, o: Op) {
    var l: Rec;
    var rs: (ver: int, max: int);
    var ok: bool;
    var fin: bool;
    var result: bool;
    while (!fin) {
      send store, eLoadLock, this;
      receive { case eLockIs: (x: Rec) { l = x; } }
      if (o.arg < 1 || o.arg > l.last) {
        announce eWriteOp, (op = id, name = o.name, token = o.arg, result = false, lver = l.ver, rver = 0, committed = false);
        fin = true;
      } else {
        send store, eLoadRes, (client = this, name = o.name);
        receive { case eResIs: (x: (ver: int, max: int)) { rs = x; } }
        if (o.arg < rs.max) {
          announce eWriteOp, (op = id, name = o.name, token = o.arg, result = false, lver = l.ver, rver = rs.ver, committed = false);
          fin = true;
        } else {
          send store, eCasRes, (client = this, name = o.name, expected = rs.ver, token = o.arg, op = id, lver = l.ver);
          receive { case eCasDone: (b: bool) { ok = b; } }
          if (ok) { result = true; fin = true; }
        }
      }
    }
    if (result) { announce eReturned, (op = id, result = 1); } else { announce eReturned, (op = id, result = 0); }
  }
}

fun startClient(s: Store, me: int, ops: seq[Op]) {
  new Client((store = s, me = me, ops = ops));
}
fun begin2(s: Store, a: seq[Op], b: seq[Op]) {
  announce eStart, (clients = 2, ops = sizeof(a) + sizeof(b));
  startClient(s, 1, a); startClient(s, 2, b);
}
fun mk(kind: int, now: int, arg: int, name: int): Op { return (kind = kind, now = now, arg = arg, name = name); }

fun runGen(prefix: bool, k1: int, k2: int) {
  var s: Store; var a: seq[Op]; var b: seq[Op]; var base: int; var span: int;
  s = new Store();
  base = 0; span = 1;
  if (prefix) { a += (0, mk(0, 0, 0, 0)); base = 1; span = 2; if (k1 == k2 || (k1 == 1 && k2 == 0)) { span = 1; } }
  if (prefix) { a += (1, mk(k1, 1, choose(span) + 1, 0)); }  // holder's second op at now=1 (lease live)
  else { a += (0, mk(k1, choose(2), 1, 0)); }
  b += (0, mk(k2, base + choose(2), choose(span) + 1, 0));
  begin2(s, a, b);
}
// Generated pairs: client 1 (optionally after acquiring at 0) runs op kind k1, client 2 kind k2.
// Kinds: 0 acquire, 1 release, 2 write. now 0..2 straddles ttl=2; tokens 1..2.
machine GenAA { start state Init { entry { runGen(true, 0, 0); } } }
machine GenAR { start state Init { entry { runGen(true, 0, 1); } } }
machine GenRR { start state Init { entry { runGen(true, 1, 1); } } }
machine GenRA { start state Init { entry { runGen(true, 1, 0); } } }
machine GenAW { start state Init { entry { runGen(true, 0, 2); } } }
machine GenWA { start state Init { entry { runGen(true, 2, 0); } } }
machine GenWW { start state Init { entry { runGen(true, 2, 2); } } }
machine GenRW { start state Init { entry { runGen(true, 1, 2); } } }
// Fresh store, no prefix: every acquire/release/write pair (acquire/release only, times 0..1, token 1).
machine Generated { start state Init { entry { runGen(false, choose(2), choose(2)); } } }
// Three clients race to acquire at one time; one lease; boundary: now == exp re-grants.
machine Contention {
  start state Init {
    entry {
      var s: Store; var a: seq[Op]; var b: seq[Op]; var c: seq[Op];
      s = new Store();      a += (0, mk(0, 0, 0, 0)); b += (0, mk(0, 0, 0, 0)); c += (0, mk(0, 1, 0, 0));
      a += (1, mk(0, 2, 0, 0)); b += (1, mk(1, 1, 1, 0)); c += (1, mk(0, 2, 0, 0));
      announce eStart, (clients = 3, ops = sizeof(a) + sizeof(b) + sizeof(c));
      startClient(s, 1, a); startClient(s, 2, b); startClient(s, 3, c);
    }
  }
}
// Stale holder: c1 holds token1, lease expires, c2 takes token2; c1's late release and write must fail.
machine StaleHolder {
  start state Init {
    entry {
      var s: Store; var a: seq[Op]; var b: seq[Op];
      s = new Store();      a += (0, mk(0, 0, 0, 0)); a += (1, mk(2, 0, 1, 0)); a += (2, mk(1, 2, 1, 0)); a += (3, mk(2, 3, 1, 0));
      b += (0, mk(0, 2, 0, 0)); b += (1, mk(2, 2, 2, 0)); b += (2, mk(1, 3, 2, 0));
      begin2(s, a, b);
    }
  }
}
// Fencing: unissued 0 and 3, equal token twice, lower after higher, two resources independent.
machine Fencing {
  start state Init {
    entry {
      var s: Store; var a: seq[Op]; var b: seq[Op];
      s = new Store();      a += (0, mk(0, 0, 0, 0)); a += (1, mk(2, 0, 0, 0)); a += (2, mk(2, 0, 3, 0));
      b += (0, mk(0, 2, 0, 0)); b += (1, mk(2, 0, 2, 1)); b += (2, mk(2, 0, 2, 0)); b += (3, mk(2, 0, 1, 1));
      begin2(s, a, b);
    }
  }
}
test tcGenerated [main=Generated]: assert Contract in { Generated, Client, Store };
test tcContention [main=Contention]: assert Contract in { Contention, Client, Store };
test tcStale [main=StaleHolder]: assert Contract in { StaleHolder, Client, Store };
test tcFencing [main=Fencing]: assert Contract in { Fencing, Client, Store };
test tcGenAA [main=GenAA]: assert Contract in { GenAA, Client, Store };
test tcGenAR [main=GenAR]: assert Contract in { GenAR, Client, Store };
test tcGenRR [main=GenRR]: assert Contract in { GenRR, Client, Store };
test tcGenRA [main=GenRA]: assert Contract in { GenRA, Client, Store };
test tcGenAW [main=GenAW]: assert Contract in { GenAW, Client, Store };
test tcGenWA [main=GenWA]: assert Contract in { GenWA, Client, Store };
test tcGenWW [main=GenWW]: assert Contract in { GenWW, Client, Store };
test tcGenRW [main=GenRW]: assert Contract in { GenRW, Client, Store };
