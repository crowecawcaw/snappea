// The test driver: it creates the machines and tells the spec how much work to expect.
machine Main {
  start state Init {
    entry {
      var counter: Counter;
      counter = new Counter();
      announce PingsPlanned, 4;
      new Client((counter = counter, pings = 2));
      new Client((counter = counter, pings = 2));
    }
  }
}

test tcTwoClients [main=Main]: assert PongsCountPings in { Main, Counter, Client };
