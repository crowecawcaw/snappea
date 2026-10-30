/*
 * Lock service with fencing tokens. Store = two independent keys, each with atomic
 * load + CAS: the lock record (last token, holder, token, expiry) and the resource
 * record (highest accepted token). No cross-key atomicity.
 * LockContract = independent oracle from the task contract and the request history.
 * Client = load / decide / CAS-with-retry algorithm, generated ops and times.
 */
type Lease = (version: int, last: int, holder: int, token: int, expires: int);
type Res = (version: int, maxTok: int);
type Op = (kind: int, now: int, tok: int);

event eLoadLock: machine;
event eLockIs: Lease;
event eLoadRes: machine;
event eResIs: Res;
event eLockTry: (client: machine, cid: int, kind: int, now: int, tok: int, expected: int, cand: Lease, changed: bool, result: int);
event eIssued: (client: machine, cid: int, token: int);
event eIssuedIs: bool;
event eWriteTry: (client: machine, cid: int, token: int, expected: int, changed: bool, ok: bool);
event eResult: int;
event eConflict;
event eClientDone;
event eDone: int;
event eAcq: (cid: int, now: int, token: int);
event eRel: (cid: int, token: int, now: int, ok: bool);
event eWrote: (cid: int, token: int, ok: bool);
event eInvoked: (cid: int, kind: int, now: int, tok: int);

spec LockContract observes eAcq, eRel, eWrote, eInvoked, eDone {
  var pending: map[int, Op];
  var last: int;
  var holder: int;
  var tok: int;
  var expires: int;
  var maxTok: int;
  var ops: int;
  start hot state Working {
    on eInvoked do (p: (cid: int, kind: int, now: int, tok: int)) {
      var op: Op;
      op = (kind = p.kind, now = p.now, tok = p.tok);
      pending[p.cid] = op;
      assert p.cid in pending;
    }
    on eAcq do (p: (cid: int, now: int, token: int)) {
      ops = ops + 1;
      assert p.cid in pending && pending[p.cid].kind == 0 && pending[p.cid].now == p.now, "result is not for the invoked acquire";
      if (holder != 0 && p.now < expires) {
        if (holder == p.cid) { assert p.token == tok, "holder must get its own token back"; }
        else { assert p.token == 0, "lock held by another client must be refused"; }
      } else {
        assert p.token == last + 1, "tokens must strictly increase by issue";
        last = p.token; holder = p.cid; tok = p.token; expires = p.now + 2;
      }
    }
    on eRel do (p: (cid: int, token: int, now: int, ok: bool)) {
      var exp: bool;
      ops = ops + 1;
      assert p.cid in pending && pending[p.cid].kind == 1 && pending[p.cid].now == p.now && pending[p.cid].tok == p.token, "result is not for the invoked release";
      exp = holder == p.cid && holder != 0 && tok == p.token && p.now < expires;
      assert p.ok == exp, "release result violates contract";
      if (exp) { holder = 0; }
    }
    on eWrote do (p: (cid: int, token: int, ok: bool)) {
      var exp: bool;
      ops = ops + 1;
      assert p.cid in pending && pending[p.cid].kind == 2 && pending[p.cid].tok == p.token, "result is not for the invoked write";
      exp = p.token >= 1 && p.token <= last && p.token >= maxTok;
      assert p.ok == exp, "fenced write result violates contract";
      if (exp) { maxTok = p.token; }
    }
    on eDone do (n: int) { assert ops == n, "driver must complete all operations"; goto Done; }
  }
  state Done { ignore eAcq, eRel, eWrote, eInvoked, eDone; }
}

