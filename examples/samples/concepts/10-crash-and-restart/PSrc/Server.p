// A ticket server. Its fields are volatile: they live in the process and are gone when
// it crashes. A crash ends this machine (`raise halt`); the restart is a new machine that
// must rebuild what it needs from Storage.

event eNext: machine;
event eTicket: int;
// From the process manager (the test driver): the process dies now.
event eCrash;
// To the process manager: the process exited, start a new one.
event eExited;

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
      send storage, eRead, this;
    }
    on eReadResp do (highWater: int) {
      last = highWater;
      goto Serving;
    }
    defer eNext;
    on eCrash goto Crashed;
  }

  state Serving {
    on eNext do (c: machine) {
      client = c;
      last = last + 1;
      // Persist before replying: a ticket the client holds must never be handed out again.
      send storage, eWrite, (server = this, value = last);
      goto Persisting;
    }
    on eCrash goto Crashed;
  }

  state Persisting {
    on eWriteAck goto Serving with {
      send client, eTicket, last;
      announce eIssued, (client = client, ticket = last);
    }
    defer eNext;
    on eCrash goto Crashed;
  }

  state Crashed {
    entry {
      send owner, eExited;
      raise halt;
    }
  }
}
