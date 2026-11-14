// The connection contract: no more sessions open than the server has slots; a
// connection tries at most twice; every connection ends closed for a reason the history
// justifies (its app asked, the server evicted it, or it was refused twice); every
// session is freed.

// Announced by the driver before it starts: the slots and the connections it will open.
event Planned: (slots: int, connections: int);
// Announced by an app (a driver) just before it asks its connection to close.
event CloseRequested: machine;
// Announced by the server.
event Accepted: (conn: machine, session: int);
event Rejected: machine;
event Evicted: machine;
event Freed: int;
// Announced by a connection when it closes.
event ConnectionClosed: (conn: machine, reason: Reason);

spec ConnectionsWithinSlots observes Planned, CloseRequested, Accepted, Rejected, Evicted, Freed,
    ConnectionClosed {
  var slots: int;
  var unclosed: int;
  // session -> connection holding it
  var open: map[int, machine];
  var accepted: set[machine];
  // connection -> refused attempts
  var refusals: map[machine, int];
  var closeRequested: set[machine];
  var evicted: set[machine];
  var closed: set[machine];

  start state Idle {
    on Planned goto Running with (p: (slots: int, connections: int)) {
      assert p.slots > 0 && p.connections > 0, "planned nothing";
      slots = p.slots;
      unclosed = p.connections;
    }
  }

  // Hot until every connection is closed and every session freed.
  hot state Running {
    on CloseRequested do (conn: machine) {
      closeRequested += (conn);
    }
    on Accepted do (a: (conn: machine, session: int)) {
      assert !(a.session in open), format("session {0} given out while still open", a.session);
      assert sizeof(open) < slots, format("{0} sessions open on {1} slots", sizeof(open) + 1, slots);
      assert !(a.conn in closed), format("{0} accepted after it closed", a.conn);
      open[a.session] = a.conn;
      accepted += (a.conn);
    }
    on Rejected do (c: machine) {
      assert sizeof(open) == slots, format("{0} rejected with a free slot", c);
      if (!(c in refusals)) { refusals[c] = 0; }
      refusals[c] = refusals[c] + 1;
      assert refusals[c] <= 2, format("{0} refused {1} times: it should give up after 2", c, refusals[c]);
    }
    on Evicted do (c: machine) {
      assert c in accepted, format("{0} evicted, but it was never accepted", c);
      evicted += (c);
    }
    on Freed do (session: int) {
      assert session in open, format("session {0} freed but not open", session);
      open -= (session);
      Settle();
    }
    on ConnectionClosed do (c: (conn: machine, reason: Reason)) {
      assert !(c.conn in closed), format("{0} closed twice", c.conn);
      if (c.reason == CapacityRefused) {
        assert !(c.conn in accepted) && c.conn in refusals && refusals[c.conn] == 2,
          format("{0} gave up without being refused twice", c.conn);
      } else if (c.reason == ClientClose) {
        assert c.conn in closeRequested, format("{0} closed by its client, but its app never asked", c.conn);
      } else {
        assert c.conn in evicted, format("{0} closed by eviction, but the server never evicted it", c.conn);
      }
      closed += (c.conn);
      unclosed = unclosed - 1;
      Settle();
    }
  }

  state Finished {
    // An app may ask to close a connection the server already evicted.
    ignore CloseRequested;
  }

  fun Settle() {
    if (unclosed == 0 && sizeof(open) == 0) { goto Finished; }
  }
}
