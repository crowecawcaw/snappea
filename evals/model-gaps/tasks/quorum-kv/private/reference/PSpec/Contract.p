// The register contract. See DESIGN.md. Each put writes a distinct positive value, so
// a value names its write; 0 is the initial value. The specs never use the design's
// helpers: the version order is restated here.

// The contract's version order: the higher counter wins, then the higher writer id.
fun SpecSupersedes(a: Version, b: Version): bool {
  if (a.counter != b.counter) { return a.counter > b.counter; }
  return a.writer > b.writer;
}

// Atomic register, stated in real-time order only (it never looks at versions).
// A value is established once its put returned or a get returned it.
//  - a get returns the initial value or a value whose put started before the get
//    returned;
//  - a get that starts after value b was established returns b or a value written
//    after b: never the initial value, never a value b's write came after.
// "Written after" is what the history proves: a put that started after b was
// established, or a value some get returned although b was established before that
// get started.
spec AtomicRegister observes WriteStarted, WriteDone, ReadStarted, ReadDone, CrashRecovered {
  // Logical clock: one tick per observed event.
  var now: int;
  // value -> tick its put started
  var started: map[int, int];
  // value -> tick it was established
  var established: map[int, int];
  // value -> the values known to be written after it
  var later: map[int, set[int]];
  // client -> tick its get in flight started
  var readStart: map[int, int];

  start state Watching {
    on WriteStarted do (w: (client: int, value: int)) {
      var b: int;
      now = now + 1;
      assert w.value > 0 && !(w.value in started),
        format("value {0} written twice or is the initial value", w.value);
      started[w.value] = now;
      later[w.value] = default(set[int]);
      foreach (b in keys(established)) {
        later[b] += (w.value);
      }
    }
    on WriteDone do (w: (client: int, value: int, version: Version)) {
      now = now + 1;
      assert w.value in started, format("put of {0} returned but never started", w.value);
      if (!(w.value in established)) { established[w.value] = now; }
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
      foreach (b in keys(established)) {
        if (established[b] < readStart[r.client] && b != r.value) {
          assert r.value != 0,
            format("client {0} read the initial value after {1} was written or read", r.client, b);
          assert !(b in later[r.value]),
            format("client {0} read stale value {1}: {2} was written after it and was already written or read when the get started", r.client, r.value, b);
          later[b] += (r.value);
        }
      }
      if (r.value != 0 && !(r.value in established)) { established[r.value] = now; }
      readStart -= (r.client);
    }
    on CrashRecovered do (c: (client: int, abandoned: bool)) {
      if (c.client in readStart) { readStart -= (c.client); }
    }
  }
}

// Replicas: after each write, a replica holds the newest version it has been offered,
// ordered by counter and then by writer id, so a late or repeated write cannot move it
// back. And a version names one write: the same version never carries two values, and
// a value is never written under two versions.
spec ReplicasKeepNewest observes Stored {
  // replica -> what it holds
  var holding: map[int, Stamped];
  var valueOf: map[Version, int];
  var versionOf: map[int, Version];

  start state Watching {
    on Stored do (s: (replica: int, write: Stamped, holds: Stamped)) {
      var expected: Stamped;
      if (s.write.version in valueOf) {
        assert valueOf[s.write.version] == s.write.value,
          format("version {0} carries value {1} and value {2}", s.write.version, valueOf[s.write.version], s.write.value);
      }
      if (s.write.value in versionOf) {
        assert versionOf[s.write.value] == s.write.version,
          format("value {0} was written as version {1} and as version {2}", s.write.value, versionOf[s.write.value], s.write.version);
      }
      valueOf[s.write.version] = s.write.value;
      versionOf[s.write.value] = s.write.version;
      if (s.replica in holding) { expected = holding[s.replica]; }
      if (SpecSupersedes(s.write.version, expected.version)) { expected = s.write; }
      assert s.holds == expected,
        format("replica {0} holds {1} after write {2}; the newest it was offered is {3}",
               s.replica, s.holds, s.write, expected);
      holding[s.replica] = s.holds;
    }
  }
}

