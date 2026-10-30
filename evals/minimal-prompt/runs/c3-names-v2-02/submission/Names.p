// Unique usernames over a per-key load/CAS store (no cross-key transactions).
// Keys: user u -> key u (value = name, 0 none); name n -> key 100+n (value = owner, -1 none).
type Op = (kind: int, name: int);          // kind 0 = register, 1 = rename; names 1..3
type Rec = (version: int, value: int);
event eLoad: (client: machine, key: int);
event eLoaded: Rec;
event eCas: (client: machine, key: int, version: int, value: int);
event eCasResult: bool;
event eAudit;
event eInvoke: (user: int, op: Op);
event eReturn: (user: int, op: Op, ok: bool);
event eAuditUser: (user: int, name: int);
event eAuditOwner: (name: int, owner: int);
event eClientDone;
event eDone;

// Contract oracle: per-user view is sequential (same-user calls never overlap) and
// derived only from invocations and results; the store layout is unknown to it.
spec NamesContract observes eInvoke, eReturn, eAuditUser, eAuditOwner, eDone {
  var view: map[int, int];
  var touched: set[int];      // name*10+user : user has invoked an op naming `name`
  var audits: int;
  var invoked: int;
  start hot state Working {
    entry { view[0] = 0; view[1] = 0; }
    on eInvoke do (p: (user: int, op: Op)) {
      touched += (p.op.name * 10 + p.user);
      invoked = invoked + 1;
      assert invoked <= 4, "driver issues two operations per user";
    }
    on eReturn do (p: (user: int, op: Op, ok: bool)) {
      var cur: int;
      var contested: bool;
      cur = view[p.user];
      contested = (p.op.name * 10 + (1 - p.user)) in touched;
      if (p.op.kind == 0) {
        if (cur == 0 || cur == p.op.name) {
          // only another user's interest in the name can justify a refusal
          assert p.ok || contested, "register refused although user had no name and nobody else wanted it";
        } else {
          assert !p.ok, "register succeeded for a user that already holds another name";
        }
        if (p.ok) { view[p.user] = p.op.name; }
      } else {
        if (cur == 0) {
          assert !p.ok, "rename succeeded for a user without a name";
        } else if (cur == p.op.name) {
          assert p.ok, "renaming to the held name must succeed";
        } else {
          assert p.ok || contested, "rename refused although nobody else wanted the name";
          if (p.ok) { view[p.user] = p.op.name; }
        }
      }
    }
    on eAuditUser do (p: (user: int, name: int)) {
      assert p.name == view[p.user], "name_of disagrees with the results of this user's calls";
      audits = audits + 1;
    }
    on eAuditOwner do (p: (name: int, owner: int)) {
      var u: int;
      var expected: int;
      expected = -1;
      while (u < 2) { if (view[u] == p.name) { expected = u; } u = u + 1; }
      assert p.owner == expected, "owner disagrees with name_of at quiescence";
      audits = audits + 1;
    }
    on eDone do {
      assert audits == 5, "audit incomplete";
      goto Complete;
    }
  }
  state Complete { }
}

machine Store {
  var recs: map[int, Rec];
  start state Ready {
    on eLoad do (p: (client: machine, key: int)) {
      if (!(p.key in recs)) { recs[p.key] = (version = 0, value = -1); }
      send p.client, eLoaded, recs[p.key];
    }
    on eCas do (p: (client: machine, key: int, version: int, value: int)) {
      var cur: Rec;
      cur = recs[p.key];
      if (cur.version == p.version) {
        recs[p.key] = (version = cur.version + 1, value = p.value);
        assert recs[p.key].version > cur.version, "versions must increase";
        send p.client, eCasResult, true;
      } else {
        send p.client, eCasResult, false;
      }
    }
  }
}

