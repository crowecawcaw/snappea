// Unique usernames over a per-key load/CAS store. See DESIGN.md.
// Keys: name n -> 0..1 (value = owner user or -1); user u -> 10+u (value = name or -1).
event eLoad: (client: machine, key: int, user: int);  // user>=0: claim probe by that user
event eLoaded: (version: int, value: int);
event eCas: (client: machine, key: int, version: int, value: int);
event eCasResult: bool;
event eClaimed: (user: int, name: int);
event eReleased: (user: int, name: int);
event eCommitted: (user: int, name: int);
event eClaimRejected: (user: int, name: int);
event eFinal: (user: int, ownerOf0: int, ownerOf1: int, nameOf: int);
event eOpReturned: (user: int, kind: int, name: int, ok: bool);
event eClientDone;
event eUserDone;
event eAllDone;

spec UniqueNames observes eOpReturned, eClaimed, eReleased, eCommitted, eClaimRejected, eFinal, eClientDone {
  var holder: map[int, int];    // name -> user, set at claim, cleared at release
  var visible: map[int, int];   // user -> committed name
  var probed: map[int, int];  // user -> name last seen held by a probe
  var ops: int;
  var done: int;
  var finals: int;
  start hot state Run {
    on eClaimed do (p: (user: int, name: int)) {
      assert p.name == 0 || p.name == 1, "claim of a name outside the model's names";
      assert !(p.name in holder), "name claimed while someone holds it";
      holder[p.name] = p.user;
    }
    on eReleased do (p: (user: int, name: int)) {
      assert p.name in holder && holder[p.name] == p.user, "released a name not held by this user";
      holder -= (p.name);
    }
    on eCommitted do (p: (user: int, name: int)) {
      assert p.name in holder && holder[p.name] == p.user, "committed a name not claimed";
      visible[p.user] = p.name;
    }
    on eClaimRejected do (p: (user: int, name: int)) {
      // rejection is only valid if someone else holds or is claiming the name
      assert p.name in holder && holder[p.name] != p.user, "rejected a free name";
      probed[p.user] = p.name;
    }
    on eOpReturned do (p: (user: int, kind: int, name: int, ok: bool)) {
      ops = ops + 1;
      if (p.ok) {
        assert p.user in visible && visible[p.user] == p.name, "success without holding the name";
      } else if (p.user in probed && probed[p.user] == p.name) {
        // rejected because someone else held it (checked at the probe)
      } else if (p.kind == 0) {
        assert p.user in visible && visible[p.user] != p.name, "register rejected without cause";
      } else {
        assert !(p.user in visible), "rename rejected without cause";
      }
      if (p.user in probed) { probed -= (p.user); }
    }
    on eClientDone do { done = done + 1; if (done == 2) { assert ops >= 4, "every user must complete both operations"; goto Check; } }
  }
  hot state Check {
    ignore eClientDone;
    on eFinal do (p: (user: int, ownerOf0: int, ownerOf1: int, nameOf: int)) {
      var n: int;
      var o: int;
      finals = finals + 1;
      n = 0;
      while (n < 2) {
        if (n == 0) { o = p.ownerOf0; } else { o = p.ownerOf1; }
        // owner(n)==u exactly when name_of(u)==n
        assert (o == p.user) == (p.nameOf == n), "owner/name_of disagree";
        if (o != -1) { assert n in holder && holder[n] == o, "owner disagrees with spec"; }
        else { assert !(n in holder), "spec says held but store says free"; }
        n = n + 1;
      }
      if (p.nameOf == -1) { assert !(p.user in visible), "name lost"; }
      else { assert p.user in visible && visible[p.user] == p.nameOf, "name_of wrong"; }
      if (finals == 2) { goto Complete; }
    }
  }
  state Complete { }
}

