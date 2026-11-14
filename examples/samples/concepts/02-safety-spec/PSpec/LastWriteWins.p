// Contract: a read returns the value of the last write that committed before it.

// Announced by the register when a write takes effect.
event WriteCommitted: (value: int);

// The spec keeps its own copy of what the register should hold, rebuilt from commit
// events, and compares every read reply (observed as it is sent) against it. It never
// reads the register's variables.
spec LastWriteWins observes WriteCommitted, ReadResponse {
  var expected: int;

  // Hot until the driver's read is answered, so a register that never acknowledges a
  // write (and so never gets read) fails too.
  start hot state Unread {
    on WriteCommitted do (c: (value: int)) {
      expected = c.value;
    }
    on ReadResponse do (r: (value: int)) {
      assert r.value == expected,
        format("read returned {0}, but the last committed write was {1}", r.value, expected);
      goto ReadAnswered;
    }
  }

  state ReadAnswered {
    on WriteCommitted do { assert false, "the driver writes nothing after its read"; }
    on ReadResponse do { assert false, "the driver reads once"; }
  }
}

// WRONG on purpose, kept as a negative control: the same check, but fed by events the
// writers announce when their acknowledgement *arrives*. Two acknowledgements to two
// different writers can arrive in either order, whatever order the writes committed in.
event WriteAckReceived: (value: int);

spec LastAckWins observes WriteAckReceived, ReadResponse {
  var expected: int;

  start hot state Unread {
    on WriteAckReceived do (c: (value: int)) {
      expected = c.value;
    }
    on ReadResponse do (r: (value: int)) {
      assert r.value == expected,
        format("read returned {0}, but the last acknowledged write was {1}", r.value, expected);
      goto ReadAnswered;
    }
  }

  state ReadAnswered {
    on WriteAckReceived do { assert false, "the driver writes nothing after its read"; }
    on ReadResponse do { assert false, "the driver reads once"; }
  }
}
