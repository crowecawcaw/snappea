// Model of the CAS-based shared queue over a versioned single-record Store.
type Puts = map[string, (item: int, ok: bool)];
type Takes = map[string, int];

event eConfig: (cap: int);
event eLoad: (client: Client);
event eLoaded: (version: int, items: seq[int], puts: Puts, takes: Takes);
event eCas: (client: Client, version: int, items: seq[int], puts: Puts, takes: Takes, kind: int, rid: string, arg: int, result: int);
event eCasResult: bool;
event eCommit: (kind: int, rid: string, arg: int, result: int);
event eReturn: (kind: int, rid: string, result: int);
event eFinished;

// Contract, derived from the task: FIFO, bounded by capacity; put fails exactly when
// full; take gives the oldest item or none (-1); a retry returns the original result;
// each (kind, request id) takes effect at most once. Rejected requests count as effects.
spec QueueContract observes eConfig, eCommit, eReturn, eFinished {
  var items: seq[int];
  var cap: int;
  var putRes: map[string, int];
  var takeRes: map[string, int];
  var returns: int;
  start hot state Running {
    on eConfig do (c: (cap: int)) { cap = c.cap; }
    on eCommit do (c: (kind: int, rid: string, arg: int, result: int)) {
      if (c.kind == 0) {
        assert !(c.rid in putRes), "request id took effect twice";
        putRes[c.rid] = c.result;
      } else {
        assert !(c.rid in takeRes), "request id took effect twice";
        takeRes[c.rid] = c.result;
      }
      if (c.kind == 0) {
        if (c.result == 1) {
          assert sizeof(items) < cap, "put accepted into a full queue";
          items += (sizeof(items), c.arg);
        } else {
          assert c.result == 0, "bad put result";
          assert sizeof(items) >= cap, "put rejected though queue not full";
        }
      } else {
        if (sizeof(items) == 0) {
          assert c.result == -1, "take on empty returned a value";
        } else {
          assert c.result == items[0], "take did not return the oldest item";
          items -= (0);
        }
      }
    }
    on eReturn do (r: (kind: int, rid: string, result: int)) {
      if (r.kind == 0) {
        assert r.rid in putRes, "returned before the request took effect";
        assert putRes[r.rid] == r.result, "retry did not return the original result";
      } else {
        assert r.rid in takeRes, "returned before the request took effect";
        assert takeRes[r.rid] == r.result, "retry did not return the original result";
      }
      returns = returns + 1;
    }
    on eFinished do {
      assert returns >= 1, "no operation completed";
      goto Done;
    }
  }
  state Done { ignore eConfig, eCommit, eReturn, eFinished; }
}

machine Store {
  var version: int;
  var items: seq[int];
  var puts: Puts;
  var takes: Takes;
  start state Serving {
    on eLoad do (p: (client: Client)) {
      send p.client, eLoaded, (version = version, items = items, puts = puts, takes = takes);
    }
    on eCas do (p: (client: Client, version: int, items: seq[int], puts: Puts, takes: Takes, kind: int, rid: string, arg: int, result: int)) {
      if (p.version == version) {
        items = p.items; puts = p.puts; takes = p.takes;
        version = version + 1;
        announce eCommit, (kind = p.kind, rid = p.rid, arg = p.arg, result = p.result);
        send p.client, eCasResult, true;
      } else {
        send p.client, eCasResult, false;
      }
    }
  }
}

machine Client {
  var cap: int;
  start state Run {
    entry (a: (s: Store, cap: int, nops: int)) {
      var n: int; var kind: int; var rid: string; var arg: int; var result: int;
      var v: int; var its: seq[int]; var ps: Puts; var ts: Takes;
      var ok: bool; var done: bool; var old: (item: int, ok: bool);
      while (n < a.nops) {
        kind = choose(2);
        // small id space: clients collide and retry each other's ids
        if (choose(2) == 0) { rid = "a"; arg = 10; } else { rid = "b"; arg = 11; }  // same id => same args
        done = false;
        while (!done) {
          send a.s, eLoad, (client = this,);
          receive { case eLoaded: (l: (version: int, items: seq[int], puts: Puts, takes: Takes)) {
            v = l.version; its = l.items; ps = l.puts; ts = l.takes;
          } }
          if (kind == 0 && rid in ps) {
            result = 0; if (ps[rid].ok) { result = 1; }
            done = true;
          } else if (kind == 1 && rid in ts) {
            result = ts[rid];
            done = true;
          } else {
            if (kind == 0) {
              if (sizeof(its) < a.cap) { its += (sizeof(its), arg); result = 1; ps[rid] = (item = arg, ok = true); }
              else { result = 0; ps[rid] = (item = arg, ok = false); }
            } else {
              if (sizeof(its) > 0) { result = its[0]; its -= (0); } else { result = -1; }
              ts[rid] = result;
            }
            send a.s, eCas, (client = this, version = v, items = its, puts = ps, takes = ts, kind = kind, rid = rid, arg = arg, result = result);
            receive { case eCasResult: (b: bool) { ok = b; } }
            done = ok;
          }
        }
        announce eReturn, (kind = kind, rid = rid, result = result);
        n = n + 1;
      }
      announce eFinished;
    }
  }
}

machine Main {
  start state Init {
    entry {
      var s: Store; var cap: int;
      cap = choose(2) + 1;
      announce eConfig, (cap = cap,);
      s = new Store();
      new Client((s = s, cap = cap, nops = 2));
      new Client((s = s, cap = cap, nops = 2));
    }
  }
}

module Model = { Main, Store, Client };
test tcQueue [main=Main]: assert QueueContract in Model;
