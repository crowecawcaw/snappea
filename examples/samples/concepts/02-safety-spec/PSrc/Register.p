// The design: a register that holds one value. Writes and reads are each one atomic
// step of the register; writers run concurrently, so writes commit in either order.

event eWrite: (client: machine, value: int);
event eWriteAck: (value: int);
event eRead: (client: machine);
event eReadResp: (value: int);

machine Register {
  var value: int;

  start state Serving {
    on eWrite do (w: (client: machine, value: int)) {
      value = w.value;
      // The commit point: the value is now what reads return. Announce it here, in the
      // same atomic step, so the spec sees writes in the order they took effect.
      announce eWriteCommitted, (value = w.value,);
      send w.client, eWriteAck, (value = w.value,);
    }
    on eRead do (r: (client: machine)) {
      send r.client, eReadResp, (value = value,);
    }
  }
}
