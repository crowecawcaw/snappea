// The elevator controller. It owns the request list and commands two devices:
//   motor: Move(dir) moves the car one floor and answers ArrivedAt(floor);
//   door:  Open answers DoorOpenAck; Close answers DoorClosedAck, or
//          DoorObstructed when something blocks the doors (they stay open).
// The controller moves the motor only after the door has acknowledged it is closed,
// and opens the door only when the motor has answered that the car is level with a
// floor. Each handler is one step of the controller's loop; button presses and device
// answers interleave with them.
//
// Scheduling: keep going in the current direction while requests lie ahead, then turn.

event CallRequested: (floor: int);
event Move: (ctl: machine, dir: int);
event ArrivedAt: (floor: int);
event Open: machine;
event Close: machine;
event DoorOpenAck;
event DoorClosedAck;
event DoorObstructed;

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
    on CallRequested do (c: (floor: int)) {
      Register(c.floor);
      Dispatch();
    }
  }

  state Moving {
    on CallRequested do (c: (floor: int)) { Register(c.floor); }
    on ArrivedAt do (a: (floor: int)) {
      floor = a.floor;
      Dispatch();
    }
  }

  state Opening {
    on CallRequested do (c: (floor: int)) { Register(c.floor); }
    on DoorOpenAck do {
      // Doors are open: passengers get on and off. Then close them.
      send door, Close, this;
      goto Closing;
    }
  }

  state Closing {
    on CallRequested do (c: (floor: int)) { Register(c.floor); }
    on DoorObstructed do {
      // Something is in the doorway: the doors stayed open. Try again.
      send door, Close, this;
    }
    on DoorClosedAck do {
      Dispatch();
    }
  }

  // Decide the next step with the doors closed and the car stopped.
  fun Register(f: int) {
    announce CallRegistered, (floor = f,);
    requests += (f);
  }

  fun Dispatch() {
    if (floor in requests) {
      // Served by this opening; a press arriving from now on needs another one.
      requests -= (floor);
      send door, Open, this;
      goto Opening;
    }
    if (sizeof(requests) == 0) {
      goto Idle;
    }
    dir = NextDirection();
    announce MoveCommanded, (dir = dir,);
    send motor, Move, (ctl = this, dir = dir);
    goto Moving;
  }

  // Called only after Dispatch served the current floor, so no request equals `floor`.
  fun NextDirection(): int {
    var f: int;
    var above: bool;
    var below: bool;
    foreach (f in requests) {
      // snappea-lint: allow mutation:swap-operator -- Dispatch already removed the current floor, so > and >= agree
      if (f > floor) { above = true; }
      // snappea-lint: allow mutation:swap-operator -- Dispatch already removed the current floor, so < and <= agree
      if (f < floor) { below = true; }
    }
    if (dir == -1 && below) { return -1; }
    if (above) { return 1; }
    return -1;
  }
}
