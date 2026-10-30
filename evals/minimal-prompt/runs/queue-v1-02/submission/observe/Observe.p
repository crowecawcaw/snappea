// Observation monitor for real call histories (sequential, one queue, capacity from eOpen).
event eOpen: (cap: int);
event ePut: (id: int, item: int, ok: bool);
event eTake: (id: int, ok: bool, val: int);   // val -1 when empty
event eSnap: (items: seq[int]);
spec QueueObserved observes eOpen, ePut, eTake, eSnap {
  var cap: int; var q: seq[int];
  var puts: map[int, (item: int, ok: bool)]; var takes: map[int, (ok: bool, val: int)];
  var i: int;
  start state S {
    on eOpen do (c: (cap: int)) { cap = c.cap; }
    on ePut do (p: (id: int, item: int, ok: bool)) {
      var ok: bool;
      if (p.id in puts) { assert puts[p.id].item == p.item; ok = puts[p.id].ok; }
      else { ok = sizeof(q) < cap; if (ok) { q += (sizeof(q), p.item); } puts[p.id] = (item = p.item, ok = ok); }
      assert p.ok == ok, "wrong put result";
    }
    on eTake do (p: (id: int, ok: bool, val: int)) {
      var r: (ok: bool, val: int);
      if (p.id in takes) { r = takes[p.id]; }
      else {
        r = (ok = false, val = -1);
        if (sizeof(q) > 0) { r = (ok = true, val = q[0]); q -= (0); }
        takes[p.id] = r;
      }
      assert p.ok == r.ok && p.val == r.val, "wrong take result";
    }
    on eSnap do (e: (items: seq[int])) {
      var s: seq[int];
      s = e.items;
      assert sizeof(s) == sizeof(q), "snapshot length";
      i = 0;
      while (i < sizeof(q)) { assert s[i] == q[i], "snapshot contents"; i = i + 1; }
    }
  }
}
