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
    on eCall do (c: (floor: int)) {
      requests += (c.floor);
      Dispatch();
    }
    ignore eDoorClosedAck;
  }

  state Moving {
    on eCall do (c: (floor: int)) { requests += (c.floor); }
    on eArrivedAt do (a: (floor: int)) {
      floor = a.floor;
      Dispatch();
    }
    ignore eDoorClosedAck;
  }

  state Opening {
    on eCall do (c: (floor: int)) { requests += (c.floor); }
    on eDoorOpenAck do {
      send door, eClose, this;
      // BUG: moves on without waiting for eDoorClosedAck.
      Dispatch();
    }
    ignore eDoorClosedAck;
  }

  fun Dispatch() {
    var f: int;
    if (floor in requests) {
      requests -= (floor);
      send door, eOpen, this;
      goto Opening;
    }
    if (sizeof(requests) == 0) {
      goto Idle;
    }
    dir = -1;
    foreach (f in requests) {
      if (f > floor) { dir = 1; }
    }
    send motor, eMove, (ctl = this, dir = dir);
    goto Moving;
  }
}
