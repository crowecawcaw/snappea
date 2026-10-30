// Model of the Registry algorithm over a per-key CAS store.
// Keys: 0..1 are names, 2..3 are users (user u uses key 2+u). Values: -1 = none, else int.
event eLoad: (client: machine, key: int);
event eLoadR: (version: int, value: int);
event eCas: (client: machine, key: int, version: int, value: int);
event eCasR: (ok: bool);
event eCommit: (key: int, value: int);
event eDone;
event eQuiet;
event eResult: (user: int, op: int, name: int, ok: bool);

// Contract: a name never has two holders; at quiescence name and user records agree.
spec Contract observes eCommit, eQuiet {
  var rec: map[int, int];
  start state S {
    entry { var i: int; while (i < 4) { rec[i] = -1; i = i + 1; } }
    on eCommit do (p: (key: int, value: int)) {
      var k: int;
      if (p.key < 2 && p.value >= 0) {
        assert rec[p.key] == -1 || rec[p.key] == p.value, "name claimed while held by another user";
      }
      rec[p.key] = p.value;
    }
    on eQuiet do {
      var n: int; var u: int;
      n = 0;
      while (n < 2) {
        if (rec[n] >= 0) { assert rec[2 + rec[n]] == n, "owner(n)=u but name_of(u)!=n"; }
        n = n + 1;
      }
      u = 0;
      while (u < 2) {
        if (rec[2 + u] >= 0) { assert rec[rec[2 + u]] == u, "name_of(u)=n but owner(n)!=u"; }
        u = u + 1;
      }
    }
  }
}


// Contract (sequential use): results follow the rules in the task, computed from the inputs.
spec Sequential observes eResult {
  var holder: map[int, int];
  var held: map[int, int];
  start state S {
    entry { var i: int; while (i < 2) { holder[i] = -1; held[i] = -1; i = i + 1; } }
    on eResult do (p: (user: int, op: int, name: int, ok: bool)) {
      var expect: bool;
      expect = false;
      if (p.op == 1) {   // rename
        if (held[p.user] >= 0 && (held[p.user] == p.name || holder[p.name] == -1)) { expect = true; }
        if (expect) { holder[held[p.user]] = -1; holder[p.name] = p.user; held[p.user] = p.name; }
      } else {           // register
        if (held[p.user] == p.name) { expect = true; }
        else if (held[p.user] == -1 && holder[p.name] == -1) {
          expect = true; holder[p.name] = p.user; held[p.user] = p.name;
        }
      }
      assert p.ok == expect, "result disagrees with the contract";
    }
  }
}

machine Store {
  var ver: map[int, int];
  var val: map[int, int];
  start state Serving {
    entry { var i: int; while (i < 4) { ver[i] = 0; val[i] = -1; i = i + 1; } }
    on eLoad do (p: (client: machine, key: int)) {
      send p.client, eLoadR, (version = ver[p.key], value = val[p.key]);
    }
    on eCas do (p: (client: machine, key: int, version: int, value: int)) {
      if (ver[p.key] == p.version) {
        ver[p.key] = p.version + 1; val[p.key] = p.value;
        announce eCommit, (key = p.key, value = p.value);
        send p.client, eCasR, (ok = true,);
      } else {
        send p.client, eCasR, (ok = false,);
      }
    }
  }
}

// Each client is one user (id 0 or 1) that registers a name, then renames to another.
machine Client {
  var store: Store;
  var boss: machine;
  var me: int;
  start state Run {
    entry (a: (store: Store, boss: machine, me: int)) {
      var n: int; var ok: bool; var op: int;
      store = a.store; boss = a.boss; me = a.me;
      op = 0;
      while (op < 3) {
        n = choose(2);
        if (op == 1) { ok = Rename(n); } else { ok = Register(n); }
        announce eResult, (user = me, op = op, name = n, ok = ok);
        op = op + 1;
      }
      send boss, eDone;
    }
  }
  fun Load(key: int): (version: int, value: int) {
    var r: (version: int, value: int);
    send store, eLoad, (client = this, key = key);
    receive { case eLoadR: (x: (version: int, value: int)) { r = x; } }
    return r;
  }
  fun Cas(key: int, version: int, value: int): bool {
    var r: bool;
    send store, eCas, (client = this, key = key, version = version, value = value);
    receive { case eCasR: (x: (ok: bool)) { r = x.ok; } }
    return r;
  }
  // Only a CAS from "free" can claim a name. A failed CAS means someone changed the
  // record, so reload and decide again.
  fun Claim(name: int): bool {
    var r: (version: int, value: int);
    while (true) {
      r = Load(name);
      if (r.value >= 0) { return false; }
      if (Cas(name, r.version, me)) { return true; }
    }
    return false;
  }
  // Only this user's calls write the user record, and they never overlap, so one CAS suffices.
  fun SetUser(value: int) {
    var r: (version: int, value: int);
    var ok: bool;
    r = Load(2 + me);
    ok = Cas(2 + me, r.version, value);
    assert ok, "user record changed under its own user";
  }
  // We hold the name, so nobody else can change it.
  fun Release(name: int) {
    var r: (version: int, value: int);
    var ok: bool;
    r = Load(name);
    assert r.value == me;
    ok = Cas(name, r.version, -1);
    assert ok, "held name changed under its holder";
  }
  fun Register(name: int): bool {
    var cur: (version: int, value: int);
    cur = Load(2 + me);
    if (cur.value >= 0) { return cur.value == name; }
    if (!Claim(name)) { return false; }
    SetUser(name);
    return true;
  }
  fun Rename(name: int): bool {
    var cur: (version: int, value: int);
    cur = Load(2 + me);
    if (cur.value < 0) { return false; }
    if (cur.value == name) { return true; }
    if (!Claim(name)) { return false; }
    SetUser(name);
    Release(cur.value);
    return true;
  }
}

machine Main {
  start state Init {
    entry {
      var s: Store;
      s = new Store();
      new Client((store = s, boss = this, me = 0));
      new Client((store = s, boss = this, me = 1));
      receive { case eDone: { } }
      receive { case eDone: { } }
      announce eQuiet;
    }
  }
}

// Same clients, one after the other: results are exactly determined.
machine SeqMain {
  start state Init {
    entry {
      var s: Store;
      s = new Store();
      new Client((store = s, boss = this, me = 0));
      receive { case eDone: { } }
      new Client((store = s, boss = this, me = 1));
      receive { case eDone: { } }
      announce eQuiet;
    }
  }
}

test tcTwoUsers [main=Main]: assert Contract in { Main, Store, Client };
test tcSequential [main=SeqMain]: assert Contract, Sequential in { SeqMain, Store, Client };
