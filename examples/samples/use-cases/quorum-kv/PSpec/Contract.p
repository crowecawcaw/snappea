// The register contract, stated over what clients announce when an operation starts
// and when it returns. The spec uses only real-time order; it never sees versions.
// Each write writes a distinct value, so a value names its write. 0 is the initial value.

event WriteStarted: (value: int);
event WriteDone: (value: int);
event ReadStarted: (reader: int);
event ReadDone: (reader: int, value: int);

// Regular register (read-after-write):
//  - a read returns the initial value or a value whose write started before the read
//    returned;
//  - a read never returns a value superseded before the read started: if write B
//    started after write A completed, and B completed before the read started, the
//    read must not return A's value (nor the initial value).
spec ReadAfterWrite observes WriteStarted, WriteDone, ReadStarted, ReadDone {
  // Logical clock: one tick per observed event.
  var now: int;
  // value -> tick its write started / completed
  var started: map[int, int];
  var done: map[int, int];
  // reader -> tick its current read started
  var readStart: map[int, int];

  start state Watching {
    on WriteStarted do (w: (value: int)) {
      now = now + 1;
      assert w.value > 0 && !(w.value in started), format("value {0} written twice or is the initial value", w.value);
      started[w.value] = now;
    }
    on WriteDone do (w: (value: int)) {
      now = now + 1;
      assert w.value in started && !(w.value in done), format("write of {0} completed twice or never started", w.value);
      done[w.value] = now;
    }
    on ReadStarted do (r: (reader: int)) {
      now = now + 1;
      assert !(r.reader in readStart), format("reader {0} started two reads at once", r.reader);
      readStart[r.reader] = now;
    }
    on ReadDone do (r: (reader: int, value: int)) {
      var b: int;
      now = now + 1;
      assert r.reader in readStart, format("reader {0} returned a read it never started", r.reader);
      assert r.value == 0 || r.value in started, format("reader {0} returned {1}, which was never written", r.reader, r.value);
      foreach (b in keys(done)) {
        // Write b completed before this read started; is the returned value older?
        if (done[b] < readStart[r.reader] && b != r.value) {
          assert r.value != 0,
            format("reader {0} returned the initial value after write {1} completed", r.reader, b);
          assert !(r.value in done && done[r.value] < started[b]),
            format("reader {0} returned stale value {1}; write {2} completed before the read started and after {1} was written", r.reader, r.value, b);
        }
      }
      readStart -= (r.reader);
    }
  }
}

// Progress: every operation the driver plans returns. The driver announces the count
// before starting clients.
event OpsPlanned: int;

spec OperationsReturn observes OpsPlanned, WriteDone, ReadDone {
  var pending: int;

  start state Idle {
    on OpsPlanned do (n: int) {
      assert n > 0, "planned no operations";
      pending = n;
      goto Busy;
    }
    on WriteDone do { assert false, "write returned with no work planned"; }
    on ReadDone do { assert false, "read returned with no work planned"; }
  }

  hot state Busy {
    on OpsPlanned do { assert false, "operations planned twice"; }
    on WriteDone do { Finish(); }
    on ReadDone do { Finish(); }
  }

  fun Finish() {
    pending = pending - 1;
    if (pending == 0) { goto Idle; }
  }
}
