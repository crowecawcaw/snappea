// Retry budget: a client makes at most three attempts. The server may drop requests;
// timers trigger retries. The rule requires success or failure within one second.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.
// $ is a model choice: the checker explores both outcomes across schedules.

// A client calls a flaky server with 3 attempts, a 300ms per-attempt timeout and 50ms /
// 100ms backoff, and must resolve (success or error) within its caller's 1s deadline. No
// numbers in this file are delivery latencies; those live in PTst/latency.toml.

type RequestData  = (client: Client, attempt: int);

event Request  : RequestData;
event Response : int;          // attempt
event Timeout : int;       // attempt
event BackoffDone : int;
event Outcome : (ok: bool, started: int);   // announced to specs

machine Server {
  start state Serving {
    on Request do (request: RequestData) {
      if ($) {
        return;
      }
      // dropped
      Elapse(ms(8), ms(36));
      send request.client, Response, request.attempt;
    }
  }
}

machine Client {
  var server: Server;
  var attempt: int;
  var started: int;
  var timeoutAfter: int;
  var backoff: seq[int];

  start state Init {
    entry (configuredServer: Server) {
      server = configuredServer;
      timeoutAfter = ms(300);
      backoff += (0, ms(50));
      backoff += (1, ms(100));
      started = Mark();
      goto Trying;
    }
  }

  state Trying {
    entry {
      attempt = attempt + 1;
      send server, Request, (client = this, attempt = attempt);
      StartTimer(this, Timeout, attempt, timeoutAfter);
    }

    on Response do (replyAttempt: int) {
      if (replyAttempt == attempt) {
        announce Outcome, (ok = true, started = started);
        goto Done;
      }
    }

    on Timeout do (replyAttempt: int) {
      if (replyAttempt != attempt) {
        return;
      }

      if (attempt == 3) {
        announce Outcome, (ok = false, started = started);
        goto Done;
      }

      StartTimer(this, BackoffDone, attempt, backoff[attempt - 1]);
      goto BackingOff;
    }
  }

  state BackingOff {
    on BackoffDone goto Trying;
    ignore Response, Timeout;
  }

  state Done {
    ignore Response, Timeout, BackoffDone;
  }
}
