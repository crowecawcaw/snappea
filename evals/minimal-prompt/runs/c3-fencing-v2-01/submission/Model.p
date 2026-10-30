// Store with per-key atomic load/CAS: key "lock" and key "res". Clients run the algorithm.
type Lease = (active: bool, holder: int, token: int, expires: int);
type LockRec = (counter: int, lease: Lease);
type ResRec = (maxUsed: int);
type LockSnap = (version: int, rec: LockRec);
type ResSnap = (version: int, rec: ResRec);

event eLoadLock: machine;
event eLockLoaded: LockSnap;
event eCasLock: (client: machine, cid: int, expected: int, rec: LockRec, kind: int, now: int);
event eLoadRes: machine;
event eResLoaded: ResSnap;
event eCasRes: (client: machine, expected: int, token: int);
event eCasOk;
event eConflict;
event eClientDone;

machine Store {
  var lv: int; var lrec: LockRec;
  var rv: int; var rrec: ResRec;
  start state Serving {
    on eLoadLock do (c: machine) { send c, eLockLoaded, (version = lv, rec = lrec); }
    on eLoadRes do (c: machine) { send c, eResLoaded, (version = rv, rec = rrec); }
    on eCasLock do (p: (client: machine, cid: int, expected: int, rec: LockRec, kind: int, now: int)) {
      if (p.expected != lv) { send p.client, eConflict; return; }
      lv = lv + 1; lrec = p.rec;
      if (p.kind == 1) { announce eGrant, (client = p.cid, token = p.rec.lease.token, now = p.now); }
      else { announce eRevoke, (client = p.cid, token = p.rec.lease.token, now = p.now); }
      send p.client, eCasOk;
    }
    on eCasRes do (p: (client: machine, expected: int, token: int)) {
      if (p.expected != rv) { send p.client, eConflict; return; }
      rv = rv + 1; rrec.maxUsed = p.token;
      announce eAccept, (token = p.token,);
      send p.client, eCasOk;
    }
  }
}