machine Store {
  var ver: map[int, int];
  var val: map[int, int];
  start state Ready {
    on eLoad do (p: (client: machine, key: int, user: int)) {
      if (p.user >= 0 && p.key in val && val[p.key] != -1) { announce eClaimRejected, (user = p.user, name = p.key); }
      if (!(p.key in ver)) { send p.client, eLoaded, (version = 0, value = -1); }
      else { send p.client, eLoaded, (version = ver[p.key], value = val[p.key]); }
    }
    on eCas do (p: (client: machine, key: int, version: int, value: int)) {
      var cur: int;
      cur = 0;
      if (p.key in ver) { cur = ver[p.key]; }
      if (cur == p.version) {
        ver[p.key] = cur + 1;
        // commit point: the monitor sees the change atomically with it
        if (p.key < 10 && p.value != -1) { announce eClaimed, (user = p.value, name = p.key); }
        if (p.key < 10 && p.value == -1) { announce eReleased, (user = val[p.key], name = p.key); }
        if (p.key >= 10) { announce eCommitted, (user = p.key - 10, name = p.value); }
        val[p.key] = p.value;
        send p.client, eCasResult, true;
      } else { send p.client, eCasResult, false; }
    }
  }
}

machine User {
  var store: machine;
  var me: int;
  var ver: int;
  var cur: int;
  start state Run {
    entry (cfg: (store: machine, me: int, boss: machine)) {
      var step: int;
      var arg: int;
      var res: bool;
      var o0: int;
      var o1: int;
      var mine: int;
      store = cfg.store; me = cfg.me;
      step = 0;
      while (step < 2) {
        arg = choose(2);
        if (step == 0 || choose(2) == 0) { res = Register(arg); announce eOpReturned, (user = me, kind = 0, name = arg, ok = res); }
        else { res = Rename(arg); announce eOpReturned, (user = me, kind = 1, name = arg, ok = res); }
        step = step + 1;
      }
      // quiescent reads through the public read paths
      announce eClientDone;
      send cfg.boss, eUserDone;
      receive { case eAllDone: { } }
      LoadKey(0); o0 = cur;
      LoadKey(1); o1 = cur;
      LoadKey(10 + me); mine = cur;
      announce eFinal, (user = me, ownerOf0 = o0, ownerOf1 = o1, nameOf = mine);
    }
  }
  fun LoadKey(key: int) { LoadK(key, -1); }
  fun LoadK(key: int, probe: int) {
    send store, eLoad, (client = this, key = key, user = probe);
    receive { case eLoaded: (r: (version: int, value: int)) { ver = r.version; cur = r.value; } }
  }
  fun CasKey(key: int, v: int, value: int): bool {
    var ok: bool;
    send store, eCas, (client = this, key = key, version = v, value = value);
    receive { case eCasResult: (b: bool) { ok = b; } }
    return ok;
  }
  // Claim a free name; retries on version conflict. Returns false if held.
  // Claim a free name; retries on version conflict. Returns false if held.
  fun Claim(name: int): bool {
    var v: int;
    var finished: bool;
    var res: bool;
    var won: bool;
    var tries: int;
    while (!finished) {
      // each failed CAS needs a competing write to this name; two users allow few
      tries = tries + 1;
      assert tries <= 3, "retry did not converge";
      LoadK(name, me);
      v = ver;
      if (cur != -1) { res = false; finished = true; }
      else {
        won = CasKey(name, v, me);
        if (won) { res = true; finished = true; }
      }
    }
    return res;
  }
  fun Release(name: int) {
    var v: int;
    var okr: bool;
    LoadKey(name);
    v = ver;
    assert cur == me;
    okr = CasKey(name, v, -1);
    assert okr;
  }
  fun SetUser(name: int) {
    var v: int;
    var oku: bool;
    LoadKey(10 + me);
    v = ver;
    oku = CasKey(10 + me, v, name);   // only this user writes its record
    assert oku;
  }
  fun Register(name: int): bool {
    var okc: bool;
    LoadKey(10 + me);
    if (cur != -1) { return cur == name; }
    okc = Claim(name);
    if (okc) { SetUser(name); }
    return okc;
  }
  fun Rename(name: int): bool {
    var old: int;
    var okc: bool;
    LoadKey(10 + me);
    old = cur;
    if (old == -1) { return false; }
    if (old == name) { return true; }
    okc = Claim(name);
    if (okc) { SetUser(name); Release(old); }
    return okc;
  }
}

machine Main {
  var us: seq[machine];
  start state Init {
    entry {
      var s: machine;
      var u: machine;
      s = new Store();
      u = new User((store = s, me = 0, boss = this));
      us += (0, u);
      u = new User((store = s, me = 1, boss = this));
      us += (1, u);
      receive { case eUserDone: { } }
      receive { case eUserDone: { } }
      send us[0], eAllDone;
      send us[1], eAllDone;
    }
  }
}

test tcNames [main=Main]: assert UniqueNames in { Main, Store, User };
