// The elevator contract, stated over what the physical devices do (the motor and the
// door announce their own movements) and what passengers ask for. Neither spec reads
// the controller: they rebuild where the car is and whether the doors are open from
// the device events.

// Driver: the car starts stopped at `floor` with doors closed; `floors` floors;
// passengers will press `requests` buttons in total.
event ElevatorPlanned: (floor: int, floors: int, requests: int);
// Passenger (driver): pressed a hall or car button for `floor`.
event Requested: (floor: int);
// Motor: the car started moving one floor in direction `dir` (+1 or -1).
event MotorStarted: (dir: int);
// Motor: the car stopped, level with `floor`.
event MotorStopped: (floor: int);
// Door: the doors finished opening / finished closing.
event DoorOpened;
event DoorClosed;

// Safety: the car never moves with doors open (or opening), the doors open only while
// the car is stopped level with a floor, and the car stays in the shaft.
spec DoorsAndMotion observes ElevatorPlanned, MotorStarted, MotorStopped, DoorOpened, DoorClosed {
  var floors: int;
  var floor: int;
  var moving: bool;
  var doorsOpen: bool;

  start state Init {
    on ElevatorPlanned do (p: (floor: int, floors: int, requests: int)) {
      floors = p.floors;
      floor = p.floor;
      goto Watching;
    }
    on MotorStarted do { assert false, "motor started before the elevator was planned"; }
    on MotorStopped do { assert false, "motor stopped before the elevator was planned"; }
    on DoorOpened do { assert false, "doors opened before the elevator was planned"; }
    on DoorClosed do { assert false, "doors closed before the elevator was planned"; }
  }

  state Watching {
    on ElevatorPlanned do { assert false, "elevator planned twice"; }
    on MotorStarted do (m: (dir: int)) {
      assert !doorsOpen, format("car moved from floor {0} with its doors open", floor);
      assert !moving, "motor started while already moving";
      assert m.dir == 1 || m.dir == -1, format("motor started in direction {0}", m.dir);
      assert floor + m.dir >= 0 && floor + m.dir < floors,
        format("car sent out of the shaft from floor {0}", floor);
      moving = true;
    }
    on MotorStopped do (m: (floor: int)) {
      assert moving, "motor stopped while not moving";
      moving = false;
      floor = m.floor;
    }
    on DoorOpened do {
      assert !moving, "doors opened while the car was moving";
      assert !doorsOpen, "doors opened twice";
      doorsOpen = true;
    }
    on DoorClosed do {
      assert doorsOpen, "doors closed twice";
      doorsOpen = false;
    }
  }
}

// Controller: received the button press for `floor` (its commit point for a request).
event CallRegistered: (floor: int);
// Controller: commanded the motor to move one floor in direction `dir`. Scheduling is
// judged here, against what the controller knew when it decided.
event MoveCommanded: (dir: int);

// Progress: every press reaches the controller, and every floor the controller
// registered is eventually served: the doors open there after the registration.
// Scheduling (bounded waiting): each move goes
// toward a registered request; the car reverses only when no registered request lies
// ahead in its direction, so it cannot pass a waiting floor back and forth.
spec RequestsServed observes ElevatorPlanned, Requested, CallRegistered, MoveCommanded, MotorStarted,
    MotorStopped, DoorOpened {
  var floor: int;
  var floors: int;
  var unrequested: int;
  // presses made but not yet received by the controller, per floor
  var inFlight: map[int, int];
  // floors the controller knows about and has not served
  var registered: set[int];
  var inFlightTotal: int;
  var dir: int;
  // the move the controller commanded and the motor has not started yet
  var commanded: bool;

  start state Init {
    on ElevatorPlanned do (p: (floor: int, floors: int, requests: int)) {
      assert p.requests > 0, "planned no requests";
      floor = p.floor;
      floors = p.floors;
      unrequested = p.requests;
      goto Pending;
    }
    on Requested do { assert false, "request before the elevator was planned"; }
    on CallRegistered do { assert false, "call registered before the elevator was planned"; }
    on MoveCommanded do { assert false, "move commanded before the elevator was planned"; }
    on MotorStarted do { assert false, "motor started before the elevator was planned"; }
    on MotorStopped do { assert false, "motor stopped before the elevator was planned"; }
    on DoorOpened do { assert false, "doors opened before the elevator was planned"; }
  }

  hot state Pending {
    on ElevatorPlanned do { assert false, "elevator planned twice"; }
    on Requested do (r: (floor: int)) {
      assert r.floor >= 0 && r.floor < floors, format("request for floor {0}, outside the building", r.floor);
      assert unrequested > 0, "more requests than planned";
      unrequested = unrequested - 1;
      inFlightTotal = inFlightTotal + 1;
      if (!(r.floor in inFlight)) { inFlight[r.floor] = 0; }
      inFlight[r.floor] = inFlight[r.floor] + 1;
    }
    on CallRegistered do (c: (floor: int)) {
      assert c.floor in inFlight && inFlight[c.floor] > 0,
        format("controller registered floor {0}, which nobody pressed", c.floor);
      inFlight[c.floor] = inFlight[c.floor] - 1;
      inFlightTotal = inFlightTotal - 1;
      registered += (c.floor);
    }
    on MoveCommanded do (m: (dir: int)) {
      assert Ahead(m.dir), format("car left floor {0} in direction {1}, away from every request", floor, m.dir);
      assert m.dir == dir || dir == 0 || !Ahead(dir),
        format("car reversed at floor {0} with a request still ahead", floor);
      assert !commanded, "move commanded while the last one had not started";
      commanded = true;
      dir = m.dir;
    }
    on MotorStarted do (m: (dir: int)) {
      assert commanded && m.dir == dir, format("motor started in direction {0} without that command", m.dir);
      commanded = false;
    }
    on MotorStopped do (m: (floor: int)) {
      floor = m.floor;
    }
    on DoorOpened do {
      registered -= (floor);
      if (unrequested == 0 && inFlightTotal == 0 && sizeof(registered) == 0) { goto Served; }
    }
  }

  state Served {
    on ElevatorPlanned do { assert false, "elevator planned twice"; }
    on Requested do { assert false, "request after every planned request was served"; }
    on CallRegistered do { assert false, "call registered after every request was served"; }
    on MoveCommanded do { assert false, "car moved after every request was served"; }
    on MotorStarted do { assert false, "motor started after every request was served"; }
    ignore MotorStopped, DoorOpened;
  }

  // Is a registered request in direction d from the car's floor?
  fun Ahead(d: int): bool {
    var f: int;
    foreach (f in registered) {
      if ((f - floor) * d > 0) { return true; }
    }
    return false;
  }
}
