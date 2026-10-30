/*
 * Lock service with fencing tokens: model of the load/CAS algorithm.
 *
 * Contract (Fencing spec, derived from TASK.md, not from the algorithm):
 *  - acquire grants only if no unexpired lease (expired iff now >= grant+ttl); token strictly
 *    increases across grants (even after release); holder re-acquire returns same token;
 *    other clients are denied while a lease is unexpired.
 *  - release succeeds iff client+token match the unexpired lease.
 *  - resource write accepted iff token was issued and >= highest accepted token.
 * Atomic primitives: per-key load and per-key CAS only (lock record, resource record).
 * Lock record keeps `last` so tokens never repeat after release/expiry.
 * Bounds: ttl 2, two clients, `now` in 0..3 (arbitrary order), one resource, one
 * attempt at each op per client, CAS retries unbounded (only 1 competitor).
 * Not modeled: crashes, multiple resources (covered by Python tests), progress.
 * Read-only outcomes (deny/reacquire/failed release) are announced by the Store only
 * if the snapshot version is still current, so they linearize at a real instant.
 *
 *   snappea check Fencing.p --timeout 20s
 */
type Lock = (version: int, last: int, has: bool, holder: string, token: int, expiry: int);
type Res = (version: int, max: int);
type LockOp = (kind: int, who: string, now: int, token: int);

event eConfig: (ttl: int);
event eLockOp: LockOp;     // 0 grant 1 reacquire 2 deny 3 release ok 4 release rejected
event eResOp: (token: int, ok: bool);
event eDone;
event eLoadLock: machine;
event eLock: Lock;
event eLoadRes: machine;
event eRes: Res;
event eLockCas: (client: machine, expected: int, rec: Lock, op: LockOp);
event eResCas: (client: machine, expected: int, max: int, token: int, ok: bool);
event eOk;
event eConflict;
event eFinished;

spec Fencing observes eConfig, eLockOp, eResOp, eDone {
  var ttl: int;
  var last: int;
  var has: bool;
  var holder: string;
  var holderToken: int;
  var expiry: int;
  var max: int;
  var grants: int;
  var accepted: int;
  start state Running {
    on eConfig do (p: (ttl: int)) { ttl = p.ttl; }
    on eLockOp do (p: LockOp) {
      if (p.kind == 0) {
        assert !(has && p.now < expiry), "granted while an unexpired lease exists";
        assert p.token > last, "token did not strictly increase";
        last = p.token; has = true; holder = p.who; holderToken = p.token;
        expiry = p.now + ttl; grants = grants + 1;
      } else if (p.kind == 1) {
        assert has && p.now < expiry && holder == p.who && holderToken == p.token,
          "reacquire did not return the holder's current token";
      } else if (p.kind == 2) {
        assert has && p.now < expiry && holder != p.who, "acquire denied though lock is free or own";
      } else if (p.kind == 3) {
        assert has && p.now < expiry && holder == p.who && holderToken == p.token,
          "release succeeded without holding the unexpired lease";
        has = false;
      } else {
        assert !(has && p.now < expiry && holder == p.who && holderToken == p.token),
          "release rejected though client holds the unexpired lease";
      }
    }
    on eResOp do (p: (token: int, ok: bool)) {
      var expected: bool;
      expected = p.token >= 1 && p.token <= last && p.token >= max;
      assert p.ok == expected, "write result violates fencing";
      if (p.ok) { max = p.token; accepted = accepted + 1; }
    }
    on eDone do {
      assert grants >= 1, "scenario never granted the lock";
      assert accepted >= 1, "scenario never accepted a write";
    }
  }
}

machine Store {
  var lock: Lock;
  var res: Res;
  start state Ready {
    on eLoadLock do (c: machine) { send c, eLock, lock; }
    on eLoadRes do (c: machine) { send c, eRes, res; }
    on eLockCas do (p: (client: machine, expected: int, rec: Lock, op: LockOp)) {
      if (p.expected == lock.version) {
        lock = p.rec;
        lock.version = p.expected + 1;      // every CAS bumps the version
        announce eLockOp, p.op;
        send p.client, eOk;
      } else { send p.client, eConflict; }
    }
    on eResCas do (p: (client: machine, expected: int, max: int, token: int, ok: bool)) {
      if (p.expected == res.version) {
        res.max = p.max;
        res.version = p.expected + 1;
        announce eResOp, (token = p.token, ok = p.ok);
        send p.client, eOk;
      } else { send p.client, eConflict; }
    }
  }
}

