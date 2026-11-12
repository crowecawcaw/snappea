// A client calls a flaky server: up to 3 attempts, a per-attempt timeout, and a backoff
// of 50 ms then 100 ms between attempts. Its caller needs an answer, success or error,
// within 1 second. The retry policy is configuration, passed in by the caller.

type tPolicy = (timeout: int, backoff: seq[int]);

event eReq: (client: machine, attempt: int);
event eResp: int;
event eTimeout: int;
event eBackoffDone: int;

// The flaky server is the environment, so it is a driver: its losses are test input.
// snappea: driver
machine Server {
  start state Serving {
    on eReq do (r: (client: machine, attempt: int)) {
      announce eReceived, r.attempt;
      // The reply may be lost.
      if ($) { return; }
      Elapse(ms(8), ms(36));
      send r.client, eResp, r.attempt;
      announce eReplied, r.attempt;
    }
  }
}

machine Client {
  var server: Server;
  var policy: tPolicy;
  var attempt: int;

  start state Init {
    entry (input: (server: Server, policy: tPolicy)) {
      server = input.server;
      policy = input.policy;
      goto Trying;
    }
  }

  state Trying {
    entry {
      attempt = attempt + 1;
      announce eAttempt, attempt;
      send server, eReq, (client = this, attempt = attempt);
      StartTimer(this, eTimeout, attempt, policy.timeout);
    }
    on eResp do (a: int) {
      // A late reply to an earlier attempt.
      if (a != attempt) { return; }
      Resolve(true);
    }
    // Each attempt's timer fires once and the attempt moves on only when it does, so
    // a timeout here is always for the current attempt.
    on eTimeout do (a: int) {
      if (attempt == 3) {
        Resolve(false);
      } else {
        StartTimer(this, eBackoffDone, attempt, policy.backoff[attempt - 1]);
        goto BackingOff;
      }
    }
  }

  state BackingOff {
    on eBackoffDone goto Trying;
    ignore eResp, eTimeout;
  }

  state Done {
    ignore eResp, eTimeout, eBackoffDone;
  }

  fun Resolve(ok: bool) {
    announce eOutcome, ok;
    goto Done;
  }
}
