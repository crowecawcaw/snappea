// The register contract. See DESIGN.md. Each put writes a distinct positive value, so
// a value names its write; 0 is the initial value. The specs never use the design's
// helpers: the version order is restated here.

// The contract's version order: the higher counter wins, then the higher writer id.
fun SpecSupersedes(a: Version, b: Version): bool {
  if (a.counter != b.counter) { return a.counter > b.counter; }
  return a.writer > b.writer;
}

// Read after write, stated in real-time order only (it never looks at versions):
//  - a get returns the initial value or a value whose put started before the get
//    returned;
//  - a get that starts after put B returned does not return the initial value, nor
//    the value of a put A that returned before B started.
spec ReadAfterWrite observes WriteStarted, WriteDone, ReadStarted, ReadDone {
  // Logical clock: one tick per observed event.
  var now: int;
  // value -> tick its put started / returned
  var started: map[int, int];
  var done: map[int, int];
  // client -> tick its get in flight started
  var readStart: map[int, int];

  start state Watching {
    on WriteStarted do (w: (client: int, value: int)) {
      now = now + 1;
      assert w.value > 0 && !(w.value in started),
        format("value {0} written twice or is the initial value", w.value);
      started[w.value] = now;
    }
    on WriteDone do (w: (client: int, value: int, version: Version)) {
      now = now + 1;
      assert w.value in started && !(w.value in done),
        format("put of {0} returned twice or never started", w.value);
      done[w.value] = now;
    }
    on ReadStarted do (r: (client: int)) {
      now = now + 1;
      readStart[r.client] = now;
    }
    on ReadDone do (r: (client: int, value: int, version: Version)) {
      var b: int;
      now = now + 1;
      assert r.client in readStart, format("client {0} returned a get it never started", r.client);
      assert r.value == 0 || r.value in started,
        format("client {0} read {1}, which was never written", r.client, r.value);
      foreach (b in keys(done)) {
        // Put b returned before this get started; is the value read older?
        if (done[b] < readStart[r.client] && b != r.value) {
          assert r.value != 0,
            format("client {0} read the initial value after the put of {1} returned", r.client, b);
          assert !(r.value in done && done[r.value] < started[b]),
            format("client {0} read stale value {1}: the put of {2} started after it and returned before the get started", r.client, r.value, b);
        }
      }
      readStart -= (r.client);
    }
  }
}

// Replicas: after each write, a replica holds the newest version it has been offered,
// ordered by counter and then by writer id, so a late or repeated write cannot move it
// back.
spec ReplicasKeepNewest observes Stored {
  // replica -> what it holds
  var holding: map[int, Stamped];

  start state Watching {
    on Stored do (s: (replica: int, write: Stamped, holds: Stamped)) {
      var expected: Stamped;
      if (s.replica in holding) { expected = holding[s.replica]; }
      if (SpecSupersedes(s.write.version, expected.version)) { expected = s.write; }
      assert s.holds == expected,
        format("replica {0} holds {1} after write {2}; the newest it was offered is {3}",
               s.replica, s.holds, s.write, expected);
      holding[s.replica] = s.holds;
    }
  }
}

// Progress: every operation the application requests returns, unless a crash
// abandons it.
spec OperationsReturn observes PutRequested, GetRequested, WriteDone, ReadDone, CrashRecovered {
  var pending: int;

  start state Quiet {
    on PutRequested do { Requested(); }
    on GetRequested do { Requested(); }
    on WriteDone do { assert false, "a put returned that nobody asked for"; }
    on ReadDone do { assert false, "a get returned that nobody asked for"; }
    ignore CrashRecovered;
  }

  hot state OperationInFlight {
    on PutRequested do { Requested(); }
    on GetRequested do { Requested(); }
    on WriteDone do { Settled(); }
    on ReadDone do { Settled(); }
    on CrashRecovered do (c: (client: int, abandoned: bool)) {
      if (c.abandoned) { Settled(); }
    }
  }

  fun Requested() {
    pending = pending + 1;
    goto OperationInFlight;
  }

  fun Settled() {
    pending = pending - 1;
    if (pending == 0) { goto Quiet; }
  }
}
