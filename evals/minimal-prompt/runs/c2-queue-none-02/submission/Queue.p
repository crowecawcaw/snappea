// Model of the shared queue: one versioned record, compare-and-swap retry loops.
type Rec = (items: seq[int], putDone: map[int, bool], takeDone: map[int, int]);
type Snap = (v: int, r: Rec);

event eLoad: machine;
event eLoadResp: Snap;
event eCas: (src: machine, v: int, r: Rec);
event eCasResp: bool;
event eDone: (kind: int, id: int, res: int);

machine Store {
  var ver: int;
  var rec: Rec;
  start state S {
    entry { }
    on eLoad do (c: machine) { send c, eLoadResp, (v = ver, r = rec); }
    on eCas do (m: (src: machine, v: int, r: Rec)) {
      if (m.v == ver) { rec = m.r; ver = ver + 1; send m.src, eCasResp, true; }
      else { send m.src, eCasResp, false; }
    }
  }
}

fun Int(b: bool): int { if (b) { return 1; } return 0; }

// A client issues an operation and then retries it with the same id.
machine Client {
  var store: machine;
  var cap: int;
  start state Init {
    entry (p: (store: machine, cap: int, isPut: bool, id: int, item: int)) {
      store = p.store; cap = p.cap;
      if (p.isPut) { DoPut(p.id, p.item); DoPut(p.id, p.item); }
      else { DoTake(p.id); DoTake(p.id); }
    }
  }
  fun DoPut(id: int, item: int) {
    var m: Snap; var r: Rec; var ok: bool; var res: bool;
    while (true) {
      send store, eLoad, this;
      receive { case eLoadResp: (x: Snap) { m = x; } }
      r = m.r;
      if (id in r.putDone) { res = r.putDone[id]; break; }
      if (sizeof(r.items) >= cap) { res = false; } else { r.items += (sizeof(r.items), item); res = true; }
      r.putDone[id] = res;
      send store, eCas, (src = this, v = m.v, r = r);
      receive { case eCasResp: (b: bool) { ok = b; } }
      if (ok) { break; }
    }
    announce eDone, (kind = 0, id = id, res = Int(res));
  }
  fun DoTake(id: int) {
    var m: Snap; var r: Rec; var ok: bool; var res: int;
    while (true) {
      send store, eLoad, this;
      receive { case eLoadResp: (x: Snap) { m = x; } }
      r = m.r;
      if (id in r.takeDone) { res = r.takeDone[id]; break; }
      if (sizeof(r.items) == 0) { res = -1; } else { res = r.items[0]; r.items -= (0); }
      r.takeDone[id] = res;
      send store, eCas, (src = this, v = m.v, r = r);
      receive { case eCasResp: (b: bool) { ok = b; } }
      if (ok) { break; }
    }
    announce eDone, (kind = 1, id = id, res = res);
  }
}

// Same request id must always yield the same result.
spec Idempotent observes eDone {
  var seen: map[(int, int), int];
  start state S {
    on eDone do (d: (kind: int, id: int, res: int)) {
      if ((d.kind, d.id) in seen) { assert seen[(d.kind, d.id)] == d.res, "retry changed result"; }
      seen[(d.kind, d.id)] = d.res;
    }
  }
}

machine Main {
  start state S {
    entry {
      var s: machine;
      s = new Store();
      new Client((store = s, cap = 2, isPut = true, id = 1, item = 10));
      new Client((store = s, cap = 2, isPut = true, id = 2, item = 20));
      new Client((store = s, cap = 2, isPut = true, id = 3, item = 30));
      new Client((store = s, cap = 2, isPut = false, id = 1, item = 0));
    }
  }
}
test tcQueue [main=Main]: assert Idempotent in { Store, Client, Main };
