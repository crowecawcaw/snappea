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

// Progress: every requested floor is eventually served: the doors open there after
// the request. A request for the floor the doors are already open at is served only
// by a later opening (the passenger pressed because the doors were closing).
spec RequestsServed observes eElevatorPlanned, eRequested, eMotorStopped, eDoorOpened {
  var floor: int;
  var floors: int;
  var unrequested: int;
  var pending: set[int];

  start state Init {
    on eElevatorPlanned do (p: (floor: int, floors: int, requests: int)) {
      assert p.requests > 0, "planned no requests";
      floor = p.floor;
      floors = p.floors;
      unrequested = p.requests;
      goto Pending;
    }
    on eRequested do { assert false, "request before the elevator was planned"; }
    on eMotorStopped do { assert false, "motor stopped before the elevator was planned"; }
    on eDoorOpened do { assert false, "doors opened before the elevator was planned"; }
  }

  hot state Pending {
    on eElevatorPlanned do { assert false, "elevator planned twice"; }
    on eRequested do (r: (floor: int)) {
      assert r.floor >= 0 && r.floor < floors, format("request for floor {0}, outside the building", r.floor);
      assert unrequested > 0, "more requests than planned";
      unrequested = unrequested - 1;
      pending += (r.floor);
    }
    on eMotorStopped do (m: (floor: int)) {
      floor = m.floor;
    }
    on eDoorOpened do {
      pending -= (floor);
      if (unrequested == 0 && sizeof(pending) == 0) { goto Served; }
    }
  }

  state Served {
    on eElevatorPlanned do { assert false, "elevator planned twice"; }
    on eRequested do { assert false, "request after every planned request was served"; }
    ignore eMotorStopped, eDoorOpened;
  }
}
