// The network: one Connection machine per client connection. It carries messages from
// the server to the client in order, and a connection allowed to drop may do so when a
// message arrives: that message, and every later one on it, is lost, and the client
// is told. The test's drop budget decides which connections may drop.


machine Network {
  var drops: int;
  start state Serving {
    entry (budget: int) { drops = budget; }
    on Connect do (c: (client: machine, server: machine, after: int)) {
      var mayDrop: bool;
      if (drops > 0) {
        drops = drops - 1;
        mayDrop = true;
      }
      new Connection((client = c.client, server = c.server, after = c.after, mayDrop = mayDrop));
    }
  }
}

machine Connection {
  var client: machine;
  var mayDrop: bool;
  start state Open {
    entry (c: (client: machine, server: machine, after: int, mayDrop: bool)) {
      client = c.client;
      mayDrop = c.mayDrop;
      send c.server, Subscribe, (conn = this, after = c.after);
    }
    on Deliver do (d: (sn: int, msg: int)) {
      if (mayDrop && $) {
        // Dropped with this message in flight.
        send client, Disconnected;
        goto Dropped;
      }
      send client, Deliver, d;
    }
  }
  state Dropped {
    ignore Deliver;
  }
}
