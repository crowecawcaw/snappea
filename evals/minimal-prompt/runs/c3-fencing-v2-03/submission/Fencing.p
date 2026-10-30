// Lock service with fencing tokens. Primitives: per-key atomic load and CAS.
// Keys: "lock" (holder, token, expiry, last issued token) and "res" (max token used, value).

type LockSnap = (ver: int, holder: int, token: int, expiry: int, last: int);
type ResSnap = (ver: int, maxUsed: int);

event eLoadLock: machine;
event eLockSnap: LockSnap;
event eCasLock: (client: machine, who: int, kind: int, now: int, argToken: int, result: int,
                 snap: LockSnap, holder: int, token: int, expiry: int, last: int);
event eLoadRes: machine;
event eResSnap: ResSnap;
event eCasRes: (client: machine, writer: int, token: int, lver: int, snap: ResSnap);
event eCasOk: bool;

// kind 0 = acquire, 1 = release. Client ids are >= 0, -1 means nobody.
event eLockState: LockSnap;
event eLockOp: (kind: int, who: int, now: int, argToken: int, result: int, ver: int, committed: bool);
event eResState: ResSnap;
event eWriteOp: (writer: int, token: int, lver: int, rver: int, accepted: bool, committed: bool);
event eLockRet: (who: int, result: int);
event eWriteRet: (writer: int, accepted: bool);
event eDone: (lockOps: int, writeOps: int);
event eClientDone;

// Independent contract, driven by the history of committed states and of op decisions.
spec Contract observes eLockState, eLockOp, eResState, eWriteOp, eLockRet, eWriteRet, eDone {
  var lockHist: map[int, LockSnap];
  var resHist: map[int, ResSnap];
  var latestLock: int;
  var latestRes: int;
  var ttl: int;
  var ops: int;
  var grants: int;
  var writesOk: int;
  var lockRets: int;
  var writeRets: int;
  var writeOps: int;
  var expect: LockSnap;
  var expectLock: bool;
  var expectRes: ResSnap;
  var expectRs: bool;
  var lastOp: map[int, int];
  var lastWrite: map[int, bool];
  start hot state Run {
    entry { ttl = 2; }
    on eLockState do (s: LockSnap) {
      if (expectLock) {
        // The committed state must be exactly what the contract says this op produces.
        assert s.ver == latestLock + 1, "lock version must advance by one";
        assert s.holder == expect.holder && s.token == expect.token && s.expiry == expect.expiry && s.last == expect.last, "committed lock state differs from contract";
        expectLock = false;
      }
      lockHist[s.ver] = s; latestLock = s.ver;
    }
    on eResState do (s: ResSnap) {
      if (expectRs) {
        assert s.ver == latestRes + 1, "resource version must advance by one";
        assert s.maxUsed == expectRes.maxUsed, "committed resource max token differs from contract";
        expectRs = false;
      }
      resHist[s.ver] = s; latestRes = s.ver;
    }
    on eLockOp do (o: (kind: int, who: int, now: int, argToken: int, result: int, ver: int, committed: bool)) {
      var s: LockSnap;
      var active: bool;
      assert o.ver in lockHist, "decision based on unknown version";
      s = lockHist[o.ver];
      active = s.holder >= 0 && o.now < s.expiry;
      if (o.committed) { assert o.ver == latestLock, "commit from a stale version"; }
      ops = ops + 1;
      lastOp[o.who] = o.result;
      if (o.kind == 0) {
        if (!active) {
          assert o.result > s.last, "token not strictly increasing";
          assert o.committed, "free lock must be granted";
          grants = grants + 1;
          expect = (ver = 0, holder = o.who, token = o.result, expiry = o.now + ttl, last = o.result); expectLock = true;
        } else if (s.holder == o.who) {
          assert o.result == s.token, "holder must get its own token back";
          assert !o.committed, "re-acquire must not change state";
        } else {
          assert o.result == 0, "acquire must fail while another holds the lease";
          assert !o.committed, "failed acquire must not change state";
        }
      } else {
        if (active && s.holder == o.who && s.token == o.argToken) {
          assert o.result == 1 && o.committed, "valid release must succeed";
          expect = (ver = 0, holder = -1, token = s.token, expiry = s.expiry, last = s.last); expectLock = true;
        } else {
          assert o.result == 0 && !o.committed, "invalid release must fail without effect";
        }
      }
    }
    // snappea-lint: allow unused-payload-field -- writer is only a map key for lastWrite
    on eWriteOp do (w: (writer: int, token: int, lver: int, rver: int, accepted: bool, committed: bool)) {
      var issued: bool;
      var lower: bool;
      assert w.lver in lockHist, "write decided from unknown lock version";
      issued = w.token >= 1 && w.token <= lockHist[w.lver].last;
      lower = false;
      lastWrite[w.writer] = w.accepted;
      writeOps = writeOps + 1;
      if (w.rver >= 0) {
        assert w.rver in resHist, "write decided from unknown resource version";
        lower = w.token < resHist[w.rver].maxUsed;
        if (w.committed) { assert w.rver == latestRes, "write commit from stale version"; }
      }
      if (w.accepted) {
        assert issued, "accepted a token that was never issued";
        assert !lower, "accepted a token lower than one already used";
        assert w.committed, "accepted write must be committed";
        writesOk = writesOk + 1;
        expectRes = (ver = 0, maxUsed = w.token); expectRs = true;
      } else {
        assert !w.committed, "rejected write must have no effect";
        assert !issued || lower, "rejected a valid write";
      }
    }
    on eLockRet do (r: (who: int, result: int)) {
      assert r.who in lastOp, "client returned without a linearized operation";
      assert lastOp[r.who] == r.result, "client returned something other than its linearized result";
      lastOp -= (r.who);
      lockRets = lockRets + 1;
    }
    on eWriteRet do (r: (writer: int, accepted: bool)) {
      assert r.writer in lastWrite, "writer returned without a linearized operation";
      assert lastWrite[r.writer] == r.accepted, "writer returned something other than its linearized result";
      lastWrite -= (r.writer);
      writeRets = writeRets + 1;
    }
    on eDone do (e: (lockOps: int, writeOps: int)) {
      assert ops == e.lockOps, "lock operations differ from the scenario";
      assert writeOps == e.writeOps, "write operations differ from the scenario";
      assert lockRets == ops, "every lock call must return";
      assert writeRets == writeOps, "every write call must return";
      assert !expectLock && !expectRs, "commit not followed by state";
      assert grants >= 1, "scenario never granted";
      goto Complete;
    }
  }
  state Complete { }
}

