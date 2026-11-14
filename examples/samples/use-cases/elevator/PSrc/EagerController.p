// Negative control, a separate machine so its code is excluded from the passing tests'
// coverage: to save a second, it commands the motor right after commanding the doors
// to close, without waiting for the door's closed acknowledgement. The motor can start
// before the doors have finished closing.
machine EagerController {
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
    ignore DoorClosedAck;
  }

  state Moving {
    on CallRequested do (c: (floor: int)) { Register(c.floor); }
    on ArrivedAt do (a: (floor: int)) {
      floor = a.floor;
      Dispatch();
    }
    ignore DoorClosedAck;
  }

  state Opening {
    on CallRequested do (c: (floor: int)) { Register(c.floor); }
    on DoorOpenAck do {
      send door, Close, this;
      // BUG: moves on without waiting for DoorClosedAck.
      Dispatch();
    }
    ignore DoorClosedAck;
  }

  fun Register(f: int) {
    announce CallRegistered, (floor = f,);
    requests += (f);
  }

  fun Dispatch() {
    var f: int;
    if (floor in requests) {
      requests -= (floor);
      send door, Open, this;
      goto Opening;
    }
    if (sizeof(requests) == 0) {
      goto Idle;
    }
    dir = -1;
    foreach (f in requests) {
      if (f > floor) { dir = 1; }
    }
    announce MoveCommanded, (dir = dir,);
    send motor, Move, (ctl = this, dir = dir);
    goto Moving;
  }
}
