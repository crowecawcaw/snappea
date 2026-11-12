// A one-shot timer, one machine per request.
//
// P has no clock. "The timer fires" is an event the scheduler can deliver before or
// after the reply, so the checker tries both orders. That is the right abstraction for
// a timeout: you do not know which comes first, and the design must be correct either way.

event eTimeout: int;
event eCancel;
// Internal: the timer's own turn to fire.
event eFire;

machine Timer {
  var holder: machine;
  var tag: int;

  start state Armed {
    entry (t: (holder: machine, tag: int)) {
      holder = t.holder;
      tag = t.tag;
      // Fire on a later turn. A cancel already queued wins; one sent after this loses
      // the race, and the timeout is delivered after the holder cancelled it.
      send this, eFire;
    }
    on eFire goto Fired;
    on eCancel goto Cancelled;
  }

  state Fired {
    entry {
      send holder, eTimeout, tag;
      announce eTimerFired, tag;
    }
    // The holder cancelled after we fired: too late.
    ignore eCancel;
  }

  state Cancelled {
    ignore eFire;
  }
}
