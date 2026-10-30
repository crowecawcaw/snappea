// Model: bounded queue with idempotent retried requests, one atomic record.
event ePut: (id: int, item: int);
event eTake: int;
event ePutResp: (id: int, ok: bool);
event eTakeResp: (id: int, val: int); // -1 means empty
event eObserved: (contents: seq[int]);

machine QueueSvc {
  var buf: seq[int];
  var putDone: map[int, bool];
  var takeDone: map[int, int];
  start state Ready {
    on ePut do (r: (id: int, item: int)) {
      var ok: bool;
      if (r.id in putDone) { ok = putDone[r.id]; }
      else {
        ok = sizeof(buf) < 2;
        if (ok) { buf += (sizeof(buf), r.item); }
        putDone[r.id] = ok;
      }
      send this, eObserved, (contents = buf,);
    }
    on eTake do (id: int) {
      var v: int;
      if (id in takeDone) { v = takeDone[id]; }
      else {
        v = -1;
        if (sizeof(buf) > 0) { v = buf[0]; buf -= (0); }
        takeDone[id] = v;
      }
      send this, eObserved, (contents = buf,);
    }
    on eObserved do (o: (contents: seq[int])) { assert sizeof(o.contents) <= 2, "capacity exceeded"; }
  }
}

machine Client {
  start state Run {
    entry {
      var q: QueueSvc;
      q = new QueueSvc();
      send q, eTake, 0; // empty
      send q, ePut, (id = 1, item = 7);
      send q, ePut, (id = 1, item = 7); // retry
      send q, ePut, (id = 2, item = 8);
      send q, ePut, (id = 3, item = 9); // full
      send q, eTake, 1;
      send q, eTake, 1; // retry
      send q, ePut, (id = 4, item = 5);
    }
  }
}

test tcRetry [main = Client]: { Client, QueueSvc };
