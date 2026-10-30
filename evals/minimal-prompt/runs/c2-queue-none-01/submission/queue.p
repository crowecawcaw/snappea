// Model of the shared queue: clients run load / compare-swap loops against one
// versioned record; retries (duplicate requests) may be sent at any time.
type tRec = (items: seq[int], putDone: map[int, bool], takeDone: map[int, int]);
// takeDone value -1 means "returned None".
event eLoad: (client: machine);
event eLoadResp: (version: int, rec: tRec);
event eCas: (client: machine, version: int, rec: tRec);
event eCasResp: (ok: bool);
event eResult: (rid: int, isPut: bool, val: int);
event eSize: int;
event eInit: (store: machine, rid: int, isPut: bool, item: int);

fun Cap(): int { return 2; }

machine Store {
  var version: int; var rec: tRec;
  start state S {
    entry { rec = (items = default(seq[int]), putDone = default(map[int, bool]), takeDone = default(map[int, int])); }
    on eLoad do (m: (client: machine)) { send m.client, eLoadResp, (version = version, rec = rec); }
    on eCas do (m: (client: machine, version: int, rec: tRec)) {
      if (m.version == version) { rec = m.rec; version = version + 1; announce eSize, sizeof(rec.items); send m.client, eCasResp, (ok = true,); }
      else { send m.client, eCasResp, (ok = false,); }
    }
  }
}

machine Client {
  var store: machine; var rid: int; var isPut: bool; var item: int;
  var v: int; var r: tRec; var out: int;
  start state Init {
    on eInit do (m: (store: machine, rid: int, isPut: bool, item: int)) {
      store = m.store; rid = m.rid; isPut = m.isPut; item = m.item;
      goto Loading;
    }
  }
  state Loading {
    entry { send store, eLoad, (client = this,); }
    on eLoadResp do (m: (version: int, rec: tRec)) {
      v = m.version; r = m.rec;
      if (isPut) {
        if (rid in r.putDone) { if (r.putDone[rid]) { out = 1; } else { out = 0; } }
        else {
          if (sizeof(r.items) < Cap()) { r.items += (sizeof(r.items), item); r.putDone[rid] = true; out = 1; }
          else { r.putDone[rid] = false; out = 0; }
        }
      } else {
        if (rid in r.takeDone) { out = r.takeDone[rid]; }
        else if (sizeof(r.items) > 0) { out = r.items[0]; r.items -= (0); r.takeDone[rid] = out; }
        else { out = -1; r.takeDone[rid] = -1; }
      }
      send store, eCas, (client = this, version = v, rec = r);
    }
    on eCasResp do (m: (ok: bool)) {
      if (m.ok) { announce eResult, (rid = rid, isPut = isPut, val = out); }
      else { goto Loading; }
    }
  }
}

// Spec: every request id gets one consistent answer, and results never exceed capacity.
spec Consistent observes eResult {
  var seen: map[(int, bool), int];
  start state S {
    on eResult do (m: (rid: int, isPut: bool, val: int)) {
      if ((m.rid, m.isPut) in seen) { assert seen[(m.rid, m.isPut)] == m.val, "retry returned a different result"; }
      else { seen[(m.rid, m.isPut)] = m.val; }
    }
  }
}

// Spec: the stored queue never holds more than the capacity.
spec Bounded observes eSize {
  start state S {
    on eSize do (n: int) { assert n <= Cap(), "queue over capacity"; }
  }
}

machine Main {
  start state S {
    entry {
      var st: machine; var i: int;
      st = new Store();
      // three puts, two takes, each duplicated (a retry)
      send (new Client()), eInit, (store = st, rid = 1, isPut = true, item = 10);
      send (new Client()), eInit, (store = st, rid = 1, isPut = true, item = 10);
      send (new Client()), eInit, (store = st, rid = 2, isPut = true, item = 20);
      send (new Client()), eInit, (store = st, rid = 3, isPut = true, item = 30);
      send (new Client()), eInit, (store = st, rid = 1, isPut = false, item = 0);
      send (new Client()), eInit, (store = st, rid = 1, isPut = false, item = 0);
    }
  }
}

test tcQueue [main = Main]: assert Consistent, Bounded in { Main, Store, Client };
