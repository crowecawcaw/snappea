/* Unique usernames over a per-key load/CAS store. See DESIGN.md.
 * Records: name record (owner or -1), user record (name or -1). Only single-key
 * load and CAS are atomic. Users 2..3, names 0..1. kind 0 = register, 1 = rename.
 * Algorithm: claim the new name by CAS on its record, then set the user record,
 * then (rename only) release the old name record.
 */
type Rec = (version: int, value: int);

event eLoad: (client: machine, key: int);
event eLoaded: Rec;
event eCas: (client: machine, key: int, version: int, value: int);
event eCasResult: bool;
event eDump: (client: machine, calls: int);
event eNext;
event eClientDone;
event eInvoked;
event eReturned: (user: int, kind: int, name: int, ok: bool);
event eFinalName: (user: int, name: int);
event eFinalOwner: (name: int, owner: int);
event eFinished: int;

// Independent oracle. Results are checked exactly while calls never overlap;
// once calls overlap, only the final state (which must match what each user was
// told, and be a bijection) is checked.
spec NamesContract observes eInvoked, eReturned, eFinalName, eFinalOwner, eFinished {
  var held: map[int, int];
  var pending: int;
  var overlapped: bool;
  var seen: int;
  var returned: int;

  fun heldBy(u: int): int {
    if (u in held) { return held[u]; }
    return -1;
  }
  fun holder(n: int): int {
    var u: int;
    var r: int;
    r = -1;
    foreach (u in keys(held)) { if (held[u] == n) { r = u; } }
    return r;
  }
  fun expect(user: int, kind: int, name: int): bool {
    var cur: int;
    cur = heldBy(user);
    if (kind == 1 && cur == -1) { return false; }
    if (cur == name) { return true; }
    if (kind == 0 && cur != -1) { return false; }
    return holder(name) == -1;
  }

  start hot state Running {
    on eInvoked do {
      pending = pending + 1;
      if (pending > 1) { overlapped = true; }
    }
    on eReturned do (p: (user: int, kind: int, name: int, ok: bool)) {
      pending = pending - 1;
      returned = returned + 1;
      if (!overlapped) {
        assert p.ok == expect(p.user, p.kind, p.name), "result disagrees with contract";
      }
      if (p.ok) { held[p.user] = p.name; }
    }
    on eFinalName do (p: (user: int, name: int)) {
      assert heldBy(p.user) == p.name, "name_of disagrees with successful calls";
      seen = seen + 1;
    }
    on eFinalOwner do (p: (name: int, owner: int)) {
      assert p.owner == holder(p.name), "owner disagrees with name_of";
      seen = seen + 1;
    }
    on eFinished do (calls: int) {
      assert seen == 4, "final state must be fully read";
      assert returned == calls, "every call must return";
      goto Complete;
    }
  }
  state Complete { }
}

// Atomic per-key primitives, like runtime.Store. Key k < 2: name record k (owner
// or -1). Key u (2 or 3): user record u (name or -1).
machine Store {
  var ver: map[int, int];
  var val: map[int, int];
  start state Ready {
    entry {
      var i: int;
      while (i < 4) { val[i] = -1; ver[i] = 0; i = i + 1; }
    }
    on eLoad do (p: (client: machine, key: int)) {
      send p.client, eLoaded, (version = ver[p.key], value = val[p.key]);
    }
    on eCas do (p: (client: machine, key: int, version: int, value: int)) {
      if (ver[p.key] == p.version) {
        ver[p.key] = p.version + 1; val[p.key] = p.value;
        send p.client, eCasResult, true;
      } else { send p.client, eCasResult, false; }
    }
    on eDump do (p: (client: machine, calls: int)) {
      var i: int;
      while (i < 2) {
        announce eFinalName, (user = 2 + i, name = val[2 + i]);
        announce eFinalOwner, (name = i, owner = val[i]);
        i = i + 1;
      }
      announce eFinished, p.calls;
    }
  }
}

