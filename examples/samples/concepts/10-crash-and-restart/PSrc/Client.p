// A client wants one ticket. When the process manager says the server restarted and the
// client is still waiting, it resends: its request may have died with the old process.

// From the process manager: the server's new address.
event eRestarted: machine;

machine Client {
  var server: machine;

  start state Asking {
    entry (s: machine) {
      server = s;
      send server, eNext, this;
    }
    on eRestarted do (s: machine) {
      server = s;
      send server, eNext, this;
    }
    on eTicket goto Holding with (t: int) {
      announce eGotTicket, (client = this, ticket = t);
    }
  }

  // A client that holds a ticket never asks again, so a second ticket is an error.
  state Holding {
    ignore eRestarted;
  }
}
