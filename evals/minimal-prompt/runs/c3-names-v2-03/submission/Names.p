// Model of the username registry over a per-key load/compare-and-swap store.
// Keys: "name:<n>" holds the user (0 = free); "user:<u>" holds the name (0 = none).
// Only single-key CAS is atomic; there is no cross-key transaction.
type Rec = (version: int, value: int);
type Op = (isRegister: bool, name: int);

event eLoad: (client: machine, isName: bool, key: int);
event eLoaded: Rec;
event eCas: (client: machine, isName: bool, key: int, version: int, value: int);
event eCasDone: bool;
event eHold: (name: int, before: int, after: int);
event eReturned: (user: int, isRegister: bool, name: int, ok: bool, contended: bool);
event eClientDone;
event eDump;
event eFinal: (names: map[int, int], users: map[int, int]);

spec Contract observes eHold, eReturned, eFinal {
  var held: map[int, int];
  var own: map[int, int];
  var returns: int;
  start hot state Working {
    on eHold do (p: (name: int, before: int, after: int)) {
      var cur: int;
      if (p.name in held) { cur = held[p.name]; }
      assert cur == p.before, "name changed hands from a holder that was not the current holder";
      held[p.name] = p.after;
    }
    on eReturned do (p: (user: int, isRegister: bool, name: int, ok: bool, contended: bool)) {
      var mine: int;
      var doable: bool;
      if (p.user in own) { mine = own[p.user]; }
      returns = returns + 1;
      if (p.isRegister) {
        if (mine != 0) {
          assert p.ok == (mine == p.name), "register by a user who has a name must succeed only for that same name";
        } else {
          doable = true;
        }
      } else {
        if (mine == 0) {
          assert !p.ok, "rename by a user with no name must fail";
        } else if (mine == p.name) {
          assert p.ok, "renaming to the held name must succeed";
        } else {
          doable = true;
        }
      }
      if (doable) {
        if (!p.contended) { assert p.ok, "uncontended register/rename must succeed"; }
        if (p.ok) { own[p.user] = p.name; }
      }
    }
    on eFinal do (p: (names: map[int, int], users: map[int, int])) {
      var n: int;
      var u: int;
      var mine: int;
      assert returns == 4, "driver must complete all operations";
      foreach (n in keys(held)) {
        if (held[n] != 0) { assert n in p.names && p.names[n] == held[n], "store content differs from the transitions it announced"; }
      }
      foreach (n in keys(p.names)) {
        if (p.names[n] != 0) { assert n in held && held[n] == p.names[n], "store content differs from the transitions it announced"; }
      }
      foreach (n in keys(p.names)) {
        if (p.names[n] != 0) {
          assert p.names[n] in p.users, "owner(n)==u but name_of(u) is empty";
          assert p.users[p.names[n]] == n, "owner(n)==u but name_of(u)!=n";
        }
      }
      foreach (u in keys(p.users)) {
        assert p.users[u] in p.names, "name_of(u)==n but owner(n) is empty";
        assert p.names[p.users[u]] == u, "name_of(u)==n but owner(n)!=u";
      }
      foreach (u in keys(own)) {
        assert u in p.users, "successful user ended with no name";
        mine = p.users[u];
        assert mine == own[u], "final name differs from the last successful operation";
      }
      goto Complete;
    }
  }
  state Complete { }
}

machine Store {
  var nameRec: map[int, Rec];
  var userRec: map[int, Rec];
  start state Ready {
    on eLoad do (p: (client: machine, isName: bool, key: int)) {
      var r: Rec;
      if (p.isName) {
        if (p.key in nameRec) { r = nameRec[p.key]; }
      } else {
        if (p.key in userRec) { r = userRec[p.key]; }
      }
      send p.client, eLoaded, r;
    }
    on eCas do (p: (client: machine, isName: bool, key: int, version: int, value: int)) {
      var r: Rec;
      var ok: bool;
      if (p.isName) {
        if (p.key in nameRec) { r = nameRec[p.key]; }
        if (r.version == p.version) {
          announce eHold, (name = p.key, before = r.value, after = p.value);
          nameRec[p.key] = (version = p.version + 1, value = p.value);
          assert nameRec[p.key].version > r.version, "versions must increase or a stale CAS could win";
          ok = true;
        }
      } else {
        if (p.key in userRec) { r = userRec[p.key]; }
        // Single writer per user record: a stale version here means two calls of
        // one user overlapped, which the contract forbids.
        assert r.version == p.version, "user record CAS lost; calls for one user overlapped";
        userRec[p.key] = (version = p.version + 1, value = p.value);
        assert userRec[p.key].version > r.version, "versions must increase or a stale CAS could win";
        ok = true;
      }
      send p.client, eCasDone, ok;
    }
    on eDump do {
      var names: map[int, int];
      var users: map[int, int];
      var k: int;
      foreach (k in keys(nameRec)) { names[k] = nameRec[k].value; }
      foreach (k in keys(userRec)) { users[k] = userRec[k].value; }
      announce eFinal, (names = names, users = users);
    }
  }
}

