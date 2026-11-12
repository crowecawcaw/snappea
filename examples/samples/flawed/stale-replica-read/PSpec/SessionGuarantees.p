// Session guarantees for one register (one key), stated over the order in which the
// primary commits writes and the values reads return. Every written value is distinct,
// so the spec knows each value's position in the commit order without trusting any
// log sequence number the model computes.

enum tOp { Read, Write }

// From the driver, before anything starts: each session's operations, in order.
event eSessionsPlanned: map[int, seq[tOp]];
// From the primary, inside the write's commit.
event eWritten: (session: int, value: int);
// From the replica, at the moment it serves a read.
event eReadServed: (session: int, value: int);

// Safety, per session:
//   read-your-writes: a read returns the session's own last write or something newer;
//   monotonic reads: a read never returns something older than an earlier read did.
// Reads return only committed values (or the initial value 0).
// Progress: every planned operation completes (hot state).
spec SessionGuarantees observes eSessionsPlanned, eWritten, eReadServed {
  // value -> position in the commit order (initial value 0 has position 0), and back
  var position: map[int, int];
  var order: seq[int];
  var plans: map[int, seq[tOp]];
  // session -> index of its next operation
  var step: map[int, int];
  // session -> position of its last write, and of the newest value it has read
  var ownWrite: map[int, int];
  var lastRead: map[int, int];
  var remaining: int;

  start state Idle {
    on eSessionsPlanned do (p: map[int, seq[tOp]]) {
      var s: int;
      plans = p;
      position[0] = 0;
      order += (0, 0);
      foreach (s in keys(p)) {
        step[s] = 0;
        ownWrite[s] = 0;
        lastRead[s] = 0;
        remaining = remaining + sizeof(p[s]);
      }
      assert remaining > 0, "planned no operations";
      goto Running;
    }
    on eWritten do { assert false, "write before the sessions were planned"; }
    on eReadServed do { assert false, "read before the sessions were planned"; }
  }

  hot state Running {
    on eSessionsPlanned do { assert false, "sessions planned twice"; }
    on eWritten do (w: (session: int, value: int)) {
      Next(w.session, Write);
      assert !(w.value in position), format("value {0} written twice", w.value);
      position[w.value] = sizeof(order);
      order += (sizeof(order), w.value);
      ownWrite[w.session] = position[w.value];
      Done();
    }
    on eReadServed do (r: (session: int, value: int)) {
      Next(r.session, Read);
      assert r.value in position, format("session {0} read value {1}, which was never written", r.session, r.value);
      assert position[r.value] >= ownWrite[r.session],
        format("read-your-writes violated: session {0} read value {1}, older than its own write of value {2}",
          r.session, r.value, order[ownWrite[r.session]]);
      assert position[r.value] >= lastRead[r.session],
        format("monotonic reads violated: session {0} read value {1} after it had already read value {2}",
          r.session, r.value, order[lastRead[r.session]]);
      lastRead[r.session] = position[r.value];
      Done();
    }
  }

  state AllDone {
    on eSessionsPlanned do { assert false, "sessions planned twice"; }
    on eWritten do (w: (session: int, value: int)) {
      assert false, format("session {0} wrote {1} beyond its plan", w.session, w.value);
    }
    on eReadServed do (r: (session: int, value: int)) {
      assert false, format("session {0} read {1} beyond its plan", r.session, r.value);
    }
  }

  // Each operation must be the session's next planned one.
  fun Next(s: int, op: tOp) {
    assert s in plans && step[s] < sizeof(plans[s]), format("session {0} did more than it planned", s);
    assert plans[s][step[s]] == op, format("session {0} did its operations out of order", s);
    step[s] = step[s] + 1;
  }

  fun Done() {
    remaining = remaining - 1;
    if (remaining == 0) { goto AllDone; }
  }
}
