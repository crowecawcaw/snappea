// A one-shot timer: a new machine starts at its creator's clock, waits `after`, then
// sends the timeout stamped creation + after. Without --timed the scheduler may still
// deliver it before an earlier reply, as with any P timer.
//
// The timer stands in for the OS clock, so it is a driver: not mutated.

type TimerConfig = (holder: machine, ev: event, tag: int, after: int);

// snappea: driver
machine Timer {
  start state Arming {
    entry (t: TimerConfig) {
      Elapse(t.after, t.after);
      send t.holder, t.ev, t.tag;
      // After the send: from here on the timeout is in the holder's inbox.
      announce TimerFired, (ev = t.ev, tag = t.tag);
    }
  }
}

fun StartTimer(holder: machine, ev: event, tag: int, after: int) {
  new Timer((holder = holder, ev = ev, tag = tag, after = after));
}