machine Client {
  var store: Store;
  var coord: machine;
  var user: int;
  var ops: seq[(kind: int, name: int)];
  var rec: Rec;
  start state Idle {
    entry (input: (store: Store, coord: machine, user: int, ops: seq[(kind: int, name: int)])) {
      store = input.store; coord = input.coord; user = input.user; ops = input.ops;
    }
    on eNext goto Run;
  }
  state Run {
    entry {
      var i: int;
      var ok: bool;
      while (i < sizeof(ops)) {
        announce eInvoked;
        if (ops[i].kind == 0) { ok = doRegister(ops[i].name); }
        else { ok = doRename(ops[i].name); }
        announce eReturned, (user = user, kind = ops[i].kind, name = ops[i].name, ok = ok);
        i = i + 1;
      }
      send coord, eClientDone;
    }
  }

  fun load(key: int) {
    send store, eLoad, (client = this, key = key);
    receive { case eLoaded: (r: Rec) { rec = r; } }
  }
  fun cas(key: int, version: int, value: int): bool {
    var res: bool;
    send store, eCas, (client = this, key = key, version = version, value = value);
    receive { case eCasResult: (b: bool) { res = b; } }
    return res;
  }
  // Take the name record for this user; false if someone else holds it.
  fun claim(name: int): bool {
    var done: bool;
    var won: bool;
    var attempts: int;
    while (!done) {
      attempts = attempts + 1;
      // Test-only bound: one rival, so few CAS rounds can be lost. Not a production limit.
      assert attempts <= 4, "claim failed to complete within bounded contention";
      load(name);
      if (rec.value != -1) { done = true; }
      else if (cas(name, rec.version, user)) { done = true; won = true; }
    }
    return won;
  }
  fun setUser(name: int) {
    var done: bool;
    var attempts: int;
    while (!done) {
      attempts = attempts + 1;
      assert attempts <= 2, "setUser should not lose a CAS: calls per user never overlap";
      load(user);
      done = cas(user, rec.version, name);
    }
  }
  // Only the holder writes a held name record, so this CAS cannot lose to a rival.
  fun release(name: int) {
    var done: bool;
    var attempts: int;
    while (!done) {
      attempts = attempts + 1;
      assert attempts <= 2, "only the holder changes a held name record";
      load(name);
      done = cas(name, rec.version, -1);
    }
  }
  fun doRegister(name: int): bool {
    var cur: int;
    load(user);
    cur = rec.value;
    if (cur == name) { return true; }
    if (cur != -1) { return false; }
    if (!claim(name)) { return false; }
    setUser(name);
    return true;
  }
  fun doRename(name: int): bool {
    var cur: int;
    load(user);
    cur = rec.value;
    if (cur == -1) { return false; }
    if (cur == name) { return true; }
    if (!claim(name)) { return false; }
    setUser(name);
    release(cur);
    return true;
  }
}

// snappea: driver
machine Scenario {
  var clients: seq[Client];
  var sequential: bool;
  var store: Store;
  var done: int;
  var calls: int;
  start state Running {
    entry (input: (sequential: bool, ops0: seq[(kind: int, name: int)], ops1: seq[(kind: int, name: int)])) {
      store = new Store();
      sequential = input.sequential;
      calls = sizeof(input.ops0) + sizeof(input.ops1);
      clients += (0, new Client((store = store, coord = this, user = 2, ops = input.ops0)));
      clients += (1, new Client((store = store, coord = this, user = 3, ops = input.ops1)));
      if (sequential) { send clients[0], eNext; }
      else { send clients[0], eNext; send clients[1], eNext; }
    }
    on eClientDone do {
      done = done + 1;
      if (sequential && done == 1) { send clients[1], eNext; }
      if (done == 2) { send store, eDump, (client = this, calls = calls); }
    }
  }
}

fun mk(k0: int, n0: int, k1: int, n1: int): seq[(kind: int, name: int)] {
  var s: seq[(kind: int, name: int)];
  s += (0, (kind = k0, name = n0));
  s += (1, (kind = k1, name = n1));
  return s;
}

fun mk1(k: int, n: int): seq[(kind: int, name: int)] {
  var s: seq[(kind: int, name: int)];
  s += (0, (kind = k, name = n));
  return s;
}

// Generated: user 0 runs two random ops, user 1 one, concurrently. (Both: see
// the fixed scenarios; this bound keeps the search exhaustive.) Both users run two ops of random kind and name, concurrently.
machine Generated {
  start state Init {
    entry {
      new Scenario((sequential = false,
        ops0 = mk(choose(2), choose(2), choose(2), choose(2)),
        ops1 = mk1(choose(2), choose(2))));
    }
  }
}
// Same, one user at a time: exact results checked.
machine GeneratedSeq {
  start state Init {
    entry {
      new Scenario((sequential = true,
        ops0 = mk(choose(2), choose(2), choose(2), choose(2)),
        ops1 = mk1(choose(2), choose(2))));
    }
  }
}
// Both register the same name, then try to rename to the other.
machine SameName {
  start state Init {
    entry { new Scenario((sequential = false, ops0 = mk(0, 0, 1, 1), ops1 = mk(0, 0, 1, 1))); }
  }
}
// Swap attempt: each holds a name, each renames to the other's.
machine Swap {
  start state Init {
    entry { new Scenario((sequential = false, ops0 = mk(0, 0, 1, 1), ops1 = mk(0, 1, 1, 0))); }
  }
}
// u0 moves off name 0 while u1 registers it.
machine Handoff {
  start state Init {
    entry { new Scenario((sequential = false, ops0 = mk(0, 0, 1, 1), ops1 = mk(1, 1, 0, 0))); }
  }
}

// Renaming to the name already held must succeed.
machine RenameSame {
  start state Init {
    entry { new Scenario((sequential = true, ops0 = mk(0, 0, 1, 0), ops1 = mk(0, 1, 1, 1))); }
  }
}
test tcRenameSame [main=RenameSame]: assert NamesContract in { RenameSame, Scenario, Client, Store };
test tcGenerated [main=Generated]: assert NamesContract in { Generated, Scenario, Client, Store };
test tcGeneratedSeq [main=GeneratedSeq]: assert NamesContract in { GeneratedSeq, Scenario, Client, Store };
test tcSameName [main=SameName]: assert NamesContract in { SameName, Scenario, Client, Store };
test tcSwap [main=Swap]: assert NamesContract in { Swap, Scenario, Client, Store };
test tcHandoff [main=Handoff]: assert NamesContract in { Handoff, Scenario, Client, Store };
