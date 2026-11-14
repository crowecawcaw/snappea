// The design: a register that holds one value. Writes and reads are each one atomic
// step of the register; writers run concurrently, so writes commit in either order.

event Write: (client: machine, value: int);
event WriteAck: (value: int);
event Read: (client: machine);
event ReadResponse: (value: int);

machine Register {
  var value: int;

  start state Serving {
    on Write do (w: (client: machine, value: int)) {
      value = w.value;
      // The commit point: the value is now what reads return. Announce it here, in the
      // same atomic step, so the spec sees writes in the order they took effect.
      announce WriteCommitted, (value = w.value,);
      send w.client, WriteAck, (value = w.value,);
    }
    on Read do (r: (client: machine)) {
      send r.client, ReadResponse, (value = value,);
    }
  }
}
