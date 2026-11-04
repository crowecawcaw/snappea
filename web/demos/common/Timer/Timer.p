// Shared environment model: timers and crash injection make nondeterministic choices so
// the checker can explore different delivery and failure orders.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// The timer state machine models the non-deterministic behavior of an OS timer

// Events used to interact with the timer machine
event StartTimer;
event CancelTimer;
event TimeOut;
event DelayedTimeOut;

machine Timer {
  // user/client of the timer
  var client: machine;

  start state Init {
    entry (_client : machine) {
      client = _client;
      goto WaitForTimerRequests;
    }
  }

  state WaitForTimerRequests {
    on StartTimer goto TimerStarted;

    ignore CancelTimer, DelayedTimeOut;
  }

  state TimerStarted {
    entry {
      // Only fire the timer with a chance of 1/10 to avoid livelocks
      if(choose(10) == 0) {
        send client, TimeOut;
        goto WaitForTimerRequests;
      }
      else {
        send this, DelayedTimeOut;
      }
    }

    on DelayedTimeOut goto TimerStarted;
    on CancelTimer goto WaitForTimerRequests;
    defer StartTimer;
  }
}

// Functions or API's to interact with the OS Timer
// create timer
fun CreateTimer(client: machine) : Timer {
  return new Timer(client);
}

// start timer
fun StartTimer(timer: Timer) {
  send timer, StartTimer;
}

// cancel timer
fun CancelTimer(timer: Timer) {
  send timer, CancelTimer;
}
