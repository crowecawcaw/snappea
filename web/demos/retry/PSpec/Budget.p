// Retry budget: a client makes at most three attempts. The server may drop requests;
// timers trigger retries. The rule requires success or failure within one second.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

// The budget is an ordinary spec. In a spec handler, clocks are those of the
// machine that announced/sent the observed event, so Elapsed(o.started) is
// measured on the client's clock.
spec ResolvesWithinDeadline observes Outcome {
  start state Watching {
    on Outcome do (o: (ok: bool, started: int)) {
      assert Elapsed(o.started) <= ms(1000),
      format("resolved (ok = {0}) after up to {1}us; deadline is 1000000us",
      o.ok, Elapsed(o.started));
    }
  }
}
