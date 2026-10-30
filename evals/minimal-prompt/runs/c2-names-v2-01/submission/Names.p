// Unique usernames over a per-key load/compare-and-swap store.
// Keys: user u -> key u (value = name or -1); name n -> key 3+n (value = owner or -1).
// Only load and single-key CAS are atomic; every send/receive is an interleaving point.
type Op = (reg: bool, name: int);
type Plan = seq[Op];
event eLoad: (client: machine, key: int);
event eLoaded: (version: int, value: int);
event eCas: (client: machine, key: int, version: int, value: int);
event eCasResult: bool;
event eDump: machine;
event eClientDone;

machine Store {
  var versions: map[int, int];
  var vals: map[int, int];
  start state Ready {
    entry {
      var k: int;
      // snappea-lint: allow mutation -- an extra unused key is harmless
      while (k < 5) { versions[k] = 0; vals[k] = -1; k = k + 1; }
    }
    on eLoad do (p: (client: machine, key: int)) {
      send p.client, eLoaded, (version = versions[p.key], value = vals[p.key]);
    }
    on eCas do (p: (client: machine, key: int, version: int, value: int)) {
      if (versions[p.key] == p.version) {
        announce eWrote, (key = p.key, old = vals[p.key], nv = p.value);
        vals[p.key] = p.value;
        // snappea-lint: allow mutation -- any changed version works
        versions[p.key] = p.version + 1;
        send p.client, eCasResult, true;
      } else {
        send p.client, eCasResult, false;
      }
    }
    on eDump do (c: machine) {
      var users: map[int, int];
      var names: map[int, int];
      var i: int;
      // snappea-lint: allow mutation -- an extra empty user entry is harmless
      while (i < 3) { users[i] = vals[i]; i = i + 1; }
      i = 0;
      while (i < 2) { names[i] = vals[3 + i]; i = i + 1; }
      announce eSnapshot, (users = users, names = names);
      send c, eClientDone;
    }
  }
}

machine Client {
  var store: Store;
  var me: int;
  start state Run {
    entry (input: (store: Store, user: int, plan: Plan, coord: machine)) {
      var i: int;
      var ok: bool;
      store = input.store;
      me = input.user;
      while (i < sizeof(input.plan)) {
        if (input.plan[i].reg) { ok = registerAs(input.plan[i].name); }
        else { ok = renameTo(input.plan[i].name); }
        announce eReturned, (user = me, reg = input.plan[i].reg, name = input.plan[i].name, ok = ok);
        i = i + 1;
      }
      send input.coord, eClientDone;
    }
  }

  fun load(key: int): (version: int, value: int) {
    var r: (version: int, value: int);
    send store, eLoad, (client = this, key = key);
    receive { case eLoaded: (x: (version: int, value: int)) { r = x; } }
    return r;
  }
  fun cas(key: int, version: int, value: int): bool {
    var r: bool;
    send store, eCas, (client = this, key = key, version = version, value = value);
    receive { case eCasResult: (x: bool) { r = x; } }
    return r;
  }
  // Take a free name. False if another user holds it.
  fun claim(name: int): bool {
    var s: (version: int, value: int);
    var done: bool;
    var result: bool;
    var tries: int;
    while (!done) {
      // snappea-lint: allow mutation -- the retry bound is a model guard, not the contract
      tries = tries + 1;
      assert tries <= 5, "claim retries without progress";
      s = load(3 + name);
      if (s.value != -1) {
        done = true;
        // snappea-lint: allow mutation -- result defaults to false
        result = false;
      }
      else if (cas(3 + name, s.version, me)) { done = true; result = true; }
    }
    return result;
  }
  fun setName(name: int) {
    var s: (version: int, value: int);
    var won: bool;
    var tries: int;
    while (!won) {
      // snappea-lint: allow mutation -- the retry bound is a model guard, not the contract
      tries = tries + 1;
      assert tries <= 5, "setName retries without progress";
      s = load(me);
      won = cas(me, s.version, name);
    }
  }
  // Only the holder releases its name, so the value is always `me` here.
  fun release(name: int) {
    var s: (version: int, value: int);
    var won: bool;
    var tries: int;
    while (!won) {
      // snappea-lint: allow mutation -- the retry bound is a model guard, not the contract
      tries = tries + 1;
      assert tries <= 5, "release retries without progress";
      s = load(3 + name);
      assert s.value == me, "released a name this user does not hold";
      won = cas(3 + name, s.version, -1);
    }
  }
  fun registerAs(name: int): bool {
    var s: (version: int, value: int);
    s = load(me);
    if (s.value != -1) { return s.value == name; }
    if (!claim(name)) { return false; }
    setName(name);
    return true;
  }
  fun renameTo(name: int): bool {
    var s: (version: int, value: int);
    var old: int;
    s = load(me);
    old = s.value;
    if (old == -1) { return false; }
    if (old == name) { return true; }
    if (!claim(name)) { return false; }
    setName(name);
    release(old);
    return true;
  }
}

