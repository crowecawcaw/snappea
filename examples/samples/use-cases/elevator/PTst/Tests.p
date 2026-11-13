// The physical devices, the passengers and the test cases. Only the controller is the
// design under test; the motor, the door and the passengers are drivers.

// Moves the car one floor per command. Moving takes time: between eMotorStarted and
// eMotorStopped other machines run.
event eStep;

machine Motor {
  var floor: int;
  var ctl: machine;

  start state Stopped {
    entry (f: int) {
      floor = f;
    }
    on eMove do (m: (ctl: machine, dir: int)) {
      ctl = m.ctl;
      announce eMotorStarted, (dir = m.dir,);
      floor = floor + m.dir;
      send this, eStep;
      goto Running;
    }
  }

  state Running {
    on eStep do {
      announce eMotorStopped, (floor = floor,);
      send ctl, eArrivedAt, (floor = floor,);
      goto Stopped, floor;
    }
  }
}

// Opens and closes on command. Up to `obstructions` close commands are blocked by
// something in the doorway: the doors stay open and the door reports it.
machine Door {
  var obstructions: int;

  start state Closed {
    entry (n: int) {
      obstructions = n;
    }
    on eOpen do (ctl: machine) {
      announce eDoorOpened;
      send ctl, eDoorOpenAck;
      goto Open;
    }
  }

  state Open {
    on eClose do (ctl: machine) {
      if (obstructions > 0) {
        obstructions = obstructions - 1;
        send ctl, eDoorObstructed;
        return;
      }
      announce eDoorClosed;
      send ctl, eDoorClosedAck;
      goto Shut;
    }
  }

  // Closed again; a separate state so the obstruction budget is not re-initialised.
  state Shut {
    on eOpen do (ctl: machine) {
      announce eDoorOpened;
      send ctl, eDoorOpenAck;
      goto Open;
    }
  }
}

// Presses buttons for `floors`, one after another, at moments the scheduler picks.
machine Passenger {
  start state Pressing {
    entry (p: (ctl: machine, floors: seq[int])) {
      var f: int;
      foreach (f in p.floors) {
        announce eRequested, (floor = f,);
        send p.ctl, eCall, (floor = f,);
      }
    }
  }
}

fun Floors1(a: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  return s;
}

fun Floors2(a: int, b: int): seq[int] {
  var s: seq[int];
  s += (0, a);
  s += (1, b);
  return s;
}

// Three floors, the car stopped at floor `first` with doors closed.
fun StartElevator(first: int, a: seq[int], b: seq[int], obstructions: int) {
  var ctl: Controller;
  announce eElevatorPlanned, (floor = first, floors = 3, requests = sizeof(a) + sizeof(b));
  ctl = new Controller((motor = new Motor(first), door = new Door(obstructions), floor = first));
  new Passenger((ctl = ctl, floors = a));
  new Passenger((ctl = ctl, floors = b));
}

// Two passengers: one rides from floor 2 down to 0, the other wants floor 1.
machine TwoPassengers {
  start state Init {
    entry { StartElevator(0, Floors2(2, 0), Floors1(1), 0); }
  }
}

// Both press for the floor the car is at. The second press can land while the doors
// are opening, open, or closing; it must still get a door opening of its own.
machine CallWhileClosing {
  start state Init {
    entry { StartElevator(0, Floors1(0), Floors1(0), 0); }
  }
}

// The car is at floor 2. One passenger calls it to floor 0; the other presses floor 1
// at any moment: before the car leaves, while it moves toward floor 1, or after it
// passed.
machine CallWhilePassing {
  start state Init {
    entry { StartElevator(2, Floors1(0), Floors1(1), 0); }
  }
}

// The doors are blocked twice before they close. The car must not move meanwhile, and
// must still serve the other floor.
machine ObstructedDoor {
  start state Init {
    entry { StartElevator(0, Floors2(0, 1), Floors1(0), 2); }
  }
}

// From any start floor, two passengers pressing three floors chosen from three, with
// or without one door obstruction.
machine GeneratedRides {
  start state Init {
    entry { StartElevator(choose(3), Floors2(choose(3), choose(3)), Floors1(choose(3)), choose(2)); }
  }
}

machine BuggyMoveBeforeClosed {
  start state Init {
    entry {
      var ctl: EagerController;
      announce eElevatorPlanned, (floor = 0, floors = 3, requests = 2);
      ctl = new EagerController((motor = new Motor(0), door = new Door(0), floor = 0));
      new Passenger((ctl = ctl, floors = Floors2(0, 2)));
    }
  }
}

// Covered: a press for the current floor arrived after the doors opened there, and the
// doors opened there again before the car moved. (A press after the car started moving
// does not count: the motor has not reported the new floor yet.)
scenario ReopenedForLatePress observes eElevatorPlanned, eRequested, eMotorStarted, eMotorStopped, eDoorOpened, eDoorClosed {
  var floor: int;
  var opened: bool;
  start hot state Watching {
    // snappea-lint: allow unused-payload-field -- only the start floor matters here
    on eElevatorPlanned do (p: (floor: int, floors: int, requests: int)) { floor = p.floor; }
    on eMotorStarted do { opened = false; }
    on eMotorStopped do (m: (floor: int)) { floor = m.floor; }
    on eDoorOpened do { opened = true; }
    on eRequested do (r: (floor: int)) {
      if (opened && r.floor == floor) { goto WaitReopen; }
    }
    ignore eDoorClosed;
  }
  hot state WaitReopen {
    on eDoorOpened goto Covered;
    // The car left without reopening: this press is not the case we want.
    on eMotorStarted do { opened = false; goto Watching; }
    ignore eElevatorPlanned, eRequested, eMotorStopped, eDoorClosed;
  }
  cold state Covered {
    ignore eElevatorPlanned, eRequested, eMotorStarted, eMotorStopped, eDoorOpened, eDoorClosed;
  }
}

test tcTwoPassengers [main=TwoPassengers]:
  assert DoorsAndMotion, RequestsServed in { TwoPassengers, Controller, Motor, Door, Passenger };

test tcCallWhileClosing [main=CallWhileClosing]:
  assert DoorsAndMotion, RequestsServed in { CallWhileClosing, Controller, Motor, Door, Passenger };

test tcCallWhilePassing [main=CallWhilePassing]:
  assert DoorsAndMotion, RequestsServed in { CallWhilePassing, Controller, Motor, Door, Passenger };

test tcObstructedDoor [main=ObstructedDoor]:
  assert DoorsAndMotion, RequestsServed in { ObstructedDoor, Controller, Motor, Door, Passenger };

test tcGeneratedRides [main=GeneratedRides]:
  assert DoorsAndMotion, RequestsServed in { GeneratedRides, Controller, Motor, Door, Passenger };

// expect-bug: with its doors open
test tcBugMoveBeforeClosed [main=BuggyMoveBeforeClosed]:
  assert DoorsAndMotion, RequestsServed in { BuggyMoveBeforeClosed, EagerController, Motor, Door, Passenger };
