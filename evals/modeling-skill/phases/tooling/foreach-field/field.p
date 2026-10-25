spec ForeachScope observes eVisit {
  var entries: map[int, int];
  var padding0: int;
  var padding1: int;
  var padding2: int;
  var item: int;
  start state Watching {
    on eVisit do {
      entries[1] = 2;
      foreach (item in keys(entries)) { assert entries[item] == 2; }
    }
  }
}