machine Client {
  var store: Store;
  var cid: int;
  var report: bool;      // announce returned results (only meaningful for sequential runs)
  var ls: LockSnap;
  var rs: ResSnap;
  var ok: bool;

  fun Acquire(now: int): int {
    var rec: LockRec; var done: bool; var result: int;
    while (!done) {
      send store, eLoadLock, this;
      receive { case eLockLoaded: (s: LockSnap) { ls = s; } }
      if (ls.rec.lease.active && now < ls.rec.lease.expires) {
        done = true;
        if (ls.rec.lease.holder == cid) { result = ls.rec.lease.token; } else { result = 0; }
      } else {
        rec = ls.rec;
        rec.counter = rec.counter + 1;
        rec.lease = (active = true, holder = cid, token = rec.counter, expires = now + 2);
        send store, eCasLock, (client = this, cid = cid, expected = ls.version, rec = rec, kind = 1, now = now);
        receive {
          case eCasOk: { done = true; result = rec.counter; }
          case eConflict: { }
        }
      }
    }
    if (report) { announce eAcquired, (client = cid, now = now, token = result); }
    return result;
  }

  fun Release(token: int, now: int): bool {
    var rec: LockRec; var done: bool; var result: bool;
    while (!done) {
      send store, eLoadLock, this;
      receive { case eLockLoaded: (s: LockSnap) { ls = s; } }
      if (ls.rec.lease.active && ls.rec.lease.holder == cid && ls.rec.lease.token == token && now < ls.rec.lease.expires) {
        rec = ls.rec;
        rec.lease.active = false;
        send store, eCasLock, (client = this, cid = cid, expected = ls.version, rec = rec, kind = 2, now = now);
        receive {
          case eCasOk: { done = true; result = true; }
          case eConflict: { }
        }
      } else { done = true; result = false; }
    }
    if (report) { announce eReleased, (client = cid, token = token, now = now, ok = result); }
    return result;
  }

  fun Write(token: int): bool {
    var done: bool; var result: bool;
    send store, eLoadLock, this;
    receive { case eLockLoaded: (s: LockSnap) { ls = s; } }
    if (token < 1 || token > ls.rec.counter) { done = true; result = false; }
    while (!done) {
      send store, eLoadRes, this;
      receive { case eResLoaded: (s: ResSnap) { rs = s; } }
      if (token < rs.rec.maxUsed) { done = true; result = false; }
      else {
        send store, eCasRes, (client = this, expected = rs.version, token = token);
        receive {
          case eCasOk: { done = true; result = true; }
          case eConflict: { }
        }
      }
    }
    if (report) { announce eWritten, (token = token, ok = result); }
    return result;
  }

  start state Run {
    entry (cfg: (store: Store, cid: int, report: bool, mode: int, parent: machine)) {
      var t: int; var r: bool; var i: int; var n: int;
      store = cfg.store; cid = cfg.cid; report = cfg.report;
      if (cfg.mode == 0) {
        // generated sequential history: arbitrary ops, out-of-order times
        while (i < 3) {
          cid = 1 + choose(2);
          n = choose(3);
          if (n == 0) { t = Acquire(choose(4)); }
          else if (n == 1) { r = Release(1 + choose(2), choose(4)); }
          else { r = Write(choose(3)); }
          i = i + 1;
        }
      } else if (cfg.mode == 2) {
        // scripted: fencing, takeover after exact expiry, stale token, release rules
        r = Write(1);                               // never issued
        cid = 1; t = Acquire(0);                    // token 1, expires 2
        cid = 1; t = Acquire(1);                    // same token
        cid = 2; t = Acquire(1);                    // None
        r = Write(1); r = Write(2);                 // ok / never issued
        cid = 2; t = Acquire(2);                    // exactly expired: token 2
        r = Write(1);
        r = Write(2); r = Write(1);                 // 2 ok, then 1 stale
        cid = 1; r = Release(1, 2);                 // not holder
        cid = 2; r = Release(1, 3);                 // wrong token
        r = Release(2, 3); r = Release(2, 3);       // ok, then already released
        cid = 1; t = Acquire(0);                    // out-of-order time, token 3
        r = Release(3, 2);                          // exactly expired
        r = Write(3); r = Write(4);
      } else if (cfg.mode == 3) {
        // scripted: release at last unexpired instant; reacquire by holder after expiry
        cid = 1; t = Acquire(5);                    // token 1, expires 7
        r = Release(1, 6);                          // ok
        t = Acquire(6);                             // token 2
        t = Acquire(8);                             // expired exactly: token 3
        cid = 2; t = Acquire(7);                    // None (expires 10)
        cid = 1; r = Release(2, 8);                 // stale token
        r = Release(3, 9);                          // ok
        r = Write(3); r = Write(2);                 // ok, then lower rejected
      } else {
        // concurrent: client 1 holds a lease, writes, releases, writes late;
        // client 2 tries to take over (possibly after expiry) and writes.
        if (cid == 1) {
          t = Acquire(0);
          if (t > 0) { r = Write(t); r = Release(t, 1 + choose(2)); r = Write(t); }
        } else {
          t = Acquire(1 + choose(3));
          if (t > 0) { r = Write(t); r = Release(t, 1 + choose(3)); }
        }
      }
      send cfg.parent, eClientDone;
    }
  }
}


machine Seq {   // one client, generated operations: exact oracle
  var done: int;
  start state Run {
    entry {
      var s: Store;
      s = new Store();
      new Client((store = s, cid = 1, report = true, mode = 0, parent = this));
    }
    on eClientDone do { }
  }
}

machine Script {  // fixed histories, exact oracle
  start state Run {
    entry {
      var s: Store;
      s = new Store();
      new Client((store = s, cid = 1, report = true, mode = 2 + choose(2), parent = this));
    }
    on eClientDone do { }
  }
}

machine Conc {  // two clients, interleaved CAS steps, commit-point safety
  var done: int;
  start state Run {
    entry {
      var s: Store;
      s = new Store();
      new Client((store = s, cid = 1, report = false, mode = 1, parent = this));
      new Client((store = s, cid = 2, report = false, mode = 1, parent = this));
    }
    on eClientDone do { done = done + 1; }
  }
}

test tcSeq [main=Seq]: assert SeqContract in { Seq, Client, Store };
test tcScript [main=Script]: assert SeqContract in { Script, Client, Store };
test tcConc [main=Conc]: assert SafetyContract in { Conc, Client, Store };
