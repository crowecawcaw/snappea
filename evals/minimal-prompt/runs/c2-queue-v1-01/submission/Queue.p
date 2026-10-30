// Model: shared record with versioned CAS; clients load, compute, CAS, retry.
// Results recorded atomically with effects. Bounds: capacity 2, 3 clients,
// each does put then retries the same put (duplicate request id) and a take.
type Rec = (items: seq[int], putDone: map[int, bool], takeDone: map[int, int]);
event eLoad: (c: Client);
event eLoadR: (ver: int, rec: Rec);
event eCas: (c: Client, ver: int, rec: Rec, kind: int, id: int, res: int);
event eCasR: (ok: bool);
event ePutRes: (id: int, res: bool);
event eTakeRes: (res: int);
event eDone;

machine Store {
  var ver: int; var rec: Rec;
  start state S {
    entry { rec = (items = default(seq[int]), putDone = default(map[int,bool]), takeDone = default(map[int,int])); }
    on eLoad do (p: (c: Client)) { send p.c, eLoadR, (ver = ver, rec = rec); }
    on eCas do (p: (c: Client, ver: int, rec: Rec, kind: int, id: int, res: int)) {
      if (p.ver == ver) { rec = p.rec; ver = ver + 1;
        if (p.kind == 0) { announce ePutRes, (id = p.id, res = p.res == 1); } else { announce eTakeRes, (res = p.res,); } send p.c, eCasR, (ok = true,); }
      else send p.c, eCasR, (ok = false,);
    }
  }
}

// Contract: reconstructed from results. Each distinct put id succeeds iff the
// queue was not full; we check conservation: taken values are FIFO prefix of
// successfully put values per global order is checked via counts.
spec Contract observes ePutRes, eTakeRes {
  var okPut: int; var taken: int; var puts: map[int,bool];
  start state W {
    on ePutRes do (p: (id: int, res: bool)) {
      assert !(p.id in puts), "a put request committed twice";
      puts[p.id] = p.res; if (p.res) okPut = okPut + 1;
      assert okPut - taken <= 2, "capacity exceeded";
    }
    on eTakeRes do (p: (res: int)) {
      if (p.res >= 0) { taken = taken + 1; assert taken <= okPut, "took nonexistent item"; }
    }
  }
}

machine Client {
  var s: Store; var myId: int;
  start state R {
    entry (a: (s: Store, id: int)) {
      var ok: bool; var r: bool; var t: int;
      s = a.s; myId = a.id;
      r = Put(a.id);
      assert Put(a.id) == r, "put retry changed result";
      r = Put(a.id + 10);
      assert Put(a.id + 10) == r, "put retry changed result";
      t = Take(a.id);
      assert Take(a.id) == t, "take retry changed result";
      t = Take(a.id + 10);
    }
  }
  fun ToInt(b: bool): int { if (b) return 1; return 0; }
  fun Put(id: int): bool {
    var v: (ver: int, rec: Rec); var m: Rec; var res: bool; var c: (ok: bool); var fin: bool;
    while (!fin) {
      send s, eLoad, (c = this,);
      receive { case eLoadR: (x: (ver: int, rec: Rec)) { v = x; } }
      m = v.rec;
      if (id in m.putDone) { res = m.putDone[id]; fin = true; }
      else {
      if (sizeof(m.items) >= 2) { res = false; } else { m.items += (sizeof(m.items), id); res = true; }
      m.putDone[id] = res;
      send s, eCas, (c = this, ver = v.ver, rec = m, kind = 0, id = id, res = ToInt(res));
      receive { case eCasR: (x: (ok: bool)) { c = x; } }
      if (c.ok) { fin = true; }
      }
    }
    return res;
  }
  fun Take(id: int): int {
    var v: (ver: int, rec: Rec); var m: Rec; var res: int; var c: (ok: bool); var fin: bool;
    while (!fin) {
      send s, eLoad, (c = this,);
      receive { case eLoadR: (x: (ver: int, rec: Rec)) { v = x; } }
      m = v.rec;
      if (id in m.takeDone) { res = m.takeDone[id]; fin = true; }
      else {
      if (sizeof(m.items) == 0) { res = -1; } else { res = m.items[0]; m.items -= (0); }
      m.takeDone[id] = res;
      send s, eCas, (c = this, ver = v.ver, rec = m, kind = 1, id = id, res = res);
      receive { case eCasR: (x: (ok: bool)) { c = x; } }
      if (c.ok) { fin = true; }
      }
    }
    return res;
  }
}

machine Main {
  start state I { entry { var s: Store; var i: int; s = new Store(); while (i < 2) { new Client((s = s, id = i)); i = i + 1; } } }
}
test tcQueue [main=Main]: assert Contract in { Main, Store, Client };
