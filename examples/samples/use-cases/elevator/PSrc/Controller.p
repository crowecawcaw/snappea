// The elevator controller. It owns the request list and commands two devices:
//   motor: eMove(dir) moves the car one floor and answers eArrivedAt(floor);
//   door:  eOpen answers eDoorOpenAck; eClose answers eDoorClosedAck, or
//          eDoorObstructed when something blocks the doors (they stay open).
// The controller moves the motor only after the door has acknowledged it is closed,
// and opens the door only when the motor has answered that the car is level with a
// floor. Each handler is one step of the controller's loop; button presses and device
// answers interleave with them.
//
// Scheduling: keep going in the current direction while requests lie ahead, then turn.

event eCall: (floor: int);
event eMove: (ctl: machine, dir: int);
event eArrivedAt: (floor: int);
event eOpen: machine;
event eClose: machine;
event eDoorOpenAck;
event eDoorClosedAck;
event eDoorObstructed;

machine Controller {
  var motor: machine;
  var door: machine;
  var floor: int;
  var dir: int;
  var requests: set[int];

  start state Starting {
    entry (input: (motor: machine, door: machine, floor: int)) {
      motor = input.motor;
      door = input.door;
      floor = input.floor;
      goto Idle;
    }
  }

  state Idle {
    on eCall do (c: (floor: int)) {
      Register(c.floor);
      Dispatch();
    }
  }

  state Moving {
    on eCall do (c: (floor: int)) { Register(c.floor); }
    on eArrivedAt do (a: (floor: int)) {
      floor = a.floor;
      Dispatch();
    }
  }

  state Opening {
    on eCall do (c: (floor: int)) { Register(c.floor); }
    on eDoorOpenAck do {
      // Doors are open: passengers get on and off. Then close them.
      send door, eClose, this;
      goto Closing;
    }
  }

  state Closing {
    on eCall do (c: (floor: int)) { Register(c.floor); }
    on eDoorObstructed do {
      // Something is in the doorway: the doors stayed open. Try again.
      send door, eClose, this;
    }
    on eDoorClosedAck do {
      Dispatch();
    }
  }

  // Decide the next step with the doors closed and the car stopped.
  fun Register(f: int) {
    announce eCallRegistered, (floor = f,);
    requests += (f);
  }

  fun Dispatch() {
    if (floor in requests) {
      // Served by this opening; a press arriving from now on needs another one.
      requests -= (floor);
      send door, eOpen, this;
      goto Opening;
    }
    if (sizeof(requests) == 0) {
      goto Idle;
    }
    dir = NextDirection();
    announce eMoveCommanded, (dir = dir,);
    send motor, eMove, (ctl = this, dir = dir);
    goto Moving;
  }

  // Called only after Dispatch served the current floor, so no request equals `floor`.
  fun NextDirection(): int {
    var f: int;
    var above: bool;
    var below: bool;
    // snappea-lint: allow mutation:swap-operator -- no request equals the current floor here, so > and >= (< and <=) agree
    foreach (f in requests) {
      if (f > floor) { above = true; }
      if (f < floor) { below = true; }
    }
    if (dir == -1 && below) { return -1; }
    if (above) { return 1; }
    return -1;
  }
}
