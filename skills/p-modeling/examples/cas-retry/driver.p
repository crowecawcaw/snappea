machine Client {
  var store: Store;
  var coordinator: machine;
  var command: Command;
  start state Run {
    entry (input: (store: Store, coordinator: machine, command: Command)) {
      store = input.store;
      coordinator = input.coordinator;
      command = input.command;
      invoke();
      send coordinator, eFirstDone;
      goto Waiting;
    }
  }
  state Waiting {
    on eAgain do {
      // Reuse the saved definition, never independently resample retry args.
      invoke();
      send coordinator, eClientDone;
      goto Finished;
    }
  }
  state Finished { }

  fun invoke() {
    var snapshot: Snapshot;
    var candidate: int;
    var nextMemos: map[int, Memo];
    var accepted: bool;
    var completed: bool;
    var attempts: int;
    while (!completed) {
      attempts = attempts + 1;
      // Test-only progress bound: there is only one competing fresh write, so
      // at most one CAS can lose. This is not a production retry limit.
      assert attempts <= 2, "operation failed to complete within bounded contention";
      send store, eLoad, this;
      receive { case eLoaded: (s: Snapshot) { snapshot = s; } }
      if (command.id in snapshot.memos) {
        send store, eReplay, (client = this, command = command);
        receive { case eResult: (result: bool) {
          announce eReturned, (id = command.id, accepted = result);
          completed = true;
        } }
      } else {
        // Calculate from the saved snapshot, not Store's live fields. A losing
        // CAS must re-run this calculation using the next load's new balance.
        candidate = snapshot.balance;
        accepted = command.credit || command.units <= candidate;
        if (accepted) {
          if (command.credit) { candidate = candidate + command.units; }
          else { candidate = candidate - command.units; }
        }
        nextMemos = snapshot.memos;
        nextMemos[command.id] = (credit = command.credit,
          units = command.units, accepted = accepted);
        send store, eTry, (client = this, command = command,
          expected = snapshot.version, balance = candidate,
          memos = nextMemos, accepted = accepted);
        receive {
          case eResult: (result: bool) {
            announce eReturned, (id = command.id, accepted = result);
            completed = true;
          }
          case eConflict: { }
        }
      }
    }
  }
}

// Same driver for generated and targeted cases. The phase barrier guarantees
// each retry follows BOTH fresh calls, rather than being an immediate retry.
// snappea: driver
// Scenario only coordinates the workload. Client contains the modeled CAS
// algorithm and is deliberately NOT marked driver, so mutations still test it.
machine Scenario {
  var left: Client;
  var right: Client;
  var firstDone: int;
  var clientsDone: int;
  start state Running {
    entry (input: (left: Command, right: Command)) {
      var store: Store;
      store = new Store();
      left = new Client((store = store, coordinator = this, command = input.left));
      right = new Client((store = store, coordinator = this, command = input.right));
    }
    on eFirstDone do {
      firstDone = firstDone + 1;
      if (firstDone == 2) { send left, eAgain; send right, eAgain; }
    }
    on eClientDone do {
      clientsDone = clientsDone + 1;
      if (clientsDone == 2) { announce eDone; }
    }
  }
}

machine Generated {
  start state Init {
    entry {
      // Independent data choices AND independent clients. Unique IDs bound the
      // memo space; debit/credit and amounts 1/3 cross the initial balance 2.
      new Scenario((left = (id = 0, credit = $, units = 1 + 2 * choose(2)),
        right = (id = 1, credit = $, units = 1 + 2 * choose(2))));
    }
  }
}

machine DelayedAcceptedRetry {
  start state Init {
    entry {
      // Both debit-1 calls succeed. Each retry must retain zero, not debit again.
      new Scenario((left = (id = 0, credit = false, units = 1),
        right = (id = 1, credit = false, units = 1)));
    }
  }
}

machine DelayedRejectedRetry {
  start state Init {
    entry {
      // Scheduler can reject debit-3 at balance 2, then credit-3. The old debit
      // must still reject although a fresh debit would now be affordable.
      new Scenario((left = (id = 0, credit = false, units = 3),
        right = (id = 1, credit = true, units = 3)));
    }
  }
}

test tcGenerated [main=Generated]:
  assert BalanceContract in { Generated, Scenario, Client, Store };
test tcDelayedAccepted [main=DelayedAcceptedRetry]:
  assert BalanceContract in { DelayedAcceptedRetry, Scenario, Client, Store };
test tcDelayedRejected [main=DelayedRejectedRetry]:
  assert BalanceContract in { DelayedRejectedRetry, Scenario, Client, Store };
