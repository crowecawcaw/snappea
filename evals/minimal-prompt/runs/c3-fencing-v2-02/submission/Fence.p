type RRec = (max: int, value: int);
type RSnap = (version: int, rec: RRec);

event eRLoad: machine;
event eRLoaded: RSnap;
event eRCas: (client: machine, token: int, value: int, expected: int);
event eRCasResult: bool;
event eWriteCommit: (version: int, token: int, value: int);
event eWriteRejected: (version: int, token: int);
event eWriterDone;
event eReadValue: (version: int, value: int);

// Tokens 1..2 have been issued by the lock service in this model.
fun Issued(): int { return 2; }

spec FenceContract observes eWriteCommit, eWriteRejected, eReadValue {
  var maxAt: map[int, int];
  var valAt: map[int, int];
  var latest: int;
  start state Watching {
    entry { maxAt[0] = 0; valAt[0] = 0; }
    on eWriteCommit do (p: (version: int, token: int, value: int)) {
      assert p.token >= 1 && p.token <= Issued(), "accepted a token that was never issued";
      assert p.token >= maxAt[p.version - 1], "accepted a token lower than an accepted one";
      maxAt[p.version] = p.token;
      valAt[p.version] = p.value;
      latest = p.version;
    }
    on eReadValue do (p: (version: int, value: int)) {
      assert p.value == valAt[p.version], "read does not return the last accepted value";
      assert p.version == latest, "read missed a committed write";
    }
    on eWriteRejected do (p: (version: int, token: int)) {
      if (p.version == -1) {
        assert p.token < 1 || p.token > Issued(), "rejected a valid token without looking at the resource";
      } else {
        assert p.token >= 1 && p.token <= Issued();
        assert p.token < maxAt[p.version], "rejected a token that is not lower than the accepted maximum";
      }
    }
  }
}

machine ResStore {
  var version: int;
  var rec: RRec;
  start state Serving {
    on eRLoad do (c: machine) { send c, eRLoaded, (version = version, rec = rec); }
    on eRCas do (p: (client: machine, token: int, value: int, expected: int)) {
      if (p.expected == version) {
        version = version + 1;
        rec = (max = p.token, value = p.value);
        announce eWriteCommit, (version = version, token = p.token, value = p.value);
        send p.client, eRCasResult, true;
      } else {
        send p.client, eRCasResult, false;
      }
    }
  }
}

machine Writer {
  start state Run {
    entry (input: (store: ResStore, coordinator: machine, t1: int, t2: int, two: bool)) {
      var ok: bool;
      ok = Write(input.store, input.t1, 1);
      if (input.two) { ok = Write(input.store, input.t2, 2); }
      send input.coordinator, eWriterDone;
    }
  }

  fun Write(store: ResStore, token: int, value: int): bool {
    var s: RSnap;
    var ok: bool;
    var res: bool;
    var fin: bool;
    if (token < 1 || token > Issued()) {
      announce eWriteRejected, (version = -1, token = token);
      return false;
    }
    while (!fin) {
      send store, eRLoad, this;
      receive { case eRLoaded: (x: RSnap) { s = x; } }
      if (token < s.rec.max) {
        announce eWriteRejected, (version = s.version, token = token);
        fin = true;
      } else {
        send store, eRCas, (client = this, token = token, value = value, expected = s.version);
        receive { case eRCasResult: (b: bool) { ok = b; } }
        if (ok) { res = true; fin = true; }
      }
    }
    return res;
  }
}

// snappea: driver
machine FenceScenario {
  var done: int;
  var store: ResStore;
  start state Run {
    entry (input: (a1: int, a2: int, atwo: bool, b1: int)) {
      store = new ResStore();
      new Writer((store = store, coordinator = this, t1 = input.a1, t2 = input.a2, two = input.atwo));
      new Writer((store = store, coordinator = this, t1 = input.b1, t2 = 0, two = false));
    }
    on eWriterDone do {
      done = done + 1;
      if (done == 2) {
        send store, eRLoad, this;
        receive { case eRLoaded: (x: RSnap) { announce eReadValue, (version = x.version, value = x.rec.value); } }
      }
    }
  }
}

// tokens 0..3 cross both ends of the issued range 1..2, concurrent writers
// snappea: driver
machine FenceGenerated { start state Init { entry { new FenceScenario((a1 = choose(4), a2 = 0, atwo = false, b1 = choose(4))); } } }
// one writer writes 2 then 1 while another writes 1 or 2: stale token must stay rejected, equal accepted
// snappea: driver
machine Stale { start state Init { entry { new FenceScenario((a1 = 2, a2 = 1 + choose(2), atwo = true, b1 = 1 + choose(2))); } } }

test tcFence [main=FenceGenerated]: assert FenceContract in { FenceGenerated, FenceScenario, Writer, ResStore };
test tcStale [main=Stale]: assert FenceContract in { Stale, FenceScenario, Writer, ResStore };
