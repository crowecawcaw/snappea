type Lease = (counter: int, holder: int, expires: int);
type Res = (maxTok: int, val: int);

event eLoadL: (c: Client);
event eLoadLR: (ver: int, lease: Lease);
event eCasL: (c: Client, ver: int, lease: Lease);
event eCasLR: (ok: bool);
event eLoadR: (c: Client);
event eLoadRR: (ver: int, res: Res);
event eCasR: (c: Client, ver: int, res: Res);
event eCasRR: (ok: bool);

// Announced at the commit point of each store write.
event eGranted: (client: int, token: int, now: int);
event eWritten: (token: int, issued: int);
event eDone;

spec Contract observes eGranted, eWritten, eDone {
  var lastToken: int;
  var lastWritten: int;
  var leaseEnd: int;
  start state S {
    on eGranted do (p: (client: int, token: int, now: int)) {
      assert p.token > lastToken, "tokens must strictly increase";
      assert p.client >= 1, "grant to unknown client";
      assert leaseEnd == 0 || p.now >= leaseEnd, "granted while another unexpired lease was held";
      lastToken = p.token;
      leaseEnd = p.now + 2;
    }
    on eWritten do (p: (token: int, issued: int)) {
      assert p.token >= lastWritten, "accepted write used a stale token";
      assert p.token >= 1 && p.token <= p.issued, "accepted write used a never-issued token";
      lastWritten = p.token;
    }
    on eDone do { }
  }
}

machine Store {
  var lver: int; var lease: Lease;
  var rver: int; var res: Res;
  start state Run {
    entry { lease = (counter = 0, holder = -1, expires = 0); res = (maxTok = 0, val = 0); }
    on eLoadL do (p: (c: Client)) { send p.c, eLoadLR, (ver = lver, lease = lease); }
    on eCasL do (p: (c: Client, ver: int, lease: Lease)) {
      if (p.ver == lver) {
        announce eGranted, (client = p.lease.holder, token = p.lease.counter, now = p.lease.expires - 2);
        lease = p.lease; lver = lver + 1;
        send p.c, eCasLR, (ok = true,);
      } else { send p.c, eCasLR, (ok = false,); }
    }
    on eLoadR do (p: (c: Client)) { send p.c, eLoadRR, (ver = rver, res = res); }
    on eCasR do (p: (c: Client, ver: int, res: Res)) {
      if (p.ver == rver) {
        announce eWritten, (token = p.res.maxTok, issued = lease.counter);
        res = p.res; rver = rver + 1;
        send p.c, eCasRR, (ok = true,);
      } else { send p.c, eCasRR, (ok = false,); }
    }
  }
}

machine Client {
  var store: Store; var id: int;
  start state Run {
    entry (a: (s: Store, id: int)) {
      var done: bool; var now: int; var ver: int; var l: Lease; var ok: bool;
      var r: Res; var tok: int; var k: int;
      store = a.s; id = a.id;
      k = 0;
      while (k < 2) {
        now = choose(4);
        // acquire (load, decide, cas, retry)
        done = false;
        while (!done) {
          send store, eLoadL, (c = this,);
          receive { case eLoadLR: (x: (ver: int, lease: Lease)) { ver = x.ver; l = x.lease; } }
          if (l.holder == id && now < l.expires) { tok = l.counter; done = true; }
          else if (l.holder == -1 || now >= l.expires) {
            l.counter = l.counter + 1; l.holder = id; l.expires = now + 2;
            send store, eCasL, (c = this, ver = ver, lease = l);
            receive { case eCasLR: (x: (ok: bool)) { ok = x.ok; } }
            if (ok) { tok = l.counter; done = true; }
          } else { tok = 0; done = true; }
        }
        if (tok > 0) {
          // fenced write
          done = false;
          while (!done) {
            send store, eLoadR, (c = this,);
            receive { case eLoadRR: (x: (ver: int, res: Res)) { ver = x.ver; r = x.res; } }
            if (tok < r.maxTok) { done = true; }
            else {
              r.maxTok = tok; r.val = id;
              send store, eCasR, (c = this, ver = ver, res = r);
              receive { case eCasRR: (x: (ok: bool)) { done = x.ok; } }
            }
          }
        }
        k = k + 1;
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
      new Client((s = s, id = 1));
      new Client((s = s, id = 2));
    }
  }
}

module Model = { Main, Store, Client };
test tcTwoClients [main=Main]: assert Contract in Model;
