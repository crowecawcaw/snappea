// Retry budget: a client makes at most three attempts. The server may drop requests;
// timers trigger retries. The rule requires success or failure within one second.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.
// $ is a model choice: the checker explores both outcomes across schedules.

// A one-shot timer, one machine per armed timer. It needs no time semantics of its own:
// the new machine starts at its creator's clock, Elapse advances that clock by `after`, so
// the timeout it sends is stamped creation + after. untimed: the timeout may be delivered
// before replies stamped earlier, as with today's $-timers. --timed: events certainly
// arriving earlier are handled first. Cancelling is done by the holder ignoring stale
// tags, which is simpler and race-free compared to a cancel message.

type TimerConfig = (holder: machine, ev: event, tag: int, after: int);

machine Timer {
  start state Arming {
    entry (t: TimerConfig) {
      Elapse(t.after, t.after);
      send t.holder, t.ev, t.tag;
    }
  }
}

fun StartTimer(holder: machine, ev: event, tag: int, after: int) {
  new Timer((holder = holder, ev = ev, tag = tag, after = after));
}
