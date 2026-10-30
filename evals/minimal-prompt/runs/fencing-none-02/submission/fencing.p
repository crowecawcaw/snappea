// Model of the lock service with fencing tokens.
// Lock record and resource record are separate keys, each updated atomically (CAS).
type tAcq = (client: int, now: int);
type tWrite = (token: int, value: int);

event eAcquire: (client: int, now: int, src: machine);
event eAcquired: int;           // token, or 0 for none
event eWrite: (token: int, value: int, src: machine);
event eWriteResult: bool;
event eAccepted: (token: int);
event eCount: machine;
event eCountIs: int;
event eIssued: (token: int);

machine Lock {
  var counter: int; var holder: int; var token: int; var expiry: int; var ttl: int;
  start state Init {
    entry { ttl = 3; holder = 0; }
    on eCount do (src: machine) { send src, eCountIs, counter; }
    on eAcquire do (r: (client: int, now: int, src: machine)) {
      if (holder != 0 && r.now < expiry) {
        if (holder == r.client) { send r.src, eAcquired, token; }
        else { send r.src, eAcquired, 0; }
      } else {
        counter = counter + 1; holder = r.client; token = counter; expiry = r.now + ttl;
        announce eIssued, (token = token,);
        send r.src, eAcquired, token;
      }
    }
  }
}

machine Resource {
  var maxTok: int; var lock: machine;
  start state Init {
    entry (l: machine) { lock = l; }
    on eWrite do (w: (token: int, value: int, src: machine)) {
      var n: int;
      send lock, eCount, this;
      receive { case eCountIs: (c: int) { n = c; } }
      if (w.token >= 1 && w.token <= n && w.token >= maxTok) {
        maxTok = w.token; announce eAccepted, (token = w.token,);
        send w.src, eWriteResult, true;
      } else { send w.src, eWriteResult, false; }
    }
  }
}

machine Client {
  var lock: machine; var res: machine; var id: int;
  start state Run {
    entry (a: (lock: machine, res: machine, id: int)) {
      var t: int; var i: int; var ok: bool;
      lock = a.lock; res = a.res; id = a.id;
      while (i < 3) {
        send lock, eAcquire, (client = id, now = choose(8), src = this);
        receive { case eAcquired: (x: int) { t = x; } }
        if (t > 0) { send res, eWrite, (token = t, value = id, src = this);
          receive { case eWriteResult: (b: bool) { ok = b; } } }
        // a never-issued token must be rejected
        send res, eWrite, (token = 100, value = id, src = this);
        receive { case eWriteResult: (b: bool) { assert !b, "unissued token accepted"; } }
        i = i + 1;
      }
    }
  }
}

// Accepted write tokens are never lower than an earlier accepted one, and were issued.
spec FencingMonotone observes eIssued, eAccepted {
  var maxIssued: int; var maxAccepted: int;
  start state S {
    on eIssued do (e: (token: int)) { assert e.token > maxIssued, "tokens must strictly increase"; maxIssued = e.token; }
    on eAccepted do (e: (token: int)) {
      assert e.token <= maxIssued, "accepted a token never issued";
      assert e.token >= maxAccepted, "accepted a stale token";
      maxAccepted = e.token;
    }
  }
}

machine Main { start state S { entry {
  var l: machine; var r: machine;
  l = new Lock(); r = new Resource(l);
  new Client((lock = l, res = r, id = 1)); new Client((lock = l, res = r, id = 2));
} } }

test tcFencing [main=Main]: assert FencingMonotone in { Main, Lock, Resource, Client };
