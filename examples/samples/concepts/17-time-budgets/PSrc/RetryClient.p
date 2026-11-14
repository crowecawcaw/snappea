// A client calls a flaky server: up to 3 attempts, a per-attempt timeout, and a backoff
// of 50 ms then 100 ms between attempts. Its caller needs an answer, success or error,
// within 1 second. The retry policy is configuration, passed in by the caller.

type Policy = (timeout: int, backoff: seq[int]);

event Request: (client: machine, attempt: int);
event Response: int;
event Timeout: int;
event BackoffDone: int;

// The flaky server is the environment, so it is a driver: its losses are test input.
// snappea: driver
machine Server {
  start state Serving {
    on Request do (r: (client: machine, attempt: int)) {
      announce Received, r.attempt;
      // The reply may be lost.
      if ($) { return; }
      Elapse(ms(8), ms(36));
      send r.client, Response, r.attempt;
      announce Replied, r.attempt;
    }
  }
}

machine Client {
  var server: Server;
  var policy: Policy;
  var attempt: int;

  start state Init {
    entry (input: (server: Server, policy: Policy)) {
      server = input.server;
      policy = input.policy;
      goto Trying;
    }
  }

  state Trying {
    entry {
      attempt = attempt + 1;
      announce Attempt, attempt;
      send server, Request, (client = this, attempt = attempt);
      StartTimer(this, Timeout, attempt, policy.timeout);
    }
    on Response do (a: int) {
      // A late reply to an earlier attempt.
      if (a != attempt) { return; }
      Resolve(true);
    }
    // Each attempt's timer fires once and the attempt moves on only when it does, so
    // a timeout here is always for the current attempt.
    on Timeout do (a: int) {
      if (attempt == 3) {
        Resolve(false);
      } else {
        StartTimer(this, BackoffDone, attempt, policy.backoff[attempt - 1]);
        goto BackingOff;
      }
    }
  }

  state BackingOff {
    on BackoffDone goto Trying;
    ignore Response, Timeout;
  }

  state Done {
    ignore Response, Timeout, BackoffDone;
  }

  fun Resolve(ok: bool) {
    announce Outcome, ok;
    goto Done;
  }
}
