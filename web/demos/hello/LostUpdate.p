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

// Each client reads once, then writes back the value it saw plus one.
// Reading and writing are separate scheduler steps: the read may become stale.
machine Client {
  var counter: Counter;
  var owner: Main;
  var rememberedValue: int;

  start state Reading {
    entry (input: (counter: Counter, owner: Main)) {
      counter = input.counter;
      owner = input.owner;
      send counter, ReadValue, this;
    }

    on ValueRead do (replyValue: int) {
      rememberedValue = replyValue;
      goto Writing;
    }
  }

  state Writing {
    entry {
      // Another client may have read the same value before this write arrives.
      send counter, WriteValue, rememberedValue + 1;
      send owner, ClientFinished;
      goto Finished;
    }
  }

  state Finished { }
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
// snappea-lint: allow no-progress-spec -- this demo is about a safety bug; the clients always finish
test tcTwoClients [main=Main]: { Main, Counter, Client };
