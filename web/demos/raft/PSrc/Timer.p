// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.
// $ is a model choice: the checker explores both outcomes across schedules.

// A Timer machine that timeouts non-deterministically.

event StartTimer;
event ElectionTimeout;
event HeartbeatTimeout;
event Tick;

// snappea: driver
machine Timer {
  var holder: machine;
  var timeoutEvent: event;

  start state Init {
    entry (setup: (user: machine, timeoutEvent: event)) {
      holder = setup.user;
      timeoutEvent = setup.timeoutEvent;
      goto TimerIdle;
    }

    ignore Tick, StartTimer;
  }

  state TimerIdle {
    on StartTimer do {
      goto TimerTick;
    }

    on Shutdown goto TimerShutdown;
    ignore Tick;
  }

  state TimerTick {
    entry {
      checkTick();
    }

    on Tick do {
      checkTick();
    }

    on Shutdown goto TimerShutdown;
    ignore StartTimer;
  }

  state TimerShutdown {
    ignore StartTimer, Tick;
  }

  fun checkTick() {
    if ($) {
      // non-deterministic timeout
      send holder, timeoutEvent;
      goto TimerIdle;
    }
    else {
      send this, Tick;
    }
  }
}
