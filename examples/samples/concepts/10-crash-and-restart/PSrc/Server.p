// A ticket server. Its fields are volatile: they live in the process and are gone when
// it crashes. A crash ends this machine (`raise halt`); the restart is a new machine that
// must rebuild what it needs from Storage.

event Next: machine;
event Ticket: int;
// From the process manager (the test driver): the process dies now.
event Crash;
// To the process manager: the process exited, start a new one.
event Exited;

machine Server {
  var storage: Storage;
  var owner: machine;
  // Volatile: the next ticket and the request being served.
  var last: int;
  var client: machine;

  start state Recovering {
    entry (input: (storage: Storage, owner: machine)) {
      storage = input.storage;
      owner = input.owner;
      send storage, Read, this;
    }
    on ReadResponse do (highWater: int) {
      last = highWater;
      goto Serving;
    }
    defer Next;
    on Crash goto Crashed;
  }

  state Serving {
    on Next do (c: machine) {
      client = c;
      last = last + 1;
      // Persist before replying: a ticket the client holds must never be handed out again.
      send storage, Write, (server = this, value = last);
      goto Persisting;
    }
    on Crash goto Crashed;
  }

  state Persisting {
    on WriteAck goto Serving with {
      send client, Ticket, last;
      announce Issued, (client = client, ticket = last);
    }
    defer Next;
    on Crash goto Crashed;
  }

  state Crashed {
    entry {
      send owner, Exited;
      raise halt;
    }
  }
}
