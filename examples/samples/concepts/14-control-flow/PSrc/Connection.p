// The client side of a connection: Idle -> Connecting (up to two attempts) -> Open ->
// Closed. However it leaves Open, by its owner's close or the server's reset, the exit
// handler says goodbye so the server frees the slot.

event Open: machine;
event Close: Reason;
event Connected: machine;
// Raised internally when the last attempt is refused.
event GiveUp: Reason;

machine Connection {
  var server: Server;
  var app: machine;
  var attempts: int;
  var session: int;

  start state Idle {
    entry (s: Server) { server = s; }
    // `on E goto S with { ... }` runs the block, then moves to S.
    on Open goto Connecting with (owner: machine) { app = owner; }
  }

  state Connecting {
    entry {
      attempts = attempts + 1;
      send server, Syn, this;
    }
    on SynAck do (r: (ok: bool, session: int)) {
      if (r.ok) {
        // `goto S, payload` passes the payload to S's entry.
        goto Open, r.session;
      }
      if (attempts < 2) {
        // Re-entering a state runs its entry again: the next attempt.
        goto Connecting;
      }
      // `raise` handles an event in this machine right now, before anything queued.
      raise GiveUp, CapacityRefused;
    }
    // The event's payload becomes Closed's entry payload.
    on GiveUp goto Closed;
  }

  state Open {
    entry (s: int) {
      session = s;
      send app, Connected, this;
    }
    on Close goto Closed;
    on Reset goto Closed;
    // Runs on every way out of Open.
    exit {
      send server, Fin, session;
    }
  }

  state Closed {
    entry (reason: Reason) {
      announce Closed, (conn = this, reason = reason);
      // Done for good: events still on their way to this machine are dropped.
      raise halt;
    }
  }
}
