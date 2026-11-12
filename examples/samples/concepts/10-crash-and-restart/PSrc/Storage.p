// Durable storage: a disk or database that outlives the server process. Its fields
// survive a server crash because it is a separate machine that never crashes.

event eRead: machine;
event eReadResp: int;
event eWrite: (server: machine, value: int);
event eWriteAck: int;

machine Storage {
  // the highest ticket ever handed out
  var highWater: int;

  start state Ready {
    on eRead do (server: machine) {
      send server, eReadResp, highWater;
    }
    on eWrite do (w: (server: machine, value: int)) {
      // One durable write: committed as soon as storage handles it, even if the server
      // that asked has crashed meanwhile and never sees the ack.
      highWater = w.value;
      send w.server, eWriteAck, w.value;
    }
  }
}
