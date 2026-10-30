// Model: lock service with fencing tokens. The lock is one record; clients race with
// arbitrary (out-of-order) times. Resource accepts a write only if the token was issued
// and is not lower than the highest accepted token. Spec: accepted tokens never decrease.
type tLease = (holder: int, token: int, expiry: int);
event eAcquire: (client: int, now: int, from: machine);
event eAcquired: int;               // token, or -1
event eWrite: (token: int, from: machine);
event eWriteOk: bool;
event eAccepted: int;
event eGo: (lock: machine, res: machine, id: int);

machine Lock {
  var last: int; var hasLease: bool; var lease: tLease;
  start state Serve {
    on eAcquire do (a: (client: int, now: int, from: machine)) {
      if (hasLease && a.now < lease.expiry) {
        if (lease.holder == a.client) { send a.from, eAcquired, lease.token; }
        else { send a.from, eAcquired, -1; }
        return;
      }
      last = last + 1;
      lease = (holder = a.client, token = last, expiry = a.now + 5);
      hasLease = true;
      send a.from, eAcquired, last;
    }
  }
}

machine Resource {
  var maxTok: int; var lock: machine;
  start state Init { entry (l: machine) { lock = l; goto Serve; } }
  state Serve {
    on eWrite do (w: (token: int, from: machine)) {
      // Issued tokens are 1..last; this model learns 'issued' from the clients' honest
      // tokens plus one forged token (0), which must be rejected.
      if (w.token < 1 || w.token < maxTok) { send w.from, eWriteOk, false; return; }
      maxTok = w.token;
      announce eAccepted, w.token;
      send w.from, eWriteOk, true;
    }
  }
}

machine Client {
  var lock: machine; var res: machine; var id: int;
  start state Init {
    entry (c: (lock: machine, res: machine, id: int)) {
      lock = c.lock; res = c.res; id = c.id;
      send lock, eAcquire, (client = id, now = choose(12), from = this);
      goto Wait;
    }
  }
  state Wait {
    on eAcquired do (t: int) {
      if (t > 0) { send res, eWrite, (token = t, from = this); }
      else { send res, eWrite, (token = 0, from = this); }
      goto Done;
    }
  }
  state Done { ignore eWriteOk; }
}

spec FencingMonotonic observes eAccepted {
  var maxTok: int;
  start state S {
    on eAccepted do (t: int) {
      assert t >= maxTok, format("accepted token {0} after {1}", t, maxTok);
      maxTok = t;
    }
  }
}

machine Main {
  start state Init {
    entry {
      var l: machine; var r: machine; var i: int;
      l = new Lock(); r = new Resource(l);
      while (i < 3) { new Client((lock = l, res = r, id = i)); i = i + 1; }
    }
  }
}

test tcFencing [main=Main]: assert FencingMonotonic in { Main, Lock, Resource, Client };
