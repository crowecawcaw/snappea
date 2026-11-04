// Retry budget: a client makes at most three attempts. The server may drop requests;
// timers trigger retries. The rule requires success or failure within one second.
// This test harness chooses the number of actors and starts the workload. The checker
// varies their scheduling and model choices.

machine Main {
  start state Init {
    entry {
      new Client(new Server());
    }
  }
}

test tcRetry [main=Main]: assert ResolvesWithinDeadline in {
  Main, Client, Server, Timer
}
;
