spec ForeachScope observes eVisit {
  var entries: map[int, int];
  var padding0: int;
  var padding1: int;
  var padding2: int;
  start state Watching {
    on eVisit do {
      var item: int;
      entries[1] = 2;
      foreach (item in keys(entries)) { assert entries[item] == 2; }
    }
  }
}
