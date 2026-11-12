// The elevator contract, stated over what the physical devices do (the motor and the
// door announce their own movements) and what passengers ask for. Neither spec reads
// the controller: they rebuild where the car is and whether the doors are open from
// the device events.

// Driver: the car starts stopped at `floor` with doors closed; `floors` floors;
// passengers will press `requests` buttons in total.
event eElevatorPlanned: (floor: int, floors: int, requests: int);
// Passenger (driver): pressed a hall or car button for `floor`.
event eRequested: (floor: int);
// Motor: the car started moving one floor in direction `dir` (+1 or -1).
event eMotorStarted: (dir: int);
// Motor: the car stopped, level with `floor`.
event eMotorStopped: (floor: int);
// Door: the doors finished opening / finished closing.
event eDoorOpened;
event eDoorClosed;

// Safety: the car never moves with doors open (or opening), the doors open only while
// the car is stopped level with a floor, and the car stays in the shaft.
spec DoorsAndMotion observes eElevatorPlanned, eMotorStarted, eMotorStopped, eDoorOpened, eDoorClosed {
  var floors: int;
  var floor: int;
  var moving: bool;
  var doorsOpen: bool;

  start state Init {
    // snappea-lint: allow unused-payload-field -- RequestsServed checks the request count
    on eElevatorPlanned do (p: (floor: int, floors: int, requests: int)) {
      floors = p.floors;
      floor = p.floor;
      goto Watching;
    }
    on eMotorStarted do { assert false, "motor started before the elevator was planned"; }
    on eMotorStopped do { assert false, "motor stopped before the elevator was planned"; }
    on eDoorOpened do { assert false, "doors opened before the elevator was planned"; }
    on eDoorClosed do { assert false, "doors closed before the elevator was planned"; }
  }

  state Watching {
    on eElevatorPlanned do { assert false, "elevator planned twice"; }
    on eMotorStarted do (m: (dir: int)) {
      assert !doorsOpen, format("car moved from floor {0} with its doors open", floor);
      assert !moving, "motor started while already moving";
      assert m.dir == 1 || m.dir == -1, format("motor started in direction {0}", m.dir);
      assert floor + m.dir >= 0 && floor + m.dir < floors,
        format("car sent out of the shaft from floor {0}", floor);
      moving = true;
    }
    on eMotorStopped do (m: (floor: int)) {
      assert moving, "motor stopped while not moving";
      moving = false;
      floor = m.floor;
    }
    on eDoorOpened do {
      assert !moving, "doors opened while the car was moving";
      assert !doorsOpen, "doors opened twice";
      doorsOpen = true;
    }
    on eDoorClosed do {
      assert doorsOpen, "doors closed twice";
      doorsOpen = false;
    }
  }
}

// Controller: received the button press for `floor` (its commit point for a request).
event eCallRegistered: (floor: int);
// Controller: commanded the motor to move one floor in direction `dir`. Scheduling is
// judged here, against what the controller knew when it decided.
event eMoveCommanded: (dir: int);

// Progress: every press reaches the controller, and every floor the controller
// registered is eventually served: the doors open there after the registration.
// Scheduling (bounded waiting): each move goes
// toward a registered request; the car reverses only when no registered request lies
// ahead in its direction, so it cannot pass a waiting floor back and forth.
spec RequestsServed observes eElevatorPlanned, eRequested, eCallRegistered, eMoveCommanded, eMotorStopped,
    eDoorOpened {
  var floor: int;
  var floors: int;
  var unrequested: int;
  // presses made but not yet received by the controller, per floor
  var inFlight: map[int, int];
  // floors the controller knows about and has not served
  var registered: set[int];
  var inFlightTotal: int;
  var dir: int;

  start state Init {
    on eElevatorPlanned do (p: (floor: int, floors: int, requests: int)) {
      assert p.requests > 0, "planned no requests";
      floor = p.floor;
      floors = p.floors;
      unrequested = p.requests;
      goto Pending;
    }
    on eRequested do { assert false, "request before the elevator was planned"; }
    on eCallRegistered do { assert false, "call registered before the elevator was planned"; }
    on eMoveCommanded do { assert false, "move commanded before the elevator was planned"; }
    on eMotorStopped do { assert false, "motor stopped before the elevator was planned"; }
    on eDoorOpened do { assert false, "doors opened before the elevator was planned"; }
  }

  hot state Pending {
    on eElevatorPlanned do { assert false, "elevator planned twice"; }
    on eRequested do (r: (floor: int)) {
      assert r.floor >= 0 && r.floor < floors, format("request for floor {0}, outside the building", r.floor);
      assert unrequested > 0, "more requests than planned";
      unrequested = unrequested - 1;
      inFlightTotal = inFlightTotal + 1;
      if (!(r.floor in inFlight)) { inFlight[r.floor] = 0; }
      inFlight[r.floor] = inFlight[r.floor] + 1;
    }
    on eCallRegistered do (c: (floor: int)) {
      assert c.floor in inFlight && inFlight[c.floor] > 0,
        format("controller registered floor {0}, which nobody pressed", c.floor);
      inFlight[c.floor] = inFlight[c.floor] - 1;
      inFlightTotal = inFlightTotal - 1;
      registered += (c.floor);
    }
    on eMoveCommanded do (m: (dir: int)) {
      assert Ahead(m.dir), format("car left floor {0} in direction {1}, away from every request", floor, m.dir);
      assert m.dir == dir || dir == 0 || !Ahead(dir),
        format("car reversed at floor {0} with a request still ahead", floor);
      dir = m.dir;
    }
    on eMotorStopped do (m: (floor: int)) {
      floor = m.floor;
    }
    on eDoorOpened do {
      registered -= (floor);
      if (unrequested == 0 && inFlightTotal == 0 && sizeof(registered) == 0) { goto Served; }
    }
  }

  state Served {
    on eElevatorPlanned do { assert false, "elevator planned twice"; }
    on eRequested do { assert false, "request after every planned request was served"; }
    on eCallRegistered do { assert false, "call registered after every request was served"; }
    on eMoveCommanded do { assert false, "car moved after every request was served"; }
    ignore eMotorStopped, eDoorOpened;
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
