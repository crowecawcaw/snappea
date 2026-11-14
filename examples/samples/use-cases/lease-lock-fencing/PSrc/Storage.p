// The storage service the lock protects. It remembers the highest fencing token it has
// seen and accepts a write only with a token at least that high. The check and the
// write are one atomic step (a conditional write on the storage side).

event Write: (client: machine, id: int, token: int);
event WriteResp: bool;

machine Storage {
  var highest: int;

  start state Serving {
    on Write do (w: (client: machine, id: int, token: int)) {
      if (w.token < highest) {
        announce WriteRejected, (id = w.id, token = w.token);
        send w.client, WriteResp, false;
        return;
      }
      highest = w.token;
      announce WriteAccepted, (id = w.id, token = w.token);
      send w.client, WriteResp, true;
    }
  }
}
