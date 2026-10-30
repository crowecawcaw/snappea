// Model: per-key load/CAS store; claim name record, then set user record, then free old name.
// Two users (0,1), two names (0,1); each user does register then rename with generated names.
// Bounds: no crashes; calls of one user sequential. Invariant at quiescence: owner/name_of inverse.
type Rec = (ver: int, val: int);   // val -1 = none
event eLoad: (c: machine, key: int);
event eLoaded: Rec;
event eCas: (c: machine, key: int, ver: int, val: int);
event eCasR: bool;
event eOp: (user: int, name: int, isReg: bool, ok: bool);
event eCommit: (key: int, val: int);   // keys 0..1 = names, 10+u = users
event eIdle; event eQuiet;

spec Unique observes eCommit, eQuiet {
  var names: map[int, int];
  var users: map[int, int];
  start state S {
    entry { names[0] = -1; names[1] = -1; users[0] = -1; users[1] = -1; }
    on eCommit do (p: (key: int, val: int)) {
      var u: int;
      if (p.key < 10) { names[p.key] = p.val; }
      else {
        users[p.key - 10] = p.val;
        // a user's name must be claimed by that user at the time it is set
        if (p.val >= 0) { assert names[p.val] == p.key - 10, "user set name it does not hold"; }
      }
      if (users[0] >= 0 && users[1] >= 0) { assert users[0] != users[1], "two users share a name"; }
    }
    on eQuiet do {
      var n: int;
      n = 0;
      while (n < 2) {
        if (names[n] >= 0) { assert users[names[n]] == n, "owner without name_of"; }
        n = n + 1;
      }
      n = 0;
      while (n < 2) {
        if (users[n] >= 0) { assert names[users[n]] == n, "name_of without owner"; }
        n = n + 1;
      }
    }
  }
}

machine Store {
  var vers: map[int, int];
  var vals: map[int, int];
  start state R {
    entry {
      var k: int;
      k = 0;
      while (k < 12) { vers[k] = 0; vals[k] = -1; k = k + 1; }
    }
    on eLoad do (p: (c: machine, key: int)) { send p.c, eLoaded, (ver = vers[p.key], val = vals[p.key]); }
    on eCas do (p: (c: machine, key: int, ver: int, val: int)) {
      if (vers[p.key] == p.ver) {
        vers[p.key] = p.ver + 1; vals[p.key] = p.val;
        announce eCommit, (key = p.key, val = p.val);
        send p.c, eCasR, true;
      } else { send p.c, eCasR, false; }
    }
  }
}

machine User {
  var st: Store; var me: int; var co: machine;
  start state Run {
    entry (a: (st: Store, me: int, co: machine)) {
      var n: int;
      st = a.st; me = a.me; co = a.co;
      n = choose(2);
      doReg(n);
      doRename(choose(2));
      send co, eIdle;
    }
  }
  fun load(k: int): Rec {
    var r: Rec;
    send st, eLoad, (c = this, key = k);
    receive { case eLoaded: (x: Rec) { r = x; } }
    return r;
  }
  fun cas(k: int, v: int, x: int): bool {
    var b: bool;
    send st, eCas, (c = this, key = k, ver = v, val = x);
    receive { case eCasR: (x2: bool) { b = x2; } }
    return b;
  }
  // claim name n for me; false if held by another
  fun claim(n: int): bool {
    var r: Rec; var done: bool; var res: bool;
    while (!done) {
      r = load(n);
      if (r.val == me) { res = true; done = true; }
      else if (r.val >= 0) { res = false; done = true; }
      else if (cas(n, r.ver, me)) { res = true; done = true; }
    }
    return res;
  }
  fun setUser(n: int) {
    var r: Rec; var ok: bool;
    r = load(10 + me);
    ok = cas(10 + me, r.ver, n);
    assert ok, "same-user calls never overlap, user CAS must win";
  }
  fun release(n: int) {
    var r: Rec; var ok: bool;
    r = load(n);
    assert r.val == me;
    ok = cas(n, r.ver, -1);
    assert ok, "only the holder changes a held name";
  }
  fun doReg(n: int) {
    var u: Rec;
    u = load(10 + me);
    if (u.val >= 0) { return; }
    if (claim(n)) { setUser(n); }
  }
  fun doRename(n: int) {
    var u: Rec;
    u = load(10 + me);
    if (u.val < 0 || u.val == n) { return; }
    if (claim(n)) { setUser(n); release(u.val); }
  }
}

machine Main {
  var done: int;
  start state I {
    entry {
      var s: Store;
      s = new Store();
      new User((st = s, me = 0, co = this));
      new User((st = s, me = 1, co = this));
    }
    on eIdle do { done = done + 1; if (done == 2) { announce eQuiet; } }
  }
}
test tcNames [main=Main]: assert Unique in { Main, User, Store };
