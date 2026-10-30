// Model of the load/CAS lock + fencing algorithm. Store is per-key atomic load/CAS.
type Rec = (v: int, last: int, holder: int, token: int, expiry: int);
event eLoadLock: (c: machine);
event eLockRec: Rec;
event eCasLock: (c: machine, ver: int, last: int, holder: int, token: int, expiry: int);
event eCasLockR: (ok: bool);
event eLoadRes: (c: machine);
event eResRec: (v: int, mx: int);
event eCasRes: (c: machine, ver: int, mx: int);
event eCasResR: (ok: bool);
event eGranted: (token: int);
event eAccepted: (token: int);
event eDone;

// Contract: tokens strictly increase/unique; accepted writes use issued tokens
// and never decrease.
spec Fencing observes eGranted, eAccepted {
  var issued: set[int];
  var last: int;
  var maxAcc: int;
  start state S {
    on eGranted do (p: (token: int)) {
      assert !(p.token in issued), "token reused";
      assert p.token > last, "tokens not increasing";
      last = p.token; issued += (p.token);
    }
    on eAccepted do (p: (token: int)) {
      assert p.token in issued, "accepted never-issued token";
      assert p.token >= maxAcc, "accepted stale token";
      maxAcc = p.token;
    }
  }
}

machine Store {
  var lv, last, holder, token, expiry, rv, rmx: int;
  start state Serve {
    on eLoadLock do (p: (c: machine)) {
      send p.c, eLockRec, (v = lv, last = last, holder = holder, token = token, expiry = expiry);
    }
    on eCasLock do (p: (c: machine, ver: int, last: int, holder: int, token: int, expiry: int)) {
      if (p.ver == lv) {
        lv = lv + 1; last = p.last; holder = p.holder; token = p.token; expiry = p.expiry;
        if (p.holder != 0) announce eGranted, (token = p.token,);
        send p.c, eCasLockR, (ok = true,);
      } else send p.c, eCasLockR, (ok = false,);
    }
    on eLoadRes do (p: (c: machine)) { send p.c, eResRec, (v = rv, mx = rmx); }
    on eCasRes do (p: (c: machine, ver: int, mx: int)) {
      if (p.ver == rv) {
        rv = rv + 1; rmx = p.mx;
        announce eAccepted, (token = p.mx,);
        send p.c, eCasResR, (ok = true,);
      } else send p.c, eCasResR, (ok = false,);
    }
  }
}

// ttl = 2. Client id 1..n. Acquire, then fenced write, with generated times.
machine Client {
  var id: int;
  var s: Store;
  start state Run {
    entry (a: (id: int, s: Store)) {
      var r: Rec; var ok: bool; var now: int; var tok: int; var got: bool;
      var rr: (v: int, mx: int); var round: int;
      id = a.id; s = a.s;
      round = 0;
      while (round < 2) {
      now = choose(4);
      got = false;
      while (!got) {
        send s, eLoadLock, (c = this,);
        receive { case eLockRec: (x: Rec) { r = x; } }
        if (r.holder != 0 && now < r.expiry) {
          if (r.holder == id) { tok = r.token; } else { tok = 0; }
          got = true;
        } else {
          send s, eCasLock, (c = this, ver = r.v, last = r.last + 1, holder = id, token = r.last + 1, expiry = now + 2);
          receive { case eCasLockR: (x: (ok: bool)) { ok = x.ok; } }
          if (ok) { tok = r.last + 1; got = true; }
        }
      }
      // Fenced write with held token, or an arbitrary (possibly never-issued) token.
      if (tok == 0 || $) tok = choose(3);
      while (tok > 0) {
        send s, eLoadRes, (c = this,);
        receive { case eResRec: (x: (v: int, mx: int)) { rr = x; } }
        if (tok < rr.mx) break;
        send s, eLoadLock, (c = this,);
        receive { case eLockRec: (x: Rec) { r = x; } }
        if (tok > r.last) break;
        send s, eCasRes, (c = this, ver = rr.v, mx = tok);
        receive { case eCasResR: (x: (ok: bool)) { ok = x.ok; } }
        if (ok) break;
      }
      if ($) {  // release (clean) if still the unexpired holder
        send s, eLoadLock, (c = this,);
        receive { case eLockRec: (x: Rec) { r = x; } }
        if (r.holder == id && now < r.expiry) {
          send s, eCasLock, (c = this, ver = r.v, last = r.last, holder = 0, token = r.token, expiry = r.expiry);
          receive { case eCasLockR: (x: (ok: bool)) { ok = x.ok; } }
        }
      }
      round = round + 1;
      }
      announce eDone;
    }
  }
}

machine Main {
  start state Init {
    entry {
      var s: Store;
      s = new Store();
      new Client((id = 1, s = s));
      new Client((id = 2, s = s));
    }
  }
}
test tcFencing [main=Main]: assert Fencing in { Main, Store, Client };
