// Same counter model as the full Playground, split for a focused editor.
export const BROKEN_MODEL = `machine Counter {
  var value: int;
  start state Serving {
    on eRead do (c: Client) { send c, eValue, value; }
    on eWrite do (v: int) { value = v; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      var v: int;
      send input.counter, eRead, this;
      receive { case eValue: (x: int) { v = x; } }
      send input.counter, eWrite, v + 1;
      send input.owner, eDone;
    }
  }
}`;

export const FIXED_MODEL = `machine Counter {
  var value: int;
  start state Serving {
    on eIncrement do { value = value + 1; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      send input.counter, eIncrement;
      send input.owner, eDone;
    }
  }
}`;

export const TEST_SETUP = `event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`;
