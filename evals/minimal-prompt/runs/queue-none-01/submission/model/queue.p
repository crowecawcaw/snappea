// Model: the queue is one versioned record updated atomically (CAS); put/take are
// idempotent per request id. Capacity is 2. Requests may be duplicated/retried.
event ePut: (client: machine, rid: int, item: int);
event eTake: (client: machine, rid: int, pad: int);
event ePutResp: (rid: int, ok: bool);
event eTakeResp: (rid: int, val: int);   // -1 means empty

machine Queue {
  var items: seq[int];
  var puts: map[int, bool];
  var takes: map[int, int];
  start state Ready {
    on ePut do (r: (client: machine, rid: int, item: int)) {
      if (!(r.rid in puts)) {
        if (sizeof(items) < 2) { items += (sizeof(items), r.item); puts[r.rid] = true; }
        else { puts[r.rid] = false; }
      }
      assert sizeof(items) <= 2, "capacity exceeded";
      send r.client, ePutResp, (rid = r.rid, ok = puts[r.rid]);
    }
    on eTake do (r: (client: machine, rid: int, pad: int)) {
      if (!(r.rid in takes)) {
        if (sizeof(items) > 0) { takes[r.rid] = items[0]; items -= (0); }
        else { takes[r.rid] = -1; }
      }
      send r.client, eTakeResp, (rid = r.rid, val = takes[r.rid]);
    }
  }
}

// Each client retries every request; retried answers must equal the first answer.
machine Client {
  var q: machine;
  var putAns: map[int, bool];
  var takeAns: map[int, int];
  start state S {
    entry (a: (q: machine, base: int)) {
      var i: int;
      q = a.q;
      i = 0;
      while (i < 3) {
        send q, ePut, (client = this, rid = a.base + i, item = a.base + i);
        send q, ePut, (client = this, rid = a.base + i, item = a.base + i);
        i = i + 1;
      }
      i = 0;
      while (i < 3) {
        send q, eTake, (client = this, rid = a.base + i, pad = 0);
        send q, eTake, (client = this, rid = a.base + i, pad = 0);
        i = i + 1;
      }
    }
    on ePutResp do (r: (rid: int, ok: bool)) {
      if (r.rid in putAns) { assert putAns[r.rid] == r.ok, "retried put changed result"; }
      else { putAns[r.rid] = r.ok; }
    }
    on eTakeResp do (r: (rid: int, val: int)) {
      if (r.rid in takeAns) { assert takeAns[r.rid] == r.val, "retried take changed result"; }
      else { takeAns[r.rid] = r.val; }
    }
  }
}

machine Main {
  start state S { entry { var q: machine; q = new Queue(); new Client((q = q, base = 0)); new Client((q = q, base = 10)); } }
}
test tcQ [main=Main]: { Main, Queue, Client };
