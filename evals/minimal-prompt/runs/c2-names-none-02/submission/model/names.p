// Model of the Registry protocol over a store with per-key compare-and-swap.
// Records: "n:<name>" -> holding user ("" = free); "u:<user>" -> held name ("" = none).
// Claim the new name first, then commit the user record, then free the old name.

type tRec = (ver: int, val: string);

event eLoad: (c: machine, key: string);
event eLoaded: tRec;
// tag: 0 none, 1 claim name, 2 free name, 3 set user record; subject is the name or user.
event eCas: (c: machine, key: string, ver: int, val: string, tag: int, subject: string);
event eCasDone: bool;
event eDone;

// Spec events, announced at the point where the record changes.
event eHold: (name: string, user: string);    // name record: free -> user
event eFree: (name: string, user: string);    // name record: user -> free
event eSetName: (user: string, before: string, after: string); // user record commit
event eQuiet;                                  // no call in progress

machine Store {
  var recs: map[string, tRec];
  start state Serving {
    on eLoad do (r: (c: machine, key: string)) {
      if (r.key in recs) send r.c, eLoaded, recs[r.key];
      else send r.c, eLoaded, (ver = 0, val = "");
    }
    on eCas do (r: (c: machine, key: string, ver: int, val: string, tag: int, subject: string)) {
      var cur: tRec;
      cur = (ver = 0, val = "");
      if (r.key in recs) cur = recs[r.key];
      if (cur.ver == r.ver) {
        recs[r.key] = (ver = r.ver + 1, val = r.val);
        // Announced in the same step as the write, so spec order is write order.
        if (r.tag == 1) announce eHold, (name = r.subject, user = r.val);
        if (r.tag == 2) announce eFree, (name = r.subject, user = cur.val);
        if (r.tag == 3) announce eSetName, (user = r.subject, before = cur.val, after = r.val);
        send r.c, eCasDone, true;
      } else send r.c, eCasDone, false;
    }
  }
}

machine Client {
  var store: machine;
  var me: string;
  var driver: machine;

  fun Load(key: string): tRec {
    var r: tRec;
    send store, eLoad, (c = this, key = key);
    receive { case eLoaded: (x: tRec) { r = x; } }
    return r;
  }
  fun Cas(key: string, ver: int, val: string, tag: int, subject: string): bool {
    var ok: bool;
    send store, eCas, (c = this, key = key, ver = ver, val = val, tag = tag, subject = subject);
    receive { case eCasDone: (b: bool) { ok = b; } }
    return ok;
  }
  // Take a free name record for me. Only I write my own user record.
  fun Claim(name: string): bool {
    var n: tRec;
    n = Load("n:" + name);
    if (n.val != "") return false;
    if (!Cas("n:" + name, n.ver, me, 1, name)) return false;
    return true;
  }
  // Release a name I hold. Nobody else writes it, so the CAS must succeed.
  fun Free(name: string) {
    var n: tRec;
    var ok: bool;
    n = Load("n:" + name);
    ok = Cas("n:" + name, n.ver, "", 2, name);
    assert ok, "free of a held name failed";
  }
  fun SetUser(old: string, nw: string) {
    var u: tRec;
    var ok: bool;
    u = Load("u:" + me);
    ok = Cas("u:" + me, u.ver, nw, 3, me);
    assert ok, "user record changed under its owner";
  }
  fun Register(name: string) {
    var u: tRec;
    u = Load("u:" + me);
    if (u.val != "") return;
    if (!Claim(name)) return;
    SetUser("", name);
  }
  fun Rename(name: string) {
    var u: tRec;
    u = Load("u:" + me);
    if (u.val == "" || u.val == name) return;
    if (!Claim(name)) return;
    SetUser(u.val, name);
    Free(u.val);
  }

  start state Init {
    entry (c: (store: machine, me: string, driver: machine, names: seq[string])) {
      var i: int;
      store = c.store; me = c.me; driver = c.driver;
      i = 0;
      while (i < sizeof(c.names)) {
        if ($) Register(c.names[i]); else Rename(c.names[i]);
        i = i + 1;
      }
      send driver, eDone;
    }
  }
}

machine Driver {
  var left: int;
  start state Run {
    entry (c: (users: seq[string], names: seq[string])) {
      var s: machine;
      var i: int;
      s = new Store();
      left = sizeof(c.users);
      i = 0;
      while (i < sizeof(c.users)) {
        new Client((store = s, me = c.users[i], driver = this, names = c.names));
        i = i + 1;
      }
    }
    on eDone do {
      left = left - 1;
      if (left == 0) announce eQuiet;
    }
  }
}

// name -> holder (name records) and user -> name (user records) agree.
spec NamesConsistent observes eHold, eFree, eSetName, eQuiet {
  var holder: map[string, string];
  var named: map[string, string];
  start state Watch {
    on eHold do (e: (name: string, user: string)) {
      assert !(e.name in holder), format("name {0} claimed while held", e.name);
      holder[e.name] = e.user;
    }
    on eFree do (e: (name: string, user: string)) {
      assert e.name in holder && holder[e.name] == e.user, format("{0} freed a name it does not hold", e.user);
      holder -= (e.name);
    }
    on eSetName do (e: (user: string, before: string, after: string)) {
      var cur: string;
      cur = "";
      if (e.user in named) cur = named[e.user];
      assert cur == e.before, format("{0} record held {1}, not {2}", e.user, cur, e.before);
      named[e.user] = e.after;
    }
    on eQuiet do {
      var n: string;
      var u: string;
      assert sizeof(holder) == sizeof(named), "name and user records differ in size";
      foreach (n in keys(holder)) {
        u = holder[n];
        assert u in named && named[u] == n, format("{0} held by {1} but user record disagrees", n, u);
      }
    }
  }
}
