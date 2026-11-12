// The connection contract: no more sessions open than the server has slots; a
// connection tries at most twice; every connection ends closed for a reason the history
// justifies; every session is freed.

// Announced by the driver before it starts: the slots and the connections it will open.
event ePlanned: (slots: int, connections: int);
// Announced by the server.
event eAccepted: (conn: machine, session: int);
event eRejected: machine;
event eFreed: int;
// Announced by a connection when it closes.
event eClosed: (conn: machine, reason: tReason);

spec Lifecycle observes ePlanned, eAccepted, eRejected, eFreed, eClosed {
  var slots: int;
  var unclosed: int;
  // session -> connection holding it
  var open: map[int, machine];
  var accepted: set[machine];
  // connection -> refused attempts
  var refusals: map[machine, int];
  var closed: set[machine];

  start state Idle {
    on ePlanned goto Running with (p: (slots: int, connections: int)) {
      assert p.slots > 0 && p.connections > 0, "planned nothing";
      slots = p.slots;
      unclosed = p.connections;
    }
    on eAccepted do { assert false, "accepted before anything was planned"; }
    on eRejected do { assert false, "rejected before anything was planned"; }
    on eFreed do { assert false, "freed before anything was planned"; }
    on eClosed do { assert false, "closed before anything was planned"; }
  }

  // Hot until every connection is closed and every session freed.
  hot state Running {
    on ePlanned do { assert false, "planned twice"; }
    on eAccepted do (a: (conn: machine, session: int)) {
      assert !(a.session in open), format("session {0} given out while still open", a.session);
      assert sizeof(open) < slots, format("{0} sessions open on {1} slots", sizeof(open) + 1, slots);
      assert !(a.conn in closed), format("{0} accepted after it closed", a.conn);
      open[a.session] = a.conn;
      accepted += (a.conn);
    }
    on eRejected do (c: machine) {
      assert sizeof(open) == slots, format("{0} rejected with a free slot", c);
      if (!(c in refusals)) { refusals[c] = 0; }
      refusals[c] = refusals[c] + 1;
      assert refusals[c] <= 2, format("{0} refused {1} times: it should give up after 2", c, refusals[c]);
    }
    on eFreed do (session: int) {
      assert session in open, format("session {0} freed but not open", session);
      open -= (session);
      Settle();
    }
    on eClosed do (c: (conn: machine, reason: tReason)) {
      assert !(c.conn in closed), format("{0} closed twice", c.conn);
      if (c.reason == Rejected) {
        assert !(c.conn in accepted) && c.conn in refusals && refusals[c.conn] == 2,
          format("{0} gave up without being refused twice", c.conn);
      } else {
        assert c.conn in accepted, format("{0} closed with {1} but was never accepted", c.conn, c.reason);
      }
      closed += (c.conn);
      unclosed = unclosed - 1;
      Settle();
    }
  }

  state Finished {
    on ePlanned do { assert false, "planned after finishing"; }
    on eAccepted do { assert false, "accepted after finishing"; }
    on eRejected do { assert false, "rejected after finishing"; }
    on eFreed do { assert false, "freed after finishing"; }
    on eClosed do { assert false, "closed after finishing"; }
  }

  fun Settle() {
    if (unclosed == 0 && sizeof(open) == 0) { goto Finished; }
  }
}
