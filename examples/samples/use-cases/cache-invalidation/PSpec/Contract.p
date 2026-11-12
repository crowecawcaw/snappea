// The cache contract, stated over events the database, the cache and the clients
// announce at their commit points. The specs rebuild what must be true from the
// history alone; they never read the cache's lease state.

event eDbCommitted: (value: int);
event eInvalidated;
event eCacheHit: (value: int);
event eCacheFilled: (value: int);
event eCacheMiss;
event eReadStarted: (reader: int);
event eReadReturned: (reader: int, value: int, fromCache: bool);
event eWriteDone: (value: int);
// From the driver: every client has finished (nothing is in flight).
event eQuiesced;
// From the driver: the read-through probe after quiescence has finished.
event eProbeDone;

// Safety. Writes commit increasing values, so "older" is "smaller".
//  1. A cache hit returns a value the database committed at some point not before the
//     last completed invalidation: at least the value committed when it ran.
//  2. Read-after-write: a read that starts after a write completed (database write and
//     cache delete both acknowledged) returns that write's value or a newer one.
//  3. At quiescence the cache is empty or equal to the database, and a read-through
//     fills it: the probe's hits return exactly the database's value, and it hits.
spec CacheFresh observes eDbCommitted, eInvalidated, eCacheMiss, eCacheFilled, eCacheHit, eReadStarted,
    eReadReturned, eWriteDone, eQuiesced, eProbeDone {
  var db: int;
  var committed: set[int];
  // Database value when the last invalidation ran.
  var floor: int;
  // Value of the last write whose client saw both acknowledgements.
  var written: int;
  // reader -> `written` when its current read started
  var readFloor: map[int, int];
  // The probe returned a read served from the cache.
  var probeHit: bool;
  // Every value the cache was ever filled with.
  var filled: set[int];
  // Hits and misses the cache recorded that no returned read has accounted for yet.
  var hits: int;
  var misses: int;
  // What the cache holds, rebuilt from fills and invalidations.
  var cached: bool;
  var cachedValue: int;

  start state Running {
    entry { committed += (0); }
    on eDbCommitted do (p: (value: int)) { Commit(p.value); }
    on eInvalidated do { floor = db; cached = false; }
    on eCacheMiss do { Missed(); }
    on eCacheFilled do (p: (value: int)) { Filled(p.value); }
    on eCacheHit do (p: (value: int)) {
      Hit(p.value);
      assert p.value in committed, format("cache served {0}, which was never committed", p.value);
      assert p.value >= floor,
        format("cache served stale value {0} after an invalidation at committed value {1}", p.value, floor);
    }
    on eReadStarted do (p: (reader: int)) {
      assert !(p.reader in readFloor), format("reader {0} started two reads at once", p.reader);
      readFloor[p.reader] = written;
    }
    on eReadReturned do (p: (reader: int, value: int, fromCache: bool)) {
      assert p.reader in readFloor, format("reader {0} returned a read it never started", p.reader);
      assert p.value in committed, format("reader {0} returned {1}, which was never committed", p.reader, p.value);
      assert p.value >= readFloor[p.reader],
        format("reader {0} returned stale value {1} after write {2} completed", p.reader, p.value, readFloor[p.reader]);
      FromCache(p.value, p.fromCache);
      readFloor -= (p.reader);
    }
    on eWriteDone do (p: (value: int)) {
      assert p.value in committed, format("write of {0} completed but was never committed", p.value);
      written = p.value;
    }
    on eQuiesced goto Quiesced;
    on eProbeDone do { assert false, "probe finished before the clients quiesced"; }
  }

  // No writer is running any more: the cache must agree with the database.
  state Quiesced {
    on eDbCommitted do { assert false, "write committed after quiescence"; }
    on eInvalidated do { assert false, "invalidation after quiescence"; }
    on eCacheMiss do { Missed(); }
    on eCacheFilled do (p: (value: int)) { Filled(p.value); }
    on eCacheHit do (p: (value: int)) {
      Hit(p.value);
      assert p.value == db, format("cache holds stale value {0} at quiescence; database has {1}", p.value, db);
    }
    on eReadStarted do (p: (reader: int)) {
      assert !(p.reader in readFloor), format("reader {0} started two reads at once", p.reader);
      readFloor[p.reader] = db;
    }
    on eReadReturned do (p: (reader: int, value: int, fromCache: bool)) {
      assert p.reader in readFloor, format("reader {0} returned a read it never started", p.reader);
      assert p.value == db, format("reader {0} returned {1} at quiescence; database has {2}", p.reader, p.value, db);
      FromCache(p.value, p.fromCache);
      if (p.fromCache) { probeHit = true; }
      readFloor -= (p.reader);
    }
    on eWriteDone do { assert false, "write completed after quiescence"; }
    on eQuiesced do { assert false, "quiesced twice"; }
    on eProbeDone do {
      assert probeHit, "no probe read after quiescence was served from the cache";
    }
  }

  // Each read is one get: a hit it returns, or a miss it reads through.
  fun FromCache(v: int, fromCache: bool) {
    if (fromCache) {
      assert v in filled, format("a read claims a cache hit for {0}, which was never in the cache", v);
      assert hits > 0, format("a read returned {0} from the cache with no recorded hit", v);
      hits = hits - 1;
    } else {
      assert misses > 0, format("a read returned {0} from the database with no recorded miss", v);
      misses = misses - 1;
    }
  }

  fun Missed() {
    misses = misses + 1;
    assert !cached, format("cache missed although it was filled with {0} and not invalidated since", cachedValue);
  }

  fun Filled(v: int) {
    assert v in committed, format("cache filled with {0}, which was never committed", v);
    cached = true;
    cachedValue = v;
    filled += (v);
  }

  fun Hit(v: int) {
    hits = hits + 1;
    assert cached, format("cache hit with {0} although nothing was filled since the last invalidation", v);
    assert v == cachedValue, format("cache hit returned {0}, but it was filled with {1}", v, cachedValue);
  }

  fun Commit(v: int) {
    assert v > db, format("model writes increasing values, got {0} after {1}", v, db);
    db = v;
    committed += (v);
  }
}

// Progress: every read and write the driver plans returns. The driver announces the
// number of operations before starting any, so a client that stops early fails.
event eWorkPlanned: int;

spec OperationsFinish observes eWorkPlanned, eReadReturned, eWriteDone {
  var pending: int;

  start state Idle {
    on eWorkPlanned do (n: int) { Plan(n); }
    on eReadReturned do { assert false, "read returned with no work planned"; }
    on eWriteDone do { assert false, "write finished with no work planned"; }
  }

  hot state Busy {
    on eWorkPlanned do { assert false, "work planned twice at once"; }
    // This spec only counts; CacheFresh checks the values.
    on eReadReturned do { Finish(); }
    on eWriteDone do { Finish(); }
  }

  fun Plan(n: int) {
    assert n > 0, "planned no operations";
    pending = n;
    goto Busy;
  }

  fun Finish() {
    pending = pending - 1;
    if (pending == 0) { goto Idle; }
  }
}