machine Store {
  var lver: int; var holder: int; var tok: int; var exp: int; var last: int;
  var rver: int; var maxUsed: int;
  start state Ready {
    entry {
      holder = -1;
      announce eLockState, (ver = 0, holder = -1, token = 0, expiry = 0, last = 0);
      announce eResState, (ver = 0, maxUsed = 0);
    }
    on eLoadLock do (c: machine) {
      send c, eLockSnap, (ver = lver, holder = holder, token = tok, expiry = exp, last = last);
    }
    on eLoadRes do (c: machine) {
      send c, eResSnap, (ver = rver, maxUsed = maxUsed);
    }
    on eCasLock do (p: (client: machine, who: int, kind: int, now: int, argToken: int, result: int,
                        snap: LockSnap, holder: int, token: int, expiry: int, last: int)) {
      if (p.snap.ver == lver) {
        announce eLockOp, (kind = p.kind, who = p.who, now = p.now, argToken = p.argToken,
                           result = p.result, ver = p.snap.ver, committed = true);
        holder = p.holder; tok = p.token; exp = p.expiry; last = p.last;
        lver = lver + 1;
        announce eLockState, (ver = lver, holder = holder, token = tok, expiry = exp, last = last);
        send p.client, eCasOk, true;
      } else {
        send p.client, eCasOk, false;
      }
    }
    on eCasRes do (p: (client: machine, writer: int, token: int, lver: int, snap: ResSnap)) {
      if (p.snap.ver == rver) {
        announce eWriteOp, (writer = p.writer, token = p.token, lver = p.lver, rver = p.snap.ver, accepted = true, committed = true);
        maxUsed = p.token;
        rver = rver + 1;
        announce eResState, (ver = rver, maxUsed = maxUsed);
        send p.client, eCasOk, true;
      } else {
        send p.client, eCasOk, false;
      }
    }
  }
}

