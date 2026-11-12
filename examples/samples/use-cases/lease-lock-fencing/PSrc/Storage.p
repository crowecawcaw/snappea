// The storage service the lock protects. It remembers the highest fencing token it has
// seen and accepts a write only with a token at least that high. The check and the
// write are one atomic step (a conditional write on the storage side).

event eWrite: (client: machine, id: int, token: int);
event eWriteResp: bool;

machine Storage {
  var highest: int;

  start state Serving {
    on eWrite do (w: (client: machine, id: int, token: int)) {
      if (w.token < highest) {
        announce eWriteRejected, (id = w.id, token = w.token);
        send w.client, eWriteResp, false;
        return;
      }
      highest = w.token;
      announce eWriteAccepted, (id = w.id, token = w.token);
      send w.client, eWriteResp, true;
    }
  }
}