machine Store {
  var lk: Lease;
  var rs: Res;
  start state Ready {
    on eLoadLock do (c: machine) { send c, eLockIs, lk; }
    on eLoadRes do (c: machine) { send c, eResIs, rs; }
    on eLockTry do (p: (client: machine, cid: int, kind: int, now: int, tok: int, expected: int, cand: Lease, changed: bool, result: int)) {
      // CAS: commit the client's candidate only if the lock is still at the version it read.
      if (p.expected != lk.version) { send p.client, eConflict; return; }
      // snappea-lint: allow mutation -- committing an unchanged candidate only bumps the version (safe, extra conflicts)
      if (p.changed) {
        lk = p.cand;
        // snappea-lint: allow mutation -- version only needs to differ; the sign of the step is irrelevant
        lk.version = lk.version + 1;
      }
      if (p.kind == 0) { announce eAcq, (cid = p.cid, now = p.now, token = p.result); }
      else { announce eRel, (cid = p.cid, token = p.tok, now = p.now, ok = p.result == 1); }
      send p.client, eResult, p.result;
    }
    on eIssued do (p: (client: machine, cid: int, token: int)) {
      if (p.token < 1 || p.token > lk.last) { announce eWrote, (cid = p.cid, token = p.token, ok = false); }
      send p.client, eIssuedIs, (p.token >= 1 && p.token <= lk.last);
    }
    on eWriteTry do (p: (client: machine, cid: int, token: int, expected: int, changed: bool, ok: bool)) {
      if (p.expected != rs.version) { send p.client, eConflict; return; }
      // snappea-lint: allow mutation:condition-true -- committing an unchanged candidate only bumps the version
      if (p.changed) {
        rs.maxTok = p.token;
        // snappea-lint: allow mutation:swap-operator -- version only needs to differ; step sign is irrelevant
        rs.version = rs.version + 1;
      }
      announce eWrote, (cid = p.cid, token = p.token, ok = p.ok);
      send p.client, eResult, 0;
    }
  }
}

machine Client {
  var store: Store;
  var coord: machine;
  var cid: int;
  start state Run {
    entry (input: (store: Store, coord: machine, cid: int, ops: seq[Op])) {
      var i: int;
      store = input.store; coord = input.coord; cid = input.cid;
      assert cid != 0, "client id must be set";
      while (i < sizeof(input.ops)) {
        announce eInvoked, (cid = cid, kind = input.ops[i].kind, now = input.ops[i].now, tok = input.ops[i].tok);
        if (input.ops[i].kind == 2) { doWrite(input.ops[i].tok); }
        else { doLock(input.ops[i].kind, input.ops[i].now, input.ops[i].tok); }
        i = i + 1;
      }
      send coord, eClientDone;
    }
  }
  fun doLock(kind: int, now: int, tok: int) {
    var l: Lease;
    var cand: Lease;
    var done: bool;
    var held: bool;
    var changed: bool;
    var result: int;
    var attempts: int;
    while (!done) {
      // snappea-lint: allow mutation -- attempts only feeds the test-only contention bound
      attempts = attempts + 1;
      assert attempts <= 4, "bounded contention exceeded";
      send store, eLoadLock, this;
      receive { case eLockIs: (s: Lease) { l = s; } }
      // Decide from the snapshot only.
      held = l.holder != 0 && now < l.expires;
      cand = l;
      // snappea-lint: allow mutation -- a stale changed flag only commits the unchanged snapshot copy
      changed = false;
      result = 0;
      if (kind == 0) {
        if (held) {
          if (l.holder == cid) { result = l.token; }
        } else {
          cand.last = l.last + 1; cand.holder = cid; cand.token = l.last + 1;
          cand.expires = now + 2; changed = true; result = l.last + 1;
        }
      } else {
        if (held && l.holder == cid && l.token == tok) {
          cand.holder = 0; changed = true; result = 1;
        }
      }
      send store, eLockTry, (client = this, cid = cid, kind = kind, now = now, tok = tok,
        expected = l.version, cand = cand, changed = changed, result = result);
      receive {
        case eResult: (r: int) { done = true; }
        case eConflict: { }
      }
    }
  }
  fun doWrite(token: int) {
    var r: Res;
    var done: bool;
    var ok: bool;
    var ok2: bool;
    var attempts: int;
    send store, eIssued, (client = this, cid = cid, token = token);
    receive { case eIssuedIs: (b: bool) { ok = b; } }
    if (!ok) { return; }
    while (!done) {
      // snappea-lint: allow mutation -- attempts only feeds the test-only contention bound
      attempts = attempts + 1;
      assert attempts <= 4, "bounded contention exceeded";
      send store, eLoadRes, this;
      receive { case eResIs: (s: Res) { r = s; } }
      ok2 = token >= r.maxTok;
      send store, eWriteTry, (client = this, cid = cid, token = token, expected = r.version,
        changed = ok2, ok = ok2);
      receive {
        case eResult: (x: int) { done = true; }
        case eConflict: { }
      }
    }
  }
}