// An operation returns only when a write quorum has what it returns: two different
// replicas handled a write of that version (the put's own write, or the get's
// write-back) before WriteDone or ReadDone. A put's version is its own (the writer is
// the client) and above every version a put or get returned before the put started.
spec QuorumBeforeReturn observes WriteStarted, WriteDone, ReadDone, Stored {
  // version -> the replicas that handled a write of it
  var handled: map[Version, set[int]];
  // the newest version any operation returned so far
  var newest: Version;
  // client -> the newest returned version when its put started
  var floor: map[int, Version];

  start state Watching {
    on Stored do (s: (replica: int, write: Stamped, holds: Stamped)) {
      if (!(s.write.version in handled)) { handled[s.write.version] = default(set[int]); }
      handled[s.write.version] += (s.replica);
    }
    on WriteStarted do (w: (client: int, value: int)) {
      floor[w.client] = newest;
    }
    on WriteDone do (w: (client: int, value: int, version: Version)) {
      assert w.version.writer == w.client,
        format("client {0} wrote version {1}, whose writer id is not its own", w.client, w.version);
      if (w.client in floor) {
        assert SpecSupersedes(w.version, floor[w.client]),
          format("client {0} wrote {1} as version {2}, not above version {3}, which had returned before the put started",
                 w.client, w.value, w.version, floor[w.client]);
      }
      CheckQuorum(w.client, w.version);
    }
    on ReadDone do (r: (client: int, value: int, version: Version)) {
      CheckQuorum(r.client, r.version);
    }
  }

  fun CheckQuorum(client: int, version: Version) {
    var n: int;
    if (version in handled) { n = sizeof(handled[version]); }
    assert n >= 2,
      format("client {0} returned version {1}, which only {2} replica(s) had been written", client, version, n);
    if (SpecSupersedes(version, newest)) { newest = version; }
  }
}

// Progress, and one operation at a time: every requested operation starts as the
// operation that was requested and then returns, unless a crash abandons it. A
// restart reports `abandoned` exactly when an operation was in flight, and that
// operation never returns.
spec OperationsReturn observes PutRequested, GetRequested, WriteStarted, WriteDone, ReadStarted, ReadDone, CrashRecovered {
  // client -> what it was asked and has not returned: a put's value, or 0 for a get
  var asked: map[int, int];
  // the clients whose operation has started
  var running: set[int];

  start state Quiet {
    on PutRequested do (p: (client: int, value: int)) { Ask(p.client, p.value); }
    on GetRequested do (g: (client: int)) { Ask(g.client, 0); }
    on WriteStarted do (w: (client: int, value: int)) { Unasked(w.client); }
    on ReadStarted do (r: (client: int)) { Unasked(r.client); }
    on WriteDone do (w: (client: int, value: int, version: Version)) { Unasked(w.client); }
    on ReadDone do (r: (client: int, value: int, version: Version)) { Unasked(r.client); }
    on CrashRecovered do (c: (client: int, abandoned: bool)) { Restart(c.client, c.abandoned); }
  }

  hot state OperationInFlight {
    on PutRequested do (p: (client: int, value: int)) { Ask(p.client, p.value); }
    on GetRequested do (g: (client: int)) { Ask(g.client, 0); }
    on WriteStarted do (w: (client: int, value: int)) { Start(w.client, w.value); }
    on ReadStarted do (r: (client: int)) { Start(r.client, 0); }
    on WriteDone do (w: (client: int, value: int, version: Version)) { Return(w.client, w.value); }
    on ReadDone do (r: (client: int, value: int, version: Version)) { Return(r.client, 0); }
    on CrashRecovered do (c: (client: int, abandoned: bool)) { Restart(c.client, c.abandoned); }
  }

  fun Ask(client: int, value: int) {
    assert !(client in asked), format("the driver asked client {0} for two operations at once", client);
    asked[client] = value;
    goto OperationInFlight;
  }

  fun Unasked(client: int) {
    assert false, format("client {0} started or returned an operation nobody asked for", client);
  }

  fun Start(client: int, value: int) {
    assert client in asked && !(client in running),
      format("client {0} started an operation it was not asked for, or twice", client);
    assert asked[client] == value,
      format("client {0} started {1}, but was asked for {2} (0 is a get)", client, value, asked[client]);
    running += (client);
  }

  fun Return(client: int, value: int) {
    assert client in running,
      format("client {0} returned an operation that was not in flight (never started, already returned, or abandoned by a crash)", client);
    assert asked[client] == value,
      format("client {0} returned {1}, but was asked for {2} (0 is a get)", client, value, asked[client]);
    Settle(client);
  }

  fun Restart(client: int, abandoned: bool) {
    assert abandoned == (client in running),
      format("client {0} restarted and reported abandoned={1}, but operation in flight={2}", client, abandoned, client in running);
    if (abandoned) { Settle(client); }
  }

  fun Settle(client: int) {
    running -= (client);
    asked -= (client);
    if (sizeof(asked) == 0) { goto Quiet; }
  }
}