machine Client {
  var store: Store;
  var user: int;
  var contended: bool;
  start state Run {
    entry (input: (store: Store, coordinator: machine, user: int, contended: bool, ops: seq[Op])) {
      var i: int;
      var ok: bool;
      store = input.store;
      user = input.user;
      contended = input.contended;
      while (i < sizeof(input.ops)) {
        if (input.ops[i].isRegister) { ok = doRegister(input.ops[i].name); }
        else { ok = doRename(input.ops[i].name); }
        announce eReturned, (user = user, isRegister = input.ops[i].isRegister,
          name = input.ops[i].name, ok = ok, contended = contended);
        i = i + 1;
      }
      send input.coordinator, eClientDone;
    }
  }

  fun load(isName: bool, key: int): Rec {
    var r: Rec;
    send store, eLoad, (client = this, isName = isName, key = key);
    receive { case eLoaded: (x: Rec) { r = x; } }
    return r;
  }
  fun cas(isName: bool, key: int, version: int, value: int): bool {
    var ok: bool;
    send store, eCas, (client = this, isName = isName, key = key, version = version, value = value);
    receive { case eCasDone: (x: bool) { ok = x; } }
    return ok;
  }
  fun claim(name: int): bool {
    var r: Rec;
    r = load(true, name);
    if (r.value != 0) { return false; }
    return cas(true, name, r.version, user);
  }
  // Only this user's own calls write "user:<u>" and a name it holds, and its calls
  // never overlap, so these two CAS operations cannot lose; a loss is a bug.
  fun setUser(name: int) {
    var r: Rec;
    r = load(false, user);
    assert cas(false, user, r.version, name), "own user record changed under its owner";
  }
  fun release(name: int) {
    var r: Rec;
    r = load(true, name);
    assert r.value == user, "held name changed under its holder";
    assert cas(true, name, r.version, 0), "held name changed under its holder";
  }
  fun doRegister(name: int): bool {
    var r: Rec;
    r = load(false, user);
    if (r.value == name) { return true; }
    if (r.value != 0) { return false; }
    if (!claim(name)) { return false; }
    setUser(name);
    return true;
  }
  fun doRename(name: int): bool {
    var r: Rec;
    var old: int;
    r = load(false, user);
    old = r.value;
    if (old == 0) { return false; }
    if (old == name) { return true; }
    if (!claim(name)) { return false; }
    setUser(name);
    release(old);
    return true;
  }
}

machine Scenario {
  var done: int;
  var store: Store;
  var input: (a: seq[Op], b: seq[Op], contended: bool, sequential: bool);
  start state Running {
    entry (cfg: (a: seq[Op], b: seq[Op], contended: bool, sequential: bool)) {
      input = cfg;
      store = new Store();
      new Client((store = store, coordinator = this, user = 1, contended = input.contended, ops = input.a));
      // Sequential scenarios start user 2 only after user 1 finished, so nothing
      // is in flight and every doable operation must succeed.
      if (!input.sequential) {
        new Client((store = store, coordinator = this, user = 2, contended = input.contended, ops = input.b));
      }
    }
    on eClientDone do {
      done = done + 1;
      if (input.sequential && done == 1) {
        new Client((store = store, coordinator = this, user = 2, contended = input.contended, ops = input.b));
      }
      if (done == 2) { send store, eDump; }
    }
  }
}

fun ops2(r1: bool, n1: int, r2: bool, n2: int): seq[Op] {
  var s: seq[Op];
  s += (0, (isRegister = r1, name = n1));
  s += (1, (isRegister = r2, name = n2));
  return s;
}

// Names are generated; kinds are fixed per scenario so the search completes.
machine Generated {
  start state Init {
    entry {
      new Scenario((a = ops2(true, 3 + choose(2), false, 3 + choose(2)),
                    b = ops2(true, 3 + choose(2), false, 3 + choose(2)),
                    contended = true, sequential = false));
    }
  }
}
machine GeneratedRegisters {
  start state Init {
    entry {
      new Scenario((a = ops2(true, 3 + choose(2), true, 3 + choose(2)),
                    b = ops2(true, 3 + choose(2), true, 3 + choose(2)),
                    contended = true, sequential = false));
    }
  }
}
machine SameNameRegister {
  start state Init {
    entry { new Scenario((a = ops2(true, 3, true, 3), b = ops2(true, 3, true, 4), contended = true, sequential = false)); }
  }
}
machine SwapNames {
  start state Init {
    entry { new Scenario((a = ops2(true, 3, false, 4), b = ops2(true, 4, false, 3), contended = true, sequential = false)); }
  }
}
machine Disjoint {
  start state Init {
    entry { new Scenario((a = ops2(true, 3, false, 5), b = ops2(true, 4, false, 6), contended = false, sequential = false)); }
  }
}
machine SelfRename {  // renaming to the held name succeeds; so does re-registering it
  start state Init {
    entry { new Scenario((a = ops2(true, 3, false, 3), b = ops2(true, 4, true, 4), contended = false, sequential = false)); }
  }
}
machine NamelessRename {  // rename before any register must fail and change nothing
  start state Init {
    entry { new Scenario((a = ops2(false, 3, true, 3), b = ops2(false, 4, true, 4), contended = false, sequential = false)); }
  }
}
machine RenameFreesOld {  // user 2 runs after user 1 renamed away from 1 and takes 1
  start state Init {
    entry { new Scenario((a = ops2(true, 3, false, 4), b = ops2(true, 3, false, 5), contended = false, sequential = true)); }
  }
}

test tcGenerated [main=Generated]: assert Contract in { Generated, Scenario, Client, Store };
test tcGeneratedRegisters [main=GeneratedRegisters]: assert Contract in { GeneratedRegisters, Scenario, Client, Store };
test tcSameName [main=SameNameRegister]: assert Contract in { SameNameRegister, Scenario, Client, Store };
test tcSwap [main=SwapNames]: assert Contract in { SwapNames, Scenario, Client, Store };
test tcDisjoint [main=Disjoint]: assert Contract in { Disjoint, Scenario, Client, Store };
test tcSelfRename [main=SelfRename]: assert Contract in { SelfRename, Scenario, Client, Store };
test tcNameless [main=NamelessRename]: assert Contract in { NamelessRename, Scenario, Client, Store };
test tcRenameFrees [main=RenameFreesOld]: assert Contract in { RenameFreesOld, Scenario, Client, Store };
