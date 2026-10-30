type Rec = (last: int, holder: int, token: int, expires: int);
type Snap = (version: int, rec: Rec);

event eLoad: machine;
event eLoaded: Snap;
event eCas: (client: machine, cid: int, kind: int, token: int, now: int, expected: int, rec: Rec);
event eCasResult: bool;
event eCommit: (version: int, kind: int, cid: int, token: int, now: int, rec: Rec);
event eReadOnly: (version: int, kind: int, cid: int, token: int, now: int, result: int);
event eClientDone;
event eReturned: (cid: int, result: int);
event eAllDone;

fun Held(r: Rec, now: int): bool {
  return r.holder != -1 && now < r.expires;
}

spec LockContract observes eCommit, eReadOnly, eReturned, eAllDone {
  var recs: map[int, Rec];
  var outcome: map[int, int];
  start hot state Watching {
    on eAllDone do { goto Finished; }
    on eReturned do (p: (cid: int, result: int)) {
      assert p.cid in outcome, "client returned without any observed outcome";
      assert p.result == outcome[p.cid], "returned value differs from the observed commit/read";
    }
    entry { recs[0] = (last = 0, holder = -1, token = 0, expires = 0); }
    on eCommit do (p: (version: int, kind: int, cid: int, token: int, now: int, rec: Rec)) {
      var prior: Rec;
      assert p.version - 1 in recs, "commit out of order";
      prior = recs[p.version - 1];
      if (p.kind == 0) {
        assert !Held(prior, p.now), "granted while another unexpired lease exists";
        assert p.rec.token == prior.last + 1, "token must be previous last + 1";
        assert p.rec.last == p.rec.token, "last must track the newest token";
        assert p.rec.holder == p.cid;
        assert p.rec.expires == p.now + 2, "lease expires at now + ttl";
      } else {
        assert Held(prior, p.now) && prior.holder == p.cid && prior.token == p.token,
          "release succeeded without holding an unexpired lease with that token";
        assert p.rec.holder == -1 && p.rec.last == prior.last, "release must only clear the holder";
      }
      recs[p.version] = p.rec;
      if (p.kind == 0) { outcome[p.cid] = p.rec.token; } else { outcome[p.cid] = 1; }
    }
    on eReadOnly do (p: (version: int, kind: int, cid: int, token: int, now: int, result: int)) {
      var r: Rec;
      r = recs[p.version];
      if (p.kind == 0) {
        // no commit happened: the caller must have been refused or handed its own lease
        assert Held(r, p.now), "acquire returned without commit although lease was free";
        if (r.holder == p.cid) { assert p.result == r.token, "same client must get same token"; }
        else { assert p.result == -1, "acquire must fail while another holds the lease"; }
        outcome[p.cid] = p.result;
      } else {
        assert !(Held(r, p.now) && r.holder == p.cid && r.token == p.token),
          "release refused although caller holds the lease";
        assert p.result == 0;
        outcome[p.cid] = 0;
      }
    }
  }
  state Finished { }
}

machine Store {
  var version: int;
  var rec: Rec;
  start state Serving {
    entry { rec = (last = 0, holder = -1, token = 0, expires = 0); }
    on eLoad do (c: machine) { send c, eLoaded, (version = version, rec = rec); }
    on eCas do (p: (client: machine, cid: int, kind: int, token: int, now: int, expected: int, rec: Rec)) {
      if (p.expected == version) {
        version = version + 1;
        rec = p.rec;
        announce eCommit, (version = version, kind = p.kind, cid = p.cid, token = p.token, now = p.now, rec = rec);
        send p.client, eCasResult, true;
      } else {
        send p.client, eCasResult, false;
      }
    }
  }
}

machine Client {
  var store: Store;
  var cid: int;
  var coordinator: machine;
  var tok: int;
  start state Run {
    entry (input: (store: Store, cid: int, coordinator: machine, relTok: int)) {
      var r: bool;
      store = input.store; cid = input.cid; coordinator = input.coordinator;
      tok = Acquire(choose(3));
      announce eReturned, (cid = cid, result = tok);
      if (input.relTok >= 0) { tok = input.relTok; }
      r = Release(tok, choose(3));
      announce eReturned, (cid = cid, result = ReleaseCode(r));
      if (cid == 0) { tok = Acquire(choose(3)); announce eReturned, (cid = cid, result = tok); }
      send coordinator, eClientDone;
    }
  }

  fun ReleaseCode(b: bool): int { if (b) { return 1; } return 0; }

  fun Acquire(now: int): int {
    var s: Snap;
    var next: Rec;
    var ok: bool;
    var res: int;
    var fin: bool;
    while (!fin) {
      send store, eLoad, this;
      receive { case eLoaded: (x: Snap) { s = x; } }
      if (Held(s.rec, now)) {
        if (s.rec.holder == cid) { announce eReadOnly, (version = s.version, kind = 0, cid = cid, token = s.rec.token, now = now, result = s.rec.token); res = s.rec.token; fin = true; }
        else {
        announce eReadOnly, (version = s.version, kind = 0, cid = cid, token = 0, now = now, result = -1);
        res = -1; fin = true; }
      } else {
      next = (last = s.rec.last + 1, holder = cid, token = s.rec.last + 1, expires = now + 2);
      send store, eCas, (client = this, cid = cid, kind = 0, token = 0, now = now, expected = s.version, rec = next);
      receive { case eCasResult: (b: bool) { ok = b; } }
      if (ok) { res = next.token; fin = true; }
      }
    }
    return res;
  }

  fun Release(token: int, now: int): bool {
    var s: Snap;
    var next: Rec;
    var ok: bool;
    var res: bool;
    var fin: bool;
    while (!fin) {
      send store, eLoad, this;
      receive { case eLoaded: (x: Snap) { s = x; } }
      if (!(Held(s.rec, now) && s.rec.holder == cid && s.rec.token == token)) {
        announce eReadOnly, (version = s.version, kind = 1, cid = cid, token = token, now = now, result = 0);
        fin = true;
      } else {
      next = (last = s.rec.last, holder = -1, token = s.rec.token, expires = s.rec.expires);
      send store, eCas, (client = this, cid = cid, kind = 1, token = token, now = now, expected = s.version, rec = next);
      receive { case eCasResult: (b: bool) { ok = b; } }
      if (ok) { res = true; fin = true; }
      }
    }
    return res;
  }
}

// snappea: driver
machine Scenario {
  var done: int;
  start state Run {
    entry (relTok: int) {
      var s: Store;
      s = new Store();
      new Client((store = s, cid = 0, coordinator = this, relTok = -1));
      new Client((store = s, cid = 1, coordinator = this, relTok = relTok));
    }
    on eClientDone do { done = done + 1; if (done == 2) { announce eAllDone; } }
  }
}

// snappea: driver
machine Generated { start state Init { entry { new Scenario(-1); } } }
// client 1 releases using a token it may not own (1 or 2) to hit wrong-token / wrong-client paths
// snappea: driver
machine WrongToken { start state Init { entry { new Scenario(1 + choose(2)); } } }

test tcLock [main=Generated]: assert LockContract in { Generated, Scenario, Client, Store };
test tcWrongToken [main=WrongToken]: assert LockContract in { WrongToken, Scenario, Client, Store };
