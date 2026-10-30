// Model of the Registry protocol over a store with per-key compare-and-swap only.
// Keys: user u (1..) holds its name id (0 = none); name key 100+n holds its owner (0 = free).
// Protocol: claim the name key first, then set the user record, then release the old name key.

event eLoad: (from: machine, key: int);
event eLoadResp: (version: int, value: int);
event eCas: (from: machine, key: int, version: int, value: int);
event eCasResp: bool;
event eVerify: (users: int, names: int);
event eDone;
// The user's record now holds `name` (0 = none).
event eHold: (user: string, name: string);

machine Store {
  var ver: map[int, int];
  var val: map[int, int];

  fun Touch(key: int) {
    if (!(key in ver)) { ver[key] = 0; val[key] = 0; }
  }

  start state Serving {
    on eLoad do (r: (from: machine, key: int)) {
      Touch(r.key);
      send r.from, eLoadResp, (version = ver[r.key], value = val[r.key]);
    }
    on eCas do (r: (from: machine, key: int, version: int, value: int)) {
      Touch(r.key);
      if (ver[r.key] == r.version) {
        ver[r.key] = ver[r.key] + 1;
        val[r.key] = r.value;
        send r.from, eCasResp, true;
      } else {
        send r.from, eCasResp, false;
      }
    }
    on eVerify do (r: (users: int, names: int)) {
      var u: int; var n: int;
      u = 1;
      while (u <= r.users) {
        Touch(u);
        if (val[u] != 0) {
          Touch(100 + val[u]);
          assert val[100 + val[u]] == u, format("user {0} holds name {1} but the name is owned by {2}", u, val[u], val[100 + val[u]]);
        }
        u = u + 1;
      }
      n = 1;
      while (n <= r.names) {
        Touch(100 + n);
        if (val[100 + n] != 0) {
          Touch(val[100 + n]);
          assert val[val[100 + n]] == n, format("name {0} is owned by user {1} who holds name {2}", n, val[100 + n], val[val[100 + n]]);
        }
        n = n + 1;
      }
    }
  }
}

machine Client {
  var store: machine;
  var me: int;
  var names: int;
  var boss: machine;

  fun Load(key: int): (version: int, value: int) {
    var r: (version: int, value: int);
    send store, eLoad, (from = this, key = key);
    receive { case eLoadResp: (x: (version: int, value: int)) { r = x; } }
    return r;
  }

  fun Cas(key: int, version: int, value: int): bool {
    var ok: bool;
    send store, eCas, (from = this, key = key, version = version, value = value);
    receive { case eCasResp: (b: bool) { ok = b; } }
    return ok;
  }

  // Take the name key for me. False if someone else holds it.
  fun Claim(name: int): bool {
    var k: (version: int, value: int);
    var won: bool;
    won = false;
    k = Load(100 + name);
    while (k.value == 0 && !won) {
      won = Cas(100 + name, k.version, me);
      if (!won) { k = Load(100 + name); }
    }
    return won;
  }

  fun Release(name: int) {
    var k: (version: int, value: int);
    var ok: bool;
    k = Load(100 + name);
    assert k.value == me, "released a name this user does not own";
    ok = Cas(100 + name, k.version, 0);
    assert ok, "name key changed while its owner released it";
  }

  fun NameStr(name: int): string {
    return format("n{0}", name);
  }

  fun SetUser(name: int) {
    var u: (version: int, value: int);
    var ok: bool;
    u = Load(me);
    ok = Cas(me, u.version, name);
    assert ok, "user key changed by someone else";
    announce eHold, (user = format("u{0}", me), name = NameStr(name));
  }

  fun Register(name: int): bool {
    var u: (version: int, value: int);
    u = Load(me);
    if (u.value == name) { return true; }
    if (u.value != 0) { return false; }
    if (!Claim(name)) { return false; }
    SetUser(name);
    return true;
  }

  fun Rename(name: int): bool {
    var u: (version: int, value: int);
    var old: int;
    u = Load(me);
    old = u.value;
    if (old == 0) { return false; }
    if (old == name) { return true; }
    if (!Claim(name)) { return false; }
    SetUser(name);
    Release(old);
    return true;
  }

  start state Init {
    entry (p: (store: machine, me: int, names: int, boss: machine)) {
      store = p.store; me = p.me; names = p.names; boss = p.boss;
      goto Run;
    }
  }

  state Run {
    entry {
      var i: int;
      var ignored: bool;
      i = 0;
      while (i < 3) {
        if ($) { ignored = Register(choose(names) + 1); }
        else { ignored = Rename(choose(names) + 1); }
        i = i + 1;
      }
      send boss, eDone;
    }
  }
}

// No two users ever hold the same name.
spec UniqueNames observes eHold {
  var held: map[string, string];
  start state Watch {
    on eHold do (p: (user: string, name: string)) {
      var u: string;
      foreach (u in keys(held)) {
        assert held[u] != p.name || u == p.user,
          format("users {0} and {1} both hold name {2}", u, p.user, p.name);
      }
      held[p.user] = p.name;
    }
  }
}

machine Driver {
  var store: machine;
  start state Init {
    entry {
      var i: int;
      store = new Store();
      i = 1;
      while (i <= 3) {
        new Client((store = store, me = i, names = 2, boss = this));
        i = i + 1;
      }
      goto Wait;
    }
  }
  state Wait {
    entry { }
    on eDone do { Count(); }
  }
  var done: int;
  fun Count() {
    done = done + 1;
    if (done == 3) { send store, eVerify, (users = 3, names = 2); }
  }
}

test tcRegistry [main = Driver]: assert UniqueNames in { Driver, Store, Client };
