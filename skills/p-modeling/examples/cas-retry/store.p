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
