// Model: lock record {issued, holder, token, expires} in one CAS key.
// Grant/Release are announced when the CAS commits.
type tLock = (issued: int, holder: int, token: int, expires: int);
event eLoad: (c: machine);
event eLoaded: (ver: int, rec: tLock);
event eCas: (c: machine, ver: int, rec: tLock, kind: int, client: int, now: int);
event eCasR: (ok: bool);
event eGrant: (client: int, token: int, now: int, expires: int);
event eRelease: (client: int, token: int, now: int);
event eDone;

spec Safety observes eGrant, eRelease, eDone {
  var lastTok: int; var lastCl: int; var lastExp: int; var live: bool;
  start state S {
    on eGrant do (g: (client: int, token: int, now: int, expires: int)) {
      assert g.token > lastTok, "tokens must strictly increase";
      assert !live || g.now >= lastExp, "granted while another lease unexpired";
      lastTok = g.token; lastCl = g.client; lastExp = g.expires; live = true;
    }
    on eRelease do (r: (client: int, token: int, now: int)) {
      assert r.client == lastCl && r.token == lastTok && live && r.now < lastExp, "bad release";
      live = false;
    }
  }
}

machine Store {
  var ver: int; var rec: tLock;
  start state Run {
    on eLoad do (p: (c: machine)) { send p.c, eLoaded, (ver = ver, rec = rec); }
    on eCas do (p: (c: machine, ver: int, rec: tLock, kind: int, client: int, now: int)) {
      if (p.ver == ver) {
        ver = ver + 1; rec = p.rec;
        if (p.kind == 1) { announce eGrant, (client = p.client, token = p.rec.token, now = p.now, expires = p.rec.expires); }
        else { announce eRelease, (client = p.client, token = p.rec.token, now = p.now); }
        send p.c, eCasR, (ok = true,); }
      else { send p.c, eCasR, (ok = false,); }
    }
  }
}

machine Client {
  var id: int;
  start state Run {
    entry (a: (s: Store, id: int)) {
      var i: int; var now: int; var done: bool; var v: int; var r: tLock;
      var ok: bool; var nr: tLock; var tok: int;
      id = a.id + 1;
      while (i < 2) {
        now = choose(6);
        done = false;
        // acquire, ttl = 2
        while (!done) {
          send a.s, eLoad, (c = this,);
          receive { case eLoaded: (l: (ver: int, rec: tLock)) { v = l.ver; r = l.rec; } }
          if (r.holder != 0 && now < r.expires) { done = true; tok = 0; }
          else {
            nr = (issued = r.issued + 1, holder = id, token = r.issued + 1, expires = now + 2);
            send a.s, eCas, (c = this, ver = v, rec = nr, kind = 1, client = id, now = now);
            receive { case eCasR: (x: (ok: bool)) { ok = x.ok; } }
            if (ok) { done = true; tok = nr.token; }
          }
        }
        // release if granted
        if (tok != 0) {
          now = now + choose(3);
          done = false;
          while (!done) {
            send a.s, eLoad, (c = this,);
            receive { case eLoaded: (l: (ver: int, rec: tLock)) { v = l.ver; r = l.rec; } }
            if (r.holder != id || r.token != tok || now >= r.expires) { done = true; }
            else {
              nr = (issued = r.issued, holder = 0, token = r.token, expires = r.expires);
              send a.s, eCas, (c = this, ver = v, rec = nr, kind = 2, client = id, now = now);
              receive { case eCasR: (x: (ok: bool)) { ok = x.ok; } }
              if (ok) { done = true; }
            }
          }
        }
        i = i + 1;
      }
    }
  }
}

machine Main {
  start state Init {
    entry {
      var s: Store;
      s = new Store();
      new Client((s = s, id = 0));
      new Client((s = s, id = 1));
    }
  }
}
test tcFencing [main=Main]: assert Safety in { Main, Store, Client };
