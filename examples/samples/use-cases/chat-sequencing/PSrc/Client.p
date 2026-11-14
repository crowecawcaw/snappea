// A chat client. It displays messages strictly in sequence order: the next expected
// seq is displayed; any other is skipped. An older one is a duplicate from a replay; a
// newer one overtook the replay and will come again in it. When its
// connection drops it reconnects at once, resuming after the last seq it displayed.

// From the network: this client's connection dropped.
event Disconnected;
// To the network: open a connection for this client to the server.
event Connect: (client: machine, server: machine, after: int);

machine Client {
  var id: int;
  var server: machine;
  var network: machine;
  var displayed: int;

  start state Connected {
    entry (input: (id: int, server: machine, network: machine)) {
      id = input.id;
      server = input.server;
      network = input.network;
      send network, Connect, (client = this, server = server, after = displayed);
    }
    on Deliver do (d: (sn: int, msg: int)) {
      if (d.sn == displayed + 1) {
        displayed = d.sn;
        announce Displayed, (client = id, sn = d.sn, msg = d.msg);
      }
    }
    on Disconnected do {
      send network, Connect, (client = this, server = server, after = displayed);
    }
  }
}

// A sender posts its messages to the room, one after another.
machine Sender {
  start state Posting {
    entry (input: (server: machine, msgs: seq[int])) {
      var m: int;
      foreach (m in input.msgs) {
        announce Posted, (msg = m,);
        send input.server, Post, (msg = m,);
      }
    }
  }
}
