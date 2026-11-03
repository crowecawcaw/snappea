// Two clients each add one to a shared counter. The final value should be 2.
// Read and write are separate messages: another client can read before we write.
event ReadValue: Client;
event ValueRead: int;
event WriteValue: int;
event ClientFinished;
event ReadFinalValue: Main;

machine Counter {
  // P initializes integers to 0. Each machine owns its own variables.
  var value: int;

  start state Serving {
    on ReadValue do (client: Client) {
      send client, ValueRead, value;
    }

    // This overwrites the value; it does not add to it.
    on WriteValue do (newValue: int) {
      value = newValue;
    }

    on ReadFinalValue do (testHarness: Main) {
      send testHarness, ValueRead, value;
    }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      var rememberedValue: int;
      send input.counter, ReadValue, this;
      receive {
        case ValueRead: (replyValue: int) {
          rememberedValue = replyValue;
        }
      }
      // The remembered value can already be stale when this write is handled.
      send input.counter, WriteValue, rememberedValue + 1;
      send input.owner, ClientFinished;
    }
  }
}

machine Main {
  var counter: Counter;
  var finishedClients: int;

  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }

    on ClientFinished do {
      finishedClients = finishedClients + 1;
      if (finishedClients == 2) {
        send counter, ReadFinalValue, this;
        goto Verify;
      }
    }
  }

  state Verify {
    on ValueRead do (finalValue: int) {
      // The rule describes the intended result, not how to implement it.
      assert finalValue == 2, format("expected 2 increments, counter is {0}", finalValue);
    }
  }
}

// The harness creates two clients, waits for completion, then checks the rule.
// A design fix: send one Increment event and let Counter add 1 in its handler.
test tcTwoClients [main=Main]: { Main, Counter, Client };
