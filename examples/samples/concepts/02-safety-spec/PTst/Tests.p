// Drivers: two writers write different values at the same time; once both writes are
// acknowledged, the driver reads.

event eWriterDone;

machine Writer {
  var owner: machine;

  start state Writing {
    entry (input: (register: Register, owner: machine, value: int)) {
      owner = input.owner;
      send input.register, eWrite, (client = this, value = input.value);
    }
    on eWriteAck do (a: (value: int)) {
      // The instrumentation mistake: only LastAckWins listens to this event.
      announce eWriteAckReceived, (value = a.value,);
      send owner, eWriterDone;
    }
  }
}

machine TwoWritersThenRead {
  var register: Register;
  var done: int;

  start state Writing {
    entry {
      register = new Register();
      new Writer((register = register, owner = this, value = 1));
      new Writer((register = register, owner = this, value = 2));
    }
    on eWriterDone do {
      done = done + 1;
      if (done == 2) {
        goto Reading;
      }
    }
  }

  state Reading {
    entry {
      send register, eRead, (client = this,);
    }
    ignore eReadResp;
  }
}

test tcTwoWritersThenRead [main=TwoWritersThenRead]:
  assert LastWriteWins in { TwoWritersThenRead, Writer, Register };

// expect-bug: but the last acknowledged write was
test tcBugSpecUsesAckOrder [main=TwoWritersThenRead]:
  assert LastAckWins in { TwoWritersThenRead, Writer, Register };
