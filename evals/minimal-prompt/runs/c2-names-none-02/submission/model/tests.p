machine TwoUsers {
  start state S { entry {
    var us: seq[string];
    var ns: seq[string];
    us += (0, "a"); us += (1, "b");
    ns += (0, "x"); ns += (1, "y");
    new Driver((users = us, names = ns));
  } }
}
test tcTwoUsers [main = TwoUsers]: assert NamesConsistent in { TwoUsers, Driver, Client, Store };

machine RepeatNames {
  start state S { entry {
    var us: seq[string];
    var ns: seq[string];
    us += (0, "a"); us += (1, "b");
    ns += (0, "x"); ns += (1, "x"); ns += (2, "y");
    new Driver((users = us, names = ns));
  } }
}
test tcRepeatNames [main = RepeatNames]: assert NamesConsistent in { RepeatNames, Driver, Client, Store };
