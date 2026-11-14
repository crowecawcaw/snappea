// A client calls a flaky server: up to 3 attempts, a per-attempt timeout, and a backoff
// of 50 ms then 100 ms between attempts. Its caller needs an answer, success or error,
// within 1 second. The retry policy is configuration, passed in by the caller.

type Policy = (timeout: int, backoff: seq[int]);

event Request: (client: machine, attempt: int);
event Reply: int;
event Timeout: int;
event BackoffDone: int;

// The flaky server is the environment, so it is a driver: its losses are test input.
// snappea: driver
machine Server {
  var dropsReplies: bool;

  start state Serving {
    entry (drops: bool) {
      dropsReplies = drops;
    }
    on Request do (r: (client: machine, attempt: int)) {
      announce Received, r.attempt;
      // The reply may be lost.
      if (dropsReplies && $) { return; }
      Elapse(ms(8), ms(36));
      // Announce before the send: the reply exists from here on, so a client that acts
      // on it is always preceded by this event, in the model and in a real log.
      announce Replied, r.attempt;
      send r.client, Reply, r.attempt;
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
    on Reply do (a: int) {
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

  // A late reply to the attempt that just timed out changes nothing: that attempt
  // already counts as failed.
  state BackingOff {
    on BackoffDone goto Trying;
    ignore Reply;
  }

  // Late replies to earlier attempts, and the last attempt's timer, still arrive.
  state Done {
    ignore Reply, Timeout;
  }

  fun Resolve(ok: bool) {
    announce Outcome, ok;
    goto Done;
  }
}