machine Scenario {
  var n: int;
  var need: int;
  var total: int;
  var serial: bool;
  var s: Store;
  var aops: seq[Op];
  var bops: seq[Op];
  start state Running {
    entry (input: (pre: seq[Op], a: seq[Op], b: seq[Op], serial: bool)) {
      s = new Store();
      // snappea-lint: allow mutation -- serial only orders the driver; the oracle checks any order
      serial = input.serial;
      aops = input.a;
      bops = input.b;
      need = 2;
      total = sizeof(input.pre) + sizeof(input.a) + sizeof(input.b);
      // snappea-lint: allow mutation -- an empty prefix client is a harmless no-op
      if (sizeof(input.pre) == 0) { begin(); }
      else { need = 3; new Client((store = s, coord = this, cid = 3, ops = input.pre)); }
    }
    on eClientDone do {
      n = n + 1;
      // pre: client 3 runs alone first (issues tokens), then A and B start.
      if (need == 3 && n == 1) { begin(); }
      // serial: B starts only after A finished (sequential history).
      if (serial && n == need - 1) { new Client((store = s, coord = this, cid = 2, ops = bops)); }
      if (n == need) { announce eDone, total; }
    }
  }
  fun begin() {
    new Client((store = s, coord = this, cid = 1, ops = aops));
    // snappea-lint: allow mutation -- serial only orders the driver; the oracle checks any order
    if (!serial) { new Client((store = s, coord = this, cid = 2, ops = bops)); }
  }
}

fun mk(kind: int, now: int, tok: int): Op {
  return (kind = kind, now = now, tok = tok);
}

// Own-token re-acquire, release at 1 (unexpired), takeover at the expiry boundary 2, write.
machine Fixed {
  start state Init {
    entry {
      var pre: seq[Op];
      var a: seq[Op];
      var b: seq[Op];
      a += (0, mk(0, 0, 0)); a += (1, mk(0, 1, 0)); a += (2, mk(1, 1, 1));
      b += (0, mk(0, 1, 0)); b += (1, mk(0, 2, 0)); b += (2, mk(2, 0, 2));
      new Scenario((pre = pre, a = a, b = b, serial = false));
    }
  }
}

// Stale-token fencing: A (token 1) writes twice around B's takeover at 2 and write of 2.
machine Stale {
  start state Init {
    entry {
      var pre: seq[Op];
      var a: seq[Op];
      var b: seq[Op];
      a += (0, mk(0, 0, 0)); a += (1, mk(2, 0, 1)); a += (2, mk(2, 0, 1));
      b += (0, mk(0, 2, 0)); b += (1, mk(2, 0, 2)); b += (2, mk(2, 0, 3));
      new Scenario((pre = pre, a = a, b = b, serial = false));
    }
  }
}

// Release exactly at expiry fails; wrong-token release fails.
machine Expiry {
  start state Init {
    entry {
      var pre: seq[Op];
      var a: seq[Op];
      var b: seq[Op];
      a += (0, mk(0, 0, 0)); a += (1, mk(1, 2, 1)); a += (2, mk(1, 1, 2));
      b += (0, mk(0, 2, 0)); b += (1, mk(1, 2, 2)); b += (2, mk(1, 3, 2));
      new Scenario((pre = pre, a = a, b = b, serial = false));
    }
  }
}

// GenLock: A acquires at 0..1 then releases (token 1..2, time 0..3); B acquires at 0..3.
// Times cross the ttl=2 boundary. B's acquire may precede, follow or expire A's lease.
machine GenLock {
  start state Init {
    entry {
      var pre: seq[Op];
      var a: seq[Op];
      var b: seq[Op];
      a += (0, mk(0, choose(2), 0));
      a += (1, mk(1, choose(4), 1 + choose(2)));
      b += (0, mk(0, choose(4), 0));
      new Scenario((pre = pre, a = a, b = b, serial = false));
    }
  }
}

// GenAcq: three acquires (A twice, B once) at times 0..3: re-acquire returns the same
// token while unexpired, a new larger token after expiry, None for the other client.
machine GenAcq {
  start state Init {
    entry {
      var pre: seq[Op];
      var a: seq[Op];
      var b: seq[Op];
      a += (0, mk(0, choose(2), 0));
      a += (1, mk(0, choose(4), 0));
      b += (0, mk(0, choose(4), 0));
      new Scenario((pre = pre, a = a, b = b, serial = false));
    }
  }
}

