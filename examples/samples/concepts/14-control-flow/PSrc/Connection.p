// The client side of a connection: Idle -> Connecting (up to two attempts) -> Open ->
// Closed. However it leaves Open, by its owner's close or the server's reset, the exit
// handler says goodbye so the server frees the slot.

event eOpen: machine;
event eClose: tReason;
event eConnected: machine;
// Raised internally when the last attempt is refused.
event eGiveUp: tReason;

machine Connection {
  var server: Server;
  var app: machine;
  var attempts: int;
  var session: int;

  start state Idle {
    entry (s: Server) { server = s; }
    // `on E goto S with { ... }` runs the block, then moves to S.
    on eOpen goto Connecting with (owner: machine) { app = owner; }
  }

  state Connecting {
    entry {
      attempts = attempts + 1;
      send server, eSyn, this;
    }
    on eSynAck do (r: (ok: bool, session: int)) {
      if (r.ok) {
        // `goto S, payload` passes the payload to S's entry.
        goto Open, r.session;
      }
      if (attempts < 2) {
        // Re-entering a state runs its entry again: the next attempt.
        goto Connecting;
      }
      // `raise` handles an event in this machine right now, before anything queued.
      raise eGiveUp, Rejected;
    }
    // The event's payload becomes Closed's entry payload.
    on eGiveUp goto Closed;
  }

  state Open {
    entry (s: int) {
      session = s;
      send app, eConnected, this;
    }
    on eClose goto Closed;
    on eReset goto Closed;
    // Runs on every way out of Open.
    exit {
      send server, eFin, session;
    }
  }

  state Closed {
    entry (reason: tReason) {
      announce eClosed, (conn = this, reason = reason);
      // Done for good: events still on their way to this machine are dropped.
      raise halt;
    }
  }
}
