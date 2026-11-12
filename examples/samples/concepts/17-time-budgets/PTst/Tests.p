fun Backoff(): seq[int] {
  var b: seq[int];
  b += (0, ms(50));
  b += (1, ms(100));
  return b;
}

// The caller starts its stopwatch as it calls.
fun Call(timeout: int) {
  announce eCalled, Mark();
  new Client((server = new Server(), policy = (timeout = timeout, backoff = Backoff())));
}

// The fixed policy: 3 x 250 ms + 50 + 100 ms of backoff = 900 ms in the worst case.
machine FixedPolicy {
  start state Init {
    entry {
      Call(ms(250));
    }
  }
}

// The first policy: 3 x 300 ms + 150 ms = 1050 ms when every attempt is lost.
machine SlowPolicy {
  start state Init {
    entry {
      Call(ms(300));
    }
  }
}

test tcRetryWithinDeadline [main=FixedPolicy]:
  assert AnswersWithinDeadline in { FixedPolicy, Client, Server, Timer };

// expect-bug: the deadline is 1000000 us
test tcBugRetryOverDeadline [main=SlowPolicy]:
  assert AnswersWithinDeadline in { SlowPolicy, Client, Server, Timer };