machine Scenario {
  var left: int;
  var store: Store;
  start state Running {
    entry (plans: seq[Plan]) {
      var i: int;
      store = new Store();
      left = sizeof(plans);
      while (i < sizeof(plans)) {
        new Client((store = store, user = i, plan = plans[i], coord = this));
        i = i + 1;
      }
    }
    on eClientDone do {
      left = left - 1;
      if (left == 0) { send store, eDump, this; goto Dumping; }
    }
  }
  state Dumping {
    on eClientDone do { announce eDone; }
  }
}

fun op(reg: bool, name: int): Op { return (reg = reg, name = name); }

machine Generated {
  start state Init {
    entry {
      var plans: seq[Plan];
      var a: Plan;
      var b: Plan;
      a += (0, op($, choose(2))); a += (1, op($, choose(2)));
      b += (0, op($, choose(2)));
      plans += (0, a); plans += (1, b);
      new Scenario(plans);
    }
  }
}
// Both users want the same name.
machine SameName {
  start state Init {
    entry {
      var plans: seq[Plan]; var a: Plan; var b: Plan; var c: Plan;
      a += (0, op(true, 0)); b += (0, op(true, 0)); c += (0, op(true, 0));
      plans += (0, a); plans += (1, b); plans += (2, c);
      new Scenario(plans);
    }
  }
}
// Each holds a name, then both rename into the other's name (cycle).
machine Swap {
  start state Init {
    entry {
      var plans: seq[Plan]; var a: Plan; var b: Plan;
      a += (0, op(true, 0)); a += (1, op(false, 1));
      b += (0, op(true, 1)); b += (1, op(false, 0));
      plans += (0, a); plans += (1, b);
      new Scenario(plans);
    }
  }
}
// One user leaves a name another wants; re-registering the held name.
machine Handoff {
  start state Init {
    entry {
      var plans: seq[Plan]; var a: Plan; var b: Plan;
      a += (0, op(true, 0)); a += (1, op(false, 1)); a += (2, op(false, 1));
      b += (0, op(true, 0)); b += (1, op(true, 0)); b += (2, op(false, 0));
      plans += (0, a); plans += (1, b);
      new Scenario(plans);
    }
  }
}
// Register-while-renaming into the same free name.
machine RenameVsRegister {
  start state Init {
    entry {
      var plans: seq[Plan]; var a: Plan; var b: Plan;
      a += (0, op(true, 0)); a += (1, op(false, 1));
      b += (0, op(true, 1)); b += (1, op(true, 0));
      plans += (0, a); plans += (1, b);
      new Scenario(plans);
    }
  }
}

// Three users on both names: distinct keys for users and names.
machine Three {
  start state Init {
    entry {
      var plans: seq[Plan]; var a: Plan; var b: Plan; var c: Plan;
      a += (0, op(true, 1)); b += (0, op(true, 0)); c += (0, op(true, 1)); c += (1, op(false, 0));
      plans += (0, a); plans += (1, b); plans += (2, c);
      new Scenario(plans);
    }
  }
}

// A lone user moves off the higher-numbered name: release must hit that name's key.
machine ReleaseHigh {
  start state Init {
    entry {
      var plans: seq[Plan]; var a: Plan;
      a += (0, op(true, 1)); a += (1, op(false, 0));
      plans += (0, a);
      new Scenario(plans);
    }
  }
}

module Model = { Scenario, Client, Store };
test tcGenerated [main=Generated]: assert Contract in { Generated, Scenario, Client, Store };
test tcSameName [main=SameName]: assert Contract in { SameName, Scenario, Client, Store };
test tcSwap [main=Swap]: assert Contract in { Swap, Scenario, Client, Store };
test tcHandoff [main=Handoff]: assert Contract in { Handoff, Scenario, Client, Store };
test tcThree [main=Three]: assert Contract in { Three, Scenario, Client, Store };
test tcReleaseHigh [main=ReleaseHigh]: assert Contract in { ReleaseHigh, Scenario, Client, Store };
test tcRenameVsRegister [main=RenameVsRegister]: assert Contract in { RenameVsRegister, Scenario, Client, Store };