// Lock client: acquire, re-acquire, release, acquire again, at generated times.
machine LockClient {
  var store: Store;
  var id: int;
  var coord: machine;
  var held: int;
  start state Run {
    entry (cfg: (store: Store, id: int, coord: machine, t0: int, long: bool)) {
      store = cfg.store; id = cfg.id; coord = cfg.coord;
      held = acquire(cfg.t0);
      if (held == 0) { held = choose(3); }
      if (cfg.long) {
        acquire(choose(4));
        release(held, choose(4));
        acquire(choose(4));
      } else {
        release(held, 1 + choose(2));
      }
      send coord, eClientDone;
    }
  }
  fun acquire(now: int): int {
    var r: int;
    r = acquireInner(now);
    announce eLockRet, (who = id, result = r);
    return r;
  }
  fun acquireInner(now: int): int {
    var s: LockSnap; var ok: bool; var done: bool; var res: int; var tries: int;
    while (!done) {
      tries = tries + 1;
      assert tries <= 5, "acquire did not finish within the contention bound";
      send store, eLoadLock, this;
      receive { case eLockSnap: (x: LockSnap) { s = x; } }
      if (s.holder >= 0 && now < s.expiry) {
        if (s.holder == id) { res = s.token; } else { res = 0; }
        announce eLockOp, (kind = 0, who = id, now = now, argToken = 0, result = res, ver = s.ver, committed = false);
        done = true;
      } else {
        res = s.last + 1;
        send store, eCasLock, (client = this, who = id, kind = 0, now = now, argToken = 0, result = res,
                               snap = s, holder = id, token = res, expiry = now + 2, last = res);
        receive { case eCasOk: (b: bool) { ok = b; } }
        done = ok;
      }
    }
    return res;
  }
  fun release(t: int, now: int) {
    var r: bool;
    r = releaseInner(t, now);
    if (r) { announce eLockRet, (who = id, result = 1); } else { announce eLockRet, (who = id, result = 0); }
  }
  fun releaseInner(t: int, now: int): bool {
    var res: bool; var s: LockSnap; var ok: bool; var done: bool; var tries: int;
    while (!done) {
      tries = tries + 1;
      assert tries <= 5, "release did not finish within the contention bound";
      send store, eLoadLock, this;
      receive { case eLockSnap: (x: LockSnap) { s = x; } }
      if (s.holder == id && s.token == t && now < s.expiry) {
        send store, eCasLock, (client = this, who = id, kind = 1, now = now, argToken = t, result = 1,
                               snap = s, holder = -1, token = s.token, expiry = s.expiry, last = s.last);
        receive { case eCasOk: (b: bool) { ok = b; } }
        done = ok;
        res = ok;
      } else {
        announce eLockOp, (kind = 1, who = id, now = now, argToken = t, result = 0, ver = s.ver, committed = false);
        done = true;
      }
    }
    return res;
  }
}

machine Writer {
  var id: int;
  start state Run {
    entry (cfg: (store: Store, coord: machine, n: int, id: int)) {
      var i: int;
      id = cfg.id;
      while (i < cfg.n) { write(cfg.store, choose(4)); i = i + 1; }
      send cfg.coord, eClientDone;
    }
  }
  fun write(store: Store, t: int) {
    var l: LockSnap; var r: ResSnap; var ok: bool; var done: bool; var tries: int;
    send store, eLoadLock, this;
    receive { case eLockSnap: (x: LockSnap) { l = x; } }
    if (t < 1 || t > l.last) {
      announce eWriteOp, (writer = id, token = t, lver = l.ver, rver = -1, accepted = false, committed = false);
      announce eWriteRet, (writer = id, accepted = false);
      return;
    }
    while (!done) {
      tries = tries + 1;
      assert tries <= 5, "write did not finish within the contention bound";
      send store, eLoadRes, this;
      receive { case eResSnap: (x: ResSnap) { r = x; } }
      if (t < r.maxUsed) {
        announce eWriteOp, (writer = id, token = t, lver = l.ver, rver = r.ver, accepted = false, committed = false);
        announce eWriteRet, (writer = id, accepted = false);
        done = true;
      } else {
        send store, eCasRes, (client = this, writer = id, token = t, lver = l.ver, snap = r);
        receive { case eCasOk: (b: bool) { ok = b; } }
        done = ok;
        if (ok) { announce eWriteRet, (writer = id, accepted = true); }
      }
    }
  }
}

machine Scenario {
  var n: int; var done: int; var expectedLock: int; var expectedWrite: int;
  start state Run {
    entry (cfg: (locks: int, writers: int, writes: int, long: bool)) {
      var s: Store; var i: int;
      s = new Store();
      n = cfg.locks + cfg.writers;
      expectedWrite = cfg.writers * cfg.writes;
      if (cfg.long) { expectedLock = cfg.locks * 4; } else { expectedLock = cfg.locks * 2; }
      while (i < cfg.locks) { new LockClient((store = s, id = i, coord = this, t0 = choose(2), long = cfg.long)); i = i + 1; }
      i = 0;
      while (i < cfg.writers) { new Writer((store = s, coord = this, n = cfg.writes, id = i)); i = i + 1; }
    }
    on eClientDone do {
      done = done + 1;
      if (done == n) { announce eDone, (lockOps = expectedLock, writeOps = expectedWrite); }
    }
  }
}

machine OneLongLockOneWriter { start state I { entry { new Scenario((locks = 1, writers = 1, writes = 2, long = true)); } } }
machine TwoLocks { start state I { entry { new Scenario((locks = 2, writers = 0, writes = 0, long = false)); } } }
machine OneLockTwoWriters { start state I { entry { new Scenario((locks = 1, writers = 2, writes = 1, long = false)); } } }

test tcTwoLocks [main=TwoLocks]:
  assert Contract in { TwoLocks, Scenario, LockClient, Writer, Store };
test tcOneLockTwoWriters [main=OneLockTwoWriters]:
  assert Contract in { OneLockTwoWriters, Scenario, LockClient, Writer, Store };
test tcOneLongLockOneWriter [main=OneLongLockOneWriter]:
  assert Contract in { OneLongLockOneWriter, Scenario, LockClient, Writer, Store };
