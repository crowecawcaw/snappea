// Model of the unique-username registry over a per-key load/CAS store.
// Keys: name n -> record key n (value = owner user or -1); user u -> key 100+u (value = name or -1).
type Rec = (ver: int, val: int);
event eLoad: (client: machine, key: int);
event eLoadR: (ver: int, val: int);
event eCas: (client: machine, key: int, ver: int, val: int);
event eCasR: (ok: bool);
event eDone;
event eAudit;
event eHeld: (name: int, user: int);
event eRes: (op: int, user: int, name: int, ok: bool);
event eFreed: (name: int, user: int);

// Contract: a name is never held by two users at once; a user releases only its own name.
spec Unique observes eHeld, eFreed, eRes {
  var holder: map[int, int];
  start state S {
    on eHeld do (p: (name: int, user: int)) {
      assert !(p.name in holder), "name granted while held by another user";
      holder[p.name] = p.user;
    }
    // snappea-lint: allow unused-payload-field -- op only matters to the Exact spec
    on eRes do (p: (op: int, user: int, name: int, ok: bool)) {
      if (p.ok) assert p.name in holder && holder[p.name] == p.user, "success reported but name not held by user";
    }
    on eFreed do (p: (name: int, user: int)) {
      assert p.name in holder && holder[p.name] == p.user, "released a name not held by this user";
      holder -= (p.name);
    }
  }
}

machine Store {
  var recs: map[int, Rec];
  start state S {
    entry {
      var n: int;
      n = 0;
      while (n < 2) { recs[n] = (ver = 0, val = -1); recs[100 + n] = (ver = 0, val = -1); n = n + 1; }
    }
    on eLoad do (p: (client: machine, key: int)) {
      send p.client, eLoadR, (ver = recs[p.key].ver, val = recs[p.key].val);
    }
    on eCas do (p: (client: machine, key: int, ver: int, val: int)) {
      var old: int;
      if (recs[p.key].ver == p.ver) {
        old = recs[p.key].val;
        recs[p.key] = (ver = p.ver + 1, val = p.val);
        if (p.key < 100 && p.val >= 0) announce eHeld, (name = p.key, user = p.val);
        if (p.key < 100 && p.val < 0) announce eFreed, (name = p.key, user = old);
        send p.client, eCasR, (ok = true,);
      } else {
        send p.client, eCasR, (ok = false,);
      }
    }
    on eAudit do {
      var n: int;
      n = 0;
      while (n < 2) {
        if (recs[n].val >= 0) assert recs[100 + recs[n].val].val == n, "owner(n)=u but name_of(u)!=n";
        if (recs[100 + n].val >= 0) assert recs[recs[100 + n].val].val == n, "name_of(u)=n but owner(n)!=u";
        n = n + 1;
      }
    }
  }
}

machine Client {
  var store: machine;
  start state Run {
    entry (p: (store: machine, user: int, first: int, second: int, boss: machine)) {
      var ok: bool;
      store = p.store;
      ok = Register(p.user, p.first);
      announce eRes, (op = 0, user = p.user, name = p.first, ok = ok);
      ok = Register(p.user, p.second);
      announce eRes, (op = 0, user = p.user, name = p.second, ok = ok);
      ok = Rename(p.user, p.second);
      announce eRes, (op = 1, user = p.user, name = p.second, ok = ok);
      ok = Rename(p.user, p.first);
      announce eRes, (op = 1, user = p.user, name = p.first, ok = ok);
      send p.boss, eDone;
    }
  }

  fun Load(key: int): Rec {
    var r: Rec;
    send store, eLoad, (client = this, key = key);
    receive { case eLoadR: (x: Rec) { r = x; } }
    return r;
  }
  fun Cas(key: int, ver: int, val: int): bool {
    var b: bool;
    send store, eCas, (client = this, key = key, ver = ver, val = val);
    receive { case eCasR: (x: (ok: bool)) { b = x.ok; } }
    return b;
  }
  fun Claim(name: int, u: int): bool {
    var r: Rec;
    var done: bool;
    var res: bool;
    while (!done) {
      r = Load(name);
      if (r.val == u) { res = true; done = true; }
      else if (r.val >= 0) { res = false; done = true; }
      else if (Cas(name, r.ver, u)) { res = true; done = true; }
    }
    return res;
  }
  fun Register(u: int, name: int): bool {
    var ur: Rec;
    var ok: bool;
    var w: bool;
    ur = Load(100 + u);
    if (ur.val >= 0 && ur.val != name) return false;
    ok = Claim(name, u);
    if (!ok) return false;
    if (ur.val < 0) { w = Cas(100 + u, ur.ver, name); assert w, "only u writes its own record"; }
    return true;
  }
  fun Rename(u: int, name: int): bool {
    var ur: Rec;
    var old: int;
    var ok: bool;
    var w: bool;
    var r: Rec;
    ur = Load(100 + u);
    if (ur.val < 0) return false;
    if (ur.val == name) return true;
    old = ur.val;
    ok = Claim(name, u);
    if (!ok) return false;
    w = Cas(100 + u, ur.ver, name); assert w, "only u writes its own record";
    r = Load(old);
    w = Cas(old, r.ver, -1); assert w, "only holder releases";
    return true;
  }
}

machine Main {
  start state Init {
    entry {
      var s: machine;
      s = new Store();
      new Client((store = s, user = 0, first = choose(2), second = choose(2), boss = this));
      new Client((store = s, user = 1, first = choose(2), second = choose(2), boss = this));
      receive { case eDone: { } }
      receive { case eDone: { } }
      send s, eAudit;
    }
  }
}

test tcRace [main=Main]: assert Unique in { Main, Store, Client };

// Exact expectations; valid only when operations run one at a time.
spec Exact observes eRes {
  var owner: map[int, int];
  var nameOf: map[int, int];
  start state S {
    on eRes do (p: (op: int, user: int, name: int, ok: bool)) {
      var expect: bool;
      var old: int;
      if (p.op == 0) {
        if (p.user in nameOf) expect = nameOf[p.user] == p.name;
        else expect = !(p.name in owner);
        if (expect && !(p.user in nameOf)) { nameOf[p.user] = p.name; owner[p.name] = p.user; }
        assert p.ok == expect, "register result differs from contract";
      } else {
        if (!(p.user in nameOf)) expect = false;
        else if (nameOf[p.user] == p.name) expect = true;
        else expect = !(p.name in owner);
        if (expect && nameOf[p.user] != p.name) {
          old = nameOf[p.user];
          owner -= (old);
          nameOf[p.user] = p.name; owner[p.name] = p.user;
        }
        assert p.ok == expect, "rename result differs from contract";
      }
    }
  }
}

machine SeqMain {
  start state Init {
    entry {
      var s: machine;
      s = new Store();
      new Client((store = s, user = 0, first = choose(2), second = choose(2), boss = this));
      receive { case eDone: { } }
      new Client((store = s, user = 1, first = choose(2), second = choose(2), boss = this));
      receive { case eDone: { } }
      send s, eAudit;
    }
  }
}
test tcSeq [main=SeqMain]: assert Exact in { SeqMain, Store, Client };
