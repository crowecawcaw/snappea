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
