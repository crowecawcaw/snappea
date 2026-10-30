// Model of LockService + Resource over a CAS store (lock record: issued, holder, token, expiry).
type LockRec = (issued: int, holder: int, token: int, expiry: int); // holder -1 = none
event eLoad: (from: machine);
event eLoadR: (ver: int, rec: LockRec);
event eCas: (from: machine, ver: int, rec: LockRec);
event eCasR: (ok: bool);
event eCommit: (client: int, token: int, expiry: int);
event eSeen: (client: int, now: int, token: int, own: bool); // outcome returned to caller; token 0 = None, else a live lease seen
event eWrite: (token: int, ok: bool);

// Contract: committed tokens strictly increase; a new lease is committed only after the previous
// one expired (expiry-10 is the grant time); a refusal or repeat result names a committed live lease.
spec Fencing observes eCommit, eSeen {
  var last: int; var expiry: int; var holders: map[int, int]; var expiries: map[int, int];
  start state S {
    on eCommit do (p: (client: int, token: int, expiry: int)) {
      assert p.token > last, "token not increasing";
      assert p.expiry - 10 >= expiry, "granted while previous lease live";
      last = p.token; expiry = p.expiry; holders[p.token] = p.client; expiries[p.token] = p.expiry;
    }
    on eSeen do (p: (client: int, now: int, token: int, own: bool)) {
      assert p.token in holders, "unknown token";
      assert p.now < expiries[p.token], "returned expired lease";
      assert (holders[p.token] == p.client) == p.own, "own/other lease mismatch";
    }
  }
}

machine Store {
  var ver: int; var rec: LockRec;
  start state Run {
    entry { rec = (issued = 0, holder = -1, token = 0, expiry = 0); }
    on eLoad do (p: (from: machine)) { send p.from, eLoadR, (ver = ver, rec = rec); }
    on eCas do (p: (from: machine, ver: int, rec: LockRec)) {
      if (p.ver == ver) { ver = ver + 1; rec = p.rec; announce eCommit, (client = p.rec.holder, token = p.rec.token, expiry = p.rec.expiry); send p.from, eCasR, (ok = true,); }
      else { send p.from, eCasR, (ok = false,); }
    }
  }
}

machine Client {
  var id: int;
  start state Run {
    entry (a: (s: Store, id: int)) {
      var r: LockRec; var v: int; var done: bool; var round: int; var now: int; var tok: int; var ok: bool;
      id = a.id;
      while (round < 2) {
      now = choose(3) * 4; done = false;
      while (!done) {
        send a.s, eLoad, (from = this,);
        receive { case eLoadR: (p: (ver: int, rec: LockRec)) { v = p.ver; r = p.rec; } }
        if (r.holder != -1 && now < r.expiry) {
          if (r.holder == id) { tok = r.token; announce eSeen, (client = id, now = now, token = tok, own = true); } else { assert r.token > 0; tok = 0; announce eSeen, (client = id, now = now, token = r.token, own = false); }
          done = true;
        } else {
          tok = r.issued + 1;
          send a.s, eCas, (from = this, ver = v, rec = (issued = tok, holder = id, token = tok, expiry = now + 10));
          receive { case eCasR: (c: (ok: bool)) { ok = c.ok; } }
          done = ok;
        }
      }
      round = round + 1;
      }
    }
  }
}

machine Main {
  start state Init {
    entry { var s: Store; s = new Store(); new Client((s = s, id = 1)); new Client((s = s, id = 2)); }
  }
}
test tcAcquire [main=Main]: assert Fencing in { Main, Store, Client };