machine Client {
  var store: Store;
  var user: int;
  var scen: machine;
  start state Run {
    entry (input: (store: Store, user: int, scen: machine, ops: seq[Op])) {
      var i: int;
      var ok: bool;
      store = input.store; user = input.user; scen = input.scen;
      while (i < sizeof(input.ops)) {
        announce eInvoke, (user = user, op = input.ops[i]);
        if (input.ops[i].kind == 0) { ok = doRegister(input.ops[i].name); }
        else { ok = doRename(input.ops[i].name); }
        announce eReturn, (user = user, op = input.ops[i], ok = ok);
        i = i + 1;
      }
      send scen, eClientDone;
      goto Idle;
    }
  }
  state Idle {
    on eAudit do {
      var u: int;
      var n: int;
      while (u < 2) {
        n = load(u).value;
        if (n == -1) { n = 0; }
        announce eAuditUser, (user = u, name = n);
        u = u + 1;
      }
      n = 1;
      while (n <= 3) { announce eAuditOwner, (name = n, owner = load(100 + n).value); n = n + 1; }
      announce eDone;
    }
  }

  fun load(key: int): Rec {
    var r: Rec;
    send store, eLoad, (client = this, key = key);
    receive { case eLoaded: (x: Rec) { r = x; } }
    return r;
  }
  fun cas(key: int, version: int, value: int): bool {
    var r: bool;
    send store, eCas, (client = this, key = key, version = version, value = value);
    receive { case eCasResult: (x: bool) { r = x; } }
    return r;
  }
  // Take the name record for this user; false if someone else holds it. Retry only
  // when the CAS lost to another claimant, then re-decide from the fresh state.
  fun claim(name: int): bool {
    var s: Rec;
    var result: bool;
    var decided: bool;
    var attempts: int;
    while (!decided) {
      // snappea-lint: allow mutation -- liveness bound only; the retry count never reaches it in a correct run
      attempts = attempts + 1;
      // Each lost CAS means another claimant won, so with 2 users one retry suffices.
      assert attempts <= 2, "claim failed to terminate within bounded contention";
      s = load(100 + name);
      if (s.value != -1) { decided = true; }
      else {
        result = cas(100 + name, s.version, user);
        decided = result;
      }
    }
    return result;
  }
  // The user record has a single writer (this user, calls never overlap): CAS must win.
  fun setName(name: int) {
    var s: Rec;
    s = load(user);
    assert cas(user, s.version, name), "user record is single-writer; CAS cannot lose";
  }
  // A held name record is written only by its holder: CAS must win.
  fun release(name: int) {
    var s: Rec;
    s = load(100 + name);
    assert s.value == user, "old name must still be held by this user";
    assert cas(100 + name, s.version, -1), "held name record is single-writer; CAS cannot lose";
  }
  fun doRegister(name: int): bool {
    var cur: int;
    cur = load(user).value;
    if (cur == name) { return true; }
    if (cur != 0 && cur != -1) { return false; }
    if (!claim(name)) { return false; }
    setName(name);
    return true;
  }
  fun doRename(name: int): bool {
    var cur: int;
    cur = load(user).value;
    if (cur == 0 || cur == -1) { return false; }
    if (cur == name) { return true; }
    if (!claim(name)) { return false; }
    setName(name);       // claim new, publish, only then free the old name
    release(cur);
    return true;
  }
}

fun Op1(kind: int, name: int): seq[Op] {
  var s: seq[Op];
  s += (0, (kind = kind, name = name));
  return s;
}
fun Op2(k1: int, n1: int, k2: int, n2: int): seq[Op] {
  var s: seq[Op];
  s += (0, (kind = k1, name = n1));
  s += (1, (kind = k2, name = n2));
  return s;
}

machine Scenario {
  var done: int;
  var left: machine;
  start state Running {
    entry (input: (a: seq[Op], b: seq[Op])) {
      var store: Store;
      store = new Store();
      left = new Client((store = store, user = 0, scen = this, ops = input.a));
      new Client((store = store, user = 1, scen = this, ops = input.b));
    }
    on eClientDone do {
      done = done + 1;
      if (done == 2) { send left, eAudit; }
    }
  }
}

machine Generated {
  start state Init {
    entry {
      // Generated: names 1..2 (fixed scenarios use 3) so renames hit free, own and contested names.
      new Scenario((a = Op2(0, 1 + choose(2), 1, 1 + choose(2)),
                    b = Op2(0, 1 + choose(2), choose(2), 1 + choose(2))));
    }
  }
}
machine SwapNames {     // each renames into the other's name
  start state Init { entry { new Scenario((a = Op2(0, 1, 1, 2), b = Op2(0, 2, 1, 1))); } }
}
machine TakeFreed {     // u1 may grab the name u0 just vacated
  start state Init { entry { new Scenario((a = Op2(0, 1, 1, 2), b = Op2(0, 1, 0, 2))); } }
}
machine SameName {
  start state Init { entry { new Scenario((a = Op1(0, 1), b = Op1(0, 1))); } }
}
machine RenameChain {   // both rename to the same free name
  start state Init { entry { new Scenario((a = Op2(0, 1, 1, 3), b = Op2(0, 2, 1, 3))); } }
}

test tcGenerated [main=Generated]: assert NamesContract in { Generated, Scenario, Client, Store };
test tcSwap [main=SwapNames]: assert NamesContract in { SwapNames, Scenario, Client, Store };
test tcTakeFreed [main=TakeFreed]: assert NamesContract in { TakeFreed, Scenario, Client, Store };
test tcSameName [main=SameName]: assert NamesContract in { SameName, Scenario, Client, Store };
test tcRenameChain [main=RenameChain]: assert NamesContract in { RenameChain, Scenario, Client, Store };
