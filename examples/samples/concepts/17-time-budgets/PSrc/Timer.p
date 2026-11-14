// A one-shot timer: a new machine starts at its creator's clock, waits `after`, then
// sends the timeout stamped creation + after. Without --timed the scheduler may still
// deliver it before an earlier reply, as with any P timer.
//
// The timer stands in for the OS clock, so it is a driver: not mutated.

type TimerConfig = (holder: machine, message: event, tag: int, after: int);

// snappea: driver
machine Timer {
  start state Arming {
    entry (t: TimerConfig) {
      Elapse(t.after, t.after);
      // Announce before the send, like the server's Replied.
      announce TimerFired, (message = t.message, tag = t.tag);
      send t.holder, t.message, t.tag;
    }
  }
}

fun StartTimer(holder: machine, message: event, tag: int, after: int) {
  new Timer((holder = holder, message = message, tag = tag, after = after));
}
