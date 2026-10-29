/*
 * File map (search these names):
 * BalanceContract = independent oracle; Store = snapshot/CAS primitive;
 * Client.invoke = candidate/retry algorithm; Scenario = delayed-retry barrier;
 * Generated/DelayedAcceptedRetry/DelayedRejectedRetry = bounds; test tc* = selection.
 *
 * CAS + stable retries: generated inputs AND independent concurrent clients.
 *
 * Contract: initial balance 2; credits succeed; debits succeed iff affordable.
 * Memoize every ID's arguments/result, including rejection. A retry returns its
 * original result and preserves current balance, even after another ID commits.
 *
 * Why split load/candidate/CAS? Two clients can read one version; one loses CAS,
 * reloads, and recomputes. Only the storage snapshot and whole-record CAS are
 * atomic. The implementation must supply those primitives, including result memo.
 * The independent oracle derives expected balance/results from command history,
 * checks actual resulting state immediately, and checks actual client replies.
 * Observations occur at Store transitions; response delivery order is not commit
 * order. A separate correct audit must not hide a wrong public reply.
 *
 * Generated: IDs 0/1, each samples credit/debit and units 1/3 ONCE; retry saved args.
 * Both first calls finish before delayed retries. Fixed cases isolate accepted
 * and rejected replay: an old rejected debit stays rejected after a later credit.
 * Hot completion requires two IDs, four operations, and two replies per ID.
 * At most two CAS rounds per invocation: only one other fresh write can interfere;
 * this test bound is NOT a production retry policy or proof of fairness.
 *
 * Run from this directory:
 *   snappea check Balance.p --no-mutation --timeout 5s
 *   python check_examples.py --binary /path/to/snappea
 * The script requires complete positives and intended assertions for lost update,
 * reapplied retry, mutation on rejection, and corrupt actual reply; compile/lint/
 * coverage/timeouts do not count. Hand-calculated JSONL probes validate only the
 * oracle (including reverse reply order, unfinished and zero work), not real code.
 *
 * Limits: two clients/IDs, one delayed retry each, small positive units, no crashes,
 * lost replies, memo expiry, conflicting ID reuse, external writers or overflow.
 * See ../durable-retry/DurableRetry.p for recovery; ../reservation/test_storage.py
 * for real Python capture. Passing P proves only these modeled bounds.
 */

// --- Contract and independent history oracle ---

type Command = (id: int, credit: bool, units: int);
type Memo = (credit: bool, units: int, accepted: bool);
type Snapshot = (version: int, balance: int, memos: map[int, Memo]);
event eLoad: machine;
event eLoaded: Snapshot;
event eTry: (client: machine, command: Command, expected: int, balance: int, memos: map[int, Memo], accepted: bool);
event eReplay: (client: machine, command: Command);
event eResult: bool;
event eConflict;
event eAgain;
event eFirstDone;
event eClientDone;
event eObserved: (command: Command, accepted: bool, balance: int);
event eReturned: (id: int, accepted: bool);
event eDone;

// This is the external contract, not a copy of the CAS algorithm: it knows no
// storage version or candidate map. Derive results from its own balance/history.
spec BalanceContract observes eObserved, eReturned, eDone {
  var balance: int;
  var saved: map[int, Memo];
  var observations: int;
  var replies: map[int, int];
  start hot state Working {
    entry { balance = 2; }
    on eObserved do (p: (command: Command, accepted: bool, balance: int)) {
      var expected: bool;
      assert p.command.units > 0, "driver issued invalid units";
      if (p.command.id in saved) {
        // A retry must preserve both its original arguments and original result.
        assert saved[p.command.id].credit == p.command.credit;
        assert saved[p.command.id].units == p.command.units;
        expected = saved[p.command.id].accepted;
      } else {
        expected = p.command.credit || p.command.units <= balance;
        saved[p.command.id] = (credit = p.command.credit,
          units = p.command.units, accepted = expected);
        if (expected) {
          if (p.command.credit) { balance = balance + p.command.units; }
          else { balance = balance - p.command.units; }
        }
      }
      assert p.accepted == expected, "result violates balance contract";
      // Inspect actual state immediately, including after a rejection or retry.
      // Merely checking the returned memo would miss a second hidden effect.
      assert p.balance == balance, "operation changed balance incorrectly";
      observations = observations + 1;
    }
    on eReturned do (p: (id: int, accepted: bool)) {
      // Replies can arrive out of order. Their saved outcomes are stable, so
      // validate the actual client result without replaying receipt order.
      assert p.id in saved, "reply before an operation linearized";
      assert p.accepted == saved[p.id].accepted, "client received wrong result";
      if (!(p.id in replies)) { replies[p.id] = 0; }
      replies[p.id] = replies[p.id] + 1;
    }
    on eDone do {
      var id: int;
      assert sizeof(saved) == 2, "scenario omitted a fresh request";
      assert observations == 4, "scenario omitted an invocation or retry";
      foreach (id in keys(saved)) {
        assert id in replies, "request never returned";
        assert replies[id] == 2, "fresh call and retry must both return";
      }
      goto Complete;
    }
  }
  state Complete { }
}

// --- Atomic snapshot/CAS store ---

machine Store {
  var version: int;
  var balance: int;
  var memos: map[int, Memo];
  start state Ready {
    entry { balance = 2; }
    on eLoad do (client: machine) {
      // One primitive returns a consistent copy. The client may hold it while
      // another client commits; that interference must not be abstracted away.
      send client, eLoaded, (version = version, balance = balance, memos = memos);
    }
    on eTry do (p: (client: machine, command: Command, expected: int, balance: int, memos: map[int, Memo], accepted: bool)) {
      if (p.expected == version) {
        // One atomic CAS commits the candidate balance AND its result memo.
        // There is no send/receive/create between these adjacent assignments.
        balance = p.balance;
        memos = p.memos;
        version = version + 1;
        announce eObserved, (command = p.command, accepted = p.accepted, balance = balance);
        send p.client, eResult, p.accepted;
      } else {
        send p.client, eConflict;
      }
    }
    on eReplay do (p: (client: machine, command: Command)) {
      // A memo-hit read is linearized here against live state. Announcing at
      // client receipt instead would confuse response order with commit order.
      assert p.command.id in memos;
      assert memos[p.command.id].credit == p.command.credit;
      assert memos[p.command.id].units == p.command.units;
      announce eObserved, (command = p.command,
        accepted = memos[p.command.id].accepted, balance = balance);
      send p.client, eResult, memos[p.command.id].accepted;
    }
  }
}

// --- Client algorithm, bounded scenarios, selected tests ---

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
