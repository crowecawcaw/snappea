fun Backoff(): seq[int] {
  var b: seq[int];
  b += (0, ms(50));
  b += (1, ms(100));
  return b;
}

// The caller starts its stopwatch as it calls.
fun Call(timeout: int, dropsReplies: bool) {
  announce Called, Mark();
  new Client((server = new Server(dropsReplies), policy = (timeout = timeout, backoff = Backoff())));
}

// The fixed policy: 3 x 250 ms + 50 + 100 ms of backoff = 900 ms in the worst case.
machine FixedPolicy {
  start state Init {
    entry {
      Call(ms(250), true);
    }
  }
}

// A server that always answers, and timeouts that never fire before the reply: the
// first attempt must succeed. This rules out a client that ignores replies and gives up
// within the deadline.
machine ServerAnswers {
  start state Init {
    entry {
      Call(ms(250), false);
    }
  }
}

// A timeout longer than any reply. The client stops waiting for it once the reply
// arrives, so in this model it never fires.
machine LongTimer {
  start state Waiting {
    entry (t: TimerConfig) {}
  }
}

// The first policy: 3 x 300 ms + 150 ms = 1050 ms when every attempt is lost.
machine SlowPolicy {
  start state Init {
    entry {
      Call(ms(300), true);
    }
  }
}

test tcRetryWithinDeadline [main=FixedPolicy]:
  assert AnswersWithinDeadline in { FixedPolicy, Client, Server, Timer };

test tcServerAnswers [main=ServerAnswers]:
  assert AnswersWithinDeadline in (union { ServerAnswers, Client, Server }, { LongTimer -> Timer });

// snappea: expect-bug "after up to 1050000 us; the deadline is 1000000 us"
test tcBugRetryOverDeadline [main=SlowPolicy]:
  assert AnswersWithinDeadline in { SlowPolicy, Client, Server, Timer };