machine Client {
  var store: Store;
  var boss: machine;
  var me: string;
  var now: int;
  var ttl: int;
  var full: bool;
  start state Run {
    entry (i: (store: Store, boss: machine, me: string, now: int, ttl: int, full: bool)) {
      var token: int;
      var ignored: bool;
      var token2: int;
      store = i.store; boss = i.boss; me = i.me; now = i.now; ttl = i.ttl;
      full = i.full;
      token = acquire();
      if (token >= 1) {
        if (full) { token2 = acquire(); }           // holder re-acquire: monitor checks same token / denial
        ignored = write(token);
        if (full) { ignored = write(token); }  // equal token is accepted
        ignored = write(token - 1);   // lower than own accepted token / never issued when 0
        if (full) { ignored = write(99); }  // never issued
        ignored = release(token);
      }
      send boss, eFinished;
    }
  }

  fun acquire(): int {
    var snap: Lock;
    var rec: Lock;
    var op: LockOp;
    var done: bool;
    while (!done) {
      send store, eLoadLock, this;
      receive { case eLock: (l: Lock) { snap = l; } }
      rec = snap;
      if (snap.has && now < snap.expiry) {
        if (snap.holder == me) { op = (kind = 1, who = me, now = now, token = snap.token); }
        else { op = (kind = 2, who = me, now = now, token = -1); }
      } else {
        op = (kind = 0, who = me, now = now, token = snap.last + 1);
        rec = (version = 0, last = snap.last + 1, has = true, holder = me,
               token = snap.last + 1, expiry = now + ttl);
      }
      send store, eLockCas, (client = this, expected = snap.version, rec = rec, op = op);
      receive { case eOk: { done = true; } case eConflict: { } }
    }
    return op.token;
  }

  fun release(token: int): bool {
    var snap: Lock;
    var rec: Lock;
    var op: LockOp;
    var done: bool;
    var ok: bool;
    while (!done) {
      send store, eLoadLock, this;
      receive { case eLock: (l: Lock) { snap = l; } }
      ok = snap.has && snap.holder == me && snap.token == token && now < snap.expiry;
      rec = snap;
      if (ok) { rec.has = false; op = (kind = 3, who = me, now = now, token = token); }
      else { op = (kind = 4, who = me, now = now, token = token); }
      send store, eLockCas, (client = this, expected = snap.version, rec = rec, op = op);
      receive { case eOk: { done = true; } case eConflict: { } }
    }
    return ok;
  }

  fun write(token: int): bool {
    var lk: Lock;
    var r: Res;
    var ok: bool;
    var done: bool;
    var newmax: int;
    while (!done) {
      send store, eLoadLock, this;
      receive { case eLock: (l: Lock) { lk = l; } }
      send store, eLoadRes, this;
      receive { case eRes: (x: Res) { r = x; } }
      ok = token >= 1 && token <= lk.last && token >= r.max;
      newmax = r.max;
      if (ok) { newmax = token; }
      send store, eResCas, (client = this, expected = r.version, max = newmax, token = token, ok = ok);
      receive { case eOk: { done = true; } case eConflict: { } }
    }
    return ok;
  }
}

machine Scenario {
  var finished: int;
  start state Running {
    entry (i: (a: int, b: int, full: bool)) {
      var s: Store;
      s = new Store();
      announce eConfig, (ttl = 2,);
      new Client((store = s, boss = this, me = "A", now = i.a, ttl = 2, full = i.full));
      new Client((store = s, boss = this, me = "B", now = i.b, ttl = 2, full = i.full));
    }
    on eFinished do {
      finished = finished + 1;
      if (finished == 2) { announce eDone; }
    }
  }
}

machine Generated {
  start state Init { entry { new Scenario((a = choose(4), b = choose(4), full = false)); } }
}
// Both clients at the same instant: contention on the free lock.
machine SameTime {
  start state Init { entry { new Scenario((a = 1, b = 1, full = true)); } }
}
// B arrives after A's lease would expire (0+2 <= 2), unreleased paths included.
machine LateB {
  start state Init { entry { new Scenario((a = 0, b = 2, full = true)); } }
}
// B's clock is behind A's: out-of-order times.
machine EarlyB {
  start state Init { entry { new Scenario((a = 3, b = 0, full = true)); } }
}

test tcGenerated [main=Generated]: assert Fencing in { Generated, Scenario, Client, Store };
test tcSameTime [main=SameTime]: assert Fencing in { SameTime, Scenario, Client, Store };
test tcLateB [main=LateB]: assert Fencing in { LateB, Scenario, Client, Store };
test tcEarlyB [main=EarlyB]: assert Fencing in { EarlyB, Scenario, Client, Store };
