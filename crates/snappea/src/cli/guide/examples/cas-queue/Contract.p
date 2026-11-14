type Command = (id: int, put: bool, item: int);
type Result = (ok: bool, present: bool, value: int);
type Memo = (item: int, result: Result);
event ePlan: (capacity: int, initial: seq[int], left: Command, right: Command);
event eObserved: (command: Command, result: Result, items: seq[int]);
event eReturned: (client: int, command: Command, result: Result);
event eDone;

spec QueueContract observes ePlan, eObserved, eReturned, eDone {
  var capacity: int;
  var items: seq[int];
  var saved: map[int, Memo];
  var commands: map[int, Command];
  var replies: map[int, int];
  var observations: int;
  start hot state Starting {
    on ePlan do (p: (capacity: int, initial: seq[int], left: Command, right: Command)) {
      assert p.capacity > 0;
      assert sizeof(p.initial) <= p.capacity;
      capacity = p.capacity;
      items = p.initial;
      commands[0] = p.left;
      commands[1] = p.right;
      replies[0] = 0;
      replies[1] = 0;
      goto Working;
    }
  }
  hot state Working {
    on eObserved do (p: (command: Command, result: Result, items: seq[int])) {
      var key: int;
      var expected: Result;
      var i: int;
      key = p.command.id * 2;
      if (p.command.put) { key = key + 1; }
      assert p.command == commands[0] || p.command == commands[1], "unplanned command";
      if (key in saved) {
        assert saved[key].item == p.command.item;
        expected = saved[key].result;
      } else {
        expected = (ok = false, present = false, value = 0);
        if (p.command.put) {
          if (sizeof(items) < capacity) {
            expected.ok = true;
            items += (sizeof(items), p.command.item);
          }
        } else {
          if (sizeof(items) > 0) {
            expected.present = true;
            expected.value = items[0];
            items -= 0;
          }
        }
        saved[key] = (item = p.command.item, result = expected);
      }
      assert p.result == expected, "wrong operation result";
      assert sizeof(p.items) == sizeof(items), "wrong queue length";
      i = 0;
      while (i < sizeof(items)) {
        assert p.items[i] == items[i], "wrong FIFO contents";
        i = i + 1;
      }
      observations = observations + 1;
    }
    on eReturned do (p: (client: int, command: Command, result: Result)) {
      var key: int;
      assert p.client in commands;
      assert p.command == commands[p.client], "reply has wrong arguments";
      key = p.command.id * 2;
      if (p.command.put) { key = key + 1; }
      assert key in saved;
      assert p.result == saved[key].result, "wrong public reply";
      replies[p.client] = replies[p.client] + 1;
      assert replies[p.client] <= 2;
    }
    on eDone do {
      assert observations == 4, "missing invocation";
      assert replies[0] == 2 && replies[1] == 2, "missing reply";
      goto Complete;
    }
  }
  state Complete { }
}
