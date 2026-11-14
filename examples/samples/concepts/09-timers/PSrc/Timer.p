// A one-shot timer, one machine per request.
//
// This untimed model treats "the timer fires" as an event delivered before or
// after the reply, so the checker tries both orders. That is the right abstraction for
// a timeout: you do not know which comes first, and the design must be correct either way.

event Timeout: int;
event Cancel;
// Internal: the timer's own turn to fire.
event Fire;

machine Timer {
  var holder: machine;
  var tag: int;

  start state Armed {
    entry (t: (holder: machine, tag: int)) {
      holder = t.holder;
      tag = t.tag;
      // Fire on a later turn. A cancel already queued wins; one sent after this loses
      // the race, and the timeout is delivered after the holder cancelled it.
      send this, Fire;
    }
    on Fire goto Fired;
    on Cancel goto Cancelled;
  }

  state Fired {
    entry {
      send holder, Timeout, tag;
      announce TimerFired, tag;
    }
    // The holder cancelled after we fired: too late.
    ignore Cancel;
  }

  state Cancelled {
    ignore Fire;
  }
}
