/* Unique usernames over a per-key load/CAS store (no cross-key transactions).
 * Run: snappea check Names.p   (NamesObs.p is a separate file for pytest; do not check both together).
 * Algorithm (solution.py): keys "n:<name>" -> owner, "u:<user>" -> name.
 *  register: load u; claim n by CAS (retry on conflict); set u by CAS.
 *  rename:   claim new by CAS; set u -> new; THEN release old (CAS to None).
 * Claim-before-publish, release-last: a name is never held by two users, and
 * once all calls finish owner(n)==u iff name_of(u)==n.
 * Model: 2 users, 2 names, 2 ops per user (first is a register); one user's calls are sequential.
 * Omits: name validation (ordinary tests), crashes between steps. */
event eLoad: (src: machine, key: int);
event eLoadR: (ver: int, val: int);
event eCas: (src: machine, key: int, ver: int, val: int);
event eCasR: (ok: bool);
event eClaimed: (name: int, user: int);
event eReleased: (name: int, user: int);
event eUserSet: (user: int, name: int);
event eFinished: (user: int);

spec Names observes eClaimed, eReleased, eUserSet, eFinished {
  var holder: map[int, int];
  var userName: map[int, int];
  var finished: set[int];
  var sawClaim: bool;
  start hot state Run {
    on eClaimed do (p: (name: int, user: int)) {
      assert !(p.name in holder), "name claimed while held by someone";
      holder[p.name] = p.user;
      sawClaim = true;
    }
    on eReleased do (p: (name: int, user: int)) {
      assert p.name in holder, "released a name nobody holds";
      assert holder[p.name] == p.user, "released a name held by another user";
      holder -= (p.name);
    }
    on eUserSet do (p: (user: int, name: int)) {
      assert p.name in holder, "user set to an unclaimed name";
      assert holder[p.name] == p.user, "user set to a name held by another";
      userName[p.user] = p.name;
    }
    on eFinished do (p: (user: int)) {
      var k: int;
      finished += (p.user);
      if (sizeof(finished) == 2) {
        assert sawClaim, "no useful work";
        foreach (k in keys(holder)) {
          assert holder[k] in userName, "held name has no user record (leak)";
          assert userName[holder[k]] == k, "held name is not its owner's name";
        }
        foreach (k in keys(userName)) {
          assert userName[k] in holder, "user name not held";
          assert holder[userName[k]] == k, "user name held by another";
        }
        goto Done;
      }
    }
  }
  state Done { }
}

machine KV {
  var recs: map[int, (ver: int, val: int)];
  start state S {
    on eLoad do (p: (src: machine, key: int)) {
      if (!(p.key in recs)) { recs[p.key] = (ver = 0, val = -1); }
      send p.src, eLoadR, (ver = recs[p.key].ver, val = recs[p.key].val);
    }
    on eCas do (p: (src: machine, key: int, ver: int, val: int)) {
      // Clients always load a key before CAS, so it exists.
      if (recs[p.key].ver == p.ver) {
        // Announce at the commit point so monitor order is store order.
        if (p.key < 1000) {
          if (p.val == -1) { announce eReleased, (name = p.key, user = recs[p.key].val); }
          else { announce eClaimed, (name = p.key, user = p.val); }
        } else {
          announce eUserSet, (user = p.key - 1000, name = p.val);
        }
        recs[p.key] = (ver = p.ver + 1, val = p.val);
        send p.src, eCasR, (ok = true,);
      } else {
        send p.src, eCasR, (ok = false,);
      }
    }
  }
}

machine Client {
  var kv: KV;
  var me: int;
  fun Load(key: int): (ver: int, val: int) {
    var r: (ver: int, val: int);
    send kv, eLoad, (src = this, key = key);
    receive { case eLoadR: (x: (ver: int, val: int)) { r = x; } }
    return r;
  }
  fun Cas(key: int, ver: int, val: int): bool {
    var ok: bool;
    send kv, eCas, (src = this, key = key, ver = ver, val = val);
    receive { case eCasR: (x: (ok: bool)) { ok = x.ok; } }
    return ok;
  }
  fun Claim(name: int): bool {
    var r: (ver: int, val: int);
    var won: bool;
    r = Load(name);
    while (r.val == -1 && !won) {
      won = Cas(name, r.ver, me);
      if (!won) { r = Load(name); }
    }
    return won;
  }
  fun SetUser(ur: (ver: int, val: int), name: int) {
    var ok: bool;
    ok = Cas(1000 + me, ur.ver, name);
    assert ok, "user record changed under a sequential user";
  }
  fun Release(name: int) {
    var r: (ver: int, val: int);
    var ok: bool;
    r = Load(name);
    ok = Cas(name, r.ver, -1);
    assert ok, "held name changed under its holder";
  }
  fun Register(name: int) {
    var ur: (ver: int, val: int);
    ur = Load(1000 + me);
    if (ur.val != -1) { return; }
    if (!Claim(name)) { return; }
    SetUser(ur, name);
  }
  fun Rename(name: int) {
    var ur: (ver: int, val: int);
    ur = Load(1000 + me);
    if (ur.val == -1 || ur.val == name) { return; }
    if (!Claim(name)) { return; }
    SetUser(ur, name);
    Release(ur.val);
  }
  start state Run {
    entry (p: (kv: KV, me: int)) {
      var i: int;
      var name: int;
      kv = p.kv; me = p.me;
      while (i < 2) {
        name = choose(2);
        if (i == 0 || $) { Register(name); } else { Rename(name); }
        i = i + 1;
      }
      announce eFinished, (user = me,);
    }
  }
}

machine Main {
  start state Init {
    entry {
      var kv: KV;
      kv = new KV();
      new Client((kv = kv, me = 0));
      new Client((kv = kv, me = 1));
    }
  }
}
test tcTwoUsers [main=Main]: assert Names in { Main, KV, Client };