// GenWrite: A acquires then writes token 0..3; B acquires at 0..3 then writes token 0..3.
machine GenWrite {
  start state Init {
    entry {
      var pre: seq[Op];
      var a: seq[Op];
      var b: seq[Op];
      a += (0, mk(0, choose(2), 0));
      a += (1, mk(2, 0, choose(4)));
      b += (0, mk(0, choose(3), 0));
      b += (1, mk(2, 0, choose(4)));
      new Scenario((pre = pre, a = a, b = b, serial = false));
    }
  }
}

// Serial histories: every result is determined, so each lock/fence rule must hold exactly.
// A: acquire, re-acquire (same token), write 1, release, release again (false).
// B: acquire at 1 after release (token 2), write 2, stale 1, unissued 9, equal 2 again, release expired.
machine SerialRelease {
  start state Init {
    entry {
      var pre: seq[Op];
      var a: seq[Op];
      var b: seq[Op];
      a += (0, mk(0, 0, 0)); a += (1, mk(0, 1, 0)); a += (2, mk(2, 0, 1));
      a += (3, mk(1, 1, 1)); a += (4, mk(1, 1, 1));
      b += (0, mk(0, 1, 0)); b += (1, mk(2, 0, 2)); b += (2, mk(2, 0, 1)); b += (3, mk(2, 0, 9));
      b += (4, mk(2, 0, 2)); b += (5, mk(1, 3, 2)); b += (6, mk(1, 2, 2));
      new Scenario((pre = pre, a = a, b = b, serial = true));
    }
  }
}

// A holds 0..2; B refused at 1 and may not release; B granted at 2 (boundary);
// A's token 1 is now stale for fencing and for release; A refused while B holds.
machine SerialTakeover {
  start state Init {
    entry {
      var pre: seq[Op];
      var a: seq[Op];
      var b: seq[Op];
      a += (0, mk(0, 0, 0)); a += (1, mk(2, 0, 1));
      b += (0, mk(0, 1, 0)); b += (1, mk(1, 1, 1)); b += (2, mk(0, 2, 0));
      b += (3, mk(2, 0, 2)); b += (4, mk(2, 0, 1));
      new Scenario((pre = pre, a = a, b = b, serial = true));
    }
  }
}

// Lost-update race on the resource: client 3 first issues tokens 1..3; then A writes 3
// while B writes 1 then 2. If B's write of 1 loaded max 0 before A's write of 3, its CAS must
// fail and retry; otherwise max drops to 1 and the later write of 2 is wrongly accepted.
machine RaceWrite {
  start state Init {
    entry {
      var pre: seq[Op];
      var a: seq[Op];
      var b: seq[Op];
      pre += (0, mk(0, 0, 0)); pre += (1, mk(0, 2, 0)); pre += (2, mk(0, 4, 0));
      a += (0, mk(2, 0, 3));
      b += (0, mk(2, 0, 1)); b += (1, mk(2, 0, 2));
      new Scenario((pre = pre, a = a, b = b, serial = false));
    }
  }
}

test tcFixed [main=Fixed]: assert LockContract in { Fixed, Scenario, Client, Store };
test tcGenLock [main=GenLock]: assert LockContract in { GenLock, Scenario, Client, Store };
test tcGenAcq [main=GenAcq]: assert LockContract in { GenAcq, Scenario, Client, Store };
test tcGenWrite [main=GenWrite]: assert LockContract in { GenWrite, Scenario, Client, Store };
test tcStale [main=Stale]: assert LockContract in { Stale, Scenario, Client, Store };
test tcExpiry [main=Expiry]: assert LockContract in { Expiry, Scenario, Client, Store };
test tcSerialRelease [main=SerialRelease]: assert LockContract in { SerialRelease, Scenario, Client, Store };
test tcSerialTakeover [main=SerialTakeover]: assert LockContract in { SerialTakeover, Scenario, Client, Store };
test tcRaceWrite [main=RaceWrite]: assert LockContract in { RaceWrite, Scenario, Client, Store };
